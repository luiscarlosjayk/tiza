//! `app.quit` closes the instance, as `:q` does (`tiza close` sends it).
//!
//! Acceptance criteria:
//! - AC1 `app.quit` takes `{"force": bool}`, `false` by default; no
//!   parameters is `{}`; an unknown key is `invalid_argument`. Its reply is
//!   `{"closing": true}` and the instance ends as with `:q`.
//! - AC2 Without `force`, when closing loses work (changes that are not
//!   saved, or the marks of stale moves), it refuses with
//!   `unsaved_changes`, and the instance stays as it was. With nothing to
//!   lose it closes in every mode (read-only, `--no-save`, autosave off).
//!   With `force` it closes.
//! - AC3 In supervised mode it does not wait for review, also while an
//!   agent change waits.
//! - AC4 The requests that wait after an `app.quit` that closes the
//!   instance are not applied. An `app.quit` that waited 10 s or more (`tiza
//!   close` stopped waiting and said that the instance did not close) gets
//!   `unavailable`, and the instance stays.
//!
//! The loop that ends on the flag is the TUI's: `qa/tests/tui/close_instance.rs`.

use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_core::protocol::{Request, Response};
use tiza_qa::ipc::{send_async, wait_pending};
use tiza_qa::{Live, scratch};
use tiza_tui::app::App;
use tiza_tui::ipc::{CLOSE_ANSWER_WAIT, IpcRequest};

const CODE: &str = "fn main() {\n    let a = 1;\n}\n";

fn quit(live: &Live) -> bool {
    live.with(|a| a.quit)
}

fn note(live: &Live) {
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 2, "text": "kept"}),
    );
}

/// The code of the refusal, and the instance still open.
fn refused(live: &Live, params: Value) -> String {
    let r = live.api("app.quit", params);
    assert!(!quit(live), "it refused, and the instance quits");
    let e = r.error.expect("a refusal");
    e.code
}

/// A change of the mode of the instance.
type SetMode = fn(&mut tiza_tui::app::App);

// ---------------------------------------------------------------- AC1

#[test]
fn app_quit_replies_closing_and_ends_the_instance() {
    for (name, params) in [
        ("quit-empty", json!({})),
        ("quit-null", Value::Null),
        ("quit-false", json!({"force": false})),
    ] {
        let live = Live::from_code(CODE, "rs", name);
        assert_eq!(live.api_ok("app.quit", params), json!({"closing": true}));
        assert!(quit(&live), "{name}");
    }
}

#[test]
fn app_quit_refuses_an_unknown_key_or_a_value_that_is_not_an_object() {
    let live = Live::from_code(CODE, "rs", "quit-keys");
    assert_eq!(refused(&live, json!({"forse": true})), "invalid_argument");
    assert_eq!(refused(&live, json!([true])), "invalid_argument");
    assert_eq!(refused(&live, json!({"force": "yes"})), "invalid_argument");
}

// ---------------------------------------------------------------- AC2

#[test]
fn app_quit_refuses_unsaved_changes_unless_forced() {
    let live = Live::from_code(CODE, "rs", "quit-unsaved");
    live.with(|a| a.autosave = false);
    note(&live);
    let r = live.api("app.quit", json!({}));
    let e = r.error.expect("a refusal");
    assert_eq!(e.code, "unsaved_changes");
    assert_eq!(
        e.message,
        "the instance has unsaved changes: tiza close --force closes it and loses them"
    );
    assert!(!quit(&live));
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1, "nothing changed");
    assert_eq!(
        live.api_ok("app.quit", json!({"force": true})),
        json!({"closing": true})
    );
    assert!(quit(&live));
}

#[test]
fn app_quit_refuses_changes_that_could_not_be_saved() {
    // Read-only: the save failed, so the change is only in the instance.
    let live = Live::from_code(CODE, "rs", "quit-read-only");
    note(&live);
    live.with(|a| {
        a.dirty = true;
        a.save_error = Some("permission denied".into());
    });
    assert_eq!(refused(&live, json!({})), "unsaved_changes");
    live.api_ok("app.quit", json!({"force": true}));
    assert!(quit(&live));
}

#[test]
fn app_quit_refuses_the_marks_of_stale_moves_unless_forced() {
    let dir = scratch("quit-stale");
    let doc = json!({
        "schema": 1,
        "source": {"language": "rs"},
        "code": CODE.lines().collect::<Vec<_>>(),
        "step": 0,
        "moves": [{"at_step": 1, "action": "code.patch", "op": "replace", "position": "2", "content": "    let a = 2;"}],
        "annotations": [],
    });
    let path = dir.join("doc.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    let d = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(d, path, dir.join("s.sock"));
    // A paste that edits line 2 marks the move (autosave saves the code).
    live.with(|a| a.replace_code("fn main() {\n    let b = 1;\n}\n"))
        .unwrap();
    assert_eq!(live.api_ok("status", json!({}))["stale_moves"], 1);
    assert_eq!(refused(&live, json!({})), "unsaved_changes");
    live.api_ok("app.quit", json!({"force": true}));
    assert!(quit(&live));
}

#[test]
fn app_quit_with_nothing_to_lose_closes_in_every_mode() {
    let modes: [(&str, SetMode); 3] = [
        ("quit-mode-read-only", |a| {
            a.save_error = Some("permission denied".into())
        }),
        ("quit-mode-no-save", |a| {
            a.no_save = true;
            a.autosave = false;
        }),
        ("quit-mode-autosave-off", |a| a.autosave = false),
    ];
    for (name, set) in modes {
        let live = Live::from_code(CODE, "rs", name);
        live.with(set);
        assert_eq!(
            live.api_ok("app.quit", json!({})),
            json!({"closing": true}),
            "{name}"
        );
        assert!(quit(&live), "{name}");
    }
    // Autosave on: the change is on disk, so nothing is lost.
    let live = Live::from_code(CODE, "rs", "quit-mode-autosave-on");
    note(&live);
    live.api_ok("app.quit", json!({}));
    assert!(quit(&live));
}

// ---------------------------------------------------------------- AC3

#[test]
fn app_quit_does_not_wait_for_review_in_supervised_mode() {
    let live = Live::from_code(CODE, "rs", "quit-supervised");
    live.api_ok("supervise.set", json!({"enabled": true}));
    let parked = send_async(
        &live.socket,
        "annotation.add",
        json!({"type": "note", "line": 2, "text": "proposed"}),
    );
    wait_pending(&live, 1);
    assert_eq!(live.api_ok("app.quit", json!({})), json!({"closing": true}));
    assert!(quit(&live));
    // The change still waits for the presenter: closing did not accept it.
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0);
    // As the TUI exits, the agent learns why the change got no result.
    live.with(|a| a.close_waiting_changes());
    let e = parked.join().unwrap().error.expect("a refusal");
    assert_eq!(e.code, "unavailable");
    assert_eq!(
        e.message,
        "Tiza closed before the review: the change was not applied"
    );
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0);
}

// ---------------------------------------------------------------- AC4

/// A request as the connection gives it to the TUI, that came at `since`,
/// and the channel of its reply.
fn request(method: &str, params: Value, since: Instant) -> (IpcRequest, Receiver<Response>) {
    let (reply, replied) = channel();
    let (_written, written) = channel();
    let req = Request {
        id: json!(1),
        method: method.to_string(),
        params,
    };
    (
        IpcRequest {
            req,
            reply,
            written,
            since,
        },
        replied,
    )
}

fn app(name: &str) -> App {
    let dir = scratch(name);
    App::new(
        tiza_core::Document::from_code(CODE, "rs", None),
        dir.join("doc.tiza"),
    )
}

#[test]
fn the_requests_after_app_quit_are_not_applied() {
    let mut app = app("quit-drain");
    let now = Instant::now();
    let (quit, quit_reply) = request("app.quit", json!({}), now);
    let note = json!({"type": "note", "line": 2, "text": "late"});
    let (add, add_reply) = request("annotation.add", note, now);
    let drained = tiza_tui::drain_requests(&mut app, vec![quit, add].into_iter());
    assert!(drained.quit_written.is_some());
    assert!(app.quit);
    assert_eq!(
        quit_reply.recv().unwrap().result,
        Some(json!({"closing": true}))
    );
    assert!(
        app.doc.annotations.is_empty(),
        "the add after it was applied"
    );
    // Not answered: its connection gives `unavailable`.
    assert!(add_reply.recv().is_err());
}

#[test]
fn an_app_quit_that_waited_too_long_is_refused() {
    let mut app = app("quit-late");
    let since = Instant::now()
        .checked_sub(CLOSE_ANSWER_WAIT + Duration::from_secs(1))
        .unwrap();
    let (quit, reply) = request("app.quit", json!({"force": true}), since);
    let drained = tiza_tui::drain_requests(&mut app, std::iter::once(quit));
    assert!(drained.quit_written.is_none());
    assert!(!app.quit, "the late app.quit closed the instance");
    let e = reply.recv().unwrap().error.expect("a refusal");
    assert_eq!(e.code, "unavailable");
    assert_eq!(
        e.message,
        "app.quit waited more than 10 s, and tiza close stops waiting then: the instance stays open. Send it again"
    );
}
