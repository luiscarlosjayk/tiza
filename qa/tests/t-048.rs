//! t-048 — Save, autosave and failures (QA-agent bug: masked save errors).
//!
//! Acceptance criteria:
//! - AC1 Autosave writes the sidecar after every change.
//! - AC2 A failed save stays visible on the status line (in the error color)
//!   even after later info messages, until a save succeeds.
//! - AC3 `q` refuses to quit while changes could not be saved; `:q!` quits;
//!   `:w other/path` then `q` works.

use codedraw_qa::{Live, scratch};
use crossterm::event::KeyCode;

#[test]
fn ac1_autosave_writes_the_sidecar() {
    let dir = scratch("t048-autosave");
    let save = dir.join("doc.codedraw");
    let live = Live::open(
        codedraw_core::Document::from_code("fn main() {}\n", "rs", None),
        save.clone(),
        dir.join("s.sock"),
    );
    live.command("note 1 hello");
    assert!(save.exists());
    assert!(std::fs::read_to_string(&save).unwrap().contains("hello"));
}

#[test]
fn ac2_ac3_failed_save_is_sticky_and_blocks_q() {
    let dir = scratch("t048-fail");
    // A directory where the document should go: the save cannot succeed.
    let save = dir.join("doc.codedraw");
    std::fs::create_dir_all(&save).unwrap();
    let live = Live::open(
        codedraw_core::Document::from_code("fn main() {}\n", "rs", None),
        save.clone(),
        dir.join("s.sock"),
    );
    live.command("note 1 three");
    let bar = live.screen(120, 20).lines().last().unwrap().to_string();
    assert!(
        bar.contains("could not save"),
        "error wins over the info message: {bar}"
    );
    assert!(live.with(|app| app.save_error.is_some()));
    live.keys(&[KeyCode::Char('q')]);
    assert!(!live.with(|app| app.quit), "q refused while the save fails");
    let bar = live.screen(120, 20).lines().last().unwrap().to_string();
    assert!(bar.contains(":q!"), "the way out comes first: {bar}");
    // Saving elsewhere clears the error and q works again.
    let other = dir.join("other.codedraw");
    live.command(&format!("w {}", other.display()));
    assert!(other.exists());
    assert!(live.with(|app| app.save_error.is_none()));
    live.keys(&[KeyCode::Char('q')]);
    assert!(live.with(|app| app.quit));
}

#[test]
fn ac3_q_bang_quits_anyway() {
    let dir = scratch("t048-qbang");
    let save = dir.join("doc.codedraw");
    std::fs::create_dir_all(&save).unwrap();
    let live = Live::open(
        codedraw_core::Document::from_code("fn main() {}\n", "rs", None),
        save,
        dir.join("s.sock"),
    );
    live.command("note 1 x");
    live.command("q!");
    assert!(live.with(|app| app.quit));
}

#[test]
fn qa_wq_does_not_quit_when_the_write_fails() {
    let dir = scratch("t048-wq");
    let save = dir.join("doc.codedraw");
    std::fs::create_dir_all(&save).unwrap();
    let live = Live::open(
        codedraw_core::Document::from_code("fn main() {}\n", "rs", None),
        save,
        dir.join("s.sock"),
    );
    live.command("wq");
    assert!(
        !live.with(|app| app.quit),
        ":wq must stay when the write fails"
    );
    assert!(live.with(|app| app.save_error.is_some()));
}
