//! t-096 — Highlight styles: fill, underline, brackets, strike.
//!
//! Acceptance criteria:
//! - AC1 Highlights and marks accept `emphasis`: fill (default tint),
//!   underline (▔ row under the span), brackets (⌈ ⌉ beside the span; a glued
//!   side goes to the row below), strike (combining stroke). Marks keep
//!   glyph + legend.
//! - AC2 `:hl … emphasis=…` (`style=` synonym on :hl only), `:mark … emphasis=…`,
//!   `:set ID emphasis=…`; wrong values list the four options.
//! - AC3 Socket `emphasis` on annotation.add/update; MCP highlight, add_mark,
//!   update_annotation.
//! - AC4 PNG/SVG export renders the same canvas.
//! - AC5 `fill` is not serialized; older documents load unchanged.

use codedraw_core::model::{Emphasis, Shape};
use codedraw_qa::{Live, McpClient, codedraw_ok, scratch};
use serde_json::json;

const CODE: &str = "fn main() {\n    let s = String::from(\"hello\");\n    takes(s);\n    let n = len( &s );\n    println!(\"{n}\");\n}\n";

fn text(live: &Live) -> String {
    live.with(|app| app.layout(70).canvas.to_plain_text())
}

fn emphasis_of(live: &Live, id: &str) -> Emphasis {
    let id = id.to_string();
    live.with(
        move |app| match &app.doc.find_annotation(&id).unwrap().shape {
            Shape::Highlight { emphasis, .. } | Shape::Mark { emphasis, .. } => *emphasis,
            _ => unreachable!(),
        },
    )
}

#[test]
fn step1_underline_row_and_brackets_with_room() {
    let live = Live::from_code(CODE, "rs", "t096-s1");
    live.command("hl 3:takes(s) emphasis=underline");
    live.command("hl 4:&s hooked emphasis=brackets");
    let t = text(&live);
    let lines: Vec<&str> = t.lines().collect();
    let i = lines.iter().position(|l| l.contains("takes(s)")).unwrap();
    assert!(lines[i + 1].contains("▔▔▔▔▔▔▔▔"), "row under line 3: {t}");
    assert!(
        !lines[i + 1].contains("let n"),
        "the under row is virtual: {t}"
    );
    assert!(t.contains("len(⌈&s⌉)"), "{t}");
    assert!(
        !t.contains("⌊"),
        "no fallback needed when there is room: {t}"
    );
}

#[test]
fn step2_glued_brackets_fall_back_to_the_row_below() {
    let live = Live::from_code(CODE, "rs", "t096-s2");
    live.command("hl 3:s emphasis=brackets");
    let t = text(&live);
    assert!(t.contains("takes(s);"), "code intact: {t}");
    let lines: Vec<&str> = t.lines().collect();
    let i = lines.iter().position(|l| l.contains("takes(s)")).unwrap();
    // `3:s` finds the first `s` (inside `takes`): a one-cell span glued on
    // both sides → a plain ▔ under it.
    let col = lines[i].find("takes(s)").unwrap() + 4;
    let under: String = lines[i + 1].chars().collect();
    assert_eq!(under.chars().nth(col), Some('▔'), "{t}");
    // A wider span: the free side keeps ⌈ beside it, the glued side goes below.
    live.command("hl 2:String::from emphasis=brackets");
    let t = text(&live);
    assert!(t.contains("let s =⌈String::from(\"hello\");"), "{t}");
    assert!(t.contains("⌋"), "{t}");
    assert!(!t.contains("⌊"), "{t}");
}

#[test]
fn step3_strike_in_text_and_png() {
    let live = Live::from_code(CODE, "rs", "t096-s3");
    live.command("hl 5:println! gone emphasis=strike");
    let t = text(&live);
    assert!(
        t.contains("p\u{336}r\u{336}i\u{336}n\u{336}t\u{336}l\u{336}n\u{336}!\u{336}"),
        "{t}"
    );
    assert!(t.contains("(\"{n}\");"), "only the span is struck: {t}");
    let dir = scratch("t096-png");
    let doc = dir.join("doc.codedraw");
    live.command(&format!("w {}", doc.display()));
    let png = dir.join("strike.png");
    codedraw_ok(&[
        "render",
        doc.to_str().unwrap(),
        "--out",
        png.to_str().unwrap(),
    ]);
    assert!(std::fs::metadata(&png).unwrap().len() > 1000);
    let svg = dir.join("strike.svg");
    codedraw_ok(&[
        "render",
        doc.to_str().unwrap(),
        "--out",
        svg.to_str().unwrap(),
    ]);
    let svg_text = std::fs::read_to_string(&svg).unwrap();
    assert!(
        svg_text.contains('\u{336}'),
        "the combining stroke reaches the SVG"
    );
}

#[test]
fn step4_set_emphasis_and_errors() {
    let live = Live::from_code(CODE, "rs", "t096-s4");
    live.command("hl 3:takes(s) emphasis=underline");
    assert_eq!(emphasis_of(&live, "a1"), Emphasis::Underline);
    live.command("set a1 emphasis=fill");
    assert_eq!(emphasis_of(&live, "a1"), Emphasis::Fill);
    live.command("set a1 style=strike");
    assert_eq!(
        emphasis_of(&live, "a1"),
        Emphasis::Strike,
        "style= is a synonym on highlights"
    );
    live.command("set a1 emphasis=wave");
    let (msg, is_err) = live.message().unwrap();
    assert!(is_err);
    assert!(msg.contains("fill, underline, brackets, strike"), "{msg}");
    live.command("hl 2:let style=glow");
    assert!(
        live.message()
            .unwrap()
            .0
            .contains("fill, underline, brackets, strike")
    );
    live.command("box 1-2 b");
    live.command("set a2 emphasis=underline");
    let (msg, is_err) = live.message().unwrap();
    assert!(is_err);
    assert!(msg.contains("highlights and marks only"), "{msg}");
}

#[test]
fn step5_socket_mcp_and_serialization() {
    let live = Live::from_code(CODE, "rs", "t096-s5");
    let v = live.api_ok(
        "annotation.add",
        json!({"type": "highlight", "at": {"line": 3, "find": "takes"}, "emphasis": "underline"}),
    );
    let id = v["id"].as_str().unwrap().to_string();
    live.api_ok("annotation.update", json!({"id": id, "emphasis": "strike"}));
    assert_eq!(emphasis_of(&live, &id), Emphasis::Strike);
    let r = live.api("annotation.update", json!({"id": id, "emphasis": "wave"}));
    assert!(r.error.is_some());

    let mut mcp = McpClient::start(&live.socket);
    let (err, t, _) = mcp.call(
        "highlight",
        json!({"at": {"line": 2, "find": "s"}, "emphasis": "brackets"}),
    );
    assert!(!err, "{t}");
    let (err, t, _) = mcp.call(
        "add_mark",
        json!({"kind": "move", "at": {"line": 3, "find": "s"}, "emphasis": "underline"}),
    );
    assert!(!err, "{t}");
    let (err, t, _) = mcp.call("update_annotation", json!({"id": id, "emphasis": "fill"}));
    assert!(!err, "{t}");
    let (err, t, _) = mcp.call(
        "highlight",
        json!({"at": {"line": 2, "find": "let"}, "emphasis": "glow"}),
    );
    assert!(err);
    assert!(t.contains("fill, underline, brackets, strike"), "{t}");

    let scene = live.api_ok("scene.get", json!({}));
    let anns = scene["document"]["annotations"].as_array().unwrap();
    let by_id = |i: &str| anns.iter().find(|a| a["id"] == i).unwrap().clone();
    assert!(
        by_id(&id).get("emphasis").is_none(),
        "fill is not serialized"
    );
    assert_eq!(by_id("a2")["emphasis"], "brackets");
    assert_eq!(by_id("a3")["emphasis"], "underline");
    assert_eq!(by_id("a3")["type"], "mark");
    let listed = live.api_ok("scene.list", json!({}));
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l.as_str().unwrap().contains("highlight (brackets)"))
    );
}

#[test]
fn step6_two_underlines_share_one_row_and_marks_keep_their_legend() {
    let live = Live::from_code(CODE, "rs", "t096-s6");
    live.command("hl 2:let emphasis=underline");
    live.command("mark move 2:s emphasis=underline");
    let t = text(&live);
    let lines: Vec<&str> = t.lines().collect();
    let i = lines
        .iter()
        .position(|l| l.contains("let s = String"))
        .unwrap();
    assert!(
        lines[i + 1].contains("▔▔▔ ▔"),
        "one row, two underlines: {t}"
    );
    assert!(
        lines[i + 2].contains("takes(s)"),
        "exactly one virtual row: {t}"
    );
    assert!(t.contains("━▶ move"), "the mark keeps glyph + legend: {t}");
}

#[test]
fn corner_edges_dimmed_and_reanchor() {
    // Span at column 0 and at the end of the line: bracket sides go below.
    let live = Live::from_code("abc def\n", "rs", "t096-edges");
    live.command("hl 1:abc emphasis=brackets");
    live.command("hl 1:def emphasis=brackets");
    let t = text(&live);
    // `abc` has no cell before it (⌊ below) and takes the shared space for
    // its ⌉; `def` then finds that cell occupied and also opens below.
    assert!(t.contains("abc⌉def⌉"), "{t}");
    assert_eq!(t.matches('⌊').count(), 2, "{t}");
    // Dimmed by a focus: still drawn (in the dimmed color), never panics.
    live.command("focus 1-1");
    live.command("spot a1");
    let _ = text(&live);
    // Paste keeps the emphasis.
    live.with(|app| app.replace_code("// top\nabc def\n").unwrap());
    assert_eq!(emphasis_of(&live, "a1"), Emphasis::Brackets);
    assert_eq!(emphasis_of(&live, "a2"), Emphasis::Brackets);
}

#[test]
fn corner_strike_keeps_grapheme_clusters_and_box_bottom_after_under_row() {
    let live = Live::from_code(
        "fn f() {\n    let x = \"é👍\";\n}\n",
        "rs",
        "t096-graphemes",
    );
    live.command("hl 2:\"é👍\" emphasis=strike");
    let t = text(&live);
    assert!(t.contains("é\u{336}"), "{t}");
    assert!(t.contains("👍\u{336}"), "one stroke per grapheme: {t}");
    live.command("box 1-2 b");
    live.command("hl 2:let emphasis=underline");
    let t = text(&live);
    let lines: Vec<&str> = t.lines().collect();
    let i = lines.iter().position(|l| l.contains("let x")).unwrap();
    assert!(lines[i + 1].contains("▔▔▔"), "{t}");
    assert!(
        lines[i + 2].contains("╰") || lines[i + 2].contains("└"),
        "box bottom after the under row: {t}"
    );
}
