//! Startup queries for Kitty's Unicode image placements.

use std::{
    io::{self, Write as _},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use nsh_core::geometry::{DevicePixels, Size};
use nsh_platform_tui::{ColorDepth, Graphics, TerminalCapabilities};

// Match the NSH host's one-pixel graphics query, cell size query and final DA1.
const QUERY: &[u8] = b"\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\\x1b[16t\x1b[c";
const QUERY_TIMEOUT: Duration = Duration::from_secs(1);

pub(super) fn placeholder_terminal() -> bool {
    std::env::var("TERM").is_ok_and(|term| {
        let term = term.to_ascii_lowercase();
        term.contains("kitty") || term.contains("ghostty")
    }) || std::env::var("TERM_PROGRAM").is_ok_and(|program| program.eq_ignore_ascii_case("ghostty"))
        || std::env::var_os("KITTY_WINDOW_ID").is_some_and(|id| !id.is_empty())
}

pub(super) fn detect(capabilities: &mut TerminalCapabilities) {
    // Some terminals answer the Kitty query but only support cursor-based
    // placement. NSH uses Unicode placeholders with 24-bit image IDs.
    if capabilities.colors != ColorDepth::TrueColor || !placeholder_terminal() {
        return;
    }
    let replies = query().unwrap_or_default();
    if replies.graphics {
        capabilities.graphics = Graphics::KittyPlaceholders;
        capabilities.cell_pixels = replies.cell_pixels.or_else(window_cell_pixels);
    }
}

fn window_cell_pixels() -> Option<Size<DevicePixels>> {
    let size = crossterm::terminal::window_size().ok()?;
    (size.columns > 0 && size.rows > 0 && size.width >= size.columns && size.height >= size.rows)
        .then(|| pixels(size.width / size.columns, size.height / size.rows))
}

fn pixels(width: u16, height: u16) -> Size<DevicePixels> {
    Size::new(
        DevicePixels(i32::from(width)),
        DevicePixels(i32::from(height)),
    )
}

#[derive(Default)]
struct Replies {
    graphics: bool,
    cell_pixels: Option<Size<DevicePixels>>,
    complete: bool,
}

fn query() -> io::Result<Replies> {
    // Read the replies in raw mode before crossterm starts decoding inputs.
    let mut output = io::stdout().lock();
    output.write_all(QUERY)?;
    output.flush()?;
    drop(output);

    let deadline = Instant::now() + QUERY_TIMEOUT;
    let mut bytes = [0; 1024];
    let mut length = 0;
    let mut replies = Replies::default();
    while length < bytes.len() && !replies.complete && !super::QUIT.load(Ordering::Relaxed) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let mut input = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: input is one initialized poll descriptor, live for the bounded call.
        let ready =
            unsafe { libc::poll(&mut input, 1, remaining.as_millis().max(1) as libc::c_int) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if input.revents & libc::POLLIN == 0 {
            break;
        }
        let tail = &mut bytes[length..];
        // SAFETY: tail is writable for its length. The app has not started its input reader.
        let count = unsafe { libc::read(input.fd, tail.as_mut_ptr().cast(), tail.len()) };
        if count < 0 {
            let error = io::Error::last_os_error();
            if matches!(
                error.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
            ) {
                continue;
            }
            return Err(error);
        }
        if count == 0 {
            break;
        }
        if tail[..count as usize].contains(&0x03) {
            super::QUIT.store(true, Ordering::Relaxed);
            break;
        }
        length += count as usize;
        replies = parse_replies(&bytes[..length]);
    }
    Ok(replies)
}

fn parse_replies(bytes: &[u8]) -> Replies {
    let mut replies = Replies::default();
    let text = String::from_utf8_lossy(bytes);
    let mut sequences = text.split('\x1b').skip(1).peekable();
    while let Some(sequence) = sequences.next() {
        if let Some((keys, "OK")) = sequence.strip_prefix("_G").and_then(|s| s.split_once(';'))
            && sequences.peek().is_some_and(|rest| rest.starts_with('\\'))
            && keys.split(',').any(|key| key == "i=31")
        {
            replies.graphics = true;
        } else if let Some((height, width)) = sequence
            .strip_prefix("[6;")
            .and_then(|s| s.split_once('t'))
            .and_then(|(size, _)| size.split_once(';'))
            && let (Ok(height @ 1..=u16::MAX), Ok(width @ 1..=u16::MAX)) =
                (height.parse::<u16>(), width.parse::<u16>())
        {
            replies.cell_pixels = Some(pixels(width, height));
        } else if let Some((attributes, _)) =
            sequence.strip_prefix("[?").and_then(|s| s.split_once('c'))
            && !attributes.is_empty()
            && attributes.bytes().all(|b| b.is_ascii_digit() || b == b';')
        {
            replies.complete = true;
        }
    }
    replies
}
