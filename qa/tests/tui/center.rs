//! The screen of a document with `"center": true`, in process (no pty).
//! The code goes to the middle of the code area: across by the offset of
//! the take, down by half of what the rows of the take leave of a fixed
//! height. The bottom row stays kept for the bar, or for the step counter
//! and the chip in presentation mode, so the code does not move when the
//! counter comes or goes. The mouse, the cursor marks and the height of
//! `J` follow the code.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use serde_json::{Value, json};
use tiza_qa::scratch;
use tiza_tui::app::{App, MouseKind};

/// Lines 1-3, the widest is line 2 (14 columns).
const FN: [&str; 3] = ["fn main() {", "    let x = 1;", "}"];

/// An app on a document of `code` with `fields`, no padding, in normal
/// mode.
fn app(name: &str, code: &[&str], fields: Value) -> App {
    let mut doc = json!({"schema": 2, "source": {"language": "rs", "first_line": 1},
        "code": code, "annotations": [], "center": true, "padding": [0, 0]});
    for (k, v) in fields.as_object().unwrap() {
        doc[k] = v.clone();
    }
    let path = scratch(name).join("doc.tiza");
    let doc = tiza_core::fs::parse_document(&doc.to_string(), None).unwrap();
    let mut a = App::new(doc, path);
    a.autosave = false;
    a
}

fn draw(a: &mut App, cols: u16, rows: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(cols, rows)).unwrap();
    t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
    t.backend().buffer().clone()
}

/// The column and the row of the first `text` on the screen.
fn find(b: &Buffer, text: &str) -> Option<(u16, u16)> {
    let area = b.area;
    (0..area.height).find_map(|y| {
        let row: String = (0..area.width).map(|x| b[(x, y)].symbol()).collect();
        row.find(text).map(|i| (row[..i].chars().count() as u16, y))
    })
}

#[test]
fn the_code_goes_to_the_middle_in_both_modes() {
    let mut a = app("center-tui-middle", &FN, json!({"numbers": false}));
    // 80 × 24: the block is 2 + 14 + 2 columns, 31 columns in; the code
    // starts 2 columns after that. 23 rows (the bar keeps the last one)
    // less 3 rows of code: 10 rows down.
    let normal = find(&draw(&mut a, 80, 24), "fn main() {");
    assert_eq!(normal, Some((33, 10)), "normal mode");
    // `J` and the other keys of the view count the rows under the offset.
    assert_eq!(a.screen_height(), 24 - 1 - 10);
    a.set_present(true);
    let present = find(&draw(&mut a, 80, 24), "fn main() {");
    assert_eq!(present, Some((33, 10)), "presentation mode");
}

#[test]
fn a_cover_of_one_step_does_not_move_when_the_counter_goes() {
    // Four rows: the counter of step 0 takes the last row and the one of
    // step 1 does not show. With a height from the rows the counter takes,
    // the code would go 9 rows down at step 0 and 10 at step 1.
    let mut a = app(
        "center-tui-counter",
        &["", "  RUST  ", "  ----  ", ""],
        json!({"numbers": false, "step": 0, "annotations": [
            {"id": "h1", "step": 1, "type": "highlight",
             "span": {"line": 2, "start": 2, "end": 6}}]}),
    );
    a.set_present(true);
    let at0 = find(&draw(&mut a, 80, 24), "RUST");
    a.next_step();
    assert_eq!(a.doc.step, 1);
    let at1 = find(&draw(&mut a, 80, 24), "RUST");
    assert_eq!(at0, at1);
    assert_eq!(at0.map(|p| p.1), Some((23 - 4) / 2 + 1));
}

#[test]
fn a_box_at_a_later_step_does_not_move_the_take_down() {
    // The box of step 3 takes a row above line 1 and a row below line 3:
    // the take has 5 rows, so the code is (23 - 5) / 2 rows down from step
    // 1 on, and at step 3 the top rail of the box is on that row.
    let mut a = app(
        "center-tui-later-box",
        &FN,
        json!({"numbers": false, "step": 1, "annotations": [
            {"id": "h1", "step": 1, "type": "highlight",
             "span": {"line": 2, "start": 4, "end": 7}},
            {"id": "b1", "step": 3, "type": "box", "from_line": 1, "to_line": 3}]}),
    );
    a.set_present(true);
    let top = (23 - 5) / 2;
    assert_eq!(
        find(&draw(&mut a, 80, 24), "fn main").map(|p| p.1),
        Some(top),
        "step 1"
    );
    a.next_step();
    a.next_step();
    assert_eq!(a.doc.step, 3);
    let b = draw(&mut a, 80, 24);
    assert_eq!(find(&b, "╭").map(|p| p.1), Some(top), "step 3");
    assert_eq!(find(&b, "fn main").map(|p| p.1), Some(top + 1), "step 3");
}

#[test]
fn a_resize_takes_the_rows_of_the_new_width() {
    // At 80 columns the note of line 2 is in the margin; at 40 the margin
    // is too narrow and it goes under its line: the take has one row more.
    let mut a = app(
        "center-tui-resize",
        &FN,
        json!({"numbers": false, "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 2, "text": "x is one"}]}),
    );
    assert_eq!(
        find(&draw(&mut a, 80, 24), "fn main").map(|p| p.1),
        Some(10)
    );
    assert_eq!(
        find(&draw(&mut a, 40, 24), "fn main").map(|p| p.1),
        Some((23 - 4) / 2)
    );
    assert_eq!(
        find(&draw(&mut a, 80, 24), "fn main").map(|p| p.1),
        Some(10)
    );
}

#[test]
fn a_click_goes_to_the_character_under_it() {
    let mut a = app("center-tui-mouse", &FN, json!({"numbers": false}));
    a.mouse = true;
    let b = draw(&mut a, 80, 24);
    let (x, y) = find(&b, "x = 1").unwrap();
    a.on_mouse(MouseKind::Down, x, y);
    a.on_mouse(MouseKind::Up, x, y);
    assert_eq!((a.cursor_line, a.cursor_col), (2, 8));
    // A click on a row of the offset, above the code, does nothing.
    a.on_mouse(MouseKind::Down, x, 2);
    a.on_mouse(MouseKind::Up, x, 2);
    assert_eq!((a.cursor_line, a.cursor_col), (2, 8));
}

#[test]
fn the_marks_of_the_cursor_line_are_in_the_moved_gutter() {
    let mut a = app(
        "center-tui-gutter",
        &FN,
        json!({"annotations": [
            {"id": "b1", "step": 1, "type": "box", "from_line": 2, "to_line": 2}]}),
    );
    draw(&mut a, 80, 24);
    tiza_tui::on_key(&mut a, KeyCode::Tab, KeyModifiers::NONE);
    a.cursor_line = 2;
    let b = draw(&mut a, 80, 24);
    let (sel_x, sel_y) = find(&b, "▸").expect("the mark of the selected box");
    let (digit_x, digit_y) = find(&b, "2 ").expect("the number of line 2");
    assert_eq!(sel_y, digit_y);
    // The mark is the first cell of the gutter, one before the digit.
    assert_eq!(sel_x + 1, digit_x);
    assert!(
        sel_x > 20,
        "the gutter is in the middle, not at the left: {sel_x}"
    );
    // The accent of the cursor line is on the digit, not on the code.
    let (code_x, _) = find(&b, "let x").unwrap();
    let accent = b[(digit_x, digit_y)].fg;
    assert_ne!(
        b[(code_x - 1, digit_y)].fg,
        accent,
        "the cell before the code"
    );
    assert_ne!(b[(code_x + 1, digit_y)].fg, accent, "the code");
}

#[test]
fn a_step_that_types_keeps_the_code_still() {
    let doc = tiza_core::fs::parse_document(
        &json!({"schema": 2, "source": {"language": "rs", "first_line": 1},
            "code": FN, "annotations": [], "center": true, "padding": [0, 0],
            "numbers": false, "step": 1,
            "moves": [{"at_step": 2, "action": "code.patch", "op": "insert", "after": "}",
                       "content": "// two more\n// lines", "typing": "char", "speed_ms": 50}]})
        .to_string(),
        None,
    )
    .unwrap();
    let mut a = App::new(doc, scratch("center-tui-typing").join("doc.tiza"));
    a.autosave = false;
    a.set_present(true);
    let before = find(&draw(&mut a, 80, 24), "fn main() {");
    a.next_step();
    assert!(a.is_typing());
    let mut now = Instant::now();
    let mut frames = 0;
    while a.is_typing() {
        now += Duration::from_millis(10);
        if a.typing_tick_at(now) {
            frames += 1;
            assert_eq!(
                find(&draw(&mut a, 80, 24), "fn main() {"),
                before,
                "frame {frames}"
            );
        }
    }
    assert!(frames > 3);
    assert_eq!(
        find(&draw(&mut a, 80, 24), "fn main() {"),
        before,
        "the end"
    );
    // The take of the last step has 5 rows.
    assert_eq!(before.map(|p| p.1), Some((23 - 5) / 2));
}

#[test]
fn a_resize_while_a_step_types_takes_the_rows_of_the_take() {
    // The rows of the take at 40 columns are first counted while the
    // step types: they are those of the take (the note goes under its line
    // at 40 columns), not those of the code being typed.
    let doc = tiza_core::fs::parse_document(
        &json!({"schema": 2, "source": {"language": "rs", "first_line": 1},
            "code": FN, "center": true, "padding": [0, 0], "numbers": false, "step": 1,
            "annotations": [{"id": "n1", "step": 1, "type": "note", "line": 2, "text": "x is one"}],
            "moves": [{"at_step": 2, "action": "code.patch", "op": "insert", "after": "}",
                       "content": "// two more\n// lines", "typing": "char", "speed_ms": 50}]})
        .to_string(),
        None,
    )
    .unwrap();
    let mut a = App::new(doc, scratch("center-tui-typing-resize").join("doc.tiza"));
    a.autosave = false;
    a.set_present(true);
    draw(&mut a, 80, 24);
    a.next_step();
    assert!(a.is_typing());
    let mut now = Instant::now();
    let mut frames = 0;
    while a.is_typing() {
        now += Duration::from_millis(10);
        if a.typing_tick_at(now) {
            frames += 1;
            // Line 1, the note under line 2 and five lines of code: 6 rows.
            let at40 = find(&draw(&mut a, 40, 24), "fn main() {").map(|p| p.1);
            assert_eq!(at40, Some((23 - 6) / 2), "frame {frames}");
        }
    }
    assert!(frames > 3);
}

#[test]
fn a_document_taller_than_the_screen_starts_at_the_top() {
    let code: Vec<String> = (1..=40).map(|i| format!("let v{i} = {i};")).collect();
    let code: Vec<&str> = code.iter().map(String::as_str).collect();
    let mut a = app("center-tui-tall", &code, json!({"numbers": false}));
    let b = draw(&mut a, 80, 24);
    let (x, y) = find(&b, "let v1 ").unwrap();
    assert_eq!(y, 0);
    // Across, it is still in the middle.
    assert!(x > 20, "{x}");
}

#[test]
fn center_on_off_and_undo() {
    let mut a = app("center-tui-command", &FN, json!({"center": false}));
    draw(&mut a, 80, 24);
    let reply = |a: &mut App, cmd: &str| {
        a.run_command(cmd);
        a.message.clone().map(|(m, _)| m).unwrap_or_default()
    };
    assert_eq!(reply(&mut a, "center"), "center off");
    assert_eq!(
        reply(&mut a, "center on"),
        "center on (saved in the document)"
    );
    assert!(a.doc.center);
    assert_eq!(reply(&mut a, "center on"), "center already on");
    assert!(reply(&mut a, "center maybe").contains("use `:center on` or `:center off`"));
    reply(&mut a, "u");
    assert!(!a.doc.center);
    // A screen too narrow for the block: the reply says from which width
    // it is centered, and the code stays at the left.
    let wide = ["fn main() { println!(\"a line of more than forty columns\"); }"];
    let mut a = app("center-tui-narrow", &wide, json!({"center": false}));
    draw(&mut a, 40, 24);
    let r = reply(&mut a, "center on");
    assert!(r.contains("so it is centered from width"), "{r}");
    assert!(r.contains("it stays at the left here"), "{r}");
}
