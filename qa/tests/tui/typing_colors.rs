//! While a `code.patch` with `typing` types, the lines that it does not type
//! keep their colors on the screen. Before, the TUI highlighted the
//! half-typed code: with `fn dangle(` typed, the open bracket gave the line
//! below other colors until the bracket closed, and the line blinked.

use std::time::{Duration, Instant};

use tiza_core::canvas::{Canvas, CellStyle};
use tiza_core::model::Document;
use tiza_qa::scratch;
use tiza_tui::app::App;

const CODE: &str = "fn main() {\n    let r = dangle();\n}\n\nfn dangle() -> &String {\n    let s = String::from(\"hello\");\n    &s\n}\n";

/// The symbols and styles of a row of the screen.
type Row = Vec<(String, CellStyle)>;

/// The row of the screen that shows `text`.
fn row(canvas: &Canvas, text: &str) -> Row {
    let y = canvas
        .to_plain_text()
        .lines()
        .position(|l| l.contains(text))
        .unwrap_or_else(|| panic!("no row shows {text:?}"));
    (0..canvas.width)
        .filter_map(|x| canvas.get(x, y))
        .map(|c| (c.sym.to_string(), c.style))
        .collect()
}

/// An instance of `CODE` whose step 1 types `fn dangle() -> String {` over
/// line 5 in `mode`, at step 0 in presentation mode or not.
fn typing_app(name: &str, mode: &str, present: bool) -> App {
    let mut doc = Document::from_code(CODE, "rs", None);
    doc.add_move(
        serde_json::from_value(serde_json::json!({
            "at_step": 1, "action": "code.patch", "op": "replace",
            "find": "fn dangle() -> &String {", "content": "fn dangle() -> String {",
            "typing": mode, "speed_ms": 50
        }))
        .unwrap(),
    )
    .unwrap();
    doc.step = 0;
    let mut app = App::new(doc, scratch(name).join("doc.tiza"));
    app.autosave = false;
    app.present = present;
    app
}

/// Goes to step 1 and gives each tick 1 ms more until the typing ends:
/// the row of line 6 on each tick that changes the screen, then at the end.
fn rows_while_typing(app: &mut App) -> (Vec<Row>, Row) {
    let mut now = Instant::now();
    app.next_step();
    assert!(app.is_typing(), "step 1 does not type");
    let start = now;
    let mut seen = vec![row(&app.layout(100).canvas, "let s = String")];
    while app.is_typing() {
        assert!(
            now - start < Duration::from_secs(60),
            "the typing never ended"
        );
        now += Duration::from_millis(1);
        if app.typing_tick_at(now) && app.is_typing() {
            seen.push(row(&app.layout(100).canvas, "let s = String"));
        }
    }
    (seen, row(&app.layout(100).canvas, "let s = String"))
}

#[test]
fn the_line_below_a_typed_bracket_keeps_its_colors_on_every_tick() {
    for present in [false, true] {
        let mut app = typing_app("typing-colors-char", "char", present);
        let (seen, done) = rows_while_typing(&mut app);
        assert!(seen.len() > 10, "{} ticks", seen.len());
        for (tick, r) in seen.iter().enumerate() {
            assert!(
                *r == done,
                "presentation {present}, tick {tick}: line 6 has other colors than at the end of the typing"
            );
        }
    }
}

#[test]
fn the_typed_line_shows_the_part_typed_so_far() {
    // The screen draws the text of the colors: a cut in the wrong place
    // would show other text than the part typed so far.
    let full = "fn dangle() -> String {";
    let mut app = typing_app("typing-colors-part", "char", true);
    let mut now = Instant::now();
    app.next_step();
    let mut texts = Vec::new();
    while app.is_typing() {
        assert!(texts.len() < 10_000, "the typing never ended");
        now += Duration::from_millis(1);
        if app.typing_tick_at(now) && app.is_typing() {
            texts.push(app.layout(100).canvas.to_plain_text());
        }
    }
    let done = app.layout(100).canvas.to_plain_text();
    let (y, done_row) = done
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains(full))
        .expect("the typed line at the end");
    let x = done_row.find(full).unwrap();
    let shown: Vec<String> = texts
        .iter()
        .map(|t| {
            t.lines()
                .nth(y)
                .unwrap_or("")
                .get(x..)
                .unwrap_or("")
                .to_string()
        })
        .collect();
    assert!(
        shown.iter().all(|s| full.starts_with(s.as_str())),
        "{shown:?}"
    );
    assert!(shown.iter().any(|s| s == "fn dangle("), "{shown:?}");
}
