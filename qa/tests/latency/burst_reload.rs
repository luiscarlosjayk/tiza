//! Follow: a save through a temporary file reloads once after the debounce,
//! and a burst of writes inside the debounce window is one reload. The burst
//! counts reloads, so the test runs alone: in a crate with many other
//! watchers, macOS can deliver one burst in two batches.

use tiza_qa::follow::{code, followed, reloads, save_like_zed, wait_reloads};

const CODE: &str = "fn main() {\n    let s = String::new();\n    takes(s);\n    println!(\"done\");\n}\n\nfn takes(s: String) {}\n";

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
    // Then a save of the same text: its read comes after the events of the
    // burst, so when it is done, a reload more of the burst would show.
    // It goes through a rename: in place, the same size can keep the stamp
    // (a coarse mtime), and the wait would not wait for its read.
    save_like_zed(&src, &CODE.replace("done", "v4"));
    assert_eq!(
        live.wait_follow_read(&src).reloads,
        2,
        "burst must collapse into one reload"
    );
    assert!(code(&live)[3].contains("v4"), "{:?}", code(&live));
}

// ---------- repeated lines: the nearest copy, never the first ----------

// ---------- QA round 2 (Luis, real Zed): ranges, formatter reflow, notes, undo ----------
