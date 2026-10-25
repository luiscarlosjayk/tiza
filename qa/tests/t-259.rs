//! An agent with the socket could turn supervised mode off with
//! `supervise.set`, and no one reviewed it: the changes that waited were
//! rejected in the presenter's name.
//!
//! Acceptance criteria:
//! - AC1 In supervised mode, `supervise.set` with `enabled: false` from the
//!   socket waits as a change to review, «turn supervised mode off». When
//!   the presenter accepts it, supervised mode goes off and the agent gets
//!   the status. When the presenter rejects it, supervised mode stays on.
//!   `enabled: true` is at once.
//! - AC2 When supervised mode goes off (an accepted request or the
//!   presenter's `:supervise off`), each change that waited is rejected
//!   with a reason that says so. A waiting `supervise.set` off gets the
//!   status instead: what it asked for has happened.
//! - AC3 `supervise.set` with wrong parameters (`null`, an array such as
//!   `[false]`, a key it does not know, no `enabled`) gets its error at
//!   once, also in supervised mode, and supervised mode stays on.
//! - AC4 When the presenter accepts a turn-off with other changes in the
//!   same decision (`a`), the others are applied first. The message of an
//!   accepted turn-off says how many waiting changes it rejected.

use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};
use tiza_core::protocol::Response;
use tiza_qa::Live;

const CODE: &str = "fn main() {\n    let a = 1;\n}\n";

/// A request sent on its own thread: its answer, when it comes.
fn send(live: &Live, method: &str, params: Value) -> mpsc::Receiver<Response> {
    let (tx, rx) = mpsc::channel();
    let socket = live.socket.clone();
    let method = method.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(tiza_tui::ipc::call(&socket, &method, params).expect("socket call"));
    });
    rx
}

/// Waits for `n` changes to review (the request reached the instance).
fn wait_pending(live: &Live, n: usize) {
    let end = std::time::Instant::now() + Duration::from_secs(10);
    while live.with(|a| a.proposals.len()) < n {
        assert!(std::time::Instant::now() < end, "no {n} pending changes");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn supervised(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.api_ok("supervise.set", json!({"enabled": true}));
    live
}

#[test]
fn ac1_turning_supervised_mode_off_waits_for_the_presenter() {
    let live = supervised("t259-off");
    let rx = send(&live, "supervise.set", json!({"enabled": false}));
    wait_pending(&live, 1);
    assert!(
        rx.recv_timeout(Duration::from_millis(300)).is_err(),
        "answered without the presenter"
    );
    let st = live.api_ok("supervise.status", json!({}));
    assert_eq!(st["supervised"], true);
    assert_eq!(st["pending"][0]["method"], "supervise.set", "{st}");
    assert_eq!(
        st["pending"][0]["summary"], "turn supervised mode off",
        "{st}"
    );
    let msg = live.with(|a| a.accept_proposal()).unwrap();
    assert!(msg.contains("turn supervised mode off"), "{msg}");
    let r = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(r.error.is_none(), "{:?}", r.error);
    assert_eq!(r.result.unwrap()["supervised"], false);
    assert!(!live.with(|a| a.supervised));
}

/// The accepted turn-off leaves the queue before it runs, so turning
/// supervised mode off does not answer it a second time.
#[test]
fn ac1_the_accepted_turn_off_gets_one_answer() {
    let live = supervised("t259-one");
    let (tx, rx) = mpsc::channel();
    let req: tiza_core::protocol::Request = serde_json::from_value(
        json!({"id": 7, "method": "supervise.set", "params": {"enabled": false}}),
    )
    .unwrap();
    live.with(move |a| a.handle_ipc(req, tx));
    assert!(rx.try_recv().is_err(), "answered before the review");
    live.with(|a| a.accept_proposal()).unwrap();
    let first = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(first.result.unwrap()["supervised"], false);
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "a second answer"
    );
}

#[test]
fn ac1_a_rejected_turn_off_leaves_supervised_mode_on() {
    let live = supervised("t259-reject");
    let rx = send(&live, "supervise.set", json!({"enabled": false}));
    wait_pending(&live, 1);
    live.with(|a| a.reject_proposal()).unwrap();
    let r = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(r.error.map(|e| e.code).as_deref(), Some("rejected"));
    assert!(live.with(|a| a.supervised));
}

#[test]
fn ac1_turning_it_on_is_at_once() {
    let live = Live::from_code(CODE, "rs", "t259-on");
    let r = send(&live, "supervise.set", json!({"enabled": true}))
        .recv_timeout(Duration::from_secs(10))
        .expect("turning on waited");
    assert_eq!(r.result.unwrap()["supervised"], true);
}

#[test]
fn ac2_accepting_the_turn_off_rejects_the_other_changes_with_the_reason() {
    let live = supervised("t259-others");
    let add = send(
        &live,
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "waits"}),
    );
    wait_pending(&live, 1);
    let off = send(&live, "supervise.set", json!({"enabled": false}));
    wait_pending(&live, 2);
    live.with(|a| a.accept_proposal_at(1)).unwrap();
    let r = add.recv_timeout(Duration::from_secs(10)).unwrap();
    let e = r.error.expect("the waiting add was applied");
    assert_eq!(e.code, "rejected");
    assert!(e.message.contains("went off before"), "{}", e.message);
    let r = off.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(r.result.unwrap()["supervised"], false);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0);
}

#[test]
fn ac2_the_presenter_turning_it_off_answers_a_waiting_turn_off_with_success() {
    let live = supervised("t259-presenter");
    let add = send(
        &live,
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "waits"}),
    );
    wait_pending(&live, 1);
    let off = send(&live, "supervise.set", json!({"enabled": false}));
    wait_pending(&live, 2);
    live.command("supervise off");
    let r = off.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(r.error.is_none(), "{:?}", r.error);
    assert_eq!(r.result.unwrap()["supervised"], false);
    let e = add
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .error
        .unwrap();
    assert_eq!(e.code, "rejected");
    assert!(e.message.contains("went off before"), "{}", e.message);
}

#[test]
fn ac3_wrong_parameters_get_their_error_at_once() {
    let live = supervised("t259-params");
    for bad in [
        Value::Null,
        json!([false]),
        json!([true]),
        json!({"enable": false}),
        json!({"enabled": false, "x": 1}),
        json!({}),
    ] {
        let r = send(&live, "supervise.set", bad.clone())
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("{bad}: parked"));
        assert_eq!(
            r.error.map(|e| e.code).as_deref(),
            Some("invalid_argument"),
            "{bad}"
        );
    }
    assert!(live.with(|a| a.supervised));
    assert_eq!(live.with(|a| a.proposals.len()), 0);
}

#[test]
fn ac4_accepting_all_applies_the_other_changes_before_the_turn_off() {
    let live = supervised("t259-all");
    let first = send(
        &live,
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "a"}),
    );
    wait_pending(&live, 1);
    let off = send(&live, "supervise.set", json!({"enabled": false}));
    wait_pending(&live, 2);
    let last = send(
        &live,
        "annotation.add",
        json!({"type": "note", "line": 2, "text": "b"}),
    );
    wait_pending(&live, 3);
    let ids: Vec<u64> = live.with(|a| a.proposals.iter().map(|p| p.id).collect());
    assert_eq!(live.with(move |a| a.decide_proposals(&ids, true)), 3);
    for (name, rx) in [("first", first), ("last", last)] {
        let r = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(r.error.is_none(), "{name}: {:?}", r.error);
    }
    let r = off.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(r.result.unwrap()["supervised"], false);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 2);
}

#[test]
fn ac4_the_accepted_turn_off_says_how_many_changes_it_rejected() {
    let live = supervised("t259-count");
    let add = send(
        &live,
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "a"}),
    );
    wait_pending(&live, 1);
    let off = send(&live, "supervise.set", json!({"enabled": false}));
    wait_pending(&live, 2);
    let msg = live.with(|a| a.accept_proposal_at(1)).unwrap();
    assert!(msg.ends_with("; 1 waiting change rejected"), "{msg}");
    assert!(
        add.recv_timeout(Duration::from_secs(10))
            .unwrap()
            .error
            .is_some()
    );
    assert!(
        off.recv_timeout(Duration::from_secs(10))
            .unwrap()
            .error
            .is_none()
    );
}
