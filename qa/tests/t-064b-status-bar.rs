//! t-064 (follow-up from the QA agent) — the status line never hides a message.
//!
//! Acceptance criteria:
//! - AC1 At 80, 100 and 120 columns an error message is still readable: the
//!   optional segments (theme, file, L, ann.) are dropped first, then the
//!   message is truncated with `…`, never removed.
//! - AC2 With the ⚠ badge present, messages still show.
//! - AC3 The Tab selection is always visible: `▸` in the gutter of the
//!   annotation's first line, and the hint on the bar when it fits.

use codedraw_qa::Live;
use crossterm::event::KeyCode;

fn bar(live: &Live, cols: u16) -> String {
    live.screen(cols, 24).lines().last().unwrap().to_string()
}

#[test]
fn ac1_messages_survive_narrow_bars() {
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "t064b-msg");
    for cols in [80u16, 100, 120, 160] {
        live.command("set a1 stroke=wavy");
        let b = bar(&live, cols);
        assert!(
            b.contains("not_found") || b.contains("a1") || b.contains("…"),
            "{cols} cols: {b}"
        );
        live.command("note 99 x");
        let b = bar(&live, cols);
        assert!(
            b.contains("line 99") || b.contains("99"),
            "{cols} cols: {b}"
        );
        assert!(
            b.chars().count() <= cols as usize,
            "{cols} cols overflow: {b}"
        );
    }
    // A long message at 80 cols is truncated with an ellipsis, not hidden.
    live.command("mark nope 2:s");
    let b = bar(&live, 80);
    assert!(b.contains("…") || b.contains("unknown"), "{b}");
}

#[test]
fn ac2_badge_and_message_coexist() {
    let live = Live::from_code(
        "fn main() {\n    let s = String::from(\"a long literal to shrink the margin\");\n}\n",
        "rs",
        "t064b-badge",
    );
    live.command("mark warn 3:zzz");
    let b = bar(&live, 80);
    assert!(b.contains('⚠'), "{b}");
    assert!(
        b.contains("zzz") || b.contains("not found") || b.contains("…"),
        "{b}"
    );
}

#[test]
fn ac3_selection_is_visible_in_the_gutter() {
    let live = Live::from_code(
        "fn main() {\n    let s = 1;\n    let t = 2;\n}\n",
        "rs",
        "t064b-sel",
    );
    live.command("note 2 a");
    live.command("note 3 b");
    live.keys(&[KeyCode::Tab]);
    let s = live.screen(120, 20);
    let l2 = s.lines().find(|l| l.contains("let s = 1")).unwrap();
    assert!(
        l2.trim_start().starts_with('▸'),
        "gutter marker on the selected annotation's line: {l2:?}"
    );
    assert!(
        s.lines().last().unwrap().contains("▸ a1"),
        "hint on the bar: {}",
        s.lines().last().unwrap()
    );
    live.keys(&[KeyCode::Tab]);
    let s = live.screen(80, 20);
    let l3 = s.lines().find(|l| l.contains("let t = 2")).unwrap();
    assert!(l3.trim_start().starts_with('▸'), "{l3:?}");
}

#[test]
fn qa_hints_shrink_in_tiers_instead_of_vanishing() {
    let live = Live::from_code("fn main() {}\n", "rs", "t064b-tiers");
    assert!(bar(&live, 160).contains("v select  :cmd  ?help"));
    // 100 columns with the small-terminal badge: the middle tier.
    let b = bar(&live, 100);
    assert!(b.contains("?help  q quit"), "{b}");
    let b = bar(&live, 80);
    assert!(b.contains("?help"), "{b}");
}

#[test]
fn qa_command_mode_shows_candidates_truncated_and_history_stops_at_oldest() {
    let live = Live::from_code("fn main() {}\n", "rs", "t064b-cands");
    live.keys(&[KeyCode::Char(':'), KeyCode::Tab]);
    let b = bar(&live, 100);
    assert!(b.contains("arrow  autosave"), "candidates listed: {b}");
    assert!(
        b.trim_end().ends_with('…'),
        "truncated with an ellipsis: {b}"
    );
    live.keys(&[KeyCode::Esc]);
    live.command("note 1 one");
    live.command("theme dark");
    live.keys(&[KeyCode::Char(':'), KeyCode::Up, KeyCode::Up, KeyCode::Up]);
    let input = live.with(|app| match &app.mode {
        codedraw_tui::app::Mode::Command(i) => i.clone(),
        _ => String::new(),
    });
    assert_eq!(input, "note 1 one", "↑ past the oldest stays on the oldest");
}
