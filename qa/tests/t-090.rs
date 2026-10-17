//! t-090 — Padding around the drawing.
//!
//! Acceptance criteria:
//! - AC1 Without `padding` in the document the TUI keeps one blank column at
//!   each side (default `(0, 1)`); `:pad V H` changes it live and is saved.
//! - AC2 `--padding V[,H]` when opening sets it for that session.
//! - AC3 Margin labels never touch the right edge: one blank column is reserved.
//! - AC4 Padding is clamped so the code column never shrinks below 20 cells
//!   and the status line stays visible.

use crossterm::event::KeyCode;
use tiza_qa::Live;

#[test]
fn ac1_default_padding_is_one_column_and_pad_changes_it() {
    let live = Live::from_code("fn main() {}\n", "rs", "t090-default");
    let default_first = live.screen(60, 10).lines().next().unwrap().to_string();
    live.command("pad 0 0");
    let bare_first = live.screen(60, 10).lines().next().unwrap().to_string();
    assert_eq!(
        default_first,
        format!(" {bare_first}"),
        "the default is exactly one blank column more than :pad 0 0"
    );
    live.command("pad 2 4");
    let screen = live.screen(60, 12);
    assert!(
        screen.lines().take(2).all(|l| l.trim().is_empty()),
        "two blank rows on top:\n{screen}"
    );
    assert_eq!(
        screen.lines().nth(2).unwrap(),
        format!("    {bare_first}"),
        "four blank columns:\n{screen}"
    );
    assert_eq!(live.with(|app| app.doc.padding), Some((2, 4)));
}

#[test]
fn ac3_margin_labels_keep_a_blank_right_column() {
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "t090-right");
    live.command(
        "note 2 a label long enough to wrap across the margin several times over and over again",
    );
    let screen = live.screen(100, 10);
    for line in screen.lines() {
        assert!(
            line.chars().count() < 100,
            "row reaches the last column: {line:?}"
        );
    }
}

#[test]
fn ac4_padding_is_clamped_on_small_terminals() {
    let live = Live::from_code("fn main() {}\n", "rs", "t090-clamp");
    live.command("pad 20 40");
    let screen = live.screen(50, 12);
    assert!(screen.contains("fn main"), "code still visible:\n{screen}");
    live.keys(&[KeyCode::Char('?')]);
    let screen = live.screen(50, 12);
    assert!(
        screen.contains("Keys") || screen.contains("keys"),
        "help overlay still opens:\n{screen}"
    );
}
