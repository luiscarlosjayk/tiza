//! A step with typed moves of different speeds, or a typed move and a
//! `wait`, takes on the screen the time of each one: its lines or
//! characters × its `speed_ms`, or the `ms` of the wait. Before, the TUI
//! counted the units of a tick at the speed of the move typed then, and
//! gave the units left over to the next moves. Thus after a move at 1 ms,
//! one tick of 15 ms also typed a move at 600 ms a line, or ended a `wait`
//! of 1000 ms.
//!
//! The clock of each test starts when the typing starts
//! (`TypingState::last`), so that the times are exact.

use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_core::model::Document;
use tiza_qa::scratch;
use tiza_tui::app::App;

/// 14 characters at 1 ms: they show by 14 ms.
fn fast() -> Value {
    json!({"at_step": 1, "action": "code.patch", "op": "insert", "after": "let a = 1;",
           "content": "    let b = 2;", "typing": "char", "speed_ms": 1})
}

/// `lines` lines at 600 ms a line.
fn slow(lines: usize) -> Value {
    let content: Vec<String> = (0..lines).map(|i| format!("    let c{i} = 3;")).collect();
    json!({"at_step": 1, "action": "code.patch", "op": "insert", "after": "let a = 1;",
           "content": content.join("\n"), "typing": "line", "speed_ms": 600})
}

fn wait(ms: u64) -> Value {
    json!({"at_step": 1, "action": "wait", "ms": ms})
}

/// An app in presentation mode that types the moves of step 1, and the
/// time when the typing starts.
fn typing(name: &str, moves: &[Value]) -> (App, Instant) {
    let mut doc = Document::from_code("fn main() {\n    let a = 1;\n}\n", "rs", None);
    for m in moves {
        doc.add_move(serde_json::from_value(m.clone()).unwrap())
            .unwrap();
    }
    doc.step = 0;
    let mut app = App::new(doc, scratch(name).join("doc.tiza"));
    app.autosave = false;
    app.present = true;
    app.next_step();
    let start = app.typing.as_ref().expect("step 1 types").last;
    (app, start)
}

/// The move typed and its units shown, `None` when the typing is over.
fn state(app: &App) -> Option<(usize, usize)> {
    app.typing.as_ref().map(|t| (t.index, t.revealed))
}

/// Ticks every 15 ms (the loop of the TUI) from `from` to `to` ms after
/// `start`, then at `to`, and gives the state there.
fn run(app: &mut App, start: Instant, from: u64, to: u64) -> Option<(usize, usize)> {
    let mut ms = from;
    while ms < to {
        app.typing_tick_at(start + Duration::from_millis(ms));
        ms += 15;
    }
    app.typing_tick_at(start + Duration::from_millis(to));
    state(app)
}

#[test]
fn a_slow_move_after_a_fast_one_takes_its_own_time() {
    let (mut app, start) = typing("fast-slow", &[fast(), slow(2)]);
    // The 14 characters by 14 ms; then one line at 614 ms, one at 1214 ms.
    assert_eq!(run(&mut app, start, 0, 100), Some((1, 0)));
    assert_eq!(run(&mut app, start, 100, 613), Some((1, 0)));
    assert_eq!(run(&mut app, start, 613, 614), Some((1, 1)));
    assert_eq!(run(&mut app, start, 614, 1213), Some((1, 1)));
    assert_eq!(run(&mut app, start, 1213, 1214), None);
}

#[test]
fn a_wait_after_a_fast_move_waits_its_time() {
    let (mut app, start) = typing("fast-wait", &[fast(), wait(1000)]);
    assert_eq!(run(&mut app, start, 0, 500), Some((1, 0)));
    assert_eq!(run(&mut app, start, 500, 1013), Some((1, 0)));
    assert!(app.is_typing());
    assert_eq!(run(&mut app, start, 1013, 1014), None);
}

#[test]
fn a_fast_move_after_a_slow_one_starts_with_the_time_left() {
    let (mut app, start) = typing("slow-fast", &[slow(1), fast()]);
    // One tick: the line takes 600 ms, and the 5 ms left show 5 characters.
    assert_eq!(run(&mut app, start, 605, 605), Some((1, 5)));
    assert_eq!(run(&mut app, start, 605, 613), Some((1, 13)));
    assert_eq!(run(&mut app, start, 613, 614), None);
}

#[test]
fn a_move_that_ends_on_a_tick_gives_the_next_one_no_unit() {
    let (mut app, start) = typing("on-the-tick", &[fast(), wait(100)]);
    // The 14th character shows at 14 ms: the wait starts there, with none
    // of its time gone.
    assert_eq!(run(&mut app, start, 14, 14), Some((1, 0)));
    assert_eq!(run(&mut app, start, 14, 113), Some((1, 0)));
    assert_eq!(run(&mut app, start, 113, 114), None);
    // The first tick 1 ms after the end of the move: that 1 ms goes to the
    // wait, at its own speed, and does not end it.
    let (mut app, start) = typing("past-the-tick", &[fast(), wait(100)]);
    assert_eq!(run(&mut app, start, 15, 15), Some((1, 0)));
}

#[test]
fn a_key_still_ends_the_typing_at_once() {
    let (mut app, start) = typing("interrupted", &[fast(), slow(2)]);
    assert_eq!(run(&mut app, start, 0, 100), Some((1, 0)));
    app.finish_typing();
    assert_eq!(state(&app), None);
    let code = app.doc.code.join("\n");
    assert!(
        code.contains("let b = 2;") && code.contains("let c0 = 3;") && code.contains("let c1 = 3;"),
        "the code of the end of the step:\n{code}"
    );
    assert!(!app.typing_tick_at(start + Duration::from_millis(2000)));
}

#[test]
fn the_screen_shows_what_each_frame_of_an_export_shows() {
    // Moves and waits of many speeds. At the start of each frame of an
    // export, the screen shows the same units, with a tick every 15 ms in
    // between.
    let moves = [
        fast(),
        wait(37),
        slow(2),
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "after": "fn main() {",
               "content": "    // seven ms a character", "typing": "char", "speed_ms": 7}),
        wait(5),
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "after": "let a = 1;",
               "content": "    let e = 5;\n    let f = 6;", "typing": "line", "speed_ms": 45}),
    ];
    let (mut app, start) = typing("as-the-export", &moves);
    let frames = app.typing.as_ref().unwrap().plan.export_frames();
    let mut at = 0;
    let mut tick = 0;
    for (index, revealed, ms) in frames {
        while tick < at {
            app.typing_tick_at(start + Duration::from_millis(tick));
            tick += 15;
        }
        app.typing_tick_at(start + Duration::from_millis(at));
        assert_eq!(state(&app), Some((index, revealed)), "at {at} ms");
        at += ms;
    }
    app.typing_tick_at(start + Duration::from_millis(at));
    assert_eq!(state(&app), None, "after the last frame, at {at} ms");
}
