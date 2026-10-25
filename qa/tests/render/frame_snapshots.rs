//! Render snapshots with ratatui's TestBackend and insta.
//!
//! Acceptance criteria:
//! - AC1 Every annotation kind (highlight, mark, box, arrow between lines and
//!   on one line, note in the margin and below, callout in the margin and
//!   below, spotlight) has a snapshot of its symbols and one of its styles
//!   (fg/bg/modifiers), because Display of the TestBackend carries no color.
//! - AC2 Presentation mode, step 0, the light theme, a V-LINE selection, a
//!   Tab-selected annotation and the ⚠ small-terminal badge are snapshotted too.
//! - AC3 Snapshots live in `qa/tests/snapshots/`; a change in the render fails
//!   the test and `cargo insta review` (or INSTA_UPDATE=always) accepts it.
//!
//! Sizes: 100×30 (recording profile) and 60×20 (narrow, labels under lines).

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::{Live, fixture};

fn open(fix: &str, name: &str, step: u32) -> Live {
    let dir = tiza_qa::scratch(name);
    let path = fixture(fix);
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": step}));
    live
}

fn snap(name: &str, live: &Live, cols: u16, rows: u16) {
    let (symbols, styles) = live.snapshot(cols, rows);
    insta::assert_snapshot!(format!("{name}-symbols"), symbols);
    insta::assert_snapshot!(format!("{name}-styles"), styles);
}

#[test]
fn every_annotation_kind() {
    // ownership: highlights, marks, boxes, arrows (lanes + same line), notes, spotlights, callout below.
    for step in [1u32, 2, 3, 5, 7] {
        let live = open(
            "shared/ownership/ownership.rs.tiza",
            &format!("own-{step}"),
            step,
        );
        snap(&format!("ownership-step{step}-100x30"), &live, 100, 30);
    }
    let live = open("shared/ownership/ownership.rs.tiza", "own-narrow", 5);
    snap("ownership-step5-60x20", &live, 60, 20);
    // callouts in the margin and below, labels beside the block
    let live = open("render/callouts/callouts.tiza", "callout-margin", 1);
    snap("callout-margin-114x14", &live, 114, 14);
    let live = open("render/callouts/callouts.tiza", "callout-below", 3);
    snap("callout-below-80x20", &live, 80, 20);
    // every arrow and border style
    let live = open("render/arrow_styles/arrows.tiza", "arrows", 1);
    snap("arrow-styles-90x24", &live, 90, 24);
    let live = open("render/border_styles/borders.tiza", "borders", 1);
    snap("border-styles-70x30", &live, 70, 30);
}

#[test]
fn modes_themes_and_states() {
    let live = open("shared/ownership/ownership.rs.tiza", "step0", 0);
    snap("ownership-step0-100x30", &live, 100, 30);
    let live = open("shared/ownership/ownership.rs.tiza", "present", 3);
    live.keys(&[KeyCode::Char('P')]);
    snap("ownership-step3-presentation-100x30", &live, 100, 30);
    let live = open("shared/ownership/ownership.rs.tiza", "light", 3);
    live.command("theme light");
    snap("ownership-step3-light-100x30", &live, 100, 30);
    let live = open("shared/ownership/ownership.rs.tiza", "vline", 3);
    live.with(|app| app.cursor_line = 15);
    live.keys(&[KeyCode::Char('V'), KeyCode::Char('j'), KeyCode::Char('j')]);
    snap("ownership-step3-vline-100x30", &live, 100, 30);
    let live = open("shared/ownership/ownership.rs.tiza", "tab", 3);
    live.keys(&[KeyCode::Tab, KeyCode::Tab]);
    snap("ownership-step3-tab-selected-100x30", &live, 100, 30);
    // The badge needs what the example no longer has at 80 columns:
    // labels cut or code clipped. Its own copy of the example as it was.
    let live = open("render/frame_snapshots/ownership.rs.tiza", "small", 3);
    snap("ownership-step3-small-80x24", &live, 80, 24);
}
