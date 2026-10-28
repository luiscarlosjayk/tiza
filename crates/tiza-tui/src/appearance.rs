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

/// How long the start-up read waits for more data before it stops: one
/// timed read (VTIME). The docs give it, and a QA test checks them.
pub const COLOR_QUERY_WAIT: Duration = Duration::from_millis(100);

/// Data that comes after `COLOR_QUERY_WAIT` times this factor ends the
/// start-up read once it is read: so about three waits at most.
pub const COLOR_QUERY_LATE_FACTOR: u32 = 2;

// VTIME counts in tenths of a second.
const _: () = assert!(COLOR_QUERY_WAIT.as_millis().is_multiple_of(100));

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
/// when there is no tty, it does not answer in time ([`COLOR_QUERY_WAIT`]),
/// or the `TIZA_NO_OSC` variable is set (tests, pipelines). Also returns what else
/// the read took from the terminal: keys typed meanwhile, in order, for the
/// event loop ([`crate::early_keys`]); they are not the query's to eat. They
/// come in pieces, as crossterm would have read them: one per read, cut
/// where an answer was taken out (so an ESC that ends one is Esc, not Alt
/// with the next key). Last, the answer that the end of the wait cut short
/// ([`cut_answer`]): its rest comes later, with the keys.
pub fn query_host() -> (Option<Host>, Vec<Vec<u8>>, Option<String>) {
    if osc_disabled() {
        return (None, Vec::new(), None);
    }
    let Some(mut tty) = input_terminal() else {
        return (None, Vec::new(), None);
    };
    let Ok(saved) = rustix::termios::tcgetattr(&tty) else {
        return (None, Vec::new(), None);
    };
    let mut raw = saved.clone();
    raw.make_raw();
    // A read returns after `deciseconds` with nothing rather than blocking.
    let deciseconds = (COLOR_QUERY_WAIT.as_millis() / 100).clamp(1, 25) as u8;
    raw.special_codes[rustix::termios::SpecialCodeIndex::VMIN] = 0;
    raw.special_codes[rustix::termios::SpecialCodeIndex::VTIME] = deciseconds;
    if rustix::termios::tcsetattr(&tty, rustix::termios::OptionalActions::Now, &raw).is_err() {
        return (None, Vec::new(), None);
    }
    let mut buf = Vec::new();
    // Where each read began in `buf`.
    let mut reads = Vec::new();
    let host = (|| {
        tty.write_all(QUERY.as_bytes()).ok()?;
        tty.flush().ok()?;
        let mut chunk = [0u8; 256];
        let start = Instant::now();
        loop {
            let n = tty.read(&mut chunk).ok()?;
            if n == 0 {
                break;
            }
            reads.push(buf.len());
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf);
            let host = parse_answers(&text);
            if host.bg.is_some() && host.fg.is_some() {
                return Some(host);
            }
            if start.elapsed() > COLOR_QUERY_WAIT * COLOR_QUERY_LATE_FACTOR {
                break;
            }
        }
        let host = parse_answers(&String::from_utf8_lossy(&buf));
        (host.bg.is_some()).then_some(host)
    })();
    let _ = rustix::termios::tcsetattr(&tty, rustix::termios::OptionalActions::Now, &saved);
    let (typed, cut) = start_up_rest(&buf, &reads, host.as_ref());
    (host, typed, cut)
}

/// What the start-up read gives the event loop besides `host`: the keys
/// ([`without_answers`]) and the answer cut short ([`cut_answer`]). When
/// the background answer came but the text answer did not, an ESC at the
/// very end is the start of that answer, not a key: its `]` and the rest
/// come later, and the capture takes them.
pub fn start_up_rest(
    buf: &[u8],
    reads: &[usize],
    host: Option<&Host>,
) -> (Vec<Vec<u8>>, Option<String>) {
    let answer_due = host.is_some_and(|h| h.bg.is_some() && h.fg.is_none());
    if answer_due && buf.last() == Some(&0x1b) && cut_answer(buf).is_none() {
        let keys = &buf[..buf.len() - 1];
        return (without_answers(keys, reads), Some("\x1b".to_string()));
    }
    (without_answers(buf, reads), cut_answer(buf))
}

/// The length of an answer's body up to and with its `ESC \` or BEL, or
/// `None` when the body has no end yet.
fn answer_len(body: &[u8]) -> Option<usize> {
    (0..body.len()).find_map(|k| match body[k] {
        0x07 => Some(k + 1),
        0x1b if body.get(k + 1) == Some(&b'\\') => Some(k + 2),
        _ => None,
    })
}

/// The answer that the start-up read cut short at its end: an `ESC ]`
/// with no `ESC \` or BEL after it. The terminal sends the rest later,
/// through the key stream, where [`OscCapture::resume`] takes it.
pub fn cut_answer(buf: &[u8]) -> Option<String> {
    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] == 0x1b && buf[i + 1] == b']' {
            match answer_len(&buf[i + 2..]) {
                Some(n) => i += 2 + n,
                None => return Some(String::from_utf8_lossy(&buf[i..]).into_owned()),
            }
        } else {
            i += 1;
        }
    }
    None
}

/// What the start-up read took from the terminal besides the answers, in
/// pieces: every `ESC ]` … `ESC \` (or BEL) sequence left out, and one cut
/// short by the timeout too (the rest of an answer, never keys); a piece
/// ends where a read (`reads`: where each began in `buf`) or an answer did.
pub fn without_answers(buf: &[u8], reads: &[usize]) -> Vec<Vec<u8>> {
    let mut out: Vec<Vec<u8>> = vec![Vec::new()];
    let cut = |out: &mut Vec<Vec<u8>>| {
        if out.last().is_some_and(|p| !p.is_empty()) {
            out.push(Vec::new());
        }
    };
    let mut i = 0;
    while i < buf.len() {
        if i > 0 && reads.contains(&i) {
            cut(&mut out);
        }
        if buf[i] == 0x1b && buf.get(i + 1) == Some(&b']') {
            match answer_len(&buf[i + 2..]) {
                Some(n) => i += 2 + n,
                None => break,
            }
            cut(&mut out);
            continue;
        }
        if let Some(p) = out.last_mut() {
            p.push(buf[i]);
        }
        i += 1;
    }
    out.retain(|p| !p.is_empty());
    out
}

/// `TIZA_NO_OSC` is set (tests, pipelines): Tiza never asks the terminal.
fn osc_disabled() -> bool {
    std::env::var_os("TIZA_NO_OSC").is_some()
}

/// The terminal crossterm reads keys from: stdin when it is one, else the
/// controlling terminal. The query goes there, so the answer comes back
/// with the keys.
fn input_terminal() -> Option<std::fs::File> {
    use std::os::fd::AsFd;
    let stdin = std::io::stdin();
    if rustix::termios::isatty(stdin.as_fd()) {
        rustix::io::dup(stdin.as_fd()).ok().map(std::fs::File::from)
    } else {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .ok()
    }
}

/// Sends the query while the TUI runs (after a resize), to the same
/// terminal as at start and only when `TIZA_NO_OSC` is not set; the answer
/// comes back through the key stream (`OscCapture`).
pub fn request_host() {
    if osc_disabled() {
        return;
    }
    if let Some(mut tty) = input_terminal() {
        let _ = tty.write_all(QUERY.as_bytes());
        let _ = tty.flush();
    }
}

/// Collects an OSC answer that crossterm hands us as key presses: `Alt+]`
/// opens it, characters follow, `Alt+\` or Ctrl-G closes it.
#[derive(Debug, Default, Clone)]
pub struct OscCapture {
    buf: Option<String>,
    started: Option<Instant>,
}

impl OscCapture {
    /// Starts the capture with the beginning of an answer that the start-up
    /// read cut short ([`cut_answer`]), so that its rest, which comes as
    /// keys, is taken too. The usual limit of a second applies.
    pub fn resume(&mut self, cut: String) {
        self.buf = Some(cut);
        self.started = Some(Instant::now());
    }

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
                // The start-up read cut the answer between the `ESC` and
                // the `\` of its end: the `\` comes alone.
                if c == '\\' && b.ends_with("\x1b\\") {
                    let host = parse_answers(b);
                    self.buf = None;
                    self.started = None;
                    return (
                        true,
                        (host.bg.is_some() || host.fg.is_some()).then_some(host),
                    );
                }
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
    fn the_start_up_read_gives_back_what_is_not_an_answer() {
        let one = |b: &[u8]| without_answers(b, &[0]);
        assert_eq!(
            one(b"n\x1b]11;rgb:f7f7/f5f5/efef\x1b\\n\x1b]10;rgb:1/2/3\x07q"),
            vec![b"n".to_vec(), b"n".to_vec(), b"q".to_vec()]
        );
        // An answer cut short by the timeout is not keys.
        assert_eq!(one(b"j\x1b]11;rgb:f7f7/f5"), vec![b"j".to_vec()]);
        // An ESC typed early stays a key, and ends its piece: a key read
        // later is not Alt with it.
        assert_eq!(one(b":\x1b"), vec![b":\x1b".to_vec()]);
        assert_eq!(
            without_answers(b":\x1bn", &[0, 2]),
            vec![b":\x1b".to_vec(), b"n".to_vec()]
        );
        assert!(one(b"").is_empty());
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

    #[test]
    fn an_answer_cut_by_the_wait_is_taken_with_its_rest() {
        let full = b"\x1b]11;rgb:1e1e/2020/2626\x1b\\";
        assert_eq!(cut_answer(full), None);
        assert_eq!(cut_answer(b"j\x1b]11;rgb:1/2/3\x07k"), None);
        assert_eq!(cut_answer(b":\x1b"), None);
        let cut = [&full[..], b"x\x1b]10;rgb:e8e8/"].concat();
        assert_eq!(cut_answer(&cut).as_deref(), Some("\x1b]10;rgb:e8e8/"));
        // The rest comes as keys: the capture takes it, then gives the next
        // key back.
        let mut c = OscCapture::default();
        c.resume("\x1b]11;rgb:1e1e/".into());
        for ch in "2020/2626".chars() {
            assert!(c.feed(KeyCode::Char(ch), KeyModifiers::NONE).0);
        }
        let (sw, h) = c.feed(KeyCode::Char('\\'), KeyModifiers::ALT);
        assert!(sw);
        assert_eq!(h.unwrap().dark(), Some(true));
        assert!(!c.feed(KeyCode::Char('e'), KeyModifiers::NONE).0);
        // Cut inside the end, `ESC` then `\`: the `\` comes alone.
        let mut c = OscCapture::default();
        c.resume("\x1b]10;rgb:1f1f/2323/2828\x1b".into());
        let (sw, h) = c.feed(KeyCode::Char('\\'), KeyModifiers::NONE);
        assert!(sw && h.is_some_and(|h| h.fg.is_some()) && !c.active());
        // Cut after the `ESC` of the text answer: the `ESC` is no key, and
        // the plain `]` and the rest join it.
        let light = b"\x1b]11;rgb:f7f7/f5f5/efef\x1b\\";
        let buf = [&light[..], b"j\x1b"].concat();
        let host = parse_answers(&String::from_utf8_lossy(&buf));
        let (typed, cut) = start_up_rest(&buf, &[0], Some(&host));
        assert_eq!((typed, cut.as_deref()), (vec![b"j".to_vec()], Some("\x1b")));
        let mut c = OscCapture::default();
        c.resume(cut.unwrap());
        for ch in "]10;rgb:1f1f/2323/2828".chars() {
            assert!(c.feed(KeyCode::Char(ch), KeyModifiers::NONE).0);
        }
        let (sw, h) = c.feed(KeyCode::Char('\\'), KeyModifiers::ALT);
        assert!(sw && h.is_some_and(|h| h.fg.is_some()));
        // With both answers in, a last ESC is a key, as before.
        let both = [&light[..], b"\x1b]10;rgb:1/2/3\x07\x1b"].concat();
        let host = parse_answers(&String::from_utf8_lossy(&both));
        let (typed, cut) = start_up_rest(&both, &[0], Some(&host));
        assert_eq!((typed, cut), (vec![b"\x1b".to_vec()], None));
    }
}
