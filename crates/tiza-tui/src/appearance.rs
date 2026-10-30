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

/// The glyphs of Tiza's own text that some terminals draw two cells wide,
/// though Unicode gives them one cell (each has an emoji form), with the
/// plain form that Tiza shows in place of each one on such a terminal.
/// A glyph of that kind that Tiza draws outside the code goes here.
pub const PROBED: [(char, &str); 4] = [
    ('\u{26A0}', "!"),
    ('\u{21AA}', ">"),
    ('\u{23F8}', "||"),
    ('\u{25B6}', ">"),
];

/// The start of the start-up probe: it saves the cursor (`ESC 7`) and
/// asks for its position (`ESC [ 6 n`). The glyphs ([`probe_glyphs`]) go
/// only when that report shows room for them ([`room_for_glyphs`]).
pub const PROBE_START: &str = "\x1b7\x1b[6n";

/// The rest of the probe: for each glyph of [`PROBED`], it puts the cursor
/// back (`ESC 8`), writes the glyph and asks again. The column of each
/// report, less the first one, is how many cells the terminal used. Last,
/// it puts the cursor back and clears from there to the end of the line:
/// the text before the cursor stays.
pub fn probe_glyphs() -> String {
    let mut s: String = PROBED
        .iter()
        .map(|(g, _)| format!("\x1b8{g}\x1b[6n"))
        .collect();
    s.push_str("\x1b8\x1b[K");
    s
}

/// Whether the glyphs of the probe go, from the first report (the cursor)
/// in a row `cols` wide: a glyph two cells wide must fit with the cursor
/// after it still in the row. Nearer the right edge, the glyph goes to the
/// next row (and at the last row the screen moves up), and a cursor in
/// the last column reports that column again: the width is wrong. At the
/// last column, `ESC [ K` also clears the last cell of the line. So there
/// the probe writes nothing more, and measures nothing.
pub fn room_for_glyphs((_, col): (u16, u16), cols: u16) -> bool {
    col.saturating_add(2) <= cols
}

/// How many cursor reports the probe asks for when the glyphs go: the
/// cursor before the glyphs, then one after each glyph.
pub const REPORTS: usize = PROBED.len() + 1;

/// Which glyphs of [`PROBED`] the terminal draws two cells wide, as the
/// start-up probe measured them. Empty when the terminal did not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Wide([bool; PROBED.len()]);

impl Wide {
    /// From the cursor reports of the probe ([`probe`]): the first is the
    /// cursor before the glyphs, then one after each glyph, in the order of
    /// [`PROBED`]. A glyph took the cells from the first column to its own.
    /// One that went to another row (the right edge) counts as narrow.
    pub fn from_reports(reports: &[(u16, u16)]) -> Wide {
        let mut wide = [false; PROBED.len()];
        if let Some(((row0, col0), after)) = reports.split_first() {
            for (w, (row, col)) in wide.iter_mut().zip(after) {
                *w = row == row0 && col.saturating_sub(*col0) >= 2;
            }
        }
        Wide(wide)
    }

    pub fn is_empty(&self) -> bool {
        !self.0.iter().any(|w| *w)
    }

    /// `s` with each glyph that this terminal draws wide in its plain form,
    /// and the emoji selector (VS16) after it left out. A plain form is
    /// ASCII: the bar counts its cells as every terminal draws them.
    pub fn plain<'a>(&self, s: &'a str) -> std::borrow::Cow<'a, str> {
        let wide = |c: char| {
            PROBED
                .iter()
                .zip(self.0)
                .find(|((g, _), w)| *w && *g == c)
                .map(|((_, p), _)| *p)
        };
        if !s.chars().any(|c| wide(c).is_some()) {
            return std::borrow::Cow::Borrowed(s);
        }
        let mut out = String::with_capacity(s.len());
        let mut after_plain = false;
        for c in s.chars() {
            if after_plain && c == '\u{FE0F}' {
                after_plain = false;
                continue;
            }
            match wide(c) {
                Some(p) => {
                    out.push_str(p);
                    after_plain = true;
                }
                None => {
                    out.push(c);
                    after_plain = false;
                }
            }
        }
        std::borrow::Cow::Owned(out)
    }
}

/// What a cursor report (`ESC [ row ; col R`) at `buf[i..]` is.
#[derive(Debug, PartialEq, Eq)]
enum Report {
    /// A whole report, this long, with its row and column.
    Whole(usize, (u16, u16)),
    /// The start of a report that the end of `buf` cut short.
    Cut,
    /// Not a report: a key, as `ESC [ A`.
    No,
}

fn report_at(buf: &[u8], i: usize) -> Report {
    if buf.get(i) != Some(&0x1b) || buf.get(i + 1) != Some(&b'[') {
        return Report::No;
    }
    let mut nums = [String::new(), String::new()];
    let mut k = 0;
    for (j, &b) in buf[i + 2..].iter().enumerate() {
        match b {
            b'0'..=b'9' => nums[k].push(b as char),
            b';' if k == 0 && !nums[0].is_empty() => k = 1,
            b'R' if k == 1 && !nums[1].is_empty() => {
                let (Ok(row), Ok(col)) = (nums[0].parse(), nums[1].parse()) else {
                    return Report::No;
                };
                return Report::Whole(j + 3, (row, col));
            }
            _ => return Report::No,
        }
    }
    Report::Cut
}

/// The cursor reports in raw terminal input, in order.
pub fn parse_reports(buf: &[u8]) -> Vec<(u16, u16)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < buf.len() {
        match report_at(buf, i) {
            Report::Whole(n, pos) => {
                out.push(pos);
                i += n;
            }
            _ => i += 1,
        }
    }
    out
}

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

/// What the start-up read found ([`query_host`]).
#[derive(Debug, Default)]
pub struct StartUp {
    /// The colours, when they were asked for and the terminal said them.
    pub host: Option<Host>,
    /// The glyphs of [`PROBED`] that the terminal draws wide.
    pub wide: Wide,
    /// Keys typed meanwhile, in pieces ([`without_answers`]).
    pub typed: Vec<Vec<u8>>,
    /// The answer that the end of the wait cut short ([`cut_answer`]).
    pub cut: Option<String>,
}

/// Asks the terminal at start-up, before crossterm owns the input: raw mode
/// on `/dev/tty`, the colour query (when `colors`) and the probe of the
/// wide glyphs ([`PROBE_START`], then [`probe_glyphs`] after the first
/// report when there is room), a timed read (VTIME), the old mode back. It
/// stops when every answer came, or when the terminal does not answer in
/// time ([`COLOR_QUERY_WAIT`]). Nothing is asked when there is no tty, or
/// when the `TIZA_NO_OSC` variable is set (tests, pipelines). Also returns
/// what else the read took from the terminal: keys typed meanwhile, in
/// order, for the event loop ([`crate::early_keys`]); they are not the
/// query's to eat. They come in pieces, as crossterm would have read them:
/// one per read, cut where an answer was taken out (so an ESC that ends one
/// is Esc, not Alt with the next key). Last, the answer that the end of the
/// wait cut short ([`cut_answer`]): its rest comes later, with the keys.
pub fn query_host(colors: bool) -> StartUp {
    if osc_disabled() {
        return StartUp::default();
    }
    let Some(mut tty) = input_terminal() else {
        return StartUp::default();
    };
    let Ok(saved) = rustix::termios::tcgetattr(&tty) else {
        return StartUp::default();
    };
    let mut raw = saved.clone();
    raw.make_raw();
    // A read returns after `deciseconds` with nothing rather than blocking.
    let deciseconds = (COLOR_QUERY_WAIT.as_millis() / 100).clamp(1, 25) as u8;
    raw.special_codes[rustix::termios::SpecialCodeIndex::VMIN] = 0;
    raw.special_codes[rustix::termios::SpecialCodeIndex::VTIME] = deciseconds;
    if rustix::termios::tcsetattr(&tty, rustix::termios::OptionalActions::Now, &raw).is_err() {
        return StartUp::default();
    }
    let mut buf = Vec::new();
    // Where each read began in `buf`.
    let mut reads = Vec::new();
    let mut ask = if colors {
        QUERY.to_string()
    } else {
        String::new()
    };
    ask.push_str(PROBE_START);
    // The width of the row, for the room of the glyphs; unknown: no room.
    let cols = rustix::termios::tcgetwinsize(&tty).map_or(0, |w| w.ws_col);
    // The reports that the read waits for: the cursor, then one for each
    // glyph when they go.
    let mut expected = 1;
    let _ = (|| {
        tty.write_all(ask.as_bytes()).ok()?;
        tty.flush().ok()?;
        let mut chunk = [0u8; 256];
        let start = Instant::now();
        let mut first = true;
        loop {
            let n = tty.read(&mut chunk).ok()?;
            if n == 0 {
                break;
            }
            reads.push(buf.len());
            buf.extend_from_slice(&chunk[..n]);
            if first && let Some(cursor) = parse_reports(&buf).first().copied() {
                first = false;
                if room_for_glyphs(cursor, cols) {
                    tty.write_all(probe_glyphs().as_bytes()).ok()?;
                    tty.flush().ok()?;
                    expected = REPORTS;
                }
            }
            if all_in(&buf, colors, expected)
                || start.elapsed() > COLOR_QUERY_WAIT * COLOR_QUERY_LATE_FACTOR
            {
                break;
            }
        }
        Some(())
    })();
    let _ = rustix::termios::tcsetattr(&tty, rustix::termios::OptionalActions::Now, &saved);
    let host =
        Some(parse_answers(&String::from_utf8_lossy(&buf))).filter(|h| colors && h.bg.is_some());
    let reports = parse_reports(&buf);
    let due = Reports {
        due: reports.len() < expected,
        answered: host.is_some() || !reports.is_empty(),
    };
    let (typed, cut) = start_up_rest(&buf, &reads, host.as_ref(), due);
    StartUp {
        host,
        wide: Wide::from_reports(&reports),
        typed,
        cut,
    }
}

/// What the start-up read knows of the cursor reports of the probe.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reports {
    /// A report did not come whole: the end of the read may be one that
    /// the wait cut short.
    pub due: bool,
    /// The terminal answered something in this read (a colour or a
    /// report). Only then is an ESC, or an `ESC [` with no digit, at the
    /// end taken for the start of a report: from a terminal that answers
    /// nothing it is the user's key.
    pub answered: bool,
}

impl Reports {
    /// Whether `buf[i..]`, the end of the read, is a report cut short.
    fn cut_at(&self, buf: &[u8], i: usize) -> bool {
        self.due
            && report_at(buf, i) == Report::Cut
            && (self.answered || buf[i..].iter().any(u8::is_ascii_digit))
    }
}

/// Whether every answer that the start-up read waits for is in `buf`: the
/// colours (when `colors`) and the cursor reports of the probe, `reports`
/// of them (one, or [`REPORTS`] when the glyphs went). Then the read stops
/// at once.
pub fn all_in(buf: &[u8], colors: bool, reports: usize) -> bool {
    let host = parse_answers(&String::from_utf8_lossy(buf));
    (!colors || (host.bg.is_some() && host.fg.is_some())) && parse_reports(buf).len() >= reports
}

/// What the start-up read gives the event loop besides `host`: the keys
/// ([`without_answers`]) and the answer cut short ([`cut_answer`]). When
/// an answer is still due (the background answer came but the text answer
/// did not, or a report of the probe did not come from a terminal that
/// answered: `reports`), an ESC at the very end is the start of that
/// answer, not a key: its `]` or `[` and the rest come later, and the
/// capture takes them.
pub fn start_up_rest(
    buf: &[u8],
    reads: &[usize],
    host: Option<&Host>,
    reports: Reports,
) -> (Vec<Vec<u8>>, Option<String>) {
    let answer_due =
        (reports.due && reports.answered) || host.is_some_and(|h| h.bg.is_some() && h.fg.is_none());
    if answer_due && buf.last() == Some(&0x1b) && cut_answer(buf, reports).is_none() {
        let keys = &buf[..buf.len() - 1];
        return (
            without_answers_due(keys, reads, reports),
            Some("\x1b".to_string()),
        );
    }
    (
        without_answers_due(buf, reads, reports),
        cut_answer(buf, reports),
    )
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
/// with no `ESC \` or BEL after it, or, while a cursor report of the probe
/// is due ([`Reports`]), an `ESC [` with only digits and `;` after it.
/// The terminal sends the rest later, through the key stream, where
/// [`OscCapture::resume`] takes it.
pub fn cut_answer(buf: &[u8], reports: Reports) -> Option<String> {
    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] == 0x1b && buf[i + 1] == b']' {
            match answer_len(&buf[i + 2..]) {
                Some(n) => i += 2 + n,
                None => return Some(String::from_utf8_lossy(&buf[i..]).into_owned()),
            }
        } else if let Report::Whole(n, _) = report_at(buf, i) {
            i += n;
        } else if reports.cut_at(buf, i) {
            return Some(String::from_utf8_lossy(&buf[i..]).into_owned());
        } else {
            i += 1;
        }
    }
    None
}

/// What the start-up read took from the terminal besides the answers, in
/// pieces: every `ESC ]` … `ESC \` (or BEL) sequence and every whole cursor
/// report (`ESC [ row ; col R`) left out, and an answer cut short by the
/// timeout too (the rest of an answer, never keys); a piece ends where a
/// read (`reads`: where each began in `buf`) or an answer did. A cut
/// cursor report is left out only when `start_up_rest` says one is due:
/// [`cut_answer`] gives it.
pub fn without_answers(buf: &[u8], reads: &[usize]) -> Vec<Vec<u8>> {
    without_answers_due(buf, reads, Reports::default())
}

fn without_answers_due(buf: &[u8], reads: &[usize], reports: Reports) -> Vec<Vec<u8>> {
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
        match report_at(buf, i) {
            Report::Whole(n, _) => {
                i += n;
                cut(&mut out);
                continue;
            }
            Report::Cut if reports.cut_at(buf, i) => break,
            _ => {}
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
    /// keys, is taken too. The usual limit of a second applies; it starts
    /// again at [`OscCapture::restart_clock`].
    pub fn resume(&mut self, cut: String) {
        self.buf = Some(cut);
        self.started = Some(Instant::now());
    }

    /// The second of an open capture starts again now. The TUI calls it
    /// after its first frame: a slow first frame (a big file, a busy
    /// machine) does not use the second before the first key is read.
    pub fn restart_clock(&mut self) {
        if self.buf.is_some() {
            self.started = Some(Instant::now());
        }
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
            // The rest of a cursor report that the start-up read cut short
            // (`ESC [ row ; col R`): digits and `;`, then `R`. Any other
            // key is the user's.
            (Some(b), KeyCode::Char(c))
                if !ctrl && !alt && (b.starts_with("\x1b[") || (b == "\x1b" && c == '[')) =>
            {
                let whole = b
                    .strip_prefix("\x1b[")
                    .and_then(|r| r.split_once(';'))
                    .is_some_and(|(row, col)| !row.is_empty() && !col.is_empty());
                if c == 'R' && whole {
                    self.buf = None;
                    self.started = None;
                    return (true, None);
                }
                if c == '[' && b == "\x1b" || c.is_ascii_digit() || c == ';' {
                    b.push(c);
                    return (true, None);
                }
                self.buf = None;
                self.started = None;
                (false, None)
            }
            // After an ESC that the start-up read took as the start of an
            // answer, only `]` goes on with one (`[` is above): any other
            // key is the user's.
            (Some(b), KeyCode::Char(c)) if !ctrl && !alt && b == "\x1b" && c != ']' => {
                self.buf = None;
                self.started = None;
                (false, None)
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
        assert_eq!(cut_answer(full, Reports::default()), None);
        assert_eq!(
            cut_answer(b"j\x1b]11;rgb:1/2/3\x07k", Reports::default()),
            None
        );
        assert_eq!(cut_answer(b":\x1b", Reports::default()), None);
        let cut = [&full[..], b"x\x1b]10;rgb:e8e8/"].concat();
        assert_eq!(
            cut_answer(&cut, Reports::default()).as_deref(),
            Some("\x1b]10;rgb:e8e8/")
        );
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
        let (typed, cut) = start_up_rest(&buf, &[0], Some(&host), Reports::default());
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
        let (typed, cut) = start_up_rest(&both, &[0], Some(&host), Reports::default());
        assert_eq!((typed, cut), (vec![b"\x1b".to_vec()], None));
    }

    #[test]
    fn the_probe_measures_each_glyph_by_the_column_of_its_report() {
        // The cursor first; then, after its report, one request per glyph,
        // in the order of `PROBED`, each from the saved cursor; the cursor
        // back at the end, and what the probe wrote cleared.
        assert_eq!(PROBE_START, "\x1b7\x1b[6n");
        let p = probe_glyphs();
        assert_eq!(p.matches("\x1b[6n").count(), REPORTS - 1);
        assert!(p.starts_with("\x1b8\u{26A0}\x1b[6n\x1b8"), "{p:?}");
        assert!(p.ends_with("\x1b8\x1b[K"), "{p:?}");
        // The reports in order, among the colour answers and keys.
        let buf = b"\x1b]11;rgb:1/2/3\x07j\x1b[12;5R\x1b[12;7R\x1b[12;6Rk\x1b[12;7R\x1b[12;6R";
        assert_eq!(
            parse_reports(buf),
            vec![(12, 5), (12, 7), (12, 6), (12, 7), (12, 6)]
        );
        // From column 5: to column 7 is two cells, to column 6 one.
        let wide = Wide::from_reports(&parse_reports(buf));
        assert_eq!(wide, Wide([true, false, true, false]));
        assert!(!wide.is_empty());
        // A glyph that went to the next row (the right edge): narrow.
        let edge = Wide::from_reports(&[(3, 80), (4, 3), (3, 81), (3, 81), (3, 81)]);
        assert!(edge.is_empty());
        // No report: nothing is wide.
        assert!(Wide::from_reports(&[]).is_empty());
        // Keys that look like a report are not one.
        assert!(parse_reports(b"\x1b[A\x1b[1;R\x1b[;2R").is_empty());
    }

    #[test]
    fn only_the_glyphs_measured_wide_take_their_plain_form() {
        let wide = Wide([true, false, true, false]);
        assert_eq!(wide.plain("\u{26A0} borrow \u{26A0}"), "! borrow !");
        // The emoji selector after a glyph that takes its plain form goes:
        // the plain form is ASCII.
        assert_eq!(wide.plain("\u{26A0}\u{FE0F} x"), "! x");
        // `↪` is drawn narrow here: it stays, as does a VS16 elsewhere. The
        // plain form of `⏸` is two cells, `||`.
        assert_eq!(wide.plain("\u{21AA} a.rs \u{23F8}"), "\u{21AA} a.rs ||");
        assert_eq!(wide.plain("a\u{FE0F}"), "a\u{FE0F}");
        // Nothing measured wide: the text as it is, not copied.
        let none = Wide::default();
        assert!(matches!(
            none.plain("\u{26A0} \u{21AA} \u{23F8}"),
            std::borrow::Cow::Borrowed("\u{26A0} \u{21AA} \u{23F8}")
        ));
    }

    #[test]
    fn the_glyphs_go_only_with_room_for_two_cells_before_the_edge() {
        // In 80 columns: from column 78 a wide glyph ends at 79, and the
        // cursor after it is at 80, in the row.
        assert!(room_for_glyphs((5, 1), 80));
        assert!(room_for_glyphs((5, 78), 80));
        // From column 79 the cursor after a wide glyph waits at 80 for the
        // wrap, and reports 80 again: one cell, wrong. At 80 (also with a
        // wrap that waits) the glyph goes to the next row.
        assert!(!room_for_glyphs((5, 79), 80));
        assert!(!room_for_glyphs((5, 80), 80));
        // The width not known: no glyph.
        assert!(!room_for_glyphs((1, 1), 0));
    }

    #[test]
    fn the_read_stops_when_every_answer_is_in() {
        let reports = b"\x1b[1;1R\x1b[1;2R\x1b[1;2R\x1b[1;2R\x1b[1;3R";
        let colors = b"\x1b]11;rgb:1/2/3\x07\x1b]10;rgb:4/5/6\x07";
        // The probe alone: its reports, the cursor and one for each glyph.
        assert!(all_in(reports, false, REPORTS));
        assert!(!all_in(&reports[..24], false, REPORTS));
        // No glyph went (no room): the report of the cursor is all.
        assert!(all_in(&reports[..6], false, 1));
        assert!(!all_in(&reports[..5], false, 1));
        // With the colours asked: both answers too.
        assert!(!all_in(reports, true, REPORTS));
        assert!(!all_in(colors, true, 1));
        assert!(all_in(&[&colors[..], &reports[..]].concat(), true, REPORTS));
        assert!(all_in(&[&colors[..], &reports[..6]].concat(), true, 1));
        // The bar's `▶` is measured too: the fourth report.
        assert_eq!(
            Wide::from_reports(&parse_reports(reports)).plain(" \u{256D}\u{25B6} "),
            " \u{256D}> "
        );
    }

    #[test]
    fn a_report_is_never_a_key() {
        let one = |b: &[u8]| without_answers(b, &[0]);
        // Whole reports go, with the colour answers; a piece ends there.
        assert_eq!(
            one(b"j\x1b[12;3Rk\x1b]10;rgb:1/2/3\x07\x1b[1;2Rq"),
            vec![b"j".to_vec(), b"k".to_vec(), b"q".to_vec()]
        );
        // An arrow key stays a key.
        assert_eq!(one(b"\x1b[A"), vec![b"\x1b[A".to_vec()]);
        // A report cut at each place, while one is due from a terminal
        // that answered: the cut part is the answer, the keys before it
        // stay keys.
        let answering = Reports {
            due: true,
            answered: true,
        };
        for cut in ["\x1b[", "\x1b[1", "\x1b[12", "\x1b[12;", "\x1b[12;3"] {
            let buf = [b"j\x1b[12;3R".as_slice(), cut.as_bytes()].concat();
            let (typed, got) = start_up_rest(&buf, &[0], None, answering);
            assert_eq!(
                (typed, got.as_deref()),
                (vec![b"j".to_vec()], Some(cut)),
                "{cut:?}"
            );
        }
        // The same bytes with no report due are keys, as before.
        let (typed, got) = start_up_rest(b"\x1b[12", &[0], None, Reports::default());
        assert_eq!((typed, got), (vec![b"\x1b[12".to_vec()], None));
        // A lone ESC at the end, while a report is due, is its start.
        let (typed, got) = start_up_rest(b"j\x1b", &[0], None, answering);
        assert_eq!((typed, got.as_deref()), (vec![b"j".to_vec()], Some("\x1b")));
        // From a terminal that answered nothing yet, an ESC or an `ESC [`
        // alone at the end is the user's key; with a digit it is a report.
        let silent = Reports {
            due: true,
            answered: false,
        };
        for keys in [&b"n:abc\x1b"[..], b"j\x1b["] {
            let (typed, got) = start_up_rest(keys, &[0], None, silent);
            assert_eq!((typed, got), (vec![keys.to_vec()], None), "{keys:?}");
        }
        let (typed, got) = start_up_rest(b"j\x1b[1", &[0], None, silent);
        assert_eq!(
            (typed, got.as_deref()),
            (vec![b"j".to_vec()], Some("\x1b[1"))
        );
    }

    #[test]
    fn the_capture_takes_the_rest_of_a_cut_report_and_gives_other_keys_back() {
        let feed_all = |c: &mut OscCapture, s: &str| {
            s.chars()
                .map(|ch| c.feed(KeyCode::Char(ch), KeyModifiers::NONE).0)
                .collect::<Vec<_>>()
        };
        // Cut after `;`: `3` and `R` are not keys, and the capture ends.
        let mut c = OscCapture::default();
        c.resume("\x1b[12;".into());
        assert_eq!(feed_all(&mut c, "3"), vec![true]);
        let (sw, h) = c.feed(KeyCode::Char('R'), KeyModifiers::SHIFT);
        assert!(sw && h.is_none() && !c.active());
        // The next key is the user's.
        assert!(!c.feed(KeyCode::Char('j'), KeyModifiers::NONE).0);
        // Cut after the ESC: `[`, the digits and `R` come as keys.
        let mut c = OscCapture::default();
        c.resume("\x1b".into());
        assert!(feed_all(&mut c, "[7;2R").iter().all(|sw| *sw));
        assert!(!c.active());
        // After a lone ESC, a key that starts no answer is the user's.
        let mut c = OscCapture::default();
        c.resume("\x1b".into());
        assert!(!c.feed(KeyCode::Char('n'), KeyModifiers::NONE).0);
        assert!(!c.active());
        // A key that is not part of a report ends the capture, and is given
        // back.
        let mut c = OscCapture::default();
        c.resume("\x1b[12".into());
        assert!(!c.feed(KeyCode::Char('j'), KeyModifiers::NONE).0);
        assert!(!c.active());
        // `R` before a whole row and column is no report: the user's.
        for cut in ["\x1b[", "\x1b[12", "\x1b[12;"] {
            let mut c = OscCapture::default();
            c.resume(cut.into());
            assert!(
                !c.feed(KeyCode::Char('R'), KeyModifiers::SHIFT).0,
                "{cut:?}"
            );
        }
    }
}
