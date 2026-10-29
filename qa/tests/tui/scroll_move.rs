//! The `scroll` move on the real TUI, in the process: the view jumps to
//! its target where `align` puts it, an older `cursor` does not pull it
//! back, the view holds the target while the rows change under it, and a
//! step with the same camera keeps a view the presenter moved, but a goto
//! lands on the view of its step.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use tiza_core::model::Document;
use tiza_core::moves::MoveAction;
use tiza_qa::{Live, fixture, scratch};
use tiza_tui::app::App;

const COLS: u16 = 80;
const ROWS: u16 = 24;

/// The document `tui/scroll_move/FILE`. `typed.tiza` types one line an
/// hour: its typing does not end by its clock during a test, also on a
/// slow machine. A key, a step change or a request ends it.
fn load(file: &str) -> Document {
    tiza_core::fs::load_document(&fixture(&format!("tui/scroll_move/{file}"))).unwrap()
}

/// The TUI on `tui/scroll_move/FILE` in presentation mode, with no
/// padding (every row of the terminal shows code), at `step`.
fn open(name: &str, file: &str, step: u32) -> Live {
    open_doc(name, load(file), step)
}

/// [`open`] on a document already loaded.
fn open_doc(name: &str, mut doc: Document, step: u32) -> Live {
    let dir = scratch(name);
    doc.step = step;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("s.sock"));
    live.command("pad 0 0");
    live.with(|a| {
        a.autosave = false;
        a.set_present(true);
        draw(a);
    });
    live
}

fn draw(a: &mut App) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(COLS, ROWS)).unwrap();
    t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
    t.backend().buffer().clone()
}

/// The screen row that shows `text` after a new frame, if one does.
fn row_of(live: &Live, text: &'static str) -> Option<u16> {
    live.with(move |a| {
        let b = draw(a);
        (0..ROWS).find(|&y| {
            let line: String = (0..COLS).map(|x| b[(x, y)].symbol()).collect();
            line.contains(text)
        })
    })
}

fn key(live: &Live, k: KeyCode) {
    live.keys(&[k]);
}

#[test]
fn a_camera_step_puts_its_line_where_align_says() {
    let live = open("scroll-align", "camera.tiza", 1);
    key(&live, KeyCode::Char('n'));
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(0), "top");
    key(&live, KeyCode::Char('n'));
    let center = row_of(&live, "let v50 = 50;").unwrap();
    assert!((11..=12).contains(&center), "center: row {center}");
    key(&live, KeyCode::Char('n'));
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 1), "bottom");
}

#[test]
fn an_older_cursor_does_not_pull_the_view_back() {
    // A `cursor` on line 10 at step 1, a `scroll` at steps 2 to 4, and
    // only a note at step 5: the view stays on the target of step 4, held
    // or moved by the presenter.
    let live = open("scroll-older-cursor", "camera.tiza", 4);
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 1));
    key(&live, KeyCode::Char('n'));
    assert_eq!(live.with(|a| a.doc.step), 5);
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 1), "held");
    key(&live, KeyCode::Char('p'));
    key(&live, KeyCode::Char('j'));
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 2));
    key(&live, KeyCode::Char('n'));
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 2), "moved");
}

#[test]
fn a_step_with_the_same_camera_keeps_a_moved_view_and_a_goto_applies_it() {
    // Steps 4 and 5 have the same camera, the `scroll` of step 4.
    let live = open("scroll-goto", "camera.tiza", 5);
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 1));
    key(&live, KeyCode::Char('G'));
    assert_eq!(row_of(&live, "let v50 = 50;"), None, "moved to the end");
    key(&live, KeyCode::Char('p'));
    assert_eq!(live.with(|a| a.doc.step), 4);
    assert_eq!(row_of(&live, "let v50 = 50;"), None, "the moved view stays");
    live.command("goto 5");
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 1), "goto");
}

#[test]
fn a_box_comes_in_whole_after_the_wait_of_its_step() {
    // Step 3 waits, then shows a box on lines 60-63; its `scroll` puts
    // the box at the bottom. While the step waits the box is not drawn;
    // when it comes in, the view holds the box with its bottom border.
    let live = open("scroll-wait", "effect.tiza", 2);
    key(&live, KeyCode::Char('n'));
    assert_eq!(live.with(|a| a.doc.step), 3);
    assert_eq!(row_of(&live, "let v63 = 63;"), Some(ROWS - 1), "waiting");
    // A key ends the wait and does nothing else.
    key(&live, KeyCode::Char('x'));
    let row = row_of(&live, "let v63 = 63;").unwrap();
    assert_eq!(row, ROWS - 2, "the last line of the box over its border");
    let border = live.with(|a| {
        let b = draw(a);
        (0..COLS).any(|x| b[(x, ROWS - 1)].symbol() == "╰")
    });
    assert!(border, "the bottom border of the box is in view");
}

#[test]
fn going_back_before_the_first_scroll_goes_to_the_top() {
    let live = open("scroll-back", "effect.tiza", 1);
    key(&live, KeyCode::Char('n'));
    assert_eq!(row_of(&live, "let v1 = 1;"), None);
    // The first key ends the effect of the box of step 2.
    key(&live, KeyCode::Char('x'));
    key(&live, KeyCode::Char('p'));
    assert_eq!(live.with(|a| a.doc.step), 1);
    assert_eq!(row_of(&live, "let v1 = 1;"), Some(0));
}

#[test]
fn with_the_pointer_on_the_effect_of_a_camera_step_plays_on() {
    // The pointer is on line 1; the `scroll` of step 2 goes to a box with
    // `draw` far below. The pointer goes off at the jump, before the
    // effect starts, so the effect does not end at its next tick.
    let live = open("scroll-pointer", "effect.tiza", 1);
    key(&live, KeyCode::Char('f'));
    assert!(live.with(|a| a.pointer.is_some()));
    // The step, the frames and the ticks in one call, with no wait between
    // them. The effect also ends by its clock after its time: only a
    // machine that stops this call that long can see that.
    let (off, playing, time, took) = live.with(|a| {
        let start = std::time::Instant::now();
        tiza_tui::on_key(a, KeyCode::Char('n'), KeyModifiers::NONE);
        let time = a.effect.as_ref().map(|e| e.duration_ms);
        draw(a);
        a.effect_tick();
        draw(a);
        a.effect_tick();
        (
            a.pointer.is_none(),
            a.is_playing_effect(),
            time,
            start.elapsed(),
        )
    });
    assert!(off, "the pointer is still on");
    let time = time.expect("the effect of step 2 does not start");
    assert!(
        playing || took >= std::time::Duration::from_millis(time),
        "the effect ended after {took:?} of its {time} ms"
    );
}

#[test]
fn the_tui_opens_on_the_target_of_its_step() {
    let live = open("scroll-open", "camera.tiza", 4);
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 1));
}

#[test]
fn the_newest_camera_wins_in_the_order_the_moves_run() {
    // The list holds a `cursor` of step 5 before a `scroll` of step 3. At
    // steps 3 and 4 the `scroll` places the view; at step 5 the `cursor`
    // is newer, though it comes first in the list.
    let live = open("scroll-unordered", "unordered.tiza", 3);
    assert_eq!(row_of(&live, "let v80 = 80;"), Some(0), "step 3");
    key(&live, KeyCode::Char('n'));
    assert_eq!(row_of(&live, "let v80 = 80;"), Some(0), "step 4");
    key(&live, KeyCode::Char('n'));
    assert_eq!(live.with(|a| (a.doc.step, a.cursor_line)), (5, 10));
    assert!(
        row_of(&live, "let v10 = 10;").is_some(),
        "step 5: the cursor"
    );
}

/// `doc` with its typed moves at 40 ms a line, for a typing that ends by
/// its own clock.
fn typing_at_40_ms(mut doc: Document) -> Document {
    for m in &mut doc.moves {
        if let MoveAction::CodePatch(p) = &mut m.action {
            p.speed_ms = Some(40);
        }
    }
    doc
}

/// Runs the typing of the step on screen to its end by its own clock,
/// within 10 s.
fn type_to_the_end(live: &Live) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while live.with(|a| {
        a.typing_tick();
        a.is_typing()
    }) {
        assert!(std::time::Instant::now() < end, "the typing does not end");
    }
}

#[test]
fn a_scroll_after_a_typed_move_jumps_when_the_typing_ends() {
    // Step 2 types two lines at line 10, then scrolls to line 100, which
    // holds `v98` after the two new lines: the typing shows where the view
    // was, and the view jumps when a key ends the typing.
    let live = open("scroll-cue-key", "typed.tiza", 1);
    key(&live, KeyCode::Char('n'));
    assert!(live.with(|a| a.is_typing()));
    assert_eq!(row_of(&live, "let v1 = 1;"), Some(0), "while it types");
    key(&live, KeyCode::Char('x'));
    assert!(!live.with(|a| a.is_typing()));
    assert_eq!(row_of(&live, "let v98 = 98;"), Some(0), "after a key");
    // When the typing ends by its own clock, the view jumps there too.
    let live = open_doc("scroll-cue-after", typing_at_40_ms(load("typed.tiza")), 1);
    key(&live, KeyCode::Char('n'));
    type_to_the_end(&live);
    assert_eq!(row_of(&live, "let v98 = 98;"), Some(0), "after the typing");
}

#[test]
fn a_scroll_before_a_typed_move_jumps_before_the_typing() {
    // Step 3 scrolls to line 60 (`v58`, after the two lines of step 2),
    // then types at line 70: the typing shows in the new view.
    let live = open("scroll-cue-before", "typed.tiza", 2);
    key(&live, KeyCode::Char('n'));
    assert!(live.with(|a| a.is_typing()));
    assert_eq!(row_of(&live, "let v58 = 58;"), Some(0), "while it types");
}

#[test]
fn a_document_or_a_step_an_agent_sends_lands_on_its_camera() {
    // `scene.put` of a document at step 4, and `scene.patch` of a step:
    // the view goes where the `scroll` of that step says, as on a step
    // change.
    let live = open("scroll-agent", "camera.tiza", 1);
    let mut doc: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(fixture("tui/scroll_move/camera.tiza")).unwrap(),
    )
    .unwrap();
    doc["step"] = serde_json::json!(4);
    live.api_ok("scene.put", serde_json::json!({ "document": doc }));
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 1), "scene.put");
    live.api_ok("scene.patch", serde_json::json!({ "step": 2 }));
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(0), "scene.patch");
}

#[test]
fn deleting_the_scroll_of_the_step_leaves_the_view() {
    let live = open("scroll-delete", "camera.tiza", 4);
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 1));
    // The `scroll` of step 4 is move #4.
    live.api_ok("move.delete", serde_json::json!({ "index": 4 }));
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 1));
    assert!(
        live.with(|a| a.scroll_held.is_none()),
        "the view is not held"
    );
}

#[test]
fn the_scroll_command_adds_a_move_at_the_step_and_the_view_goes_there() {
    let live = open("scroll-command", "camera.tiza", 1);
    live.command("scroll 90 align=top");
    let (msg, error) = live.message().unwrap();
    assert!(!error, "{msg}");
    assert_eq!(msg, "move #6 added: step 1 scrolls to line 90 (top)");
    assert!(msg.chars().count() <= 80, "{msg}");
    assert_eq!(row_of(&live, "let v90 = 90;"), Some(0));
    // A text, and an annotation of a later step.
    live.command("scroll @'let v95 = 95;' align=bottom");
    assert!(!live.message().unwrap().1);
    assert_eq!(row_of(&live, "let v95 = 95;"), Some(ROWS - 1));
    live.command("scroll b7 align=top");
    let (msg, error) = live.message().unwrap();
    assert!(!error, "{msg}");
    assert!(msg.ends_with("scrolls to b7 (top)"), "{msg}");
    let moves = live.api_ok("move.list", serde_json::json!({}));
    assert_eq!(moves["moves"].as_array().unwrap().len(), 8, "{moves}");
}

#[test]
fn the_scroll_command_says_its_usage() {
    let live = open("scroll-command-usage", "camera.tiza", 1);
    for bad in [
        "scroll",
        "scroll 50 align=middle",
        "scroll 50 then",
        "scroll x=1",
    ] {
        live.command(bad);
        let (msg, error) = live.message().unwrap();
        assert!(error, ":{bad}: {msg}");
        assert!(
            msg.contains("scroll") || msg.contains("align"),
            ":{bad}: {msg}"
        );
    }
    let moves = live.api_ok("move.list", serde_json::json!({}));
    assert_eq!(moves["moves"].as_array().unwrap().len(), 5, "nothing added");
    live.command("step 0");
    live.command("scroll 50");
    assert!(live.message().unwrap().1, "step 0 takes no move");
}

#[test]
fn a_target_on_the_first_screen_stays_where_it_is() {
    // Near the start no alignment can be met: the view stays at the top,
    // with no overflow (`bottom` once subtracted past zero).
    for align in ["bottom", "center", "show", "top"] {
        let live = open(&format!("scroll-start-{align}"), "camera.tiza", 1);
        live.command(&format!("scroll 3 align={align}"));
        assert!(!live.message().unwrap().1, "{align}");
        let row = row_of(&live, "let v3 = 3;");
        if align == "top" {
            assert_eq!(row, Some(0), "{align}");
        } else {
            assert_eq!(live.with(|a| a.scroll), 0, "{align}");
            assert!(row.is_some(), "{align}");
        }
    }
}

#[test]
fn a_goto_to_the_step_on_screen_puts_the_view_back() {
    let live = open("scroll-goto-same", "camera.tiza", 4);
    key(&live, KeyCode::Char('G'));
    assert_eq!(row_of(&live, "let v50 = 50;"), None);
    live.command("goto 4");
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(ROWS - 1));
}

#[test]
fn a_move_deleted_while_a_step_types_leaves_the_view_on_the_last_scroll() {
    // Step 2 types, then scrolls to line 100 (`v98`). While it types, an
    // agent deletes a move of step 3: the typing ends, and the view goes
    // where the `scroll` of step 2 puts it.
    let live = open("scroll-delete-typing", "typed.tiza", 1);
    key(&live, KeyCode::Char('n'));
    assert!(live.with(|a| a.is_typing()));
    live.api_ok("move.delete", serde_json::json!({ "index": 4 }));
    assert!(!live.with(|a| a.is_typing()));
    assert_eq!(row_of(&live, "let v98 = 98;"), Some(0));
}

#[test]
fn an_agent_sending_a_step_with_no_scroll_puts_the_view_at_the_top() {
    let live = open("scroll-agent-top", "effect.tiza", 1);
    key(&live, KeyCode::Char('n'));
    key(&live, KeyCode::Char('x'));
    assert_eq!(row_of(&live, "let v1 = 1;"), None);
    live.api_ok("scene.patch", serde_json::json!({ "step": 1 }));
    assert_eq!(row_of(&live, "let v1 = 1;"), Some(0));
}

#[test]
fn a_held_view_follows_its_annotation_when_it_moves() {
    // Step 7 has the box b7 on lines 100-104, the target of step 6 with
    // `bottom`. An agent moves the box: the view goes with it.
    let live = open("scroll-annotation-moves", "camera.tiza", 7);
    assert!(row_of(&live, "let v104 = 104;").is_some());
    live.api_ok(
        "annotation.update",
        serde_json::json!({"id": "b7", "move": {"from_line": 60, "to_line": 64}}),
    );
    let row = row_of(&live, "let v64 = 64;").unwrap();
    assert!(row >= ROWS - 3, "row {row}: the box at the bottom");
}

#[test]
fn the_scroll_command_takes_a_line_as_move_list_writes_it() {
    let live = open("scroll-command-l", "camera.tiza", 1);
    live.command("scroll L90 align=top");
    assert!(!live.message().unwrap().1);
    assert_eq!(row_of(&live, "let v90 = 90;"), Some(0));
    live.command("scroll @'unclosed");
    let (msg, error) = live.message().unwrap();
    assert!(error && msg.starts_with("usage: :scroll"), "{msg}");
}

#[test]
fn the_pointer_stays_when_the_new_view_shows_its_line() {
    // The pointer on line 55; step 2 puts line 50 at the top, so line 55
    // is still in view.
    let live = open("scroll-pointer-stays", "camera.tiza", 1);
    live.api_ok("pointer.set", serde_json::json!({ "line": 55 }));
    key(&live, KeyCode::Char('n'));
    assert_eq!(row_of(&live, "let v50 = 50;"), Some(0));
    let pointer = live.with(|a| a.pointer.map(|p| (p.line, p.to)));
    assert_eq!(pointer, Some((55, 55)));
}

#[test]
fn an_agent_edit_during_a_typing_does_not_move_the_view_early() {
    // Step 2 types, then scrolls to line 100 (`v98`). An agent edits a
    // note while it types: the view moves only when the typing ends.
    let live = open("scroll-edit-typing", "typed.tiza", 1);
    key(&live, KeyCode::Char('n'));
    assert!(live.with(|a| a.is_typing()));
    live.api_ok(
        "annotation.update",
        serde_json::json!({"id": "n1", "label": "edited"}),
    );
    assert!(live.with(|a| a.is_typing()));
    assert_eq!(row_of(&live, "let v1 = 1;"), Some(0), "while it types");
    key(&live, KeyCode::Char('x'));
    assert!(!live.with(|a| a.is_typing()));
    assert_eq!(row_of(&live, "let v98 = 98;"), Some(0), "after a key");
}

#[test]
fn undoing_a_scroll_leaves_the_view() {
    let live = open("scroll-undo", "effect.tiza", 1);
    live.command("scroll 60 align=top");
    assert_eq!(row_of(&live, "let v60 = 60;"), Some(0));
    key(&live, KeyCode::Char('u'));
    assert_eq!(
        live.api_ok("move.list", serde_json::json!({}))["moves"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(row_of(&live, "let v60 = 60;"), Some(0), "the view stays");
}

#[test]
fn a_target_ends_before_a_box_that_starts_on_the_next_line() {
    // At step 7 the box b7 starts on line 100: line 99 at the bottom is
    // the last row, the top border of the box under it.
    let live = open("scroll-box-next", "camera.tiza", 7);
    live.command("scroll 99 align=bottom");
    assert_eq!(row_of(&live, "let v99 = 99;"), Some(ROWS - 1));
}

#[test]
fn leaving_a_step_while_it_types_keeps_the_pointer() {
    // The pointer on line 2; step 2 types, then scrolls far below. Going
    // back while it types does not apply that `scroll`: the pointer stays.
    let live = open("scroll-leave-typing", "typed.tiza", 1);
    live.api_ok("pointer.set", serde_json::json!({ "line": 2 }));
    key(&live, KeyCode::Char('n'));
    assert!(live.with(|a| a.is_typing()));
    live.api_ok("step.prev", serde_json::json!({}));
    assert_eq!(live.with(|a| a.doc.step), 1);
    assert_eq!(live.with(|a| a.pointer.map(|p| p.line)), Some(2));
}

#[test]
fn a_pointer_that_stays_after_a_jump_keeps_the_cursor_with_it() {
    // Step 2 jumps to the box b2 (lines 98-102). The pointer is on line
    // 105, which the new view still shows: it stays, and the cursor stays
    // with it, so `k` moves it to line 104, not near the target.
    for present in [true, false] {
        let live = open(
            &format!("scroll-pointer-cursor-{present}"),
            "effect.tiza",
            1,
        );
        live.with(move |a| a.set_present(present));
        live.api_ok("pointer.set", serde_json::json!({ "line": 105 }));
        key(&live, KeyCode::Char('n'));
        let after = live.with(|a| {
            draw(a);
            (a.cursor_line, a.pointer.map(|p| p.line))
        });
        assert_eq!(after, (105, Some(105)), "present {present}: after the jump");
        // The box of the step comes in with `draw`: end it, then `k`.
        live.with(|a| a.finish_effect());
        key(&live, KeyCode::Char('k'));
        let after = live.with(|a| {
            draw(a);
            (a.cursor_line, a.pointer.map(|p| p.line))
        });
        assert_eq!(after, (104, Some(104)), "present {present}: after k");
    }
}

#[test]
fn a_step_with_the_same_camera_keeps_the_cursor_on_a_pointer_that_stayed() {
    // camera.tiza: step 4 jumps to line 50 at the bottom; step 5 keeps
    // that camera. The pointer on line 45 stays in view at the jump, and
    // at step 5 the cursor is still with it.
    let live = open("scroll-pointer-same-camera", "camera.tiza", 3);
    live.with(|a| a.set_present(false));
    live.api_ok("pointer.set", serde_json::json!({ "line": 45 }));
    for step in [4, 5] {
        key(&live, KeyCode::Char('n'));
        let at = live.with(|a| {
            draw(a);
            (a.doc.step, a.cursor_line, a.pointer.map(|p| p.line))
        });
        assert_eq!(at, (step, 45, Some(45)), "step {step}");
    }
}
