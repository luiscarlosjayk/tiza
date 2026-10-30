//! The follow of the tests that read the followed file by hand: no watcher.
//!
//! Each watcher of the tests put load on fseventsd, for every process of
//! the machine: with many at once, a start took more than 30 s and the
//! test failed. Most tests read the file by hand (`App::read_save`) and need
//! no event of a watcher, so their follow has none (`crate::by_hand`).
//!
//! Acceptance criteria:
//! - AC1 After `App::set_follow_by_hand`, every follow that starts has no
//!   watcher: `:follow on`, `follow.set` and the follow that moves to the
//!   file of a `:ref`. It is not «starting» either, so a test does not wait
//!   for it.
//! - AC2 It makes no read when the watcher would start: `:follow on`
//!   reads the file once, inside the call, and no read is due after it.
//!   Then only the test reads the file.
//! - AC3 The empty first write of an editor is read again after the
//!   debounce, as with a watcher.
//! - AC4 A read by hand while a watcher is on panics, and so does
//!   `set_follow_by_hand` after a follow with a watcher started: the
//!   watcher would read the same save at a time the test does not know.
//!   A read beside the watcher with a follow by hand panics too.
//! - AC5 A panic on the TUI thread of `Live` fails only its test: before,
//!   the drop of `Live` panicked on the poisoned lock, and the whole test
//!   binary aborted.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::json;
use tiza_qa::{Live, scratch};
use tiza_tui::follow::Poll;

const MAIN: &str = "fn main() {\n    println!(\"main\");\n}\n";
const OTHER: &str = "fn other() {\n    println!(\"other\");\n}\n";

/// `main.rs` and `other.rs` in a scratch folder, `main.rs` opened as the
/// TUI opens it, with follow off.
fn opened(name: &str) -> (Live, PathBuf) {
    let dir = scratch(name);
    std::fs::write(dir.join("main.rs"), MAIN).unwrap();
    std::fs::write(dir.join("other.rs"), OTHER).unwrap();
    let opened = tiza_core::fs::open(dir.join("main.rs").to_str().unwrap()).unwrap();
    let live = Live::open(opened.document, opened.save_path, dir.join("live.sock"));
    (live, dir)
}

/// What watches the followed file (`Follow::watching`), or `None` with
/// follow off.
fn watching(live: &Live) -> Option<&'static str> {
    live.with(|app| app.follow.as_ref().map(|f| f.watching()))
}

/// Whether a read of the followed file is due after the debounce.
fn reload_due(live: &Live) -> bool {
    live.with(|app| app.follow.as_ref().is_some_and(|f| f.reload_due()))
}

fn code(live: &Live) -> Vec<String> {
    live.with(|app| app.doc.code.clone())
}

/// AC1, AC2: by `:follow on`, by `follow.set` and after `:ref`, the
/// follow has no watcher, it is not starting, and no read is due.
#[test]
fn a_follow_by_hand_has_no_watcher() {
    let (live, _dir) = opened("no-watcher");
    crate::by_hand(&live);
    // In one call: `Live::command` would wait for a follow that starts.
    let (starting, due) = live.with(|app| {
        app.run_command("follow on");
        let due = app.follow.as_ref().is_some_and(|f| f.reload_due());
        (app.follow_starting(), due)
    });
    assert!(!starting, "a follow by hand is not starting");
    assert!(!due, "a read is due after follow on");
    assert_eq!(watching(&live), Some("by_hand"));
    live.command("follow off");
    assert_eq!(watching(&live), None);
    live.api_ok("follow.set", json!({"enabled": true}));
    assert!(!reload_due(&live), "a read is due after follow.set");
    assert_eq!(watching(&live), Some("by_hand"));
    let due = live.with(|app| {
        app.run_command("ref other.rs");
        app.follow.as_ref().is_some_and(|f| f.reload_due())
    });
    assert!(!due, "a read is due after :ref");
    let path = live.with(|app| app.follow.as_ref().map(|f| f.path.clone()));
    assert!(
        path.as_ref().is_some_and(|p| p.ends_with("other.rs")),
        "{path:?}"
    );
    assert_eq!(watching(&live), Some("by_hand"));
}

/// AC2: the polls of the follow just after it starts give no `Started`,
/// so no read of the file is due.
#[test]
fn a_follow_by_hand_makes_no_read_when_it_starts() {
    let (live, _dir) = opened("no-start-read");
    crate::by_hand(&live);
    let (watch, due, polls) = live.with(|app| {
        app.set_follow(true).unwrap();
        let f = app.follow.as_mut().expect("following");
        (f.watching(), f.reload_due(), [f.poll(), f.poll(), f.poll()])
    });
    // A watcher still starting gives no `Started` yet either.
    assert_eq!(watch, "by_hand");
    assert!(!due, "a read is due after the start");
    assert_eq!(polls, [Poll::Idle, Poll::Idle, Poll::Idle]);
}

/// AC3: an empty first write is read by hand and waits; the second write
/// is read after the debounce, with no other read by hand. Both writes and
/// the read are in one call, so the read again after the debounce comes
/// after the second write.
#[test]
fn the_empty_first_write_is_read_again_after_the_debounce() {
    let (live, dir) = opened("defer");
    crate::by_hand(&live);
    live.command("follow on");
    let src = dir.join("main.rs");
    let kept = live.with(move |app| {
        std::fs::write(&src, "").unwrap();
        app.read_save();
        let kept = app.doc.code.clone();
        std::fs::write(&src, OTHER).unwrap();
        kept
    });
    assert_eq!(kept, MAIN.lines().map(str::to_string).collect::<Vec<_>>());
    let end = Instant::now() + Duration::from_secs(10);
    while code(&live) != OTHER.lines().map(str::to_string).collect::<Vec<_>>() {
        assert!(Instant::now() < end, "no read after the debounce in 10 s");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// AC4: with a watcher on, a read by hand and `set_follow_by_hand` panic.
/// All in one call, while the watcher starts: the test does not wait for
/// it. With a follow by hand, a read beside the watcher panics.
#[test]
fn a_read_by_hand_beside_a_watcher_panics() {
    let (live, _dir) = opened("beside");
    let (watch, read, set) = live.with(|app| {
        app.run_command("follow on");
        let watch = app.follow.as_ref().map(|f| f.watching());
        let read = catch_unwind(AssertUnwindSafe(|| app.read_save())).is_err();
        let set = catch_unwind(AssertUnwindSafe(|| app.set_follow_by_hand())).is_err();
        (watch, read, set)
    });
    assert!(matches!(watch, Some("starting" | "running")), "{watch:?}");
    assert!(read, "a read by hand with a watcher on");
    assert!(set, "by hand after a follow with a watcher");
    let (live, _dir) = opened("beside-by-hand");
    crate::by_hand(&live);
    live.command("follow on");
    let beside = live
        .with(|app| catch_unwind(AssertUnwindSafe(|| app.read_save_beside_the_watcher())).is_err());
    assert!(beside, "a read beside the watcher with a follow by hand");
}

/// AC5: a panic on the TUI thread fails this test alone, with the message
/// of `Live::with`.
#[test]
#[should_panic(expected = "the TUI thread of Live panicked")]
fn a_panic_on_the_tui_thread_fails_only_its_test() {
    let (live, _dir) = opened("panic");
    live.with(|_| panic!("a panic on the TUI thread"));
}
