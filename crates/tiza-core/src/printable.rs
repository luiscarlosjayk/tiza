//! Control characters made visible.
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
//! | bidi controls (U+061C, U+200E/F, U+202A–U+202E, U+2066–U+2069) | in the code, [`INVISIBLE_MARK`]; elsewhere in a drawing nothing (no cell); `�` in a message |
//! | other invisible characters (`Default_Ignorable_Code_Point`: U+200B, U+2060, U+FEFF, a stray U+200C/U+200D, U+00AD, variation selectors after a letter, tags…) | in the code, [`INVISIBLE_MARK`] (unless it does a job: [`code_glyph`]); elsewhere in a drawing nothing (no cell) |
//!
//! Every placeholder is one cell wide, as wide as the character it stands
//! for, so the layout's columns do not move; in the code an invisible
//! character takes the cell of its mark ([`code_glyph`]). The tab keeps
//! its handling: the code expands it to spaces, a message keeps it.
//!
//! [`cell_symbol`] is what [`crate::canvas::Canvas::put`] stores, so every
//! drawing (TUI, txt, md, ansi, svg, png, gif, video, cast, `render.text`)
//! goes through it; [`printable_text`] and [`printable_line`] are for the
//! text Tiza prints outside a drawing (errors, `tiza check`, titles), and
//! [`escape_json_controls`] for JSON it prints. [`unseen`] and
//! [`char_name`] name the characters of the code that are not shown as
//! they are, for the note of `code.get`.

use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;

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

/// The cell that stands for an invisible character in the code (a bidi
/// control, a zero-width space…), drawn in the theme's error colour:
/// `ad⬚min` is not `admin`, and whoever explains the code sees it.
pub const INVISIBLE_MARK: &str = "\u{2B1A}";

/// A `Default_Ignorable_Code_Point` of Unicode: a character with nothing
/// to show (zero-width spaces and joiners, bidi controls, variation
/// selectors, invisible operators, fillers, tags…), so it can hide a
/// difference in the code. (The byte order mark that starts a file is
/// dropped on reading.)
fn is_default_ignorable(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'..='\u{1160}'
            | '\u{17B4}'..='\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{1BCA0}'..='\u{1BCA3}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

/// A letter or sign of a script that uses ZWJ (U+200D) and ZWNJ (U+200C)
/// to shape its words: Arabic and Persian, Syriac, Thaana, N'Ko, Mandaic,
/// the Indic and Brahmic scripts (Devanagari to Sinhala, Tibetan, Myanmar,
/// Khmer, Mongolian, Javanese, Balinese, Sundanese, Kaithi…).
fn uses_joiners(c: char) -> bool {
    !is_default_ignorable(c)
        && matches!(
            c,
            '\u{0600}'..='\u{08FF}'
                | '\u{0900}'..='\u{0DFF}'
                | '\u{0F00}'..='\u{109F}'
                | '\u{1700}'..='\u{18AF}'
                | '\u{1900}'..='\u{1AAF}'
                | '\u{1B00}'..='\u{1C4F}'
                | '\u{A800}'..='\u{A8FF}'
                | '\u{A900}'..='\u{AAFF}'
                | '\u{ABC0}'..='\u{ABFF}'
                | '\u{FB50}'..='\u{FDFF}'
                | '\u{FE70}'..='\u{FEFC}'
                | '\u{10800}'..='\u{10FFF}'
                | '\u{11000}'..='\u{11FFF}'
        )
}

/// A Han ideograph, which takes an ideographic variation selector.
fn is_ideograph(c: char) -> bool {
    matches!(
        c,
        '\u{3400}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{20000}'..='\u{3FFFF}'
    )
}

/// Whether the default-ignorable character `chars[i]` does a job here, so
/// it is left as it is. A ZWJ or ZWNJ joins within the grapheme
/// (something visible follows it in the cluster: an emoji ZWJ sequence
/// such as man ZWJ woman ZWJ girl, an Indic conjunct such as KA VIRAMA ZWJ
/// SSA) or follows a letter of a script that uses joiners (the ZWNJ of a
/// Persian word); a variation selector follows a symbol or an ideograph
/// (the VS16 of an emoji, the keycap `1` VS16 U+20E3, a Han variant), not a
/// Latin letter; a Mongolian variation selector or vowel separator
/// follows a Mongolian letter; tags spell a subdivision flag (U+1F3F4 …
/// U+E007F). Every other one is marked: a stray joiner, one between Latin
/// letters (`ad` ZWJ `min`, which segmentation keeps in the cluster of the
/// `d`), a zero-width space, a bidi control, an invisible operator, a
/// filler, tags that smuggle text.
fn does_a_job(chars: &[char], i: usize, last_visible: Option<usize>) -> bool {
    let c = chars[i];
    let prev = i.checked_sub(1).map(|k| chars[k]);
    match c {
        '\u{200C}' | '\u{200D}' => {
            last_visible.is_some_and(|v| i < v) || prev.is_some_and(uses_joiners)
        }
        '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}' => prev.is_some_and(|p| {
            if matches!(p, '0'..='9' | '#' | '*') {
                return chars.get(i + 1) == Some(&'\u{20E3}');
            }
            !p.is_ascii()
                && !is_default_ignorable(p)
                && (!p.is_alphabetic() || is_ideograph(p) || uses_joiners(p))
        }),
        '\u{180B}'..='\u{180F}' => prev.is_some_and(|p| ('\u{1820}'..='\u{18AF}').contains(&p)),
        // The tags of a subdivision flag: the black flag, 2 to 6 tag
        // digits or small letters (`gbsct`), the cancel tag. Any other
        // tag text stays marked, so no words hide in a flag.
        '\u{E0020}'..='\u{E007F}' => {
            let n = chars.len();
            (4..=8).contains(&n)
                && chars[0] == '\u{1F3F4}'
                && chars[n - 1] == '\u{E007F}'
                && chars[1..n - 1]
                    .iter()
                    .all(|c| matches!(c, '\u{E0030}'..='\u{E0039}' | '\u{E0061}'..='\u{E007A}'))
        }
        _ => false,
    }
}

/// The grapheme `g` of the code as it is drawn: what is shown of it and the
/// number of [`INVISIBLE_MARK`]s that follow, one per invisible character
/// in it that does no job there ([`does_a_job`] says which ones are left
/// as they are). Everything else is `g` unchanged, with no mark. Linear in
/// the length of `g`, however many joiners it holds.
pub fn code_glyph(g: &str) -> (Cow<'_, str>, usize) {
    let Some(chars) = marked_chars(g) else {
        return (Cow::Borrowed(g), 0);
    };
    let shown = chars.iter().filter(|(_, m)| !m).map(|(c, _)| c).collect();
    let marks = chars.iter().filter(|(_, m)| *m).count();
    (Cow::Owned(shown), marks)
}

/// The characters of the grapheme `g` of the code, each with whether the
/// code draws it as an [`INVISIBLE_MARK`]: an invisible character that does
/// no job there. `None` when `g` has no invisible character (most graphemes,
/// with no allocation). [`code_glyph`] and [`unseen`] both use it, so the
/// screen and the note of `code.get` agree.
fn marked_chars(g: &str) -> Option<Vec<(char, bool)>> {
    if g.is_ascii() || !g.chars().any(is_default_ignorable) {
        return None;
    }
    let chars: Vec<char> = g.chars().collect();
    let visible = |c: &char| unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0) > 0;
    let last_visible = chars.iter().rposition(visible);
    Some(
        chars
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                (
                    c,
                    is_default_ignorable(c) && !does_a_job(&chars, i, last_visible),
                )
            })
            .collect(),
    )
}

/// The characters of the code line `line` that the screen does not show as
/// they are, in order, each with its grapheme (from 0, as the `col` of an
/// anchor: a joiner inside the grapheme of a letter has the column of the
/// letter). These are the ones [`code_glyph`] draws as [`INVISIBLE_MARK`]
/// and the control characters, which the screen and a text reply show as
/// their picture ([`control_picture`]: not the tab). `code.get` names them,
/// so an agent can put the real character in a `find`.
pub fn unseen(line: &str) -> Vec<(usize, char)> {
    if line
        .bytes()
        .all(|b| (0x20..0x7f).contains(&b) || b == b'\t')
    {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (col, g) in line.graphemes(true).enumerate() {
        let marked = marked_chars(g);
        for (i, c) in g.chars().enumerate() {
            if marked.as_ref().is_some_and(|m| m[i].1) || control_picture(c).is_some() {
                out.push((col, c));
            }
        }
    }
    out
}

/// The names of the C0 control characters, U+0000 to U+001F.
const C0_NAMES: [&str; 32] = [
    "null",
    "start of heading",
    "start of text",
    "end of text",
    "end of transmission",
    "enquiry",
    "acknowledge",
    "bell",
    "backspace",
    "tab",
    "line feed",
    "vertical tab",
    "form feed",
    "carriage return",
    "shift out",
    "shift in",
    "data link escape",
    "device control one",
    "device control two",
    "device control three",
    "device control four",
    "negative acknowledge",
    "synchronous idle",
    "end of transmission block",
    "cancel",
    "end of medium",
    "substitute",
    "escape",
    "file separator",
    "group separator",
    "record separator",
    "unit separator",
];

/// The name of a character that [`unseen`] gives: its Unicode name (or
/// its common alias) in lower case, such as `zero width space` or
/// `escape`; for a range, a short name such as `variation selector 16`,
/// `tag 'a'` (the letter that the tag hides) or `C1 control`; else
/// `invisible character`.
pub fn char_name(c: char) -> Cow<'static, str> {
    let n = c as u32;
    Cow::Borrowed(match c {
        '\0'..='\x1f' => C0_NAMES[n as usize],
        '\x7f' => "delete",
        '\u{85}' => "next line",
        '\u{80}'..='\u{9f}' => "C1 control",
        '\u{00AD}' => "soft hyphen",
        '\u{034F}' => "combining grapheme joiner",
        '\u{061C}' => "arabic letter mark",
        '\u{115F}' => "hangul choseong filler",
        '\u{1160}' => "hangul jungseong filler",
        '\u{17B4}' => "khmer vowel inherent aq",
        '\u{17B5}' => "khmer vowel inherent aa",
        '\u{180B}' => "mongolian free variation selector one",
        '\u{180C}' => "mongolian free variation selector two",
        '\u{180D}' => "mongolian free variation selector three",
        '\u{180E}' => "mongolian vowel separator",
        '\u{180F}' => "mongolian free variation selector four",
        '\u{200B}' => "zero width space",
        '\u{200C}' => "zero width non-joiner",
        '\u{200D}' => "zero width joiner",
        '\u{200E}' => "left-to-right mark",
        '\u{200F}' => "right-to-left mark",
        '\u{202A}' => "left-to-right embedding",
        '\u{202B}' => "right-to-left embedding",
        '\u{202C}' => "pop directional formatting",
        '\u{202D}' => "left-to-right override",
        '\u{202E}' => "right-to-left override",
        '\u{2060}' => "word joiner",
        '\u{2061}' => "function application",
        '\u{2062}' => "invisible times",
        '\u{2063}' => "invisible separator",
        '\u{2064}' => "invisible plus",
        '\u{2066}' => "left-to-right isolate",
        '\u{2067}' => "right-to-left isolate",
        '\u{2068}' => "first strong isolate",
        '\u{2069}' => "pop directional isolate",
        '\u{206A}' => "inhibit symmetric swapping",
        '\u{206B}' => "activate symmetric swapping",
        '\u{206C}' => "inhibit arabic form shaping",
        '\u{206D}' => "activate arabic form shaping",
        '\u{206E}' => "national digit shapes",
        '\u{206F}' => "nominal digit shapes",
        '\u{3164}' => "hangul filler",
        '\u{FE00}'..='\u{FE0F}' => {
            return Cow::Owned(format!("variation selector {}", n - 0xFE00 + 1));
        }
        '\u{FEFF}' => "byte order mark",
        '\u{FFA0}' => "halfwidth hangul filler",
        '\u{1BCA0}' => "shorthand format letter overlap",
        '\u{1BCA1}' => "shorthand format continuing overlap",
        '\u{1BCA2}' => "shorthand format down step",
        '\u{1BCA3}' => "shorthand format up step",
        '\u{1D173}' => "musical symbol begin beam",
        '\u{1D174}' => "musical symbol end beam",
        '\u{1D175}' => "musical symbol begin tie",
        '\u{1D176}' => "musical symbol end tie",
        '\u{1D177}' => "musical symbol begin slur",
        '\u{1D178}' => "musical symbol end slur",
        '\u{1D179}' => "musical symbol begin phrase",
        '\u{1D17A}' => "musical symbol end phrase",
        '\u{E0001}' => "language tag",
        '\u{E0020}' => "tag space",
        '\u{E0021}'..='\u{E007E}' => {
            let letter = char::from_u32(n - 0xE0000).unwrap_or('?');
            return Cow::Owned(format!("tag {letter:?}"));
        }
        '\u{E007F}' => "cancel tag",
        '\u{E0100}'..='\u{E01EF}' => {
            return Cow::Owned(format!("variation selector {}", n - 0xE0100 + 17));
        }
        _ => "invisible character",
    })
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

    #[test]
    fn unseen_names_what_the_code_does_not_show() {
        let named = |line: &str| -> Vec<(usize, String)> {
            unseen(line)
                .into_iter()
                .map(|(col, c)| (col, format!("U+{:04X} {}", c as u32, char_name(c))))
                .collect()
        };
        assert_eq!(
            named("ad\u{200B}min"),
            [(2, "U+200B zero width space".to_string())]
        );
        // A stray joiner is in the grapheme of the letter before it.
        assert_eq!(
            named("ad\u{200D}min"),
            [(1, "U+200D zero width joiner".to_string())]
        );
        assert_eq!(
            named("a\x1bb\0\x7f\u{9b}\u{85}"),
            [
                (1, "U+001B escape".to_string()),
                (3, "U+0000 null".to_string()),
                (4, "U+007F delete".to_string()),
                (5, "U+009B C1 control".to_string()),
                (6, "U+0085 next line".to_string()),
            ]
        );
        assert_eq!(
            named("x\u{FEFF}\u{202E}\u{FE0F}"),
            [
                (1, "U+FEFF byte order mark".to_string()),
                (2, "U+202E right-to-left override".to_string()),
                // A control ends its grapheme: the selector is one of its own.
                (3, "U+FE0F variation selector 16".to_string()),
            ]
        );
        assert_eq!(
            named("\u{E0068}\u{E0069}\u{E0100}"),
            [
                (0, "U+E0068 tag 'h'".to_string()),
                (0, "U+E0069 tag 'i'".to_string()),
                (0, "U+E0100 variation selector 17".to_string()),
            ]
        );
        // What does a job, the tab and plain text: nothing.
        for line in [
            "\u{1F468}\u{200D}\u{1F469}",
            "\u{0645}\u{06CC}\u{200C}\u{062E}",
            "\u{2764}\u{FE0F}",
            "1\u{FE0F}\u{20E3}",
            // The flag of Scotland, a Han variant, a Mongolian variant.
            "\u{1F3F4}\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}",
            "\u{8FBB}\u{E0100}",
            "\u{1820}\u{180B}",
            "\tlet x = \"ñ\";",
            "plain ñ",
            "",
        ] {
            assert_eq!(named(line), [], "{line:?}");
        }
    }

    /// The note of `code.get` and the mark of the screen use one rule: for
    /// every invisible character after each kind of character, `unseen`
    /// names exactly the characters that `code_glyph` marks.
    #[test]
    fn unseen_and_the_mark_of_the_screen_agree() {
        let ranges = [
            0x00AD..=0x00AD,
            0x034F..=0x034F,
            0x061C..=0x061C,
            0x115F..=0x1160,
            0x17B4..=0x17B5,
            0x180B..=0x180F,
            0x200B..=0x200F,
            0x202A..=0x202E,
            0x2060..=0x206F,
            0x3164..=0x3164,
            0xFE00..=0xFE0F,
            0xFEFF..=0xFEFF,
            0xFFA0..=0xFFA0,
            0x1BCA0..=0x1BCA3,
            0x1D173..=0x1D17A,
            0xE0000..=0xE0FFF,
        ];
        let before = [
            "",
            "a",
            "1",
            "\u{1F468}",
            "\u{2764}",
            "\u{0645}",
            "\u{0915}\u{094D}",
            "\u{4E00}",
            "\u{1820}",
            "\u{1F3F4}",
        ];
        let after = ["", "b", "\u{1F469}", "\u{20E3}", "\u{0937}", "\u{E007F}"];
        let mut seen = 0;
        for c in ranges.into_iter().flatten().filter_map(char::from_u32) {
            for b in before {
                for a in after {
                    let line = format!("{b}{c}{a}");
                    let marks: usize = line.graphemes(true).map(|g| code_glyph(g).1).sum();
                    let named = unseen(&line)
                        .iter()
                        .filter(|(_, c)| control_picture(*c).is_none())
                        .count();
                    assert_eq!(named, marks, "{line:?}");
                    seen += marks;
                }
            }
        }
        // The walk marks most of them and leaves the ones that do a job.
        assert!(seen > 10_000, "{seen}");
        for c in (0..=0x9f).filter_map(char::from_u32) {
            let line = format!("a{c}b");
            let named = unseen(&line).len();
            assert_eq!(named, usize::from(control_picture(c).is_some()), "{c:?}");
        }
    }
}
