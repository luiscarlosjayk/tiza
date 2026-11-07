//! A step that types the tenth line of the code: the code does not move
//! one column to the right when the gutter needs a second digit, with or
//! without `center`, in presentation mode and in normal mode.

use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use serde_json::json;
use tiza_qa::scratch;
use tiza_tui::app::App;

fn draw(a: &mut App, cols: u16, rows: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(cols, rows)).unwrap();
    t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
    t.backend().buffer().clone()
}

/// The column of the first `text` on the screen.
fn column(b: &Buffer, text: &str) -> Option<usize> {
    let area = b.area;
    (0..area.height).find_map(|y| {
        let row: String = (0..area.width).map(|x| b[(x, y)].symbol()).collect();
        row.find(text).map(|i| row[..i].chars().count())
    })
}

/// The columns of `let v1` on step 1, then in every frame of step 2, which
/// types line 9 and then lines 10 and 11 (two moves) after eight lines.
fn columns(name: &str, center: bool, present: bool, numbers: bool) -> Vec<Option<usize>> {
    let code: Vec<String> = (1..=8).map(|i| format!("let v{i} = {i};")).collect();
    let doc = json!({"schema": 2, "source": {"language": "rs", "first_line": 1},
        "code": code, "annotations": [], "center": center, "numbers": numbers, "step": 1,
        "moves": [{"at_step": 2, "action": "code.patch", "op": "insert",
                   "after": "let v8 = 8;", "content": "let v9 = 9;",
                   "typing": "char", "speed_ms": 20},
                  {"at_step": 2, "action": "code.patch", "op": "insert",
                   "after": "let v9 = 9;", "content": "let v10 = 10;\nlet v11 = 11;",
                   "typing": "char", "speed_ms": 20}]});
    let doc = tiza_core::fs::parse_document(&doc.to_string(), None).unwrap();
    let mut a = App::new(doc, scratch(name).join("doc.tiza"));
    a.autosave = false;
    a.set_present(present);
    let mut out = vec![column(&draw(&mut a, 80, 24), "let v1 =")];
    a.next_step();
    assert!(a.is_typing());
    let mut now = Instant::now();
    while a.is_typing() {
        now += Duration::from_millis(10);
        if a.typing_tick_at(now) {
            out.push(column(&draw(&mut a, 80, 24), "let v1 ="));
        }
    }
    out.push(column(&draw(&mut a, 80, 24), "let v1 ="));
    out
}

#[test]
fn a_step_that_types_the_tenth_line_does_not_move_the_code() {
    for center in [false, true] {
        // Presentation mode: the take does not move at all.
        let cols = columns("typing-gutter-present", center, true, true);
        assert!(
            cols.iter().all(|c| *c == cols[0] && c.is_some()),
            "center {center}, presentation: {cols:?}"
        );
        // Normal mode lays out each step on its own: the code can move
        // when the step changes, not while the step types.
        let cols = columns("typing-gutter-normal", center, false, true);
        assert!(
            cols[1..].iter().all(|c| *c == cols[1] && c.is_some()),
            "center {center}, normal mode: {cols:?}"
        );
    }
}

#[test]
fn a_step_that_types_without_line_numbers_has_no_gutter() {
    for present in [true, false] {
        let cols = columns("typing-gutter-none", false, present, false);
        assert!(
            cols.iter().all(|c| *c == cols[0] && c.is_some()),
            "presentation {present}: {cols:?}"
        );
    }
}
