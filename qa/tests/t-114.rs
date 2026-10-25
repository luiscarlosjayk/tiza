//! t-114 — Box title row when the box starts at the first line of the
//! frame (line 1, a `FILE:A-B` range, or the first visible row after a
//! scroll).
//!
//! Acceptance criteria:
//! - AC1 A box whose range starts at the first line of the frame still gets
//!   its title row above it (a virtual row), with the title on the top
//!   border like any other box.
//! - AC2 Same for a box at the first line of a `FILE:A-B` range or of a
//!   `--focus` window; presentation mode and exports agree with the TUI.
//! - AC3 Boxes that start lower are unchanged (t-053 snapshots).
//! - AC4 This file, with a fixture whose box starts at line 1; CHANGELOG.

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::{Live, fixture, scratch, tiza_ok};

fn title_then_code(screen: &str, title: &str, first_code: &str) {
    let lines: Vec<&str> = screen.lines().collect();
    let t = lines
        .iter()
        .position(|l| l.contains(title))
        .unwrap_or_else(|| panic!("no title «{title}»:\n{screen}"));
    // The title sits on the top border (rails of outer arrows may precede it).
    assert!(
        lines[t].contains(title) && lines[t].trim_end().ends_with('╮'),
        "title not on the top border: {}",
        lines[t]
    );
    assert!(
        lines[t + 1].contains(first_code) && lines[t + 1].contains('│'),
        "the code row is not right under the title:\n{screen}"
    );
}

#[test]
fn ac1_ac4_a_box_on_line_1_has_its_title_row_in_the_tui_and_in_render() {
    let fix = fixture("t-114/top.tiza");
    let out = tiza_ok(&["render", fix.to_str().unwrap(), "--width", "60"]);
    title_then_code(&out, "╭─ first", "fn main()");
    assert!(out.trim_start().starts_with("╭─ first"), "{out}");

    let doc = tiza_core::fs::open(fix.to_str().unwrap()).unwrap().document;
    let dir = scratch("t114-top");
    let live = Live::open(doc, dir.join("top.tiza"), dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": 1}));
    let s = live.screen(60, 12);
    title_then_code(&s, "╭─ first", "fn main()");
    // The same with no padding, in presentation mode, and through the API.
    live.command("pad 0 0");
    title_then_code(&live.screen(60, 12), "╭─ first", "fn main()");
    live.keys(&[KeyCode::Char('P')]);
    title_then_code(&live.screen(60, 12), "╭─ first", "fn main()");
    let r = live.api_ok("render.text", json!({"width": 60}));
    title_then_code(r["text"].as_str().unwrap(), "╭─ first", "fn main()");
}

#[test]
fn ac2_a_box_on_the_first_line_of_a_range_or_focus_window() {
    let dir = scratch("t114-range");
    let src = dir.join("main.rs");
    std::fs::copy(fixture("ownership/ownership.rs"), &src).unwrap();
    let opened = tiza_core::fs::open(&format!("{}:15-17", src.display())).unwrap();
    let live = Live::open(
        opened.document,
        dir.join("main.rs.tiza"),
        dir.join("live.sock"),
    );
    live.command("box 15-17 takes");
    live.command("step 1");
    title_then_code(&live.screen(60, 10), "╭─ takes", "fn takes");
    live.keys(&[KeyCode::Char('P')]);
    title_then_code(&live.screen(60, 10), "╭─ takes", "fn takes");
    let out = dir.join("r.txt");
    live.command(&format!("export {}", out.display()));
    title_then_code(
        &std::fs::read_to_string(&out).unwrap(),
        "╭─ takes",
        "fn takes",
    );
    // A --focus window dims the rest but keeps every row: the box at the
    // top of the window still has its title.
    let doc = tiza_core::fs::open(fixture("ownership/ownership.rs.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let l = tiza_core::layout::layout(
        &doc,
        &tiza_core::Theme::dark(),
        &tiza_core::layout::LayoutOptions {
            width: 100,
            step: 7,
            focus: Some((15, 17)),
            ..Default::default()
        },
    );
    title_then_code(&l.canvas.to_plain_text(), "╭─ takes owns s", "fn takes");
}

#[test]
fn ac1_scrolling_keeps_the_title_row_with_the_first_line_of_the_box() {
    // Before t-114 the TUI scrolled the cursor line to the first row and
    // left the box's title row (a virtual row above it) out of the screen.
    let code = (1..=30)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let live = Live::from_code(&code, "txt", "t114-scroll");
    live.command("box 20-22 deep");
    live.command("step 1");
    // Down past the box so its title scrolls out, then back to its first line.
    for _ in 0..27 {
        live.keys(&[KeyCode::Char('j')]);
    }
    let s = live.screen(40, 8);
    assert!(!s.contains("╭─ deep"), "{s}");
    for _ in 0..8 {
        live.keys(&[KeyCode::Char('k')]);
    }
    let s = live.screen(40, 8);
    title_then_code(&s, "╭─ deep", "line 20");
    assert!(
        s.starts_with("     ╭─ deep"),
        "the title is the first row:\n{s}"
    );
    // And the bottom border follows the cursor on the box's last line when
    // it fits in the viewport.
    live.keys(&[KeyCode::Char('j'), KeyCode::Char('j')]);
    let s = live.screen(40, 8);
    assert!(s.contains("╰────"), "{s}");
}
