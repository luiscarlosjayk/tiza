//! Steps and the keys that move them.
//!
//! Acceptance criteria:
//! - AC1 `n`/`p` and PageDown/PageUp move the step (`→`/`←` only in
//!   presentation mode, where the arrow key tests check them); `h`/`l` move
//!   the column.
//! - AC2 Step 0 shows only the code; the document remembers its step.

use crossterm::event::KeyCode;
use tiza_qa::Live;

#[test]
fn n_p_and_the_page_keys_change_the_step_and_hl_the_column() {
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "keys");
    live.command("note 1 a");
    live.command("note 2 b");
    live.command("step a2 2");
    live.command("step 0");
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(live.with(|app| app.doc.step), 1);
    live.keys(&[KeyCode::PageDown]);
    assert_eq!(live.with(|app| app.doc.step), 2);
    live.keys(&[KeyCode::Char('p'), KeyCode::PageUp, KeyCode::Char('p')]);
    assert_eq!(live.with(|app| app.doc.step), 0, "never below 0");
    live.with(|app| app.cursor_line = 2);
    let col = live.with(|app| app.cursor_col);
    live.keys(&[KeyCode::Char('l')]);
    assert_eq!(live.with(|app| app.cursor_col), col + 1);
    assert_eq!(
        live.with(|app| app.doc.step),
        0,
        "l does not touch the step"
    );
}

#[test]
fn step_one_past_the_last_opens_an_empty_step() {
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "newstep");
    live.command("note 1 first");
    live.command("step 2");
    let (msg, is_err) = live.with(|app| app.message.clone()).unwrap();
    assert!(!is_err, "{msg}");
    assert_eq!(msg, "step 2/2: empty, new annotations go here");
    // The bar and `status` count the new step too: never `step 2/1`.
    let bar = live.screen(120, 20).lines().last().unwrap().to_string();
    assert!(bar.contains("step 2/2") && !bar.contains("2/1"), "{bar}");
    let label = live.api_ok("status", serde_json::json!({}))["step_label"].clone();
    assert_eq!(label, "step 2/2", "{label}");
    live.command("note 2 second");
    assert_eq!(live.with(|app| (app.doc.step, app.doc.max_step())), (2, 2));
    live.command("step 9");
    let (msg, is_err) = live.with(|app| app.message.clone()).unwrap();
    assert!(is_err);
    assert!(
        msg.contains("beyond the last step (2); :step 3 opens a new one"),
        "{msg}"
    );
    assert_eq!(live.with(|app| app.doc.step), 2);
}

#[test]
fn on_the_empty_step_step_n_and_step_set_stay_there() {
    // One step; `:step 2` opens the empty step 2. There, `:step 2`, `n`
    // and the socket's `step.set` stay on it: they do not go back to
    // step 1. `p` goes back.
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "empty-stay");
    live.command("note 1 first");
    live.command("step 2");
    assert_eq!(live.with(|app| app.doc.step), 2);
    live.command("step 2");
    let (msg, is_err) = live.with(|app| app.message.clone()).unwrap();
    assert!(!is_err, "{msg}");
    assert_eq!(msg, "step 2/2: empty, new annotations go here");
    assert_eq!(live.with(|app| app.doc.step), 2, ":step 2");
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(live.with(|app| app.doc.step), 2, "n");
    live.api_ok("step.set", serde_json::json!({"step": 2}));
    assert_eq!(live.with(|app| app.doc.step), 2, "step.set");
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!(live.with(|app| app.doc.step), 1, "p");
}

#[test]
fn on_the_empty_step_the_presentation_counter_counts_it_as_the_bar_does() {
    // The counter of presentation mode said `3/2` on the empty step 3,
    // and the bar said `step 3/3`.
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "empty-counter");
    live.command("note 1 first");
    live.command("step 2");
    live.command("note 2 second");
    live.command("step 3");
    live.with(|a| a.set_present(true));
    let last = presenter_last_row(&live);
    assert!(last.ends_with(" 3/3"), "{last:?}");
    // A document of one step shows no counter at step 1, and `2/2` on the
    // empty step 2: the presenter sees that the step is a new one.
    let one = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "one-counter");
    one.command("note 1 first");
    one.with(|a| a.set_present(true));
    let last = presenter_last_row(&one);
    assert!(!last.contains('/'), "{last:?}");
    one.with(|a| a.set_present(false));
    one.command("step 2");
    one.with(|a| a.set_present(true));
    let last = presenter_last_row(&one);
    assert!(last.ends_with(" 2/2"), "{last:?}");
}

/// The last row of a 70×14 frame, without the spaces at its end.
fn presenter_last_row(live: &Live) -> String {
    let screen = live.screen(70, 14);
    screen.lines().last().unwrap_or("").trim_end().to_string()
}

#[test]
fn on_the_empty_step_n_goes_to_step_0_when_the_document_loops() {
    let live = Live::from_code("fn main() {\n    let s = 1;\n}\n", "rs", "empty-loop");
    live.command("note 1 first");
    live.api_ok("scene.patch", serde_json::json!({"loop": true}));
    live.command("step 2");
    assert_eq!(live.with(|app| app.doc.step), 2);
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(live.with(|app| app.doc.step), 0);
}
