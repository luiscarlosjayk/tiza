//! Save, autosave and failures (QA-agent bug: masked save errors).
//!
//! Acceptance criteria:
//! - AC1 Autosave writes the sidecar after every change.
//! - AC2 A failed save stays visible on the status line (in the error color)
//!   even after later info messages, until a save succeeds. `status` gives
//!   its reason in `save_error` (`null` after a save that worked).
//! - AC3 `q` refuses to quit while changes could not be saved; `:q!` quits;
//!   `:saveas other/path` then `q` works (`:w other/path` only writes a
//!   copy now, as in Vim).

use crossterm::event::KeyCode;
use tiza_qa::{Live, scratch};

#[test]
fn autosave_writes_the_sidecar() {
    let dir = scratch("autosave");
    let save = dir.join("doc.tiza");
    let live = Live::open(
        tiza_core::Document::from_code("fn main() {}\n", "rs", None),
        save.clone(),
        dir.join("s.sock"),
    );
    live.command("note 1 hello");
    assert!(save.exists());
    assert!(std::fs::read_to_string(&save).unwrap().contains("hello"));
}

#[test]
fn failed_save_is_sticky_and_blocks_q() {
    let dir = scratch("fail");
    // A directory where the document should go: the save cannot succeed.
    let save = dir.join("doc.tiza");
    std::fs::create_dir_all(&save).unwrap();
    let live = Live::open(
        tiza_core::Document::from_code("fn main() {}\n", "rs", None),
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
    // An agent sees it too: `status` gives the reason.
    let why = live.api_ok("status", serde_json::json!({}))["save_error"].clone();
    assert!(why.as_str().is_some_and(|w| !w.is_empty()), "{why}");
    live.keys(&[KeyCode::Char('q')]);
    assert!(!live.with(|app| app.quit), "q refused while the save fails");
    let bar = live.screen(120, 20).lines().last().unwrap().to_string();
    assert!(bar.contains(":q!"), "the way out comes first: {bar}");
    // Saving elsewhere clears the error and q works again.
    let other = dir.join("other.tiza");
    live.command(&format!("saveas {}", other.display()));
    assert!(other.exists());
    assert!(live.with(|app| app.save_error.is_none()));
    let status = live.api_ok("status", serde_json::json!({}));
    assert!(status["save_error"].is_null(), "{status}");
    live.keys(&[KeyCode::Char('q')]);
    assert!(live.with(|app| app.quit));
}

#[test]
fn q_bang_quits_anyway() {
    let dir = scratch("qbang");
    let save = dir.join("doc.tiza");
    std::fs::create_dir_all(&save).unwrap();
    let live = Live::open(
        tiza_core::Document::from_code("fn main() {}\n", "rs", None),
        save,
        dir.join("s.sock"),
    );
    live.command("note 1 x");
    live.command("q!");
    assert!(live.with(|app| app.quit));
}

#[test]
fn wq_does_not_quit_when_the_write_fails() {
    let dir = scratch("wq");
    let save = dir.join("doc.tiza");
    std::fs::create_dir_all(&save).unwrap();
    let live = Live::open(
        tiza_core::Document::from_code("fn main() {}\n", "rs", None),
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
