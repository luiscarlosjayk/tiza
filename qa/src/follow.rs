//! Helpers of the tests that follow a file: save it, wait for the reload,
//! read the code and the saved document.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use tiza_core::Document;

use crate::{Live, scratch};

/// The code of the repro of [`repro`]: a move at step 1, no `step`.
pub const OLD: &str = "fn main() {\n    let x = 1;\n}\n";

/// How long a positive wait may take: generous, because macOS FSEvents and
/// loaded CI runners can deliver an event well after the write.
pub const WAIT: Duration = Duration::from_secs(10);

/// The code of the document of the live app, one string per line.
pub fn code(live: &Live) -> Vec<String> {
    live.with(|app| app.doc.code.clone())
}

/// `main.rs` (OLD) and `doc.tiza` (the repro) in a scratch folder.
pub fn files(name: &str, note: bool) -> (PathBuf, PathBuf, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, OLD).unwrap();
    let tiza = dir.join("doc.tiza");
    std::fs::write(&tiza, repro(note).to_string()).unwrap();
    (dir, src, tiza)
}

/// A source file next to its sidecar document, opened by reference.
pub fn followed(name: &str, code: &str) -> (Live, PathBuf) {
    let dir = scratch(name);
    let src = std::path::absolute(dir.join("main.rs")).unwrap();
    std::fs::write(&src, code).unwrap();
    let mut doc = Document::from_code(code, "rs", Some(src.display().to_string()));
    doc.reference_file(src.display().to_string(), None);
    let live = Live::open(doc, dir.join("main.rs.tiza"), dir.join("live.sock"));
    (live, src)
}

/// The lines of `text`.
pub fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

/// The reload counter of `follow.status` (0 when there is none).
pub fn reloads(live: &Live) -> u64 {
    live.api_ok("follow.status", json!({}))["reloads"]
        .as_u64()
        .unwrap_or(0)
}

/// The document of the repro, with a note or without it.
pub fn repro(note: bool) -> Value {
    let mut doc = json!({
        "schema": 1,
        "source": {"language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "annotations": [],
        "moves": [{
            "at_step": 1, "action": "code.patch", "op": "replace",
            "find": "x = 1", "content": "    let x = 2;"
        }]
    });
    if note {
        // Something drawn on the patched code, for the fresh-load checks.
        doc["annotations"] =
            json!([{"id": "a1", "step": 1, "type": "note", "line": 2, "text": "now 2"}]);
    }
    doc
}

/// Saves like Zed does: write a temp file, then rename over the target.
pub fn save_like_zed(path: &Path, content: &str) {
    let tmp = path.with_extension("rs.tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

/// The `code` of the document saved at `tiza`.
pub fn saved_code(tiza: &Path) -> Value {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(tiza).unwrap()).unwrap();
    v["code"].clone()
}

/// Waits (up to `WAIT`) until the reload counter reaches `n`.
pub fn wait_reloads(live: &Live, n: u64) {
    let start = Instant::now();
    while reloads(live) < n {
        assert!(
            start.elapsed() < WAIT,
            "timed out waiting for reload #{n} (at {})",
            reloads(live)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
