//! Follow: a save through a temporary file reloads once after the debounce,
//! and a burst of writes inside the debounce window is one reload. The burst
//! counts reloads, so the test runs alone: in a crate with many other
//! watchers, macOS can deliver one burst in two batches.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::json;
use tiza_core::Document;
use tiza_qa::{Live, scratch};

const CODE: &str = "fn main() {\n    let s = String::new();\n    takes(s);\n    println!(\"done\");\n}\n\nfn takes(s: String) {}\n";

/// A source file next to its sidecar document, opened by reference.
fn followed(name: &str, code: &str) -> (Live, PathBuf) {
    let dir = scratch(name);
    let src = std::path::absolute(dir.join("main.rs")).unwrap();
    std::fs::write(&src, code).unwrap();
    let mut doc = Document::from_code(code, "rs", Some(src.display().to_string()));
    doc.reference_file(src.display().to_string(), None);
    let live = Live::open(doc, dir.join("main.rs.tiza"), dir.join("live.sock"));
    (live, src)
}

/// Saves like Zed does: write a temp file, then rename over the target.
fn save_like_zed(path: &Path, content: &str) {
    let tmp = path.with_extension("rs.tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

fn reloads(live: &Live) -> u64 {
    live.api_ok("follow.status", json!({}))["reloads"]
        .as_u64()
        .unwrap_or(0)
}

/// How long a positive wait may take: generous, because macOS FSEvents and
/// loaded CI runners can deliver an event well after the write.
const WAIT: Duration = Duration::from_secs(10);

/// Quiet time after which "nothing happened" is believed (several times the
/// debounce plus the worst event latency seen on CI).
fn settle() {
    std::thread::sleep(Duration::from_millis(700));
}

/// Waits (up to `WAIT`) until the reload counter reaches `n`.
fn wait_reloads(live: &Live, n: u64) {
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

fn code(live: &Live) -> Vec<String> {
    live.with(|app| app.doc.code.clone())
}

#[test]
fn tmp_rename_save_reloads_once_after_debounce() {
    let _alone = crate::alone();
    let (live, src) = followed("ac1", CODE);
    live.command("follow on");
    assert_eq!(live.message().unwrap().0, "following main.rs");
    assert_eq!(
        reloads(&live),
        0,
        "enabling with unchanged content is not a reload"
    );

    save_like_zed(&src, &CODE.replace("done", "finished"));
    wait_reloads(&live, 1);
    assert!(code(&live)[3].contains("finished"), "{:?}", code(&live));
    let (msg, is_err) = live.message().unwrap();
    assert!(!is_err);
    assert_eq!(msg, "main.rs reloaded (#1): 0 annotations kept");

    // A burst of writes within the debounce window is one reload.
    // The writes follow each other with no pause: the burst lasts about a
    // millisecond, far inside the debounce window.
    for i in 0..5 {
        std::fs::write(&src, CODE.replace("done", &format!("v{i}"))).unwrap();
    }
    wait_reloads(&live, 2);
    settle();
    assert_eq!(reloads(&live), 2, "burst must collapse into one reload");
    assert!(code(&live)[3].contains("v4"), "{:?}", code(&live));
}

// ---------- repeated lines: the nearest copy, never the first ----------

// ---------- QA round 2 (Luis, real Zed): ranges, formatter reflow, notes, undo ----------
