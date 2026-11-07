//! The title of the terminal tab while a talk is presented.
//!
//! When presentation mode starts and the talk has a `title`, Tiza pushes
//! the title of the terminal on the xterm title stack (`CSI 22;0 t`) and
//! sets its own (`OSC 0`, the icon and the window title). When
//! presentation mode ends, or Tiza quits by any way, it pops the title
//! back (`CSI 23;0 t`). Without a title, Tiza writes none of these.
//!
//! A terminal without the stack ignores the `CSI … t` sequences, so
//! nothing shows on the screen. It keeps the title of the talk after Tiza
//! quits: no terminal lets a program read the old title back.

use std::io::{self, Write};
use std::sync::Mutex;

/// The longest title, in characters: a tab shows far less.
pub const MAX_CHARS: usize = 120;

/// Push the title of the terminal on the stack (icon and window title).
const PUSH: &[u8] = b"\x1b[22;0t";
/// Pop it back.
const POP: &[u8] = b"\x1b[23;0t";

/// The title that Tiza set, while it is on the tab: shared with the crash
/// hook, which pops it back too.
static SHOWN: Mutex<Option<String>> = Mutex::new(None);

/// The title as the tab gets it: printable characters only, so that a
/// title cannot write an escape sequence or a BEL to the terminal, and
/// [`MAX_CHARS`] at most.
fn clean(title: &str) -> String {
    tiza_core::printable::printable_line(title)
        .chars()
        .take(MAX_CHARS)
        .collect()
}

/// The bytes that take the tab from `shown` (the title that Tiza set, if
/// any) to `want`, and the new `shown`. A first title pushes the old one;
/// another title replaces it; no title pops the old one back.
fn change(shown: Option<&str>, want: Option<&str>) -> (Vec<u8>, Option<String>) {
    let want = want.map(clean).filter(|t| !t.trim().is_empty());
    if want.as_deref() == shown {
        return (Vec::new(), want);
    }
    let mut out = Vec::new();
    match &want {
        Some(title) => {
            if shown.is_none() {
                out.extend_from_slice(PUSH);
            }
            out.extend_from_slice(format!("\x1b]0;{title}\x07").as_bytes());
        }
        None => out.extend_from_slice(POP),
    }
    (out, want)
}

/// Shows `want` on the tab, or gives the old title back when it is
/// `None`. Called on each turn of the event loop; it writes only when the
/// title changes. A write that fails (a closed terminal) is ignored.
pub fn follow(want: Option<&str>) {
    let Ok(mut shown) = SHOWN.lock() else {
        return;
    };
    let (bytes, now) = change(shown.as_deref(), want);
    if !bytes.is_empty() {
        let mut out = io::stdout();
        let _ = out.write_all(&bytes).and_then(|()| out.flush());
    }
    *shown = now;
}

/// Gives the old title back if Tiza set one: on every way out, and in the
/// crash hook.
pub fn restore() {
    follow(None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_pushes_the_old_one_and_none_pops_it() {
        let (bytes, shown) = change(None, Some("Ownership en Rust"));
        assert_eq!(bytes, b"\x1b[22;0t\x1b]0;Ownership en Rust\x07");
        assert_eq!(shown.as_deref(), Some("Ownership en Rust"));
        // The same title again: nothing to write.
        let (bytes, _) = change(shown.as_deref(), Some("Ownership en Rust"));
        assert!(bytes.is_empty());
        // Another title: set, no second push.
        let (bytes, shown) = change(shown.as_deref(), Some("Borrowing"));
        assert_eq!(bytes, b"\x1b]0;Borrowing\x07");
        // No title: pop once, then nothing.
        let (bytes, shown) = change(shown.as_deref(), None);
        assert_eq!(bytes, b"\x1b[23;0t");
        let (bytes, _) = change(shown.as_deref(), None);
        assert!(bytes.is_empty());
    }

    #[test]
    fn a_title_writes_no_control_character_and_is_cut() {
        let (bytes, shown) = change(None, Some("a\x1b]0;evil\x07b\nc\u{9b}d"));
        let title = shown.unwrap();
        assert!(!title.chars().any(char::is_control), "{title:?}");
        // Only the push and the one OSC that sets the title.
        assert_eq!(bytes.iter().filter(|b| **b == 0x1b).count(), 2);
        assert_eq!(bytes.iter().filter(|b| **b == 0x07).count(), 1);
        let long = "x".repeat(300);
        assert_eq!(
            change(None, Some(&long)).1.unwrap().chars().count(),
            MAX_CHARS
        );
        // A blank title is no title: the tab is not touched.
        assert_eq!(change(None, Some("  ")), (Vec::new(), None));
    }
}
