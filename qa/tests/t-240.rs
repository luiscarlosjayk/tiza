//! `:hl L:text` with a text the code does not have said `text "signature"
//! not found in the code`: not which code it searched (a `code.link` can
//! show another file), nor that notes and labels are not searched. And the
//! hint for a text on another line was cut on the bar at 120 columns
//! (`use 13:VecDeq…`).
//!
//! Acceptance criteria:
//! - AC1 Not found anywhere: «signature» is not in the code shown
//!   (src/queue.rs:10-36), and `L:text` looks in the code, not in notes or
//!   labels.
//! - AC2 Found in a file the demo shows at another step: which file, line
//!   and step.
//! - AC3 The suggested command is never cut so that it cannot be read.
//! - AC4 This file; CHANGELOG (Changed).
//!
//! Corner cases: a text on a line outside the `code.link` window says it is
//! outside the window; `:arrow` and `:mark` use the same anchors and give
//! the same messages.

use tiza_qa::{Live, example};

/// examples/stack-and-queue.tiza at `step` (2-3: src/queue.rs:10-36, 4:
/// src/stack.rs, 5: src/queue.rs:17-23), never saved.
fn at_step(name: &str, step: u32) -> Live {
    let path = example("stack-and-queue.tiza");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let dir = tiza_qa::scratch(name);
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.with(move |a| {
        a.autosave = false;
        a.set_step(step);
        a.size = (120, 30);
    });
    live
}

/// The error a command left on the bar.
fn error(live: &Live, cmd: &str) -> String {
    live.command(cmd);
    let (msg, is_err) = live.message().unwrap_or_default();
    assert!(is_err, "{cmd}: not an error: {msg}");
    msg
}

#[test]
fn step1_ac1_a_text_nowhere_says_which_code_was_searched() {
    let live = at_step("t240-nowhere", 3);
    assert_eq!(
        error(&live, "hl 22:signature"),
        "«signature» is not in the code shown (src/queue.rs:10-36). \
         L:text skips notes and labels."
    );
}

#[test]
fn step2_a_text_on_another_line_says_which_line() {
    let live = at_step("t240-other-line", 3);
    assert_eq!(
        error(&live, "hl 22:push_back"),
        "use 18:push_back («push_back» is on L18, not L22)"
    );
}

#[test]
fn step3_ac3_the_suggested_command_is_readable_at_120_columns() {
    let live = at_step("t240-bar", 3);
    let msg = error(&live, "hl 22:VecDeque");
    assert!(msg.starts_with("use 13:VecDeque "), "{msg}");
    let screen = live.screen(120, 30);
    let bar = screen.lines().last().unwrap();
    assert!(bar.contains("use 13:VecDeque"), "{bar}");
}

#[test]
fn ac2_a_text_in_a_file_another_step_shows() {
    // `Vec::new` is in src/stack.rs (step 4), not in src/queue.rs.
    let live = at_step("t240-other-file", 3);
    assert_eq!(
        error(&live, "hl 22:Vec::new"),
        "«Vec::new» is in src/stack.rs at L9, shown at step 4."
    );
}

#[test]
fn corner_a_text_outside_the_window() {
    // Step 5 shows src/queue.rs:17-23; `VecDeque::new` is on L13.
    let live = at_step("t240-window", 5);
    assert_eq!(
        error(&live, "hl 20:VecDeque::new"),
        "«VecDeque::new» is in src/queue.rs at L13, outside the lines shown at step 5 (17-23)."
    );
}

#[test]
fn corner_arrow_and_mark_give_the_same_messages() {
    let live = at_step("t240-arrow-mark", 3);
    let nowhere = "«signature» is not in the code shown (src/queue.rs:10-36). \
                   L:text skips notes and labels.";
    assert_eq!(error(&live, "arrow 18:push_back 22:signature x"), nowhere);
    assert_eq!(error(&live, "mark warn 22:signature"), nowhere);
    assert_eq!(
        error(&live, "mark warn 22:push_back"),
        "use 18:push_back («push_back» is on L18, not L22)"
    );
}

#[test]
fn ac4_changelog() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(
        changelog.contains("L:text skips notes and labels"),
        "CHANGELOG"
    );
}
