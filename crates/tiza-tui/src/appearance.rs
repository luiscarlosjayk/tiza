//! The host terminal's appearance: asks for the background (OSC 11)
//! and foreground (OSC 10) colours, decides light or dark by luminance, and
//! parses the answers — the one that comes back before crossterm reads the
//! terminal (at start-up, through `/dev/tty` with a timed read) and the ones
//! that arrive later through the key stream (after a resize re-query).
//!
//! Mode 2031 (the terminal tells us when its colours change) is NOT
//! enabled: crossterm's input parser does not know the `CSI ? 997 ; n n`
//! report and would swallow the next key press with it. A resize re-asks
//! instead, which is what most terminals send when their theme changes;
//! `kill -WINCH $(pgrep tiza)` asks by hand.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use tiza_core::canvas::Rgb;

/// What the terminal said about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Host {
    pub bg: Option<Rgb>,
    pub fg: Option<Rgb>,
}

impl Host {
    /// Dark when the background's luminance (299/587/114 rule) is under
    /// half; `None` when the terminal did not say.
    pub fn dark(&self) -> Option<bool> {
        self.bg.map(is_dark)
    }
}

/// The 299/587/114 luminance rule, threshold at half.
pub fn is_dark(c: Rgb) -> bool {
    let lum = 0.299 * c.0 as f32 + 0.587 * c.1 as f32 + 0.114 * c.2 as f32;
    lum < 127.5
}

/// The query both paths send: background, then foreground.
pub const QUERY: &str = "\x1b]11;?\x1b\\\x1b]10;?\x1b\\";

/// Parses one OSC colour answer body: `rgb:1515/1717/1b1b` (1-4 hex digits
/// per channel), `rgb:15/17/1b`, or `#151a1b`.
pub fn parse_osc_color(body: &str) -> Option<Rgb> {
    let body = body.trim();
    if let Some(hex) = body.strip_prefix('#') {
        return match hex.len() {
            6 => u32::from_str_radix(hex, 16).ok().map(Rgb::hex),
            _ => None,
        };
    }
    let spec = body.strip_prefix("rgb:")?;
    let parts: Vec<&str> = spec.split('/').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut out = [0u8; 3];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || p.len() > 4 || !p.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        // The top eight bits of the channel: `1515` → 0x15, `f` → 0xff.
        let v = u32::from_str_radix(p, 16).ok()?;
        let bits = 4 * p.len() as u32;
        out[i] = if bits >= 8 {
            (v >> (bits - 8)) as u8
        } else {
            (v * 17) as u8
        };
    }
    Some(Rgb(out[0], out[1], out[2]))
}

/// Pulls every `OSC 10/11 ; colour` answer out of raw terminal input
/// (terminated by `ESC \` or BEL), ignoring anything else.
pub fn parse_answers(input: &str) -> Host {
    let mut host = Host::default();
    let mut rest = input;
    while let Some(start) = rest.find("\x1b]") {
        let after = &rest[start + 2..];
        let end = after
            .find("\x1b\\")
            .map(|e| (e, 2))
            .into_iter()
            .chain(after.find('\x07').map(|e| (e, 1)))
            .min_by_key(|(e, _)| *e);
        let Some((end, skip)) = end else { break };
        let seq = &after[..end];
        if let Some(body) = seq.strip_prefix("11;") {
            host.bg = parse_osc_color(body).or(host.bg);
        } else if let Some(body) = seq.strip_prefix("10;") {
            host.fg = parse_osc_color(body).or(host.fg);
        }
        rest = &after[end + skip..];
    }
    host
}

/// Asks the terminal at start-up, before crossterm owns the input: raw mode
/// on `/dev/tty`, the query, a timed read (VTIME), the old mode back. `None`
/// when there is no tty, it does not answer within `timeout`, or the
/// `TIZA_NO_OSC` variable is set (tests, pipelines).
pub fn query_host(timeout: Duration) -> Option<Host> {
    if std::env::var_os("TIZA_NO_OSC").is_some() {
        return None;
    }
    // The controlling terminal, or stdin when that is the terminal (a pty
    // without a session, as in tests).
    let mut tty = match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
    {
        Ok(f) => f,
        Err(_) => {
            use std::os::fd::AsFd;
            let stdin = std::io::stdin();
            if !rustix::termios::isatty(stdin.as_fd()) {
                return None;
            }
            let dup = rustix::io::dup(stdin.as_fd()).ok()?;
            std::fs::File::from(dup)
        }
    };
    let saved = rustix::termios::tcgetattr(&tty).ok()?;
    let mut raw = saved.clone();
    raw.make_raw();
    // A read returns after `deciseconds` with nothing rather than blocking.
    let deciseconds = (timeout.as_millis() / 100).clamp(1, 25) as u8;
    raw.special_codes[rustix::termios::SpecialCodeIndex::VMIN] = 0;
    raw.special_codes[rustix::termios::SpecialCodeIndex::VTIME] = deciseconds;
    rustix::termios::tcsetattr(&tty, rustix::termios::OptionalActions::Now, &raw).ok()?;
    let result = (|| {
        tty.write_all(QUERY.as_bytes()).ok()?;
        tty.flush().ok()?;
        let mut buf = Vec::new();
        let mut chunk = [0u8; 256];
        let start = Instant::now();
        loop {
            let n = tty.read(&mut chunk).ok()?;
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf);
            let host = parse_answers(&text);
            if host.bg.is_some() && host.fg.is_some() {
                return Some(host);
            }
            if start.elapsed() > timeout * 2 {
                break;
            }
        }
        let host = parse_answers(&String::from_utf8_lossy(&buf));
        (host.bg.is_some()).then_some(host)
    })();
    let _ = rustix::termios::tcsetattr(&tty, rustix::termios::OptionalActions::Now, &saved);
    result
}

/// Sends the query while the TUI runs; the answer comes back through the
/// key stream (`OscCapture`).
pub fn request_host() {
    let mut out = std::io::stdout();
    let _ = out.write_all(QUERY.as_bytes());
    let _ = out.flush();
}

/// Collects an OSC answer that crossterm hands us as key presses: `Alt+]`
/// opens it, characters follow, `Alt+\` or Ctrl-G closes it.
#[derive(Debug, Default, Clone)]
pub struct OscCapture {
    buf: Option<String>,
    started: Option<Instant>,
}

impl OscCapture {
    /// Feeds one key. Returns `Some(host)` when an answer closed and said
    /// something, `None` otherwise; `swallowed` tells whether the key was
    /// part of an answer (and must not reach the editor).
    pub fn feed(
        &mut self,
        code: crossterm::event::KeyCode,
        mods: crossterm::event::KeyModifiers,
    ) -> (bool, Option<Host>) {
        use crossterm::event::{KeyCode, KeyModifiers};
        let alt = mods.contains(KeyModifiers::ALT);
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        // A stale capture (no terminator within a second) is dropped so the
        // keyboard never stays hijacked.
        if let Some(t) = self.started
            && t.elapsed() > Duration::from_secs(1)
        {
            self.buf = None;
            self.started = None;
        }
        match (&mut self.buf, code) {
            (None, KeyCode::Char(']')) if alt => {
                self.buf = Some(String::from("\x1b]"));
                self.started = Some(Instant::now());
                (true, None)
            }
            (None, _) => (false, None),
            (Some(b), KeyCode::Char('\\')) if alt => {
                b.push_str("\x1b\\");
                let host = parse_answers(b);
                self.buf = None;
                self.started = None;
                (
                    true,
                    (host.bg.is_some() || host.fg.is_some()).then_some(host),
                )
            }
            (Some(b), KeyCode::Char('g')) if ctrl => {
                b.push('\x07');
                let host = parse_answers(b);
                self.buf = None;
                self.started = None;
                (
                    true,
                    (host.bg.is_some() || host.fg.is_some()).then_some(host),
                )
            }
            // A second answer begins inside the capture (`ESC ]` again).
            (Some(b), KeyCode::Char(']')) if alt => {
                b.push_str("\x1b]");
                (true, None)
            }
            (Some(b), KeyCode::Char(c)) if !ctrl && !alt => {
                b.push(c);
                (true, None)
            }
            (Some(_), _) => {
                // Anything else is the user: give the key back and stop.
                self.buf = None;
                self.started = None;
                (false, None)
            }
        }
    }

    pub fn active(&self) -> bool {
        self.buf.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn osc_answers_parse_in_every_form() {
        assert_eq!(
            parse_osc_color("rgb:1515/1717/1b1b"),
            Some(Rgb(0x15, 0x17, 0x1B))
        );
        assert_eq!(parse_osc_color("rgb:15/17/1b"), Some(Rgb(0x15, 0x17, 0x1B)));
        assert_eq!(parse_osc_color("rgb:f/0/8"), Some(Rgb(0xFF, 0, 0x88)));
        assert_eq!(parse_osc_color("#f7f5ef"), Some(Rgb(0xF7, 0xF5, 0xEF)));
        assert_eq!(parse_osc_color("rgb:zz/00/00"), None);
        let h = parse_answers("\x1b]11;rgb:f7f7/f5f5/efef\x1b\\\x1b]10;rgb:1f1f/2323/2828\x07junk");
        assert_eq!(h.bg, Some(Rgb(0xF7, 0xF5, 0xEF)));
        assert_eq!(h.fg, Some(Rgb(0x1F, 0x23, 0x28)));
        assert_eq!(h.dark(), Some(false));
        assert!(is_dark(Rgb(0x15, 0x17, 0x1B)) && !is_dark(Rgb(0xF7, 0xF5, 0xEF)));
        assert_eq!(parse_answers("nothing").dark(), None);
    }

    #[test]
    fn a_capture_swallows_the_answer_and_gives_other_keys_back() {
        let mut c = OscCapture::default();
        let (sw, h) = c.feed(KeyCode::Char('j'), KeyModifiers::NONE);
        assert!(!sw && h.is_none());
        assert!(c.feed(KeyCode::Char(']'), KeyModifiers::ALT).0);
        for ch in "11;rgb:1515/1717/1b1b".chars() {
            assert!(c.feed(KeyCode::Char(ch), KeyModifiers::NONE).0);
        }
        let (sw, h) = c.feed(KeyCode::Char('\\'), KeyModifiers::ALT);
        assert!(sw);
        assert_eq!(h.unwrap().dark(), Some(true));
        assert!(!c.active());
        // A user key in the middle ends the capture and is not swallowed.
        c.feed(KeyCode::Char(']'), KeyModifiers::ALT);
        let (sw, _) = c.feed(KeyCode::Down, KeyModifiers::NONE);
        assert!(!sw && !c.active());
    }
}
