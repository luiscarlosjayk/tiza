//! t-182 — Control characters in the code (ESC, NUL, BEL…) no longer break
//! exports nor reach a terminal raw.
//!
//! Acceptance criteria:
//! - AC1 Every export (svg, png, gif, txt, md, ansi, cast, tape,
//!   `render.text`, MCP `export_image` and resources) and the TUI draw the
//!   control characters of the code as visible one-cell placeholders (C0 →
//!   `␀`…`␟`, DEL → `␡`, C1 → `�`), never as raw bytes; bidi controls never
//!   reorder the line (since t-190 each shows as a one-cell ⬚ in the code);
//!   the SVG is valid XML; PNG/GIF are written. Annotation text is never
//!   drawn raw either (since t-190 its controls show as their pictures).
//! - AC2 Errors never quote a control character raw (the CLI, `tiza check`,
//!   the MCP), after the t-167 filter.
//! - AC3 `.ansi` and `.cast` hold only the escape sequences Tiza writes
//!   (SGR colours, and clear/home in a cast frame); a title cannot start a
//!   line of a `.tape`.
//! - AC4 This test with ESC (OSC and CSI), NUL, BEL, DEL, C1 and a bidi
//!   override; the CHANGELOG.
//!
//! Corner cases: tabs keep their handling (expanded to spaces in the code);
//! a wide character next to a placeholder keeps the columns aligned.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, scratch, tiza, tiza_ok};

/// Every kind of control character the card names, one per line.
const CODE: &str = concat!(
    "a\x1b]0;pwned\x07b\n",           // OSC: retitles the window
    "let c = \"\x1b[2J\x1b[H\";\n",   // CSI: clears the screen
    "let n = \"\0\x7f\";\n",          // NUL, DEL
    "let k = \"\u{9b}31m\u{85}\";\n", // C1 CSI, NEL
    "\u{202E}abc\u{202C} = 1\n",      // bidi override (Trojan Source)
    "\tx = 2\n",                      // tab
);

/// A character a terminal acts on or XML forbids, other than the newline
/// and the tab (a carriage return counts: it rewrites the line).
fn unsafe_char(c: char) -> bool {
    (c < ' ' && !matches!(c, '\n' | '\t'))
        || ('\u{7f}'..='\u{9f}').contains(&c)
        || matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

fn assert_clean(what: &str, s: &str) {
    let bad: Vec<String> = s
        .chars()
        .filter(|c| unsafe_char(*c))
        .map(|c| format!("U+{:04X}", c as u32))
        .collect();
    assert!(bad.is_empty(), "{what} holds raw {bad:?}:\n{s}");
}

/// `s` without the escape sequences Tiza writes: CSI sequences whose final
/// byte is in `finals` and whose parameters are digits and `;`. Any other
/// ESC (an OSC, a CSI with another final, a bare ESC) fails the test.
fn strip_tiza_sequences(what: &str, s: &str, finals: &[char]) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        assert_eq!(it.next(), Some('['), "{what}: an ESC that is not a CSI");
        let mut params = String::new();
        loop {
            let n = it.next().unwrap_or_else(|| panic!("{what}: CSI cut short"));
            if n.is_ascii_digit() || n == ';' {
                params.push(n);
            } else {
                assert!(
                    finals.contains(&n),
                    "{what}: CSI `{params}{n}` is not one Tiza writes"
                );
                break;
            }
        }
    }
    out
}

fn write(dir: &Path, name: &str, text: &str) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, text).unwrap();
    p
}

/// The placeholders every rendering of [`CODE`] shows.
fn assert_placeholders(what: &str, s: &str) {
    for want in [
        "a␛]0;pwned␇b",
        "\"␛[2J␛[H\"",
        "\"␀␡\"",
        "\"�31m�\"",
        "⬚abc⬚ = 1",
    ] {
        assert!(s.contains(want), "{what}: no `{want}` in\n{s}");
    }
}

#[test]
fn ac1_test_step_1_ansi_shows_placeholders_and_cannot_retitle_the_terminal() {
    let dir = scratch("t182-ansi");
    let f = write(&dir, "f.rs", CODE);
    let ansi = tiza_ok(&["render", f.to_str().unwrap(), "--format", "ansi"]);
    // AC3: only Tiza's own SGR sequences; everything else is text.
    let text = strip_tiza_sequences("ansi", &ansi, &['m']);
    assert_clean("ansi", &text);
    assert_placeholders("ansi", &text);
}

#[test]
fn ac1_txt_md_and_render_text_show_placeholders() {
    let dir = scratch("t182-txt");
    let f = write(&dir, "f.rs", CODE);
    for format in ["txt", "md"] {
        let out = tiza_ok(&["render", f.to_str().unwrap(), "--format", format]);
        assert_clean(format, &out);
        assert_placeholders(format, &out);
    }
}

#[test]
fn ac1_test_step_2_svg_is_valid_xml_and_png_gif_are_written() {
    let dir = scratch("t182-img");
    let f = write(&dir, "f.rs", CODE);
    let svg = dir.join("x.svg");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        svg.to_str().unwrap(),
    ]);
    let text = std::fs::read_to_string(&svg).unwrap();
    // XML 1.0 allows no C0 but tab, LF and CR, and no U+FFFE/U+FFFF.
    assert_clean("svg", &text);
    assert!(!text.contains(['\u{FFFE}', '\u{FFFF}']));
    // (Runs split at each syntax colour.)
    for want in ["␛", "pwned␇b", "␀␡", "�31m�"] {
        assert!(text.contains(want), "the SVG shows `{want}`");
    }
    if Path::new("/usr/bin/xmllint").exists() {
        let out = std::process::Command::new("/usr/bin/xmllint")
            .args(["--noout", svg.to_str().unwrap()])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "xmllint: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    for (ext, magic) in [("png", &b"\x89PNG"[..]), ("gif", &b"GIF89a"[..])] {
        let out = dir.join(format!("x.{ext}"));
        let run = tiza(&[
            "render",
            f.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]);
        let err = String::from_utf8_lossy(&run.stderr);
        assert!(run.status.success(), "{ext}: {err}");
        assert_clean(&format!("{ext} stderr"), &err);
        assert!(std::fs::read(&out).unwrap().starts_with(magic), "{ext}");
    }
}

#[test]
fn ac1_annotation_text_is_never_drawn_raw() {
    let dir = scratch("t182-labels");
    let doc = json!({
        "schema": 1,
        "title": "T\u{1b}]0;pwned\u{7}",
        "source": {"language": "rs", "first_line": 1},
        "code": ["let a = 1;", "let b = 2;"],
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 1,
             "text": "note \u{1b}[31mred\u{9b}\u{202E}evil"},
            {"id": "h1", "step": 1, "type": "highlight",
             "span": {"line": 2, "start": 4, "end": 5}, "label": "lbl\u{7}\u{202E}x"},
            {"id": "c1", "step": 1, "type": "callout", "from_line": 1, "to_line": 2,
             "title": "t\u{1b}", "text": "body\u{1b}]0;x\u{7}\nline two"}
        ]
    });
    let f = write(&dir, "labels.tiza", &doc.to_string());
    for format in ["txt", "md", "ansi"] {
        let out = tiza_ok(&["render", f.to_str().unwrap(), "--format", format]);
        let text = strip_tiza_sequences(format, &out, &['m']);
        assert_clean(format, &text);
        assert!(text.contains("red"), "{format}: the note is drawn:\n{text}");
    }
    let svg = dir.join("x.svg");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        svg.to_str().unwrap(),
    ]);
    let text = std::fs::read_to_string(&svg).unwrap();
    assert_clean("svg", &text);
    assert!(text.contains("<title>T␛]0;pwned␇</title>"), "svg title");
}

#[test]
fn ac1_test_step_3_the_tui_shows_the_placeholders() {
    let live = Live::from_code(CODE, "rs", "t182-tui");
    let screen = live.screen(100, 12);
    assert_clean("TUI", &screen);
    assert_placeholders("TUI", &screen);
}

#[test]
fn ac1_ac2_mcp_render_resources_exports_and_errors() {
    let live = Live::from_code(CODE, "rs", "t182-mcp");
    let dir = scratch("t182-mcp-out");
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("render_text", json!({}));
    assert!(!err, "{text}");
    assert_clean("render_text", &text);
    assert_placeholders("render_text", &text);
    let (err, text, _) = mcp.call("get_code", json!({}));
    assert!(!err, "{text}");
    assert_clean("get_code", &text);
    assert!(text.contains("a␛]0;pwned␇b"), "{text}");
    for uri in [
        "tiza://code",
        "tiza://render",
        "tiza://render.md",
        "tiza://scene",
    ] {
        let r = mcp.request("resources/read", json!({"uri": uri}));
        let text = r["result"]["contents"][0]["text"]
            .as_str()
            .unwrap_or_default();
        assert!(!text.is_empty(), "{uri}: {r}");
        assert_clean(uri, text);
    }
    // The scene's JSON keeps the value: the C1 and bidi controls are escaped.
    let r = mcp.request("resources/read", json!({"uri": "tiza://scene"}));
    let scene: Value =
        serde_json::from_str(r["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(scene["document"]["code"][3], "let k = \"\u{9b}31m\u{85}\";");
    for ext in ["txt", "ansi", "md", "svg"] {
        let path = dir.join(format!("x.{ext}"));
        let (err, text, _) = mcp.call("export_image", json!({"path": path.to_str().unwrap()}));
        assert!(!err, "{ext}: {text}");
        let out = std::fs::read_to_string(&path).unwrap();
        let out = strip_tiza_sequences(ext, &out, &['m']);
        assert_clean(&format!("export_image .{ext}"), &out);
        assert!(out.contains("pwned␇b"), "{ext}");
    }
    for ext in ["png", "gif"] {
        let path = dir.join(format!("x.{ext}"));
        let (err, text, _) = mcp.call("export_image", json!({"path": path.to_str().unwrap()}));
        assert!(!err, "{ext}: {text}");
        assert!(path.exists(), "{ext}");
    }
    // AC2: a find that matches nothing is answered without the raw ESC
    // (the instance quotes it with Rust escapes).
    let (err, text, raw) = mcp.call("highlight", json!({"at": {"find": "zz\u{1b}]0;nope\u{7}"}}));
    assert!(err, "{raw}");
    assert_clean("MCP error", &text);
    assert!(
        text.contains(r#"text "zz\u{1b}]0;nope\u{7}" not found in the code"#),
        "{text}"
    );
    // A field name serde quotes back (`unknown field`) is shown.
    let (err, text, raw) = mcp.call(
        "update_annotation",
        json!({"id": "a1", "x\u{1b}]0;f\u{7}": 1}),
    );
    assert!(err, "{raw}");
    assert_clean("MCP parameter error", &text);
    assert!(text.contains("unknown field `x␛]0;f␇`"), "{text}");
    // An export's error quotes the path the agent sent: shown.
    let bad = dir.join("no\u{1b}]0;dir\u{7}").join("x.png");
    let (err, text, raw) = mcp.call("export_image", json!({"path": bad.to_str().unwrap()}));
    assert!(err, "{raw}");
    assert_clean("MCP export error", &text);
    assert!(
        text.contains("could not write") && text.contains("no␛]0;dir␇"),
        "{text}"
    );
}

#[test]
fn ac2_theme_notes_quote_the_overrides_without_raw_controls() {
    let dir = scratch("t182-theme");
    let doc = r##"{"schema": 1, "source": {"language": "rs", "first_line": 1}, "code": ["let a = 1;"],
        "theme_overrides": {"fg\u001b]0;THEMEKEY\u0007": "#fff", "accent": "\u001b]0;V\u0007", "x\u001b[31m": {}}}"##;
    let f = write(&dir, "th.tiza", doc);
    let run = tiza(&["render", f.to_str().unwrap(), "--format", "txt"]);
    assert!(run.status.success());
    let err = String::from_utf8_lossy(&run.stderr);
    assert_clean("theme notes", &err);
    for want in ["fg␛]0;THEMEKEY␇", "accent: ␛]0;V␇", "[x␛[31m]"] {
        assert!(err.contains(want), "no `{want}` in\n{err}");
    }
}

#[test]
fn ac3_a_tape_is_refused_for_a_path_vhs_would_read_as_commands() {
    let dir = scratch("t182-tape-path");
    // A document named so that its tape line would type and run more.
    let evil = "ev\nType \"touch PWNED\"\nEnter\n\u{1b}]0;x\u{7}.rs";
    let f = write(&dir, evil, "let a = 1;\n");
    let tape = dir.join("x.tape");
    let run = tiza(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        tape.to_str().unwrap(),
    ]);
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success(), "a tape was written:\n{err}");
    assert!(!tape.exists(), "nothing is written");
    assert_clean("tape refusal", &err);
    assert!(
        err.contains("cannot write a .tape for the document path"),
        "{err}"
    );
    // The output path, with a `"` that would close VHS's `Output "…"`.
    let ok = write(&dir, "ok.rs", "let a = 1;\n");
    let out = dir.join("o\"x.tape");
    let run = tiza(&[
        "render",
        ok.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success(), "{err}");
    assert!(!out.exists());
    assert!(
        err.contains("cannot write a .tape for the output path"),
        "{err}"
    );
    // A plain path still works.
    let good = dir.join("good.tape");
    tiza_ok(&[
        "render",
        ok.to_str().unwrap(),
        "--out",
        good.to_str().unwrap(),
    ]);
    assert!(good.exists());
}

#[test]
fn ac2_cli_errors_and_check_never_print_a_raw_control() {
    let dir = scratch("t182-errors");
    let doc = json!({
        "schema": 1,
        "source": {"language": "rs", "first_line": 1},
        "code": ["let a = 1;"],
        "moves": [
            {"at_step": 2, "action": "code.patch", "op": "replace",
             "find": "zz\u{1b}]0;pwned\u{7}", "content": "x"}
        ]
    });
    let f = write(&dir, "bad.tiza", &doc.to_string());
    let run = tiza(&["check", f.to_str().unwrap()]);
    assert!(!run.status.success());
    let out = String::from_utf8_lossy(&run.stdout);
    assert_clean("tiza check", &out);
    assert!(out.contains("zz␛]0;pwned␇"), "{out}");
    let run = tiza(&["render", f.to_str().unwrap(), "--format", "txt"]);
    assert!(!run.status.success());
    let err = String::from_utf8_lossy(&run.stderr);
    assert_clean("tiza render error", &err);
    assert!(err.contains("zz␛]0;pwned␇"), "{err}");

    // An error from the live instance, through `tiza api`.
    let live = Live::from_code(CODE, "rs", "t182-api");
    let sock = live.socket.to_str().unwrap().to_string();
    let run = tiza(&[
        "api",
        "annotation.add",
        &json!({"type": "highlight", "at": {"find": "q\u{1b}]0;x\u{7}"}}).to_string(),
        "--socket",
        &sock,
    ]);
    assert!(!run.status.success());
    let err = String::from_utf8_lossy(&run.stderr);
    assert_clean("tiza api error", &err);
    assert!(
        err.contains(r#"text "q\u{1b}]0;x\u{7}" not found in the code"#),
        "{err}"
    );
    // `tiza api` JSON keeps the value with every control escaped.
    let out = tiza_ok(&["api", "code.get", "--socket", &sock]);
    assert_clean("tiza api code.get", &out);
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(v["text"].as_str().unwrap().contains("\u{9b}31m\u{85}"));
    let out = tiza_ok(&["scene", "text", "--socket", &sock]);
    assert_clean("tiza scene text", &out);
    assert_placeholders("tiza scene text", &out);
}

#[test]
fn ac3_cast_frames_hold_only_tiza_sequences_and_the_tape_title_one_line() {
    let dir = scratch("t182-cast");
    let doc = json!({
        "schema": 1,
        "title": "T\u{1b}]0;pwned\u{7}\nType \"rm -rf x\"\u{9b}",
        "source": {"language": "rs", "first_line": 1},
        "code": CODE.lines().collect::<Vec<_>>(),
    });
    let f = write(&dir, "rec.tiza", &doc.to_string());
    let cast = dir.join("x.cast");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        cast.to_str().unwrap(),
    ]);
    let cast = std::fs::read_to_string(&cast).unwrap();
    let mut frames = 0;
    for line in cast.lines() {
        let v: Value = serde_json::from_str(line).unwrap();
        if let Some(t) = v["title"].as_str() {
            assert_clean("cast title", t);
        }
        if v[1] == "o" {
            frames += 1;
            let data = v[2].as_str().unwrap();
            // Tiza's frame: background SGR, clear (J), home (H), then SGR.
            let text = strip_tiza_sequences("cast frame", data, &['m', 'J', 'H']);
            // Its rows end in CRLF, Tiza's own; any other CR is not.
            assert_clean("cast frame", &text.replace("\r\n", "\n"));
            assert!(text.contains("a␛]0;pwned␇b"), "{text}");
        }
    }
    assert!(frames > 0);
    let tape = dir.join("x.tape");
    tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--out",
        tape.to_str().unwrap(),
    ]);
    let tape = std::fs::read_to_string(&tape).unwrap();
    assert_clean("tape", &tape);
    assert!(
        !tape.lines().any(|l| l.starts_with("Type \"rm")),
        "the title started a tape command:\n{tape}"
    );
    assert!(tape.contains("# T␛]0;pwned␇␊Type \"rm -rf x\"�"), "{tape}");
}

#[test]
fn corner_tabs_keep_their_handling() {
    let dir = scratch("t182-tab");
    let f = write(&dir, "f.rs", "\tx = 2\n  \ty\n");
    let txt = tiza_ok(&["render", f.to_str().unwrap(), "--format", "txt"]);
    assert!(!txt.contains('␉'), "a tab is never a placeholder:\n{txt}");
    assert!(!txt.contains('\t'), "the code's tabs are expanded:\n{txt}");
    let rows: Vec<&str> = txt.lines().collect();
    // Tab stops of 4: both texts start at the same column.
    let col = |r: &str, ch: char| r.chars().position(|c| c == ch).unwrap();
    assert_eq!(col(rows[0], 'x'), col(rows[1], 'y'), "{txt}");
}

#[test]
fn corner_wide_characters_next_to_placeholders_keep_alignment() {
    let dir = scratch("t182-wide");
    // 中 is two cells, ␛ one: x sits 3 cells into the code on both lines.
    let doc = json!({
        "schema": 1,
        "source": {"language": "txt", "first_line": 1},
        "code": ["中\u{1b}x = 1", "abcx = 2"],
        "annotations": [
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 1, "start": 2, "end": 3}},
            {"id": "h2", "step": 1, "type": "highlight", "span": {"line": 2, "start": 3, "end": 4}}
        ]
    });
    let f = write(&dir, "wide.tiza", &doc.to_string());
    let ansi = tiza_ok(&["render", f.to_str().unwrap(), "--format", "ansi"]);
    let mut cols = Vec::new();
    for row in ansi.lines().take(2) {
        // The highlight starts a new SGR run right before the `x`.
        let x_at = row.find('x').unwrap();
        assert!(
            row[..x_at].ends_with('m'),
            "the highlight is on the x: {row:?}"
        );
        let before = strip_tiza_sequences("row", &row[..x_at], &['m']);
        let cells: usize = before.chars().map(|c| if c == '中' { 2 } else { 1 }).sum();
        cols.push(cells);
    }
    assert_eq!(cols[0], cols[1], "x is in the same column on both lines");
    let txt = tiza_ok(&["render", f.to_str().unwrap(), "--format", "txt"]);
    assert!(txt.contains("中␛x = 1"), "{txt}");
}
