//! Invisible characters. An annotation text showed a control character as
//! a silent space; the code dropped its bidi controls and zero-width
//! characters from the drawing, so `ad` U+200B `min` read as `admin`;
//! and a label with zero-width spaces was cut at the canvas edge with no
//! `…` while `tiza check` called it clean (each one took a cell the layout
//! did not count).
//!
//! Acceptance criteria:
//! - AC1 An annotation text with ESC, NUL, BEL or DEL shows the control
//!   pictures (`␛`, `␀`, `␇`, `␡`) in the TUI and every export, never raw
//!   and never a silent space.
//! - AC2 A bidi control in the code shows as a one-cell ⬚ (U+2B1A) in the
//!   theme's error colour; the columns after it line up, and anchors, find
//!   and the cursor address the right characters.
//! - AC3 Zero-width characters in the code (U+200B, U+2060, U+FEFF but the
//!   one that starts the file, a ZWJ/ZWNJ that joins nothing) show the same
//!   mark; inside an emoji ZWJ sequence, an Indic conjunct or a word of a
//!   script that uses joiners nothing changes. A BOM that starts a file is
//!   dropped on every read (a referenced file re-read kept it as an
//!   invisible column 0 of line 1).
//! - AC4 Zero-width characters in annotation texts take no cell: what the
//!   layout measures is what is drawn, and `check`, `clipped` and
//!   `visible` agree with it; a callout with them stays in its border; the
//!   stored text is not changed.
//! - AC5 Every output agrees; a file without these characters is laid out
//!   as before.
//! - AC6 This file (TUI pty, txt, md, ansi, svg; a PNG is drawn from the
//!   SVG and carries no text of its own), with zwsp-note and random
//!   documents whose oracle reads the letters drawn; CHANGELOG; README.
//!
//! Corner cases: several invisible characters in a row and one at the end
//! of a line; a find target holding a zero-width character; a BOM at the
//! start of a file and one in the middle; ZWNJ in a Persian word; a label
//! made only of zero-width characters (the same as no label, as one of
//! only spaces).

use serde_json::{Value, json};
use tiza_core::Document;
use tiza_core::canvas::code_width;
use tiza_core::layout::{Layout, LayoutOptions, Row, col_to_x, layout};
use tiza_qa::{Live, fixture, scratch, tiza_ok};

const MARK: &str = "\u{2B1A}";

fn lay(doc: &Document, width: usize, step: u32) -> Layout {
    let opts = LayoutOptions {
        width,
        step,
        line_numbers: doc.numbers,
        margin_cap: doc.margin.map(usize::from),
        ..Default::default()
    };
    layout(doc, &tiza_core::Theme::dark(), &opts)
}

fn doc_of(v: &Value) -> Document {
    tiza_core::fs::parse_document(&v.to_string(), None).unwrap()
}

fn code_doc(code: &[&str], annotations: Value) -> Value {
    json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1},
        "code": code, "annotations": annotations
    })
}

/// The cells of the code line `line` as drawn, from the code column on.
fn code_cells(l: &Layout, line: u32) -> Vec<(String, tiza_core::canvas::CellStyle)> {
    let y = l.row_of_line[&line];
    (l.code_x..l.canvas.width)
        .filter_map(|x| l.canvas.get(x, y))
        .filter(|c| !c.sym.as_str().is_empty())
        .map(|c| (c.sym.to_string(), c.style))
        .collect()
}

fn code_text(l: &Layout, line: u32) -> String {
    let s: String = code_cells(l, line).into_iter().map(|(s, _)| s).collect();
    s.trim_end().to_string()
}

/// The text the cells `[x0, x1)` of the row of `line` hold.
fn cells_text(l: &Layout, line: u32, x0: usize, x1: usize) -> String {
    let y = l.row_of_line[&line];
    (x0..x1)
        .filter_map(|x| l.canvas.get(x, y))
        .map(|c| c.sym.to_string())
        .collect()
}

fn write(name: &str, file: &str, text: &str) -> std::path::PathBuf {
    let dir = scratch(name);
    let p = dir.join(file);
    std::fs::write(&p, text).unwrap();
    p
}

fn render(path: &std::path::Path, format: &str, extra: &[&str]) -> String {
    let mut args = vec!["render", path.to_str().unwrap(), "--format", format];
    args.extend_from_slice(extra);
    tiza_ok(&args)
}

/// No raw control character: everything but the newline and the tab.
fn assert_clean(what: &str, s: &str) {
    let bad: Vec<String> = s
        .chars()
        .filter(|&c| (c < ' ' && !matches!(c, '\n' | '\t')) || ('\u{7f}'..='\u{9f}').contains(&c))
        .map(|c| format!("U+{:04X}", c as u32))
        .collect();
    assert!(bad.is_empty(), "{what} holds raw {bad:?}:\n{s}");
}

/// `s` without the SGR sequences an ANSI export writes.
fn strip_sgr(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' && it.peek() == Some(&'[') {
            for d in it.by_ref() {
                if d == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ---------- AC1: control characters in annotation texts ----------

fn controls_doc() -> Value {
    json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1},
        // Lines after the callout's, for its block to stand beside them in
        // the margin.
        "code": ["let a = 1;", "let b = 2;", "let c = 3;", "", "", "", ""],
        "steps": [{"step": 1, "title": "t\u{7}itle"}],
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 1,
             "text": "clear \u{1b}[2J bell \u{7} nul \u{0} del \u{7f}"},
            {"id": "h1", "step": 1, "type": "highlight",
             "span": {"line": 2, "start": 4, "end": 5}, "label": "h\u{1b}l"},
            {"id": "c1", "step": 1, "type": "callout", "from_line": 3, "to_line": 3,
             "title": "ti\u{7f}", "text": "body \u{7} one\nline\ttwo"},
            {"id": "n2", "step": 1, "type": "note", "line": 3, "text": "tab\there\nnewline"}
        ]
    })
}

fn assert_pictures(what: &str, s: &str) {
    assert_clean(what, s);
    for want in ["clear ␛[2J bell ␇ nul ␀ del ␡", "h␛l", "body ␇ one"] {
        assert!(s.contains(want), "{what}: no `{want}` in\n{s}");
    }
    // A tab or a line break in a one-line label is a space, as before.
    assert!(s.contains("tab here newline"), "{what}:\n{s}");
}

#[test]
fn step1_ac1_annotation_controls_show_their_pictures_in_every_export() {
    let f = write("t190-ac1", "c.tiza", &controls_doc().to_string());
    for format in ["txt", "md"] {
        let out = render(&f, format, &["--step", "1", "--width", "160"]);
        assert_pictures(format, &out);
    }
    let ansi = render(&f, "ansi", &["--step", "1", "--width", "160"]);
    assert_pictures("ansi", &strip_sgr(&ansi));
    let svg = f.with_extension("svg");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--step",
        "1",
        "--width",
        "160",
        "--out",
        svg.to_str().unwrap(),
    ]);
    let text = std::fs::read_to_string(&svg).unwrap();
    assert_clean("svg", &text);
    for want in ["␛[2J", "␇", "␀", "␡", "h␛l"] {
        assert!(text.contains(want), "svg: no `{want}`");
    }
    // The text kept in the document is the one written.
    let doc = tiza_core::fs::load_document(&f).unwrap();
    let note = doc.find_annotation("n1").unwrap();
    assert!(
        serde_json::to_string(&note.shape)
            .unwrap()
            .contains("\\u001b[2J"),
        "{:?}",
        note.shape
    );
}

#[test]
fn ac1_the_tui_shows_the_pictures() {
    let f = write("t190-ac1-tui", "c.tiza", &controls_doc().to_string());
    let doc = tiza_core::fs::load_document(&f).unwrap();
    let live = Live::open(doc, f.clone(), f.with_file_name("live.sock"));
    live.command("step 1");
    let screen = live.screen(170, 24);
    assert_pictures("TUI", &screen);
    let text = live.api_ok("render.text", json!({"width": 160}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_pictures("render.text", &text);
    // The annotation list too, in its label column.
    live.keys(&[crossterm::event::KeyCode::Char('L')]);
    let screen = live.screen(170, 24);
    assert!(screen.contains("h␛l"), "{screen}");
}

// ---------- AC2, AC3: invisible characters in the code ----------

/// QA 1's Trojan Source: an RLO and LRIs in a comment.
const TROJAN: [&str; 3] = [
    "fn main() {",
    "    let ok = 1; // \u{202E} } \u{2066} if admin \u{2069} \u{2066}",
    "}",
];

#[test]
fn step2_ac2_bidi_controls_show_a_mark_and_the_columns_line_up() {
    let doc = doc_of(&code_doc(&TROJAN, json!([])));
    let l = lay(&doc, 120, 1);
    assert_eq!(
        code_text(&l, 2),
        format!("    let ok = 1; // {MARK} }} {MARK} if admin {MARK} {MARK}")
    );
    // Each mark in the theme's error colour.
    let error = tiza_core::Theme::dark().error;
    let marks: Vec<_> = code_cells(&l, 2)
        .into_iter()
        .filter(|(s, _)| s == MARK)
        .collect();
    assert_eq!(marks.len(), 4);
    assert!(
        marks.iter().all(|(_, st)| st.fg == Some(error)),
        "{marks:?}"
    );
    // The anchor of `admin` lands on `admin`.
    let text = doc.line_text(2).unwrap();
    // (Each control is a grapheme of its own: chars are graphemes here.)
    let start = text[..text.find("admin").unwrap()].chars().count() as u32;
    let x0 = l.code_x + col_to_x(text, start, 4);
    let x1 = l.code_x + col_to_x(text, start + 5, 4);
    assert_eq!(cells_text(&l, 2, x0, x1), "admin");
    assert_eq!(code_width(text, 4), code_text(&l, 2).chars().count());
}

#[test]
fn step2_ac2_a_highlight_and_the_cursor_after_the_controls() {
    let code = TROJAN.join("\n") + "\n";
    let live = Live::from_code(&code, "rs", "t190-ac2-live");
    live.command("hl 2:admin");
    let (span, doc) = live.with(|a| {
        let ann = a.doc.annotations.last().cloned().unwrap();
        (ann, a.doc.clone())
    });
    let tiza_core::model::Shape::Highlight { span, .. } = span.shape else {
        panic!("{span:?}")
    };
    let l = lay(&doc, 120, 1);
    let text = doc.line_text(2).unwrap();
    let (x0, x1) = (
        l.code_x + col_to_x(text, span.start, 4),
        l.code_x + col_to_x(text, span.end, 4),
    );
    assert_eq!(cells_text(&l, 2, x0, x1), "admin");
    let y = l.row_of_line[&2];
    assert!(
        (x0..x1).all(|x| l.canvas.get(x, y).unwrap().style.bg.is_some()),
        "the highlight covers admin"
    );
    // The mouse on the `a` of admin is the grapheme `span.start`.
    let screen = live.screen(120, 12);
    let (row, col) = screen
        .lines()
        .enumerate()
        .find_map(|(r, line)| {
            line.find(" if admin")
                .map(|i| (r, line[..i].chars().count() + 4))
        })
        .expect("line 2 on screen");
    assert_eq!(
        live.with(move |a| a.hit_test(col as u16, row as u16)),
        Some((2, span.start)),
        "{screen}"
    );
}

#[test]
fn step3_ac3_a_zero_width_space_in_an_identifier_shows_the_mark() {
    let code = "let ad\u{200B}min = 1;\nlet admin = 2;\n";
    let live = Live::from_code(code, "rs", "t190-ac3");
    live.command("hl 1:min");
    let doc = live.with(|a| a.doc.clone());
    let l = lay(&doc, 100, 1);
    assert_eq!(code_text(&l, 1), format!("let ad{MARK}min = 1;"));
    assert_eq!(code_text(&l, 2), "let admin = 2;");
    let tiza_core::model::Shape::Highlight { span, .. } = &doc.annotations[0].shape else {
        panic!()
    };
    let text = doc.line_text(1).unwrap();
    assert_eq!(
        cells_text(
            &l,
            1,
            l.code_x + col_to_x(text, span.start, 4),
            l.code_x + col_to_x(text, span.end, 4)
        ),
        "min"
    );
    // The TUI and every export.
    let screen = live.screen(100, 10);
    assert!(
        screen.contains(&format!("let ad{MARK}min = 1;")),
        "{screen}"
    );
    let f = write("t190-ac3-files", "z.rs", code);
    for format in ["txt", "md"] {
        let out = render(&f, format, &[]);
        assert!(out.contains(&format!("ad{MARK}min")), "{format}:\n{out}");
    }
    let ansi = strip_sgr(&render(&f, "ansi", &[]));
    assert!(ansi.contains(&format!("ad{MARK}min")), "ansi:\n{ansi}");
    let svg = f.with_extension("svg");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        svg.to_str().unwrap(),
    ]);
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.contains(MARK), "svg");
    let png = f.with_extension("png");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        png.to_str().unwrap(),
    ]);
    assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"));
}

#[test]
fn ac3_which_joiners_and_zero_width_characters_are_marked() {
    for (code, want) in [
        // Marked: they join nothing.
        ("ad\u{200D}min", format!("ad{MARK}min")),
        ("ad\u{200C}min", format!("ad{MARK}min")),
        ("\u{200D}x", format!("{MARK}x")),
        ("a\u{2060}b", format!("a{MARK}b")),
        ("mid\u{FEFF}dle", format!("mid{MARK}dle")),
        (
            "a\u{200B}\u{200B}\u{200B}b",
            format!("a{MARK}{MARK}{MARK}b"),
        ),
        ("end\u{200B}", format!("end{MARK}")),
        (
            "x\u{200E}y\u{200F}z\u{061C}",
            format!("x{MARK}y{MARK}z{MARK}"),
        ),
        // Left alone: they join within the cluster, or follow a letter of
        // a script that uses them.
        (
            "👨\u{200D}👩\u{200D}👧",
            "👨\u{200D}👩\u{200D}👧".to_string(),
        ),
        ("क्\u{200D}ष", "क्\u{200D}ष".to_string()),
        ("می\u{200C}خواهم", "می\u{200C}خواهم".to_string()),
        ("cafe\u{301}", "cafe\u{301}".to_string()),
    ] {
        let doc = doc_of(&code_doc(&[code], json!([])));
        let l = lay(&doc, 80, 1);
        assert_eq!(code_text(&l, 1), want, "{code:?}");
        assert_eq!(
            code_width(code, 4),
            tiza_core::canvas::display_width(&want, 4),
            "{code:?}"
        );
    }
}

#[test]
fn ac3_a_find_target_with_a_zero_width_character_matches() {
    let code = "let ad\u{200B}min = 1;\nlet admin = 2;\n";
    let live = Live::from_code(code, "rs", "t190-find");
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "highlight", "at": {"line": 1, "find": "ad\u{200B}min"}}),
    );
    assert_eq!(r["visible"], json!(true), "{r}");
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "highlight", "at": {"line": 2, "find": "admin"}}),
    );
    assert_eq!(r["visible"], json!(true), "{r}");
    // `admin` is not on line 1: the search, which starts there, finds the
    // one on line 2.
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "highlight", "at": {"line": 1, "find": "admin"}}),
    );
    let id = r["id"].as_str().unwrap().to_string();
    let line = live.with(move |a| match &a.doc.find_annotation(&id).unwrap().shape {
        tiza_core::model::Shape::Highlight { span, .. } => span.line,
        s => panic!("{s:?}"),
    });
    assert_eq!(line, 2);
}

#[test]
fn ac3_a_bom_that_starts_the_file_is_dropped_on_every_read() {
    let dir = scratch("t190-bom");
    let src = dir.join("b.rs");
    std::fs::write(&src, "\u{FEFF}fn main() {}\nlet x = \"\u{FEFF}\";\n").unwrap();
    // Opened, and re-read through the `.tiza` that references it.
    let opened = tiza_core::fs::open(src.to_str().unwrap()).unwrap().document;
    assert_eq!(opened.line_text(1), Some("fn main() {}"));
    let tiza = dir.join("b.rs.tiza");
    std::fs::write(
        &tiza,
        json!({"schema": 1, "source": {"path": "b.rs", "language": "rs", "first_line": 1},
               "code": {"file": "b.rs"}})
        .to_string(),
    )
    .unwrap();
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    assert_eq!(doc.line_text(1), Some("fn main() {}"));
    let l = lay(&doc, 80, 1);
    assert_eq!(code_text(&l, 1), "fn main() {}");
    // One in the middle is marked.
    assert_eq!(code_text(&l, 2), format!("let x = \"{MARK}\";"));
}

// ---------- AC4: zero-width characters in annotation texts ----------

/// What is drawn outside the code's own text, blanks dropped: on a code
/// row the cells right of the line's text, on any other row every cell
/// right of the gutter.
fn drawn_outside_code(l: &Layout, doc: &Document) -> String {
    let mut out = String::new();
    for (y, row) in l.rows.iter().enumerate() {
        let from = match row {
            Row::Code(line) => l.code_x + code_width(doc.line_text(*line).unwrap_or(""), 4),
            _ => l.code_x,
        };
        for x in from..l.canvas.width {
            if let Some(c) = l.canvas.get(x, y) {
                out.push_str(c.sym.as_str());
            }
        }
    }
    out.split_whitespace().collect()
}

#[test]
fn step4_ac4_zwsp_note_is_whole_or_cut_with_an_ellipsis_at_every_width() {
    let doc = tiza_core::fs::load_document(&fixture("t-190/zwsp-note.tiza")).unwrap();
    let mut whole = 0;
    for width in 60..=200 {
        let l = lay(&doc, width, 1);
        let drawn = drawn_outside_code(&l, &doc);
        let words = drawn.matches("word").count();
        if words == 25 {
            whole += 1;
            assert!(!drawn.contains('…'), "{width}: {drawn}");
            assert!(
                l.clipped_labels.is_empty(),
                "{width}: {:?}",
                l.clipped_labels
            );
        } else {
            assert!(drawn.contains('…'), "{width}: {words} words, no …: {drawn}");
            assert!(!l.clipped_labels.is_empty(), "{width}: cut, but clean");
        }
    }
    assert!(whole > 0, "never whole in 60-200");
    // `tiza check` agrees: no label is clipped from the width it names.
    let clean = tiza_core::check::min_clean_width(&doc, &tiza_core::Theme::dark()).unwrap();
    for width in [clean, clean + 1, 200] {
        assert!(lay(&doc, width, 1).clipped_labels.is_empty(), "{width}");
    }
    assert!(
        !lay(&doc, clean - 1, 1).clipped_labels.is_empty(),
        "{clean}"
    );
    // The stored text keeps its zero-width spaces.
    let text = std::fs::read_to_string(fixture("t-190/zwsp-note.tiza")).unwrap();
    assert!(text.contains("\\u200b") || text.contains('\u{200B}'));
}

#[test]
fn step4_ac4_check_and_render_at_180_agree() {
    let f = fixture("t-190/zwsp-note.tiza");
    let out = render(&f, "txt", &["--width", "180", "--step", "1"]);
    let words = out.matches("word").count();
    // Four of them are in the code.
    assert!(words == 25 + 4 || out.contains('…'), "{out}");
    let check = tiza_ok(&["check", f.to_str().unwrap(), "--width", "180"]);
    let clipped = !lay(&tiza_core::fs::load_document(&f).unwrap(), 180, 1)
        .clipped_labels
        .is_empty();
    assert_eq!(
        !check.contains(" 0 warnings"),
        clipped,
        "check says:\n{check}\nrender:\n{out}"
    );
}

#[test]
fn ac4_visible_and_clipped_agree_with_the_drawing() {
    let live = Live::from_code("fn main() {\n    let a = 1;\n}\n", "rs", "t190-visible");
    live.with(|a| a.size = (100, 30));
    let text = "word\u{200B}".repeat(40);
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 2, "text": text, "step": 1}),
    );
    live.command("step 1");
    let doc = live.with(|a| a.doc.clone());
    let width = live.with(|a| a.screen_width());
    let l = lay(&doc, width, 1);
    let drawn = drawn_outside_code(&l, &doc);
    let cut = drawn.matches("word").count() < 40;
    assert_eq!(cut, drawn.contains('…'), "{drawn}");
    assert_eq!(r.get("clipped").is_some(), cut, "{r}");
}

#[test]
fn corner_a_label_made_only_of_zero_width_characters_draws_nothing() {
    let doc = doc_of(&code_doc(
        &["let a = 1;"],
        json!([{"id": "n1", "step": 1, "type": "note", "line": 1, "text": "\u{200B}\u{2060}\u{200D}"}]),
    ));
    let l = lay(&doc, 80, 1);
    let drawn = drawn_outside_code(&l, &doc);
    assert!(!drawn.chars().any(char::is_alphanumeric), "{drawn:?}");
    assert!(l.clipped_labels.is_empty());
}

/// A small deterministic generator (xorshift).
struct Gen(u64);

impl Gen {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn words(&mut self, max: u64) -> String {
        const W: [&str; 12] = [
            "the",
            "borrow",
            "zero\u{200B}width",
            "👩\u{200D}💻",
            "cafe\u{301}",
            "\u{301}",
            "word\u{2060}joiner",
            "no\u{A0}break",
            "全\u{3000}角",
            "bom\u{FEFF}",
            "a\u{200B}",
            "join\u{200D}er",
        ];
        let n = 1 + self.below(max);
        (0..n)
            .map(|_| W[self.below(W.len() as u64) as usize])
            .collect::<Vec<_>>()
            .join(if self.below(4) == 0 { "" } else { " " })
    }
}

/// The letters of `s`, in order.
fn letters(s: &str) -> String {
    s.chars().filter(|c| c.is_alphabetic()).collect()
}

#[test]
fn ac6_random_documents_draw_every_letter_or_an_ellipsis() {
    let mut g = Gen(0x7a11_1900_5eed_0002);
    let (mut checked, mut cut) = (0, 0);
    for i in 0..150 {
        // Code with no letters, so every letter drawn is the label's.
        let code: Vec<String> = (0..3 + g.below(5))
            .map(|_| {
                format!(
                    "{} = {};",
                    "0".repeat(1 + g.below(9) as usize),
                    "1".repeat(1 + g.below(30) as usize)
                )
            })
            .collect();
        let lines = code.len() as u64;
        let l = 1 + g.below(lines);
        let long = if g.below(3) == 0 { 30 } else { 8 };
        let text = g.words(long);
        let kind = g.below(4);
        let ann = match kind {
            0 => json!({"id": "a", "step": 1, "type": "note", "line": l, "text": text}),
            1 => {
                json!({"id": "a", "step": 1, "type": "highlight", "span": {"line": l, "start": 0, "end": 1}, "label": text})
            }
            2 => {
                json!({"id": "a", "step": 1, "type": "mark", "kind": "warn", "span": {"line": l, "start": 0, "end": 1}, "label": text})
            }
            _ => {
                json!({"id": "a", "step": 1, "type": "callout", "from_line": l, "to_line": l, "text": text})
            }
        };
        let v = json!({"schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code, "annotations": [ann]});
        let doc = doc_of(&v);
        let want = letters(&text);
        for width in [40, 60, 80, 100, 140, 200] {
            let lay = lay(&doc, width, 1);
            let drawn = drawn_outside_code(&lay, &doc);
            // A mark writes its kind's name before its label.
            let got = letters(&drawn);
            let got = if kind == 2 {
                got.replacen("warning", "", 1)
            } else {
                got
            };
            let clipped = !lay.clipped_labels.is_empty();
            if got == want {
                assert!(
                    !clipped,
                    "#{i} at {width}: drawn whole, called clipped\n{v}"
                );
            } else {
                assert!(
                    want.starts_with(&got) && clipped,
                    "#{i} at {width}: drew {got:?} of {want:?} (clipped: {clipped})\n{drawn}\n{v}"
                );
                cut += 1;
            }
            checked += 1;
        }
    }
    assert!(checked >= 900 && cut > 0, "{checked} checked, {cut} cut");
}

#[test]
fn ac6_pty_the_real_terminal_shows_the_mark_and_the_pictures() {
    let dir = scratch("t190-pty");
    let doc = json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "step": 1,
        "code": ["let ad\u{200B}min = 1;", "let admin = 2;"],
        "annotations": [{"id": "n1", "step": 1, "type": "note", "line": 2,
                         "text": "clear \u{1b}[2J bell \u{7}"}]
    });
    std::fs::write(dir.join("d.tiza"), doc.to_string()).unwrap();
    let body = r##"
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
s = Pty([binary, "--no-save", "d.tiza"], 30, 120, env=env)
ok = s.wait_for(lambda out: "ad\u2b1amin" in plain(out) and "\u241b[2J" in plain(out), 10, "the mark and the picture")
print("SEEN", ok, flush=True)
s.send(b":q\r")
print("EXIT", s.finish(10))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "pty.py",
        body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(out.contains("SEEN True"), "{out}");
    assert!(out.contains("EXIT 0"), "{out}");
}

// ---------- AC5: files without these characters ----------

#[test]
fn ac5_without_these_characters_every_measure_is_the_old_one() {
    // The new measures differ from the old ones only on the characters
    // this card is about: every example lays out as before.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples");
    let mut seen = 0;
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_none_or(|x| x != "tiza") {
            continue;
        }
        let doc = tiza_core::fs::load_document(&p).unwrap();
        for line in &doc.code {
            assert_eq!(
                code_width(line, 4),
                tiza_core::canvas::display_width(line, 4),
                "{}: {line}",
                p.display()
            );
        }
        for a in &doc.annotations {
            let v = serde_json::to_value(&a.shape).unwrap();
            for key in ["label", "text", "title"] {
                for t in v[key].as_str().unwrap_or("").lines() {
                    assert_eq!(
                        tiza_core::canvas::drawn_width(t),
                        tiza_core::canvas::display_width(t, 4),
                        "{}: {} {key}",
                        p.display(),
                        a.id
                    );
                }
            }
        }
        seen += 1;
    }
    assert!(seen >= 3, "{seen} examples");
}

#[test]
fn ac6_changelog_and_readme() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("U+2B1A"), "CHANGELOG");
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    assert!(readme.contains("⬚"), "README");
}

// ---------- Review ----------

fn doc_in(lang: &str, code: &[&str], annotations: Value) -> Document {
    doc_of(&json!({
        "schema": 1, "source": {"language": lang, "first_line": 1},
        "code": code, "annotations": annotations
    }))
}

#[test]
fn rev_a_grammar_that_splits_a_cluster_draws_what_it_measures() {
    // TypeScript's grammar splits the ZWNJ from its letter and TOML an
    // emoji from its ZWJ: the drawing goes by whole graphemes, as the
    // columns are measured.
    for lang in [
        "ts", "toml", "js", "rs", "py", "go", "c", "md", "yaml", "json",
    ] {
        for (code, want) in [
            (
                "const \u{645}\u{6cc}\u{200C}\u{62e}\u{648}\u{627}\u{647}\u{645} = 1;",
                None,
            ),
            ("\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467} = 1;", None),
            (
                "let ad\u{200B}min = 1;",
                Some(format!("let ad{MARK}min = 1;")),
            ),
        ] {
            let doc = doc_in(lang, &[code], json!([]));
            let l = lay(&doc, 100, 1);
            let want = want.unwrap_or_else(|| code.to_string());
            assert_eq!(code_text(&l, 1), want, "{lang}: {code:?}");
            assert_eq!(
                code_width(code, 4),
                tiza_core::canvas::display_width(&want, 4),
                "{lang}: {code:?}"
            );
        }
        // A highlight after the split cluster lands on its text.
        let code = "const \u{645}\u{6cc}\u{200C}\u{62e} = 42;";
        let doc = doc_in(
            lang,
            &[code],
            json!([{"id": "h", "step": 1, "type": "highlight", "span": {"line": 1, "start": 12, "end": 14}}]),
        );
        let l = lay(&doc, 100, 1);
        let (x0, x1) = (
            l.code_x + col_to_x(code, 12, 4),
            l.code_x + col_to_x(code, 14, 4),
        );
        assert_eq!(cells_text(&l, 1, x0, x1), "42", "{lang}");
        let y = l.row_of_line[&1];
        assert!(
            (x0..x1).all(|x| l.canvas.get(x, y).unwrap().style.bg.is_some()),
            "{lang}: the highlight is on 42"
        );
    }
}

#[test]
fn rev_many_joiners_in_one_grapheme_lay_out_in_linear_time() {
    // `a` and 100,000 ZWJs are one grapheme: deciding each joiner by
    // looking at all the ones after it took minutes.
    let code = format!("a{}", "\u{200D}".repeat(100_000));
    let doc = doc_of(&code_doc(&[&code], json!([])));
    let start = std::time::Instant::now();
    let l = lay(&doc, 120, 1);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(10),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(code_width(&code, 4), 100_001);
    assert!(code_text(&l, 1).ends_with('…'));
}

#[test]
fn rev_every_default_ignorable_character_is_marked_unless_it_does_a_job() {
    for c in [
        '\u{00AD}',
        '\u{034F}',
        '\u{115F}',
        '\u{1160}',
        '\u{17B4}',
        '\u{17B5}',
        '\u{180B}',
        '\u{180E}',
        '\u{2061}',
        '\u{2062}',
        '\u{2063}',
        '\u{2064}',
        '\u{206A}',
        '\u{3164}',
        '\u{FE00}',
        '\u{FE0F}',
        '\u{FFA0}',
        '\u{E0001}',
        '\u{E0061}',
        '\u{E0062}',
        '\u{1BCA0}',
        '\u{1D173}',
    ] {
        let code = format!("ad{c}min");
        let doc = doc_of(&code_doc(&[&code], json!([])));
        let l = lay(&doc, 80, 1);
        assert_eq!(
            code_text(&l, 1),
            format!("ad{MARK}min"),
            "U+{:04X}",
            c as u32
        );
        assert_eq!(code_width(&code, 4), 6, "U+{:04X}", c as u32);
    }
    // A Mongolian variation selector cannot carry a joiner past the rule.
    let doc = doc_of(&code_doc(&["ad\u{180B}\u{200D}min"], json!([])));
    assert_eq!(
        code_text(&lay(&doc, 80, 1), 1),
        format!("ad{MARK}{MARK}min")
    );
    // Left alone where they do a job.
    for code in [
        "\u{2764}\u{FE0F}",
        "1\u{FE0F}\u{20E3}",
        "\u{1F3F4}\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}",
        "\u{1820}\u{180B}\u{1821}",
        "\u{845B}\u{E0100}",
        "\u{1F469}\u{1F3FD}\u{200D}\u{1F4BB}",
        "\u{1F441}\u{200D}\u{1F5E8}",
        "\u{1F3C3}\u{200D}\u{2640}\u{FE0F}",
    ] {
        let doc = doc_of(&code_doc(&[code], json!([])));
        assert_eq!(code_text(&lay(&doc, 80, 1), 1), code, "{code:?}");
    }
}

#[test]
fn rev_joiners_of_more_scripts_are_left_alone() {
    for code in [
        "\u{A9B2}\u{A9C0}\u{200C}\u{A9B1}",    // Javanese
        "\u{1B13}\u{1B44}\u{200C}\u{1B31}",    // Balinese
        "\u{1108D}\u{110B9}\u{200D}\u{1108E}", // Kaithi
        "\u{D9A}\u{DCA}\u{200D}\u{DBB}",       // Sinhala
    ] {
        let doc = doc_of(&code_doc(&[code], json!([])));
        assert_eq!(code_text(&lay(&doc, 80, 1), 1), code, "{code:?}");
    }
}

#[test]
fn rev_a_mark_keeps_its_colour_under_a_highlight() {
    let doc = doc_of(&code_doc(
        &["let ad\u{200B}min = 1;"],
        json!([{"id": "h", "step": 1, "type": "highlight", "span": {"line": 1, "start": 4, "end": 10}}]),
    ));
    let error = tiza_core::Theme::dark().error;
    for step in [0, 1] {
        let l = lay(&doc, 80, step);
        let marks: Vec<_> = code_cells(&l, 1)
            .into_iter()
            .filter(|(s, _)| s == MARK)
            .collect();
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].1.fg, Some(error), "step {step}");
    }
}

#[test]
fn rev_a_step_title_shows_its_control_pictures_on_the_bar() {
    let f = write(
        "t190-title",
        "t.tiza",
        &json!({
            "schema": 1, "source": {"language": "rs", "first_line": 1}, "step": 1,
            "code": ["let a = 1;"],
            "steps": [{"step": 1, "title": "Intro\u{1b}[2J bell\u{7}"}],
            "annotations": [{"id": "n1", "step": 1, "type": "note", "line": 1, "text": "x"}]
        })
        .to_string(),
    );
    let doc = tiza_core::fs::load_document(&f).unwrap();
    let live = Live::open(doc, f.clone(), f.with_file_name("live.sock"));
    let screen = live.screen(120, 12);
    assert!(screen.contains("Intro␛[2J bell␇"), "{screen}");
}

// ---------- On the margin callouts of t-201 ----------

/// The cell's text at `(x, y)`, or nothing outside the canvas.
fn sym(l: &Layout, x: usize, y: usize) -> String {
    l.canvas
        .get(x, y)
        .map_or(String::new(), |c| c.sym.to_string())
}

#[test]
fn rev2_a_callout_with_zero_width_spaces_stays_in_its_border() {
    // On main the zero-width spaces each took a cell the block did not
    // count: the text crossed the right border (61 and 100 columns).
    let doc = tiza_core::fs::load_document(&fixture("t-190/zwsp-callout.tiza")).unwrap();
    let (mut beside, mut under) = (0, 0);
    for width in 40..=200 {
        let l = lay(&doc, width, 1);
        let corner = (0..l.canvas.height).find_map(|y| {
            (0..l.canvas.width)
                .find(|&x| sym(&l, x, y) == "╭")
                .map(|x| (x, y))
        });
        let (x0, top) = corner.unwrap_or_else(|| panic!("{width}: no block"));
        let x1 = (x0..l.canvas.width)
            .find(|&x| sym(&l, x, top) == "╮")
            .unwrap_or_else(|| panic!("{width}: no right corner"));
        if matches!(l.rows[top], Row::Code(_)) {
            beside += 1;
        } else {
            under += 1;
        }
        let mut inside = String::new();
        let mut y = top + 1;
        while sym(&l, x0, y) != "╰" {
            assert_eq!(sym(&l, x0, y), "│", "{width}: row {y}");
            assert_eq!(sym(&l, x1, y), "│", "{width}: row {y} crosses its border");
            for x in x1 + 1..l.canvas.width {
                assert!(sym(&l, x, y).trim().is_empty(), "{width}: row {y} at {x}");
            }
            inside.extend((x0 + 1..x1).map(|x| sym(&l, x, y)));
            y += 1;
            assert!(y < l.canvas.height, "{width}: no bottom");
        }
        assert_eq!(sym(&l, x1, y), "╯", "{width}: bottom row");
        assert!(
            letters(&inside) == "zerowidthspacesinacallouttext" || inside.contains('…'),
            "{width}: {inside:?}"
        );
    }
    assert!(beside > 0 && under > 0, "beside {beside}, under {under}");
}

#[test]
fn rev2_a_label_of_only_zero_width_characters_is_no_label() {
    // blank-null (a copy of t-201's) has no labels; zero-width-labels has
    // U+200B U+2060 U+200D as the label of a highlight, a mark, an arrow
    // and a box, as the title of two callouts, and a note of two U+200B.
    let a = fixture("t-190/zero-width-labels.tiza");
    let b = fixture("t-190/blank-null.tiza");
    for w in [61, 80, 100, 140] {
        let w = w.to_string();
        for format in ["txt", "svg"] {
            let extra = ["--width", w.as_str(), "--step", "1"];
            let (ra, rb) = (render(&a, format, &extra), render(&b, format, &extra));
            assert_eq!(ra, rb, "{format} at {w}\n{ra}");
        }
    }
}

// ---------- Review 2 ----------

#[test]
fn rev2_a_note_or_callout_text_of_only_zero_width_characters_is_empty() {
    // Refused like a text of only spaces; before, it was added and drew
    // nothing, and `check` said nothing.
    let live = Live::from_code("a = 1\nb = 2\n", "sh", "t190-empty-text");
    let r = live.api(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "\u{200B}\u{2060}", "step": 1}),
    );
    let e = r.error.expect("refused");
    assert!(e.message.contains("the note is empty"), "{}", e.message);
    let r = live.api(
        "annotation.add",
        json!({"type": "callout", "from_line": 2, "text": "\u{200B}\n\u{200B}", "step": 1}),
    );
    let e = r.error.expect("refused");
    assert!(
        e.message.contains("the callout text is empty"),
        "{}",
        e.message
    );
    // `tiza check` names a stored one.
    let doc = code_doc(
        &["let a = 1;"],
        json!([{"id": "n1", "step": 1, "type": "note", "line": 1, "text": "\u{200B}\u{200B}"}]),
    );
    let f = write("t190-empty-check", "e.tiza", &doc.to_string());
    let out = tiza_qa::tiza(&["check", f.to_str().unwrap()]);
    assert!(!out.status.success(), "check passes");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("n1") && text.contains("the note is empty"),
        "{text}"
    );
}

#[test]
fn rev2_white_space_controls_in_a_label_are_spaces_everywhere() {
    // A vertical tab, a form feed or NEL in a label is a space, as a tab
    // is: a wrapped label (a note, a margin label) took it as one, while a
    // box label drew `␋`. A label of only these is no label.
    let doc = doc_of(&code_doc(
        &["let a = 1;", "let b = 2;", "let c = 3;"],
        json!([
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 1, "start": 0, "end": 3}, "label": "v\u{b}t"},
            {"id": "b1", "step": 1, "type": "box", "from_line": 2, "to_line": 2, "label": "f\u{c}f n\u{85}l"},
            {"id": "h3", "step": 1, "type": "highlight", "span": {"line": 3, "start": 0, "end": 3}, "label": "\u{b}\u{85}"}
        ]),
    ));
    let text = lay(&doc, 80, 1).canvas.to_plain_text();
    assert!(text.contains("v t") && text.contains("f f n l"), "{text}");
    for c in [
        '\u{240B}', '\u{240C}', '\u{FFFD}', '\u{b}', '\u{c}', '\u{85}',
    ] {
        assert!(!text.contains(c), "U+{:04X} in\n{text}", c as u32);
    }
    let mut none = doc.clone();
    none.annotations.retain(|a| a.id != "h3");
    assert_eq!(text, lay(&none, 80, 1).canvas.to_plain_text());
}

#[test]
fn rev2_a_label_cut_in_a_box_border_is_measured_as_drawn() {
    // Each lam-alef pair is two graphemes, two cells, but the whole string
    // measured one cell each: the label ran over the box's corner.
    let doc = doc_of(&json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1},
        "code": ["let a = 1;", "let b = 2;"],
        "annotations": [{"id": "b1", "step": 1, "type": "box", "from_line": 1, "to_line": 2,
                         "label": "\u{644}\u{627}".repeat(6)}]
    }));
    for width in 20..=40 {
        let l = lay(&doc, width, 1);
        let top = (0..l.canvas.height)
            .find(|&y| (0..l.canvas.width).any(|x| sym(&l, x, y) == "╭"))
            .expect("a box");
        let row: String = (0..l.canvas.width).map(|x| sym(&l, x, top)).collect();
        assert!(row.contains('╮'), "{width}: {row}");
        let cut = !l.clipped_labels.is_empty();
        assert_eq!(row.contains('…'), cut, "{width}: {row}");
    }
}

#[test]
fn rev2_a_struck_mark_keeps_the_error_colour() {
    let doc = doc_of(&code_doc(
        &["x\u{2060}y = 1;"],
        json!([{"id": "h1", "step": 1, "type": "highlight", "span": {"line": 1, "start": 0, "end": 4}, "emphasis": "strike"}]),
    ));
    let l = lay(&doc, 80, 1);
    let error = tiza_core::Theme::dark().error;
    let cells = code_cells(&l, 1);
    let mark = cells
        .iter()
        .find(|(s, _)| s.starts_with(MARK))
        .unwrap_or_else(|| panic!("no mark: {cells:?}"));
    assert!(mark.0.ends_with('\u{336}'), "struck: {:?}", mark.0);
    assert_eq!(mark.1.fg, Some(error), "{mark:?}");
}

#[test]
fn rev2_a_long_word_with_zero_width_spaces_wraps_at_the_full_width() {
    // Each zero-width space counted one cell where a long word is split:
    // the rows of a 20-column callout held 10 letters.
    let doc = doc_of(&code_doc(
        &["x;", "", "", "", "", "", ""],
        json!([{"id": "c1", "step": 1, "type": "callout", "from_line": 1,
                "text": "a\u{200B}".repeat(40), "width": 20, "placement": "margin"}]),
    ));
    let l = lay(&doc, 120, 1);
    let rows: Vec<String> = (0..l.canvas.height)
        .map(|y| {
            (0..l.canvas.width)
                .map(|x| sym(&l, x, y))
                .collect::<String>()
        })
        .filter(|r| r.contains("│ a"))
        .collect();
    let counts: Vec<usize> = rows.iter().map(|r| r.matches('a').count()).collect();
    assert_eq!(counts, vec![20, 20], "{rows:#?}");
}

#[test]
fn rev2_tags_in_a_flag_are_marked_unless_they_name_a_subdivision() {
    let flag = |tags: &str| -> String {
        let t: String = tags
            .chars()
            .map(|c| char::from_u32(0xE0000 + c as u32).unwrap())
            .collect();
        format!("\u{1F3F4}{t}\u{E007F}")
    };
    for (tags, marked) in [
        ("gbsct", false),
        ("gbeng", false),
        ("rm -rf", true),
        ("rmrfall", true),
        ("G", true),
    ] {
        let code = format!("f = \"{}\";", flag(tags));
        let doc = doc_of(&code_doc(&[&code], json!([])));
        let text = code_text(&lay(&doc, 80, 1), 1);
        assert_eq!(text.contains(MARK), marked, "{tags}: {text}");
    }
}

#[test]
fn rev2_a_step_message_shows_the_titles_control_pictures() {
    // `:goto`, and `:next`/`:prev` when they cannot move, name the step
    // with its title: its BEL as `␇`, as the status bar draws it.
    let mut doc = code_doc(
        &["let a = 1;"],
        json!([{"id": "n1", "step": 1, "type": "note", "line": 1, "text": "one"}]),
    );
    doc["steps"] = json!([{"step": 0, "title": "s\u{7}tart"}, {"step": 1, "title": "t\u{7}itle"}]);
    let f = write("t190-step-msg", "c.tiza", &doc.to_string());
    let live = Live::open(
        tiza_core::fs::load_document(&f).unwrap(),
        f.clone(),
        f.with_extension("sock"),
    );
    for (cmd, want) in [
        ("goto 1", "t\u{2407}itle"),
        ("next", "t\u{2407}itle"),
        ("prev", ""),
        ("prev", "s\u{2407}tart"),
    ] {
        live.command(cmd);
        let m = live.message().unwrap_or_default().0;
        assert!(m.contains(want), "{cmd}: {m:?}");
        assert!(!m.contains('\u{7}'), "{cmd}: {m:?}");
    }
}
