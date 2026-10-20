//! Control characters made visible (t-182).
//!
//! Code and annotation text can carry bytes a terminal or an XML parser
//! acts on: ESC starts a sequence that moves the cursor or retitles the
//! window, NUL and most of C0 are not valid XML, and a bidi override
//! reorders what follows it. None of them is ever drawn or printed raw:
//!
//! | Character | Shown as |
//! |---|---|
//! | C0 U+0000–U+001F (not the tab) | its Control Picture, `␀` … `␟` (`␛` for ESC) |
//! | `\r\n` as one grapheme | `␤` |
//! | DEL U+007F | `␡` |
//! | C1 U+0080–U+009F | `�` |
//! | bidi controls (U+061C, U+200E/F, U+202A–U+202E, U+2066–U+2069) | nothing in a drawing (they take no cell); `�` in a message |
//!
//! Every placeholder is one cell wide, as wide as the character it stands
//! for, so the layout's columns do not move. The tab keeps its handling:
//! the code expands it to spaces, a message keeps it.
//!
//! [`cell_symbol`] is what [`crate::canvas::Canvas::put`] stores, so every
//! drawing (TUI, txt, md, ansi, svg, png, gif, video, cast, `render.text`)
//! goes through it; [`printable_text`] and [`printable_line`] are for the
//! text Tiza prints outside a drawing (errors, `tiza check`, titles), and
//! [`escape_json_controls`] for JSON it prints.

use std::borrow::Cow;

/// The one-cell picture of a C0, DEL or C1 control character; `None` for
/// any other character, the tab included.
pub fn control_picture(c: char) -> Option<char> {
    match c {
        '\t' => None,
        '\0'..='\x1f' => char::from_u32(0x2400 + c as u32),
        '\x7f' => Some('\u{2421}'),
        '\u{80}'..='\u{9f}' => Some('\u{FFFD}'),
        _ => None,
    }
}

/// A character with the Unicode `Bidi_Control` property: it changes the
/// order in which the text around it is shown.
pub fn is_bidi_control(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

fn unsafe_char(c: char) -> bool {
    control_picture(c).is_some() || is_bidi_control(c)
}

fn plain(s: &str) -> bool {
    s.bytes().all(|b| (0x20..0x7f).contains(&b) || b == b'\t') || !s.chars().any(unsafe_char)
}

/// What a cell stores for the grapheme `g`: `g` itself, or `g` with its
/// control characters replaced by their pictures and its bidi controls
/// dropped (an empty result means the grapheme takes no cell, as its
/// width already says).
pub fn cell_symbol(g: &str) -> Cow<'_, str> {
    if plain(g) {
        return Cow::Borrowed(g);
    }
    // The only grapheme with two control characters, one cell wide.
    if g == "\r\n" {
        return Cow::Borrowed("\u{2424}");
    }
    Cow::Owned(
        g.chars()
            .filter(|c| !is_bidi_control(*c))
            .map(|c| control_picture(c).unwrap_or(c))
            .collect(),
    )
}

fn printable(s: &str, keep_newlines: bool) -> Cow<'_, str> {
    if plain(s) && (keep_newlines || !s.contains('\n')) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(
        s.chars()
            .map(|c| match c {
                '\n' if keep_newlines => c,
                c if is_bidi_control(c) => '\u{FFFD}',
                c => control_picture(c).unwrap_or(c),
            })
            .collect(),
    )
}

/// `s` safe to print on a terminal, lines kept: every control character
/// but the tab and the newline becomes its picture, and a bidi control
/// becomes `�` (shown, so a text that does not match says why).
pub fn printable_text(s: &str) -> Cow<'_, str> {
    printable(s, true)
}

/// [`printable_text`] for a one-line value (a title, a tape comment): the
/// newline becomes `␊` too, so it cannot start a line of its own.
pub fn printable_line(s: &str) -> Cow<'_, str> {
    printable(s, false)
}

/// Serialized JSON with DEL, the C1 controls and the bidi controls written
/// as `\uXXXX` escapes (serde_json escapes only C0): the same value, safe
/// to print on a terminal. Those characters only ever occur inside JSON
/// strings, where the escape is the same character.
pub fn escape_json_controls(json: &str) -> Cow<'_, str> {
    // Not C0: serde_json already escapes it inside strings, and outside
    // them a pretty printer's newlines are the layout.
    let escaped = |c: char| matches!(c, '\x7f'..='\u{9f}') || is_bidi_control(c);
    if !json.chars().any(escaped) {
        return Cow::Borrowed(json);
    }
    let mut out = String::with_capacity(json.len() + 16);
    for c in json.chars() {
        if escaped(c) {
            out.push_str(&format!("\\u{:04x}", c as u32));
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn controls_become_one_cell_pictures() {
        assert_eq!(cell_symbol("\x1b"), "␛");
        assert_eq!(cell_symbol("\0"), "␀");
        assert_eq!(cell_symbol("\x07"), "␇");
        assert_eq!(cell_symbol("\x7f"), "␡");
        assert_eq!(cell_symbol("\u{9b}"), "\u{FFFD}");
        assert_eq!(cell_symbol("\r\n"), "␤");
        assert_eq!(cell_symbol("\u{202E}"), "");
        assert_eq!(cell_symbol("\t"), "\t");
        assert_eq!(cell_symbol("a"), "a");
        assert_eq!(cell_symbol("中"), "中");
        // Each picture is exactly as wide as what it stands for.
        for g in ["\x1b", "\0", "\x07", "\x7f", "\u{9b}", "\r\n", "\r"] {
            assert_eq!(
                UnicodeWidthStr::width(&*cell_symbol(g)),
                UnicodeWidthStr::width(g),
                "{g:?}"
            );
        }
    }

    #[test]
    fn messages_keep_their_lines_and_titles_do_not() {
        assert_eq!(printable_text("a\x1b]0;x\x07\nb\tc"), "a␛]0;x␇\nb\tc");
        assert_eq!(printable_line("a\nType \"q\""), "a␊Type \"q\"");
        assert_eq!(printable_text("\u{202E}abc"), "\u{FFFD}abc");
        assert!(matches!(printable_text("plain ñ"), Cow::Borrowed(_)));
    }

    #[test]
    fn json_escapes_are_the_same_value() {
        let v = serde_json::json!({"t": "a\u{9b}b\u{202E}c\x7f"});
        let s = serde_json::to_string_pretty(&v).unwrap();
        let safe = escape_json_controls(&s);
        assert!(!safe.chars().any(|c| unsafe_char(c) && c != '\n'), "{safe}");
        assert_eq!(serde_json::from_str::<serde_json::Value>(&safe).unwrap(), v);
    }
}
