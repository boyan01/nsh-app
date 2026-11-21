//! Terminal transport and process lifecycle for the installed executable.

use std::{
    error::Error,
    fs::File,
    io::{self, BufWriter, IsTerminal as _, stdout},
    path::Path,
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
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use nsh_platform_tui::{
    AppOptions, AppView, ColorDepth, Input, Modifiers, MouseButton, MouseInput, MouseKind, Root,
    RunEvent, TerminalCapabilities,
};
use ratatui_core::terminal::Terminal;
use ratatui_crossterm::CrosstermBackend;

use crate::runtime;

#[cfg(target_os = "linux")]
use nsh_platform_linux::CacheLocation;
#[cfg(target_os = "macos")]
use nsh_platform_macos::CacheLocation;

mod graphics;
#[cfg(target_os = "macos")]
mod macos;

static QUIT: AtomicBool = AtomicBool::new(false);
static INSTALLED: AtomicBool = AtomicBool::new(false);

pub(super) fn installed() -> bool {
    INSTALLED.load(Ordering::Acquire)
}

pub(super) fn prepare(directory: &Path) -> Result<File, Box<dyn Error>> {
    let lock = crate::shell::instance::acquire_terminal(directory)?;
    mixin_desktop_api::init_file_logging(
        "Mixin".into(),
        env!("CARGO_PKG_VERSION").into(),
        "2".into(),
    )?;
    Ok(lock)
}

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
    let _instance = match prepare(&directory) {
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
    let root = Root::of::<crate::App>()
        .app_id("one.mixin.messenger.desktop")
        .tokens(tokens);
    let mut options = AppOptions::default();
    options.cache = nsh_platform_tui::CacheOptions::at(CacheLocation::Dir(directory.join("cache")));
    options.data_dir = Some(directory);
    options.cells = nsh_platform_tui::CellSize::new(
        nsh_core::Points(super::COLUMN),
        nsh_core::Points(super::ROW),
    )?;
    options.capabilities.colors = color_depth();
    INSTALLED.store(true, Ordering::Release);
    let result = fullscreen(root, options);

    // The screen is already restored. Only release the instance lock once
    // pending user commands and account shutdown have drained.
    runtime::finish_shutdown();
    log::info!(target: "nsh::mixin", "Terminal shutdown finished");
    log::logger().flush();
    result.map_err(Into::into)
}

pub(super) fn color_depth() -> ColorDepth {
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
        _ if graphics::placeholder_terminal() => ColorDepth::TrueColor,
        _ if std::env::var("TERM").is_ok_and(|value| value.contains("256color")) => {
            ColorDepth::Ansi256
        }
        _ => ColorDepth::Ansi16,
    }
}

fn fullscreen(root: Root, mut options: AppOptions) -> io::Result<()> {
    let _session = Screen::enter(&mut options.capabilities)?;
    if QUIT.load(Ordering::Relaxed) {
        return Ok(());
    }
    let mut surface = AppView::new(options);
    let mut terminal =
        Terminal::new(surface.measure_backend(CrosstermBackend::new(BufWriter::new(stdout()))))?;
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

struct Screen {
    fullscreen: bool,
}

impl Screen {
    fn enter(capabilities: &mut TerminalCapabilities) -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut screen = Self { fullscreen: false };
        graphics::detect(capabilities);
        if QUIT.load(Ordering::Relaxed) {
            return Ok(screen);
        }
        screen.fullscreen = true;
        execute!(
            stdout(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableMouseCapture,
            Clear(ClearType::All),
            MoveTo(0, 0)
        )?;
        Ok(screen)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        if self.fullscreen {
            let _ = execute!(
                stdout(),
                DisableMouseCapture,
                DisableBracketedPaste,
                LeaveAlternateScreen,
                Show
            );
        }
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
        Event::Mouse(mouse) => RunEvent::Input(Input::Mouse(mouse_input(mouse))),
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
            let mut modifiers = modifiers(key.modifiers);
            modifiers.shift |= key.code == KeyCode::BackTab;
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

fn modifiers(keys: KeyModifiers) -> Modifiers {
    Modifiers {
        ctrl: keys.contains(KeyModifiers::CONTROL),
        alt: keys.contains(KeyModifiers::ALT),
        shift: keys.contains(KeyModifiers::SHIFT),
        cmd: keys.contains(KeyModifiers::SUPER),
    }
}

fn mouse_input(mouse: event::MouseEvent) -> MouseInput {
    use event::MouseEventKind;

    let button = |button| match button {
        event::MouseButton::Left => MouseButton::Left,
        event::MouseButton::Right => MouseButton::Right,
        event::MouseButton::Middle => MouseButton::Middle,
    };
    let mut input = MouseInput::default();
    input.column = mouse.column;
    input.row = mouse.row;
    input.modifiers = modifiers(mouse.modifiers);
    input.kind = match mouse.kind {
        MouseEventKind::Down(b) => MouseKind::Down(button(b)),
        MouseEventKind::Up(b) => MouseKind::Up(button(b)),
        MouseEventKind::Moved | MouseEventKind::Drag(_) => MouseKind::Move,
        MouseEventKind::ScrollUp => MouseKind::Wheel { x: 0, y: -1 },
        MouseEventKind::ScrollDown => MouseKind::Wheel { x: 0, y: 1 },
        MouseEventKind::ScrollLeft => MouseKind::Wheel { x: -1, y: 0 },
        MouseEventKind::ScrollRight => MouseKind::Wheel { x: 1, y: 0 },
    };
    input
}
