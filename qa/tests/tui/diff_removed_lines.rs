//! The lines that a `code.patch` move with `diff` removes, in the TUI:
//! a row of their own over the line under them (their owner). A click on
//! one goes to its owner, a selection of lines tints those between its
//! lines, and the view keeps its code when they come and go.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use serde_json::json;
use tiza_qa::{Live, scratch};
use tiza_tui::app::{App, MouseKind, MouseOutcome};

const COLS: u16 = 80;
const ROWS: u16 = 24;

fn draw(a: &mut App) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(COLS, ROWS)).unwrap();
    t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
    t.backend().buffer().clone()
}

fn key(a: &mut App, k: KeyCode) {
    tiza_tui::on_key(a, k, KeyModifiers::NONE);
}

/// 160 lines `let vN = N;` with `moves`, a note on line 90 at step 2 (in
/// the view of line 80), at `step`, with
/// no padding, in presentation mode when `present`.
fn open(name: &str, moves: serde_json::Value, step: u32, present: bool) -> Live {
    open_with(name, json!([]), moves, step, present)
}

/// [`open`] with more `annotations`.
fn open_with(
    name: &str,
    annotations: serde_json::Value,
    moves: serde_json::Value,
    step: u32,
    present: bool,
) -> Live {
    let code: Vec<String> = (1..=160).map(|i| format!("let v{i} = {i};")).collect();
    let mut all = vec![json!({"id": "n2", "step": 2, "type": "note", "line": 90, "text": "two"})];
    all.extend(annotations.as_array().unwrap().iter().cloned());
    let doc = json!({
        "schema": 2, "source": {"language": "rs", "first_line": 1}, "code": code,
        "annotations": all, "moves": moves, "step": step
    });
    let dir = scratch(name);
    std::fs::write(dir.join("doc.tiza"), doc.to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&dir.join("doc.tiza")).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("s.sock"));
    live.command("pad 0 0");
    live.with(move |a| {
        a.autosave = false;
        a.set_present(present);
        draw(a);
    });
    live
}

/// A `delete` with `diff` of `let v80 = 80;` at step 1: at step 1 it is a
/// removed line over line 80 (`let v81 = 81;`).
fn delete_v80() -> serde_json::Value {
    json!([{"at_step": 1, "action": "code.patch", "op": "delete", "find": "let v80 = 80;", "diff": true}])
}

/// The screen row that shows `text` in a new frame.
fn screen_row(live: &Live, text: &'static str) -> Option<u16> {
    live.with(move |a| {
        let b = draw(a);
        (0..ROWS).find(|&y| {
            let row: String = (0..COLS).map(|x| b[(x, y)].symbol()).collect();
            row.contains(text)
        })
    })
}

#[test]
fn a_click_on_a_removed_line_goes_to_the_start_of_its_owner() {
    let live = open("diff-tui-click", delete_v80(), 1, false);
    live.command("mouse on");
    live.with(|a| {
        a.cursor_line = 80;
        draw(a);
    });
    let gone = screen_row(&live, "let v80 = 80;").expect("the removed line in view");
    let out = live.with(move |a| {
        let x = a.hit.code_x + 5;
        let out = a.on_mouse(MouseKind::Down, x, gone + a.hit.origin.1);
        a.on_mouse(MouseKind::Up, x, gone + a.hit.origin.1);
        out
    });
    assert_eq!(out, MouseOutcome::Moved(80, 0));
}

#[test]
fn a_view_that_starts_on_a_removed_line_keeps_its_owner_at_the_step_after() {
    let live = open("diff-tui-anchor", delete_v80(), 1, true);
    // The first row of the view is the removed line.
    let rows = live.with(|a| a.layout(usize::from(COLS)).row_of_line[&80] - 1);
    live.with(move |a| {
        for _ in 0..rows {
            key(a, KeyCode::Char('j'));
        }
    });
    assert_eq!(screen_row(&live, "let v80 = 80;"), Some(0));
    assert_eq!(screen_row(&live, "let v81 = 81;"), Some(1));
    // At step 2 the removed line is gone: line 80 is on top, and the code
    // does not go up or down more than the row that left.
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(live.with(|a| a.doc.step), 2);
    assert_eq!(screen_row(&live, "let v81 = 81;"), Some(0));
    assert_eq!(screen_row(&live, "let v80 = 80;"), None);
}

#[test]
fn a_view_on_a_removed_line_in_a_box_keeps_its_owner_on_its_row() {
    // A box on lines 80-81: at step 1 its title, the removed line, then
    // line 80. The view starts on the removed line, under the title.
    let boxed = json!([{"id": "bx", "step": 1, "type": "box", "from_line": 80, "to_line": 81, "label": "here"}]);
    let live = open_with("diff-tui-anchor-box", boxed, delete_v80(), 1, true);
    let rows = live.with(|a| a.layout(usize::from(COLS)).row_of_line[&80] - 1);
    live.with(move |a| {
        for _ in 0..rows {
            key(a, KeyCode::Char('j'));
        }
    });
    assert_eq!(screen_row(&live, "let v80 = 80;"), Some(0));
    assert_eq!(screen_row(&live, "let v81 = 81;"), Some(1));
    // At step 2 the title of the box takes the row of the removed line:
    // line 80 stays on its row, it does not go up.
    live.with(|a| key(a, KeyCode::Char('n')));
    assert_eq!(screen_row(&live, "here"), Some(0));
    assert_eq!(screen_row(&live, "let v81 = 81;"), Some(1));
}

#[test]
fn a_removed_line_wider_than_the_code_shows_whole_in_normal_mode() {
    // Normal mode has no geometry of the whole take: the code column of the
    // step makes room for its removed lines too.
    let long = "let a_removed_line_that_is_wider_than_the_others = 0;";
    // Step 1 writes the long line, step 2 takes it away again.
    let moves = json!([
        {"at_step": 1, "action": "code.patch", "op": "replace", "find": "let v81 = 81;",
            "content": long, "diff": true},
        {"at_step": 2, "action": "code.patch", "op": "replace", "find": long,
            "content": "let x = 1;", "diff": true}
    ]);
    let boxed = json!([{"id": "bx", "step": 2, "type": "box", "from_line": 79, "to_line": 82}]);
    let live = open_with("diff-tui-wide", boxed, moves, 2, false);
    live.with(|a| a.cursor_line = 81);
    // The removed line is whole, inside the box: the right border is right
    // of its end.
    let row = live.with(move |a| {
        let b = draw(a);
        (0..ROWS)
            .map(|y| (0..COLS).map(|x| b[(x, y)].symbol()).collect::<String>())
            .find(|r| r.contains("let a_removed_line"))
    });
    let row = row.expect("the removed line in view");
    assert!(row.contains(long), "{row}");
    assert!(row.trim_end().ends_with('│'), "{row}");
}

/// At step 1 (from step 0, in presentation mode), a `scroll` to line 80
/// at the top, then `moves`: the rows of `let v81 = 81;` while the step
/// types or waits, and when it ends (a key ends it).
fn held_rows(name: &str, moves: serde_json::Value) -> (Option<u16>, Option<u16>, Live) {
    let mut all = vec![json!({"at_step": 1, "action": "scroll", "line": 80, "align": "top"})];
    all.extend(moves.as_array().unwrap().iter().cloned());
    let live = open(name, json!(all), 0, true);
    live.with(|a| key(a, KeyCode::Char('n')));
    assert!(live.with(|a| a.is_typing()), "step 1 types or waits");
    let during = screen_row(&live, "let v81 = 81;");
    live.with(|a| key(a, KeyCode::Char('n')));
    assert!(!live.with(|a| a.is_typing()));
    assert_eq!(live.with(|a| a.doc.step), 1);
    let after = screen_row(&live, "let v81 = 81;");
    (during, after, live)
}

#[test]
fn a_held_view_keeps_a_row_for_the_removed_lines_that_come_when_the_typing_ends() {
    // A typed replace of v80: the old line stays until the typing gets to
    // its new line, and comes back over it, removed, when the typing ends.
    let (during, after, live) = held_rows(
        "diff-tui-held-typed",
        json!([{"at_step": 1, "action": "code.patch", "op": "replace", "find": "let v80 = 80;",
            "content": "let w = 0;", "typing": "line", "speed_ms": 3_600_000, "diff": true}]),
    );
    // The view kept the row of the removed line over line 80, so the code
    // under it does not move when it comes.
    assert_eq!(screen_row(&live, "let v80 = 80;"), Some(0));
    assert_eq!(screen_row(&live, "let w = 0;"), Some(1));
    assert_eq!((during, after), (Some(2), Some(2)));
}

#[test]
fn a_removed_line_takes_the_row_of_its_line_when_a_wait_ends() {
    // A wait, then a delete of v80: until the wait ends, v80 is still a
    // line of the code, and then its row shows it as removed.
    let (during, after, live) = held_rows(
        "diff-tui-held-wait",
        json!([
            {"at_step": 1, "action": "wait", "ms": 3_600_000},
            {"at_step": 1, "action": "code.patch", "op": "delete", "find": "let v80 = 80;", "diff": true}
        ]),
    );
    assert_eq!(screen_row(&live, "let v80 = 80;"), Some(0));
    assert_eq!((during, after), (Some(1), Some(1)));
}

#[test]
fn a_selection_of_lines_tints_the_removed_lines_between_them() {
    let live = open("diff-tui-select", delete_v80(), 1, false);
    let tint = live.with(|a| {
        a.cursor_line = 79;
        draw(a);
        key(a, KeyCode::Char('V'));
        key(a, KeyCode::Char('j'));
        let b = draw(a);
        let row = |text: &str| {
            (0..ROWS)
                .find(|&y| {
                    let r: String = (0..COLS).map(|x| b[(x, y)].symbol()).collect();
                    r.contains(text)
                })
                .unwrap()
        };
        let x = a.hit.code_x + 1;
        let bg = |y: u16| b[(x, y)].bg;
        (
            bg(row("let v79 = 79;")),
            bg(row("let v80 = 80;")),
            bg(row("let v81 = 81;")),
            bg(row("let v78 = 78;")),
        )
    });
    // 79 and 81 (line 80) are selected, and the removed line between them.
    assert_eq!(tint.0, tint.2);
    assert_eq!(tint.1, tint.0, "the removed line between them");
    assert_ne!(tint.3, tint.0, "line 78 is not selected");
}

#[test]
fn the_cursor_that_goes_up_to_a_line_shows_its_box_title_and_removed_lines() {
    let boxed = json!([{"id": "bx", "step": 1, "type": "box", "from_line": 80, "to_line": 81, "label": "here"}]);
    let live = open_with("diff-tui-cursor-up", boxed, delete_v80(), 1, false);
    // The cursor far below, then on line 80: the view goes up to it, with
    // the rows over it that are its own.
    live.with(|a| {
        a.cursor_line = 140;
        draw(a);
        a.cursor_line = 80;
    });
    assert_eq!(screen_row(&live, "here"), Some(0));
    assert_eq!(screen_row(&live, "let v80 = 80;"), Some(1));
    assert_eq!(screen_row(&live, "let v81 = 81;"), Some(2));
}
