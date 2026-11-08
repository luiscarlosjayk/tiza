//! The removed lines of a `diff` and follow mode. A save at the step of
//! the diff waits (the step has a patch applied) and loads at step 0. Back
//! at the step of the diff, the removed lines come from the new file: the
//! screen keeps nothing of them from before the save.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::follow::{lines, reloads};
use tiza_qa::{Live, scratch};

const OLD: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";
/// A save of another line: the delete still finds `let b`.
const OTHER_LINE: &str = "fn main() {\n    let a = 10;\n    let b = 2;\n    let c = 3;\n}\n";
/// A save of the line that the delete removes.
const REMOVED_LINE: &str = "fn main() {\n    let a = 1;\n    let b = 20;\n    let c = 3;\n}\n";

/// `main.rs` (OLD) and its sidecar: step 1 deletes `let b` with `diff`.
/// At step 0, following by hand.
fn followed(name: &str) -> (Live, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, OLD).unwrap();
    let doc = json!({
        "schema": 2,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "annotations": [],
        "step": 0,
        "moves": [{
            "at_step": 1, "action": "code.patch", "op": "delete",
            "find": "let b", "diff": true
        }]
    });
    let tiza = dir.join("main.rs.tiza");
    std::fs::write(&tiza, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(doc, tiza, dir.join("live.sock"));
    crate::by_hand(&live);
    live.command("follow on");
    assert_eq!(live.message().unwrap_or_default().0, "following main.rs");
    (live, src)
}

fn goto(live: &Live, step: u32) {
    live.api_ok("step.set", json!({ "step": step }));
}

fn removed(live: &Live) -> Value {
    live.api_ok("code.get", json!({}))["removed"].clone()
}

fn removed_b(text: &str) -> Value {
    json!([{"line": 3, "place": "before", "lines": [text]}])
}

fn screen_text(live: &Live) -> String {
    live.api_ok("render.text", json!({"width": 60}))["text"]
        .as_str()
        .unwrap()
        .to_string()
}

/// The row of the screen text that holds `needle`, and the row under it.
fn rows_of(live: &Live, needle: &str) -> (String, String) {
    let text = screen_text(live);
    let rows: Vec<&str> = text.lines().collect();
    let i = rows
        .iter()
        .position(|r| r.contains(needle))
        .unwrap_or_else(|| panic!("no {needle}:\n{text}"));
    (
        rows[i].to_string(),
        rows.get(i + 1).unwrap_or(&"").to_string(),
    )
}

/// Saves like an editor (a temp file renamed over the target), then reads
/// it as the watcher does after a save.
fn save_and_read(live: &Live, src: &Path, content: &str) {
    let tmp = src.with_extension("rs.tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, src).unwrap();
    live.with(|app| app.read_save());
}

/// At step 1, saves `content`: follow waits. Step 0 loads it, then the
/// presenter is back at step 1.
fn save_at_the_diff_and_come_back(live: &Live, src: &Path, content: &str) {
    goto(live, 1);
    assert_eq!(removed(live), removed_b("    let b = 2;"));
    save_and_read(live, src, content);
    let status = live.api_ok("follow.status", json!({}));
    assert_eq!(status["paused"], true, "{status}");
    assert_eq!(status["pending"], true, "{status}");
    assert_eq!(reloads(live), 0);
    goto(live, 0);
    assert_eq!(reloads(live), 1);
    assert_eq!(live.with(|app| app.doc.base().to_vec()), lines(content));
    goto(live, 1);
}

#[test]
fn a_reload_of_another_line_keeps_the_removed_line_over_its_line() {
    let (live, src) = followed("diff-follow-other-line");
    save_at_the_diff_and_come_back(&live, &src, OTHER_LINE);
    assert_eq!(removed(&live), removed_b("    let b = 2;"));
    let (gone, under) = rows_of(&live, "let b = 2;");
    assert!(gone.contains('−'), "{gone}");
    assert!(under.contains("let c = 3;"), "{under}");
    let (above, _) = rows_of(&live, "let a = 10;");
    assert!(!above.contains('−'), "{above}");
}

#[test]
fn a_reload_of_the_removed_line_shows_its_new_text() {
    let (live, src) = followed("diff-follow-removed-line");
    save_at_the_diff_and_come_back(&live, &src, REMOVED_LINE);
    assert_eq!(removed(&live), removed_b("    let b = 20;"));
    let (gone, under) = rows_of(&live, "let b = 20;");
    assert!(gone.contains('−'), "{gone}");
    assert!(under.contains("let c = 3;"), "{under}");
    let text = screen_text(&live);
    assert!(!text.contains("let b = 2;"), "{text}");
}
