//! `get_code` and `tiza://code` name the characters of the code that the
//! screen does not show as they are. The screen drew a zero-width space or
//! a stray ZWJ as ⬚, but the text replies gave it raw with no sign, so an
//! agent did not see what the presenter sees.
//!
//! Acceptance criteria (Luis's decision (b), 2026-10-30):
//! - AC1 The text stays the real text, so a `find` works as before.
//! - AC2 `code.get` gives `invisible`, one `{line, col, code_point, name}`
//!   for each character that the screen shows as ⬚ or as a control
//!   picture, in order; `col` is the grapheme from 0, as the `col` of an
//!   anchor. A character that does a job (an emoji ZWJ, the ZWNJ of a
//!   Persian word), a tab and the byte order mark that starts a file get
//!   nothing.
//! - AC3 `get_code` and `tiza://code` add one note for each line with such
//!   characters after the code, `# line N: U+200B zero width space, col 5`;
//!   with none, the reply is the code alone.
//! - AC4 The notes follow the view of the step: a `code.link` and its
//!   window (the lines of the file), and the patches of the step.
//! - AC5 The notes and the ⬚ of the screen agree; the column and the code
//!   point of a note are what an anchor needs.

use serde_json::json;
use tiza_core::model::grapheme_byte_offset;
use tiza_qa::{Live, McpClient, scratch, tiza_ok};

const HEAD: &str = "\n# The code above holds these characters, but you cannot see them or it \
                    shows a picture. col is the grapheme, from 0. A find for a text with one \
                    of them must hold the real character, for example \"ad\\u200bmin\" in \
                    JSON. To point at the character itself, use line and col.\n";

/// `code.get`'s `invisible` as `(line, col, "U+XXXX name")`.
fn invisible(live: &Live) -> Vec<(u64, u64, String)> {
    let v = live.api_ok("code.get", json!({}));
    v["invisible"]
        .as_array()
        .unwrap_or_else(|| panic!("no invisible: {v}"))
        .iter()
        .map(|i| {
            (
                i["line"].as_u64().unwrap(),
                i["col"].as_u64().unwrap(),
                format!(
                    "{} {}",
                    i["code_point"].as_str().unwrap(),
                    i["name"].as_str().unwrap()
                ),
            )
        })
        .collect()
}

fn entry(line: u64, col: u64, what: &str) -> (u64, u64, String) {
    (line, col, what.to_string())
}

fn get_code(mcp: &mut McpClient) -> String {
    let (err, text, _) = mcp.call("get_code", json!({}));
    assert!(!err, "{text}");
    text
}

fn code_resource(mcp: &mut McpClient) -> String {
    let v = mcp.request("resources/read", json!({"uri": "tiza://code"}));
    v["result"]["contents"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("{v}"))
        .to_string()
}

/// The code listing of `code.get` as a text reply shows it.
fn listing(live: &Live) -> String {
    let v = live.api_ok("code.get", json!({}));
    tiza_core::printable::printable_text(v["text"].as_str().unwrap()).into_owned()
}

const MATRIX: [&str; 13] = [
    "let ad\u{200B}min = 1;",
    "let ad\u{200D}min = 2;",
    "let f = \"\u{1F468}\u{200D}\u{1F469}\";",
    "let p = \"\u{0645}\u{06CC}\u{200C}\u{062E}\";",
    // Not ASCII, so the tab is looked at.
    "\tlet tab = \"ñ\";",
    "let e = \"a\x1bb\0\x7f\u{9b}\";",
    "x\u{2060}y\u{FEFF}z",
    "fn f() {}",
    "let end = 1;\u{200B}",
    "\u{200B}\u{2060}",
    "let r = \"\u{202E}abc\";",
    // A tab and a wide character are one grapheme each, not their cells.
    "\tx\u{200B}",
    "中\u{200B}",
];

#[test]
fn each_line_with_invisible_characters_gets_a_note() {
    let live = Live::from_code(&(MATRIX.join("\n") + "\n"), "rs", "invisible");
    assert_eq!(
        invisible(&live),
        [
            entry(1, 6, "U+200B zero width space"),
            // A stray joiner is in the grapheme of the `d` before it.
            entry(2, 5, "U+200D zero width joiner"),
            entry(6, 10, "U+001B escape"),
            entry(6, 12, "U+0000 null"),
            entry(6, 13, "U+007F delete"),
            entry(6, 14, "U+009B C1 control"),
            entry(7, 1, "U+2060 word joiner"),
            entry(7, 3, "U+FEFF byte order mark"),
            // At the end of the line: the number of graphemes before it.
            entry(9, 12, "U+200B zero width space"),
            entry(10, 0, "U+200B zero width space"),
            entry(10, 1, "U+2060 word joiner"),
            entry(11, 9, "U+202E right-to-left override"),
            entry(12, 2, "U+200B zero width space"),
            entry(13, 1, "U+200B zero width space"),
        ]
    );
    // The text keeps the real characters.
    let v = live.api_ok("code.get", json!({}));
    let text = v["text"].as_str().unwrap();
    for line in MATRIX {
        assert!(text.contains(line), "{line:?} not in {text:?}");
    }
    let note = format!(
        "{HEAD}\
         # line 1: U+200B zero width space, col 6\n\
         # line 2: U+200D zero width joiner, col 5\n\
         # line 6: U+001B escape, col 10; U+0000 null, col 12; U+007F delete, col 13; U+009B C1 control, col 14\n\
         # line 7: U+2060 word joiner, col 1; U+FEFF byte order mark, col 3\n\
         # line 9: U+200B zero width space, col 12\n\
         # line 10: U+200B zero width space, col 0; U+2060 word joiner, col 1\n\
         # line 11: U+202E right-to-left override, col 9\n\
         # line 12: U+200B zero width space, col 2\n\
         # line 13: U+200B zero width space, col 1\n"
    );
    let mut mcp = McpClient::start(&live.socket);
    let code = get_code(&mut mcp);
    assert_eq!(code, format!("{}{note}", listing(&live)));
    // The code before the note is as before: controls as their pictures,
    // the zero-width characters raw.
    assert!(code.contains("let e = \"a␛b␀␡\u{FFFD}\";"), "{code}");
    assert!(code.contains("let ad\u{200B}min = 1;"), "{code}");
    assert_eq!(code_resource(&mut mcp), code);
    // `tiza api code.get` gives the same list.
    let out = tiza_ok(&["api", "code.get", "--socket", live.socket.to_str().unwrap()]);
    let cli: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(cli["invisible"], v["invisible"]);
}

/// Whether the note `U+XXXX name` is of a C0, DEL or C1 control.
fn is_control(what: &str) -> bool {
    let hex = what[2..].split(' ').next().unwrap();
    u32::from_str_radix(hex, 16).unwrap() < 0xA0
}

#[test]
fn the_notes_and_the_marks_of_the_screen_agree() {
    let live = Live::from_code(&(MATRIX.join("\n") + "\n"), "rs", "agree");
    let frame = live.api_ok("render.text", json!({}));
    let frame = frame["text"].as_str().unwrap();
    for (n, line) in MATRIX.iter().enumerate() {
        let n = n as u64 + 1;
        // A control character shows as its picture, not as ⬚.
        let notes = invisible(&live)
            .into_iter()
            .filter(|(l, _, what)| *l == n && !is_control(what))
            .count();
        // The frame row of the line: its number, then the code.
        let row = frame
            .lines()
            .find(|r| r.trim_start().starts_with(&format!("{n} ")))
            .unwrap_or_else(|| panic!("line {n} not in {frame}"));
        assert_eq!(row.matches('\u{2B1A}').count(), notes, "{line:?}: {row}");
    }
}

#[test]
fn code_without_them_is_the_code_alone() {
    let live = Live::from_code("fn main() {\n\tlet ñ = \"中\";\n}\n", "rs", "none");
    assert_eq!(invisible(&live), []);
    let mut mcp = McpClient::start(&live.socket);
    let code = get_code(&mut mcp);
    assert_eq!(code, listing(&live));
    assert!(!code.contains('#'), "{code}");
    assert_eq!(code_resource(&mut mcp), code);
}

#[test]
fn a_linked_file_gives_the_lines_of_the_file() {
    let dir = scratch("invisible-link");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    // The byte order mark that starts the file is dropped on reading.
    std::fs::write(
        dir.join("src/other.rs"),
        "\u{FEFF}// head\nfn b() {}\nlet x\u{200B}y = 1;\nfn c() {}\n",
    )
    .unwrap();
    let path = dir.join("doc.tiza");
    let doc = json!({
        "schema": 1, "source": {"language": "rs"}, "code": ["fn a() {}"],
        "moves": [
            {"at_step": 1, "action": "code.link", "file": "src/other.rs"},
            {"at_step": 2, "action": "code.link", "file": "src/other.rs", "lines": "3-4"}
        ]
    });
    std::fs::write(&path, doc.to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(doc, path, dir.join("live.sock"));
    let mut mcp = McpClient::start(&live.socket);
    // A document opens at step 1: step 0 shows the code before the moves.
    live.api_ok("step.set", json!({"step": 0}));
    assert_eq!(invisible(&live), []);
    for step in [1, 2] {
        live.api_ok("step.set", json!({"step": step}));
        assert_eq!(
            invisible(&live),
            [entry(3, 5, "U+200B zero width space")],
            "step {step}"
        );
        let view = if step == 1 {
            "src/other.rs"
        } else {
            "src/other.rs:3-4"
        };
        let note = format!("{HEAD}# line 3: U+200B zero width space, col 5\n");
        let code = get_code(&mut mcp);
        assert_eq!(code, format!("# {view}\n{}{note}", listing(&live)));
        assert_eq!(code_resource(&mut mcp), format!("{}{note}", listing(&live)));
    }
}

#[test]
fn the_patches_of_the_step_bring_and_take_the_note() {
    let dir = scratch("invisible-patch");
    let path = dir.join("doc.tiza");
    let doc = json!({
        "schema": 1, "source": {"language": "rs"}, "code": ["fn a() {", "}"],
        "moves": [
            {"at_step": 1, "action": "code.patch", "op": "insert", "position": "2",
             "content": "    let x\u{200B}y = 1;"},
            {"at_step": 2, "action": "code.patch", "op": "delete", "find": "let x"}
        ]
    });
    std::fs::write(&path, doc.to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(doc, path, dir.join("live.sock"));
    let mut mcp = McpClient::start(&live.socket);
    // A document opens at step 1: step 0 shows the code before the moves.
    live.api_ok("step.set", json!({"step": 0}));
    assert_eq!(invisible(&live), []);
    live.api_ok("step.set", json!({"step": 1}));
    assert_eq!(invisible(&live), [entry(2, 9, "U+200B zero width space")]);
    let code = get_code(&mut mcp);
    assert!(
        code.ends_with(&format!("{HEAD}# line 2: U+200B zero width space, col 9\n")),
        "{code}"
    );
    live.api_ok("step.set", json!({"step": 2}));
    assert_eq!(invisible(&live), []);
    assert_eq!(get_code(&mut mcp), listing(&live));
}

/// The grapheme `i` of `s`, from 0.
fn grapheme(s: &str, i: usize) -> &str {
    let at = |i: usize| grapheme_byte_offset(s, i as u32).unwrap_or(s.len());
    &s[at(i)..at(i + 1)]
}

/// The span that `find` gives for a text.
fn found_span(live: &Live, line: u64, find: &str) -> (u32, u32) {
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "highlight", "at": {"line": line, "find": find}}),
    );
    let id = r["id"].as_str().unwrap().to_string();
    live.with(move |a| match &a.doc.find_annotation(&id).unwrap().shape {
        tiza_core::model::Shape::Highlight { span, .. } => (span.line, span.start),
        s => panic!("{s:?}"),
    })
}

#[test]
fn a_note_gives_what_an_anchor_needs() {
    let live = Live::from_code(&(MATRIX.join("\n") + "\n"), "rs", "anchor");
    let v = live.api_ok("code.get", json!({}));
    // The grapheme at the column of each note holds its character, so
    // `line` and `col` point at it.
    for n in v["invisible"].as_array().unwrap() {
        let line = MATRIX[n["line"].as_u64().unwrap() as usize - 1];
        let col = n["col"].as_u64().unwrap() as usize;
        let hex = &n["code_point"].as_str().unwrap()[2..];
        let c = char::from_u32(u32::from_str_radix(hex, 16).unwrap()).unwrap();
        assert!(grapheme(line, col).contains(c), "{n}: {line:?}");
    }
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "highlight", "at": {"line": 2, "col": 5, "end": 6}}),
    );
    assert_eq!(r["visible"], json!(true), "{r}");
    // A find for a text with the real character starts at its grapheme.
    let notes = invisible(&live);
    for (line, find, c) in [
        (1, "ad\u{200B}min", '\u{200B}'),
        (2, "d\u{200D}", '\u{200D}'),
        (9, "\u{200B}", '\u{200B}'),
        (10, "\u{2060}", '\u{2060}'),
        (12, "x\u{200B}", '\u{200B}'),
    ] {
        let code_point = format!("U+{:04X}", c as u32);
        let col = notes
            .iter()
            .find(|(l, _, what)| *l == line && what.starts_with(&code_point))
            .unwrap_or_else(|| panic!("no {code_point} on line {line}: {notes:?}"))
            .1;
        let (l, start) = found_span(&live, line, find);
        // The grapheme of the character in the text of the find.
        let within = (0..find.len())
            .find(|&i| grapheme(find, i).contains(c))
            .unwrap() as u64;
        assert_eq!(
            (u64::from(l), u64::from(start) + within),
            (line, col),
            "{find:?}"
        );
    }
    // The picture of a control character is not the character.
    let r = live.api(
        "annotation.add",
        json!({"type": "highlight", "at": {"line": 6, "find": "a␛b"}}),
    );
    assert!(r.error.is_some(), "{r:?}");
}

#[test]
fn a_line_break_in_a_line_of_embedded_code_is_named() {
    let dir = scratch("invisible-break");
    let path = dir.join("doc.tiza");
    let doc = json!({
        "schema": 1, "source": {"language": "rs"},
        "code": ["let s = \"a\r\nb\";", "x\ny"]
    });
    std::fs::write(&path, doc.to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(doc, path, dir.join("live.sock"));
    // `\r\n` is one grapheme: two characters with one column.
    assert_eq!(
        invisible(&live),
        [
            entry(1, 10, "U+000D carriage return"),
            entry(1, 10, "U+000A line feed"),
            entry(2, 1, "U+000A line feed"),
        ]
    );
}
