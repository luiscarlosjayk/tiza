//! Keys typed while the start-up colour query read the terminal
//! ([`crate::appearance::query_host`]): that read takes them along with the
//! answer, before crossterm owns the input, so they are turned into the key
//! presses crossterm would have read from the same bytes and handled first.
//! The rules are crossterm's (0.29, `event/sys/unix/parse.rs`): `ESC x` is
//! Alt+x, `\r` Enter, `\n` Ctrl-J (the terminal is in raw mode), control
//! bytes Ctrl+letter, an uppercase letter carries Shift; the arrows, Home,
//! End, BackTab, the numbered keys (`ESC [ 3 ~`…) and F1-F12, with their
//! modifiers. A sequence it does not know is skipped, whole.

use crossterm::event::{KeyCode, KeyModifiers};

/// The key presses in `bytes`, in order.
pub fn keys(bytes: &[u8]) -> Vec<(KeyCode, KeyModifiers)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let (key, used) = one(&bytes[i..]);
        if let Some(k) = key {
            out.push(k);
        }
        i += used.max(1);
    }
    out
}

/// The first key of `b` and how many bytes it took.
fn one(b: &[u8]) -> (Option<(KeyCode, KeyModifiers)>, usize) {
    let none = KeyModifiers::NONE;
    match b[0] {
        0x1b => match b.get(1) {
            None => (Some((KeyCode::Esc, none)), 1),
            Some(b'[') => csi(b),
            Some(b'O') => match b.get(2) {
                None => (Some((KeyCode::Char('O'), KeyModifiers::ALT)), 2),
                Some(&c) => (ss3(c).map(|k| (k, none)), 3),
            },
            Some(0x1b) => (Some((KeyCode::Esc, none)), 1),
            Some(_) => {
                let (key, used) = one(&b[1..]);
                (key.map(|(c, m)| (c, m | KeyModifiers::ALT)), used + 1)
            }
        },
        b'\r' => (Some((KeyCode::Enter, none)), 1),
        b'\t' => (Some((KeyCode::Tab, none)), 1),
        0x7f => (Some((KeyCode::Backspace, none)), 1),
        c @ 0x01..=0x1a => (
            Some((KeyCode::Char((c - 1 + b'a') as char), KeyModifiers::CONTROL)),
            1,
        ),
        c @ 0x1c..=0x1f => (
            Some((
                KeyCode::Char((c - 0x1c + b'4') as char),
                KeyModifiers::CONTROL,
            )),
            1,
        ),
        0 => (Some((KeyCode::Char(' '), KeyModifiers::CONTROL)), 1),
        _ => {
            // One UTF-8 character (an invalid byte is skipped).
            let len = match b[0] {
                0xc0..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf7 => 4,
                _ => 1,
            };
            let Some(c) = b
                .get(..len)
                .and_then(|s| std::str::from_utf8(s).ok())
                .and_then(|s| s.chars().next())
            else {
                return (None, 1);
            };
            let mods = if c.is_uppercase() {
                KeyModifiers::SHIFT
            } else {
                none
            };
            (Some((KeyCode::Char(c), mods)), len)
        }
    }
}

/// `ESC O x`: F1-F4, the arrows, Home and End.
fn ss3(c: u8) -> Option<KeyCode> {
    Some(match c {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'P'..=b'S' => KeyCode::F(1 + c - b'P'),
        _ => return None,
    })
}

/// `ESC [ params final`: the key and its modifiers (`;N`, N-1 a bit set of
/// Shift 1, Alt 2, Ctrl 4), or nothing for one it does not know.
fn csi(b: &[u8]) -> (Option<(KeyCode, KeyModifiers)>, usize) {
    let Some(end) = b[2..].iter().position(|c| (0x40..=0x7e).contains(c)) else {
        // Cut short: the rest is not keys.
        return (None, b.len());
    };
    let used = end + 3;
    let params = std::str::from_utf8(&b[2..2 + end]).unwrap_or("");
    let nums: Vec<u8> = params.split(';').map(|p| p.parse().unwrap_or(0)).collect();
    let mods = match nums.get(1) {
        Some(&m) if m > 1 => {
            let bits = m - 1;
            let mut k = KeyModifiers::NONE;
            if bits & 1 != 0 {
                k |= KeyModifiers::SHIFT;
            }
            if bits & 2 != 0 {
                k |= KeyModifiers::ALT;
            }
            if bits & 4 != 0 {
                k |= KeyModifiers::CONTROL;
            }
            k
        }
        _ => KeyModifiers::NONE,
    };
    let code = match b[2 + end] {
        b'A' => KeyCode::Up,
        b'B' => KeyCode::Down,
        b'C' => KeyCode::Right,
        b'D' => KeyCode::Left,
        b'H' => KeyCode::Home,
        b'F' => KeyCode::End,
        b'P' => KeyCode::F(1),
        b'Q' => KeyCode::F(2),
        b'S' => KeyCode::F(4),
        b'Z' => return (Some((KeyCode::BackTab, KeyModifiers::SHIFT)), used),
        b'~' => match nums.first() {
            Some(1 | 7) => KeyCode::Home,
            Some(2) => KeyCode::Insert,
            Some(3) => KeyCode::Delete,
            Some(4 | 8) => KeyCode::End,
            Some(5) => KeyCode::PageUp,
            Some(6) => KeyCode::PageDown,
            Some(&n @ 11..=15) => KeyCode::F(n - 10),
            Some(&n @ 17..=21) => KeyCode::F(n - 11),
            Some(&n @ 23..=24) => KeyCode::F(n - 12),
            _ => return (None, used),
        },
        _ => return (None, used),
    };
    (Some((code, mods)), used)
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyCode::*;

    const NO: KeyModifiers = KeyModifiers::NONE;

    #[test]
    fn plain_keys_controls_and_text() {
        assert_eq!(
            keys("nN:\r\t\x7f\x03é".as_bytes()),
            vec![
                (Char('n'), NO),
                (Char('N'), KeyModifiers::SHIFT),
                (Char(':'), NO),
                (Enter, NO),
                (Tab, NO),
                (Backspace, NO),
                (Char('c'), KeyModifiers::CONTROL),
                (Char('é'), NO),
            ]
        );
        // Raw mode: `\n` is Ctrl-J, as crossterm reads it.
        assert_eq!(keys(b"\n"), vec![(Char('j'), KeyModifiers::CONTROL)]);
    }

    #[test]
    fn escape_alt_and_sequences() {
        assert_eq!(keys(b"\x1b"), vec![(Esc, NO)]);
        assert_eq!(keys(b"\x1bn"), vec![(Char('n'), KeyModifiers::ALT)]);
        assert_eq!(keys(b"\x1b\x1b"), vec![(Esc, NO), (Esc, NO)]);
        assert_eq!(
            keys(b"\x1b[A\x1bOB\x1b[1;5C\x1b[3~\x1b[6~\x1b[Z\x1b[15~"),
            vec![
                (Up, NO),
                (Down, NO),
                (Right, KeyModifiers::CONTROL),
                (Delete, NO),
                (PageDown, NO),
                (BackTab, KeyModifiers::SHIFT),
                (F(5), NO),
            ]
        );
        // Unknown or cut short: skipped, never read as letters.
        assert_eq!(keys(b"\x1b[?1;2cn\x1b[12"), vec![(Char('n'), NO)]);
    }
}
