//! Terminal transport and process lifecycle for the installed executable.

use std::{
    error::Error,
    io::{self, BufWriter, IsTerminal as _, stdout},
    process::ExitCode,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use crossterm::{
    cursor::{MoveTo, Show},
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use nsh_platform_tui::{AppOptions, AppView, ColorDepth, Input, Modifiers, Root, RunEvent};
use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;

use crate::runtime;

#[cfg(target_os = "macos")]
mod macos;

static QUIT: AtomicBool = AtomicBool::new(false);

pub fn run() -> ExitCode {
    match run_installed() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Mixin TUI: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run_installed() -> Result<(), Box<dyn Error>> {
    if !io::stdin().is_terminal() || !stdout().is_terminal() {
        return Err("run mixin --tui from an interactive terminal".into());
    }
    let directory = mixin_desktop_api::data_directory()?;
    let _instance = match super::prepare(&directory) {
        Ok(instance) => instance,
        #[cfg(target_os = "macos")]
        Err(error) if macos::should_explain_access(&*error) => {
            eprintln!("Mixin TUI: {error}");
            return macos::explain_access().map_err(Into::into);
        }
        Err(error) => return Err(error),
    };
    let _signals = runtime::quit_on_signal(|| QUIT.store(true, Ordering::Relaxed))?;

    // Open the shared runtime while startup errors (including Flutter's Hive
    // lock) can still be printed to the invoking shell.
    futures::executor::block_on(runtime::run(mixin_desktop_api::DesktopClient::open()))?;
    if QUIT.load(Ordering::Relaxed) {
        return Ok(());
    }

    let tokens = nsh_base::tokens_of(Some(include_str!("../../theme.json")))?;
    let root = Root::of::<super::TerminalApp>()
        .app_id("one.mixin.messenger.desktop")
        .tokens(tokens);
    let mut options = AppOptions::default();
    options.data_dir = Some(directory);
    options.capabilities.colors = color_depth();
    let result = fullscreen(root, options);

    // The screen is already restored. Only release the instance lock once
    // pending user commands and account shutdown have drained.
    runtime::finish_shutdown();
    log::info!(target: "nsh::mixin", "Terminal shutdown finished");
    log::logger().flush();
    result.map_err(Into::into)
}

pub(crate) fn image_supported() -> bool {
    matches!(color_depth(), ColorDepth::TrueColor | ColorDepth::Ansi256)
}

fn color_depth() -> ColorDepth {
    match std::env::var("NSH_TUI_COLORS").as_deref() {
        Ok("truecolor") => ColorDepth::TrueColor,
        Ok("256") => ColorDepth::Ansi256,
        Ok("16") => ColorDepth::Ansi16,
        Ok("mono") => ColorDepth::Monochrome,
        _ if std::env::var_os("NO_COLOR").is_some() => ColorDepth::Monochrome,
        _ if std::env::var("COLORTERM")
            .is_ok_and(|value| value == "truecolor" || value == "24bit") =>
        {
            ColorDepth::TrueColor
        }
        _ if std::env::var("TERM").is_ok_and(|value| value.contains("256color")) => {
            ColorDepth::Ansi256
        }
        _ => ColorDepth::Ansi16,
    }
}

fn fullscreen(root: Root, options: AppOptions) -> io::Result<()> {
    let _session = Screen::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(BufWriter::new(stdout())))?;
    let mut surface = AppView::new(options);
    let failure = Arc::new(Mutex::new(None));
    let reported = failure.clone();
    surface.on_app_error(move |error| {
        let error = error.to_string();
        log::error!("Terminal surface failed: {error}");
        *reported.lock().unwrap() = Some(error);
    });
    surface.load(root)?;
    nsh_platform_tui::run_app(&mut terminal, &mut surface, |wait| {
        if QUIT.load(Ordering::Relaxed) || failure.lock().unwrap().is_some() {
            return Ok(RunEvent::Exit);
        }
        poll(wait)
    })?;
    if let Some(error) = failure.lock().unwrap().take() {
        return Err(io::Error::other(error));
    }
    Ok(())
}

struct Screen;

impl Screen {
    fn enter() -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let screen = Self;
        execute!(
            stdout(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            Clear(ClearType::All),
            MoveTo(0, 0)
        )?;
        Ok(screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(stdout(), DisableBracketedPaste, LeaveAlternateScreen, Show);
        let _ = terminal::disable_raw_mode();
    }
}

fn poll(wait: Duration) -> io::Result<RunEvent> {
    if !event::poll(wait)? {
        return Ok(RunEvent::Idle);
    }
    let event = match event::read() {
        Ok(event) => event,
        Err(error) if error.kind() == io::ErrorKind::Interrupted => return Ok(RunEvent::Idle),
        Err(error) => return Err(error),
    };
    use event::Event;
    Ok(match event {
        Event::Resize(..) => RunEvent::Redraw,
        Event::Paste(text) => RunEvent::Input(Input::Paste(text)),
        Event::Key(key) if key.kind != KeyEventKind::Release => {
            if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
                return Ok(RunEvent::Exit);
            }
            let name = match key.code {
                KeyCode::Char(c) => c.to_string(),
                KeyCode::Enter => "Enter".into(),
                KeyCode::Esc => "Escape".into(),
                KeyCode::Backspace => "Backspace".into(),
                KeyCode::Delete => "Delete".into(),
                KeyCode::Left => "ArrowLeft".into(),
                KeyCode::Right => "ArrowRight".into(),
                KeyCode::Up => "ArrowUp".into(),
                KeyCode::Down => "ArrowDown".into(),
                KeyCode::PageUp => "PageUp".into(),
                KeyCode::PageDown => "PageDown".into(),
                KeyCode::Home => "Home".into(),
                KeyCode::End => "End".into(),
                KeyCode::Tab | KeyCode::BackTab => "Tab".into(),
                KeyCode::F(number) => format!("F{number}"),
                _ => return Ok(RunEvent::Idle),
            };
            let modifiers = Modifiers {
                ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
                alt: key.modifiers.contains(KeyModifiers::ALT),
                shift: key.modifiers.contains(KeyModifiers::SHIFT) || key.code == KeyCode::BackTab,
                cmd: key.modifiers.contains(KeyModifiers::SUPER),
            };
            if key.kind == KeyEventKind::Repeat {
                RunEvent::Input(Input::KeyRepeat {
                    key: name,
                    modifiers,
                })
            } else {
                RunEvent::Input(Input::Key {
                    key: name,
                    modifiers,
                    release: false,
                })
            }
        }
        _ => RunEvent::Idle,
    })
}
