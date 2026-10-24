//! The follow tests hung 75 s and failed with «socket call: WouldBlock»:
//! the in-process harness made `follow.set` wait inside the request for the
//! file watcher, and on macOS that start took seconds.
//!
//! The cause, measured: fseventsd reads the folder of the client's
//! executable each time a watcher starts, and serves one start at a time
//! per process. `target/debug/deps` held a million old `.o` files, so each
//! start took about 2 s, and the starts of the tests that run in parallel
//! waited for each other (the last ones 40 to 60 s). The tiza binary, in
//! `target/debug`, starts its watcher in 1 ms.
//!
//! Acceptance criteria:
//! - AC1 The harness never blocks a socket request on the watcher start:
//!   the test waits on a condition (the watcher runs) with its own budget,
//!   outside the request.
//! - AC2 A start that takes longer than the budget fails the test with a
//!   message that names a slow FSEvents start and the time waited.
//! - AC3 The cause is measured (above): not the pile-up of watchers that
//!   stop, which only follows from the slow starts. The harness does not
//!   wait for them.
//! - AC4 No `#[ignore]`, no retries, no sleeps as synchronisation.
//! - AC5 The follow suites pass 5 runs in a row with the Mac loaded as in
//!   a full gate (evidence outside the repo, on the card).
//!
//! Corner case: the product does not change; `follow.set` still answers
//! at once and the TUI never waits for fseventsd. t-228 shows it with the
//! binary and a start held 3 s (`TIZA_TEST_HOOK=follow:slow`); the
//! in-process tests have no hooks.

use std::time::{Duration, Instant};

use serde_json::json;
use tiza_qa::Live;

const CODE: &str = "fn main() {\n    println!(\"hi\");\n}\n";

/// A followed `main.rs` in a document of its own.
fn followed(name: &str) -> (Live, std::path::PathBuf) {
    let dir = tiza_qa::scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, CODE).unwrap();
    let mut doc = tiza_core::Document::from_code(CODE, "rs", None);
    doc.reference_file("main.rs", None);
    let live = Live::open(doc, dir.join("main.rs.tiza"), dir.join("s.sock"));
    (live, src)
}

#[test]
fn ac2_a_watcher_past_the_budget_fails_and_names_fsevents() {
    // A watcher that never starts: the wait ends at its budget.
    let asked = Instant::now();
    let why = tiza_qa::wait_for_watcher(Duration::from_millis(300), || true).unwrap_err();
    assert!(
        asked.elapsed() < Duration::from_secs(5),
        "the wait went on past its budget: {:?}",
        asked.elapsed()
    );
    // The time waited: the budget or a little more, on a loaded machine.
    let waited: f64 = why
        .strip_prefix("the follow watcher did not start in ")
        .and_then(|rest| rest.split_once(" s"))
        .and_then(|(secs, _)| secs.parse().ok())
        .unwrap_or_else(|| panic!("{why}"));
    assert!((0.3..5.0).contains(&waited), "{why}");
    if cfg!(target_os = "macos") {
        assert!(why.contains(" s: FSEvents starts it slowly."), "{why}");
        assert!(
            why.contains("fseventsd reads the folder of the test binary for each watcher ("),
            "{why}"
        );
        assert!(why.contains(" files);"), "{why}");
        assert!(why.ends_with("run `cargo clean` to empty it"), "{why}");
    }
}

#[test]
fn ac1_the_app_answers_requests_while_the_harness_waits() {
    // The wait runs on the test's thread: requests to the instance are
    // answered at once all along it.
    let (live, _) = followed("t243-answers");
    let mut answered = 0;
    let why = tiza_qa::wait_for_watcher(Duration::from_millis(400), || {
        let asked = Instant::now();
        let reply = tiza_tui::ipc::call(&live.socket, "follow.status", json!({})).unwrap();
        assert!(reply.error.is_none(), "{reply:?}");
        assert!(
            asked.elapsed() < Duration::from_secs(2),
            "{:?}",
            asked.elapsed()
        );
        answered += 1;
        true
    })
    .unwrap_err();
    assert!(
        why.starts_with("the follow watcher did not start in "),
        "{why}"
    );
    assert!(answered >= 2, "{answered}");
}

#[test]
fn ac1_the_harness_waits_for_the_watcher_after_follow_on() {
    // `command`, `api` and `keys` return with the watcher running. (A save
    // made while it starts is read too, once it runs; the reload below only
    // shows that follow works.)
    let (live, src) = followed("t243-harness");
    live.command("follow on");
    assert!(!live.with(|app| app.follow_starting()));
    live.api_ok("follow.set", json!({"enabled": false}));
    live.api_ok("follow.set", json!({"enabled": true}));
    assert!(!live.with(|app| app.follow_starting()));
    std::fs::write(&src, CODE.replace("hi", "hello")).unwrap();
    let start = Instant::now();
    while live.api_ok("follow.status", json!({}))["reloads"] != 1 {
        assert!(start.elapsed() < Duration::from_secs(10), "no reload");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        live.with(|app| app.doc.base().join("\n")).contains("hello"),
        "the save was read"
    );
}

#[test]
fn ac1_follow_set_answers_and_the_watcher_runs_soon_after() {
    // The raw call, as an agent makes it: the reply does not wait for the
    // watcher, and the harness then sees it run within its budget.
    let (live, _) = followed("t243-raw");
    let reply = tiza_tui::ipc::call(&live.socket, "follow.set", json!({"enabled": true})).unwrap();
    assert!(reply.error.is_none(), "{reply:?}");
    live.wait_follow_started_within(tiza_qa::FOLLOW_START_BUDGET)
        .unwrap();
    assert_eq!(live.api_ok("follow.status", json!({}))["enabled"], true);
}
