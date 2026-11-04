//! A spotlight is active from its step to its `until`. An `until` before
//! its step makes a spotlight that never shows. `annotation.add` refuses
//! it, and `annotation.update` refuses a `step` or a `shape` that makes
//! it, with the text of the error `until` of `tiza check`. A refusal
//! changes nothing: no annotation, no undo entry, no new revision.
//!
//! A document from a file, `scene.put` and `scene.patch` keep such a
//! spotlight, as a file does, and `check` reports it. An update of
//! another field of it still works.

use std::sync::mpsc;
use std::time::Duration;

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_core::protocol::Response;
use tiza_qa::{Live, fixture, scratch};

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";

/// The text of the refusal, after the id of the annotation.
fn never_shows(until: u32, step: u32) -> String {
    format!(
        "until {until} is before its step {step}: the spotlight never shows. \
         Give an until of {step} or later, or remove it."
    )
}

/// The document of the scene.
fn document(live: &Live) -> Value {
    live.api_ok("scene.get", json!({}))["document"].clone()
}

/// The annotation `id` of the scene.
fn annotation(live: &Live, id: &str) -> Value {
    document(live)["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == id)
        .unwrap_or_else(|| panic!("no {id}"))
        .clone()
}

/// The error of a call that must fail, as `code: message`. The call must
/// change nothing: the same document and the same revision.
fn refused(live: &Live, method: &str, params: Value) -> String {
    let before = (document(live), live.with(|a| a.revision));
    let r = live.api(method, params);
    let e = r
        .error
        .unwrap_or_else(|| panic!("{method} must fail: {:?}", r.result));
    let after = (document(live), live.with(|a| a.revision));
    assert_eq!(before, after, "a refused {method} changed the document");
    format!("{}: {}", e.code, e.message)
}

/// The `until` findings of `check`, as (id, step, message).
fn until_findings(live: &Live) -> Vec<(String, u64, String)> {
    live.api_ok("check", json!({}))["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "until")
        .map(|f| {
            (
                f["id"].as_str().unwrap().to_string(),
                f["step"].as_u64().unwrap(),
                f["message"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn spot(id: &str, step: u32, until: Option<u32>) -> Value {
    let mut s = json!({"id": id, "type": "spotlight", "from_line": 2, "to_line": 3,
        "step": step});
    if let Some(u) = until {
        s["until"] = json!(u);
    }
    s
}

#[test]
fn an_add_with_until_before_its_step_is_refused() {
    let live = Live::from_code(CODE, "rs", "until-add");
    let e = refused(&live, "annotation.add", spot("s1", 3, Some(2)));
    assert_eq!(
        e,
        format!("invalid_argument: annotation s1: {}", never_shows(2, 3))
    );
    assert!(
        document(&live)["annotations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // No undo entry: the undo has nothing to take back.
    live.command("u");
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("nothing to undo"), "{msg}");
}

#[test]
fn an_add_without_step_is_judged_at_the_step_it_gets() {
    let live = Live::from_code(CODE, "rs", "until-default-step");
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 4, "text": "last", "step": 5}),
    );
    live.api_ok("step.set", json!({"step": 5}));
    // No `step`: the spotlight comes at step 5, the step on screen.
    let mut s = spot("s1", 0, Some(2));
    s.as_object_mut().unwrap().remove("step");
    let e = refused(&live, "annotation.add", s);
    assert!(e.ends_with(&never_shows(2, 5)), "{e}");
    // Step 0 comes in as step 1.
    let e = refused(&live, "annotation.add", spot("s2", 0, Some(0)));
    assert!(e.ends_with(&never_shows(0, 1)), "{e}");
}

#[test]
fn an_until_at_or_after_its_step_is_taken() {
    let live = Live::from_code(CODE, "rs", "until-taken");
    live.api_ok("annotation.add", spot("same", 3, Some(3)));
    live.api_ok("annotation.add", spot("after", 3, Some(4)));
    live.api_ok("annotation.add", spot("none", 3, None));
    assert_eq!(annotation(&live, "same")["until"], json!(3));
    assert_eq!(annotation(&live, "after")["until"], json!(4));
    assert!(annotation(&live, "none").get("until").is_none());
    assert!(until_findings(&live).is_empty());
}

#[test]
fn an_update_that_moves_the_step_past_until_is_refused() {
    let live = Live::from_code(CODE, "rs", "until-update-step");
    live.api_ok("annotation.add", spot("s1", 2, Some(3)));
    let e = refused(&live, "annotation.update", json!({"id": "s1", "step": 5}));
    assert_eq!(
        e,
        format!("invalid_argument: annotation s1: {}", never_shows(3, 5))
    );
    // To the `until` itself is a step of its own.
    live.api_ok("annotation.update", json!({"id": "s1", "step": 3}));
    assert_eq!(annotation(&live, "s1")["step"], json!(3));
    // The `:` line goes the same way.
    live.command("step s1 4");
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains(&never_shows(3, 4)), "{msg}");
    assert_eq!(annotation(&live, "s1")["step"], json!(3));
}

#[test]
fn an_update_whose_shape_puts_until_before_its_step_is_refused() {
    let live = Live::from_code(CODE, "rs", "until-update-shape");
    live.api_ok("annotation.add", spot("s1", 3, None));
    let e = refused(
        &live,
        "annotation.update",
        json!({"id": "s1",
            "shape": {"type": "spotlight", "from_line": 2, "to_line": 4, "until": 1}}),
    );
    assert_eq!(
        e,
        format!("invalid_argument: annotation s1: {}", never_shows(1, 3))
    );
    live.api_ok(
        "annotation.update",
        json!({"id": "s1",
            "shape": {"type": "spotlight", "from_line": 2, "to_line": 4, "until": 3}}),
    );
    assert_eq!(annotation(&live, "s1")["until"], json!(3));
}

#[test]
fn a_spotlight_from_a_file_with_until_before_its_step_loads_and_can_be_edited() {
    let dir = scratch("until-from-a-file");
    let tiza = dir.join("doc.tiza");
    std::fs::copy(fixture("api/spotlight_until/never_shows.tiza"), &tiza).unwrap();
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(doc, tiza.clone(), dir.join("live.sock"));
    assert_eq!(
        until_findings(&live),
        vec![("s1".to_string(), 3, never_shows(2, 3))]
    );
    // Another field: taken, and the `until` stays.
    live.api_ok("annotation.update", json!({"id": "s1", "color": "blue"}));
    let s1 = annotation(&live, "s1");
    assert_eq!((&s1["color"], &s1["until"]), (&json!("blue"), &json!(2)));
    assert_eq!(until_findings(&live).len(), 1);
    // The step: judged like any update of it.
    let e = refused(&live, "annotation.update", json!({"id": "s1", "step": 4}));
    assert!(e.ends_with(&never_shows(2, 4)), "{e}");
}

#[test]
fn a_put_and_a_patch_keep_a_spotlight_that_never_shows() {
    let live = Live::from_code(CODE, "rs", "until-put");
    let mut doc = document(&live);
    doc["annotations"] = json!([spot("s1", 3, Some(2))]);
    live.api_ok("scene.put", json!({"document": doc}));
    assert_eq!(annotation(&live, "s1")["until"], json!(2));
    assert_eq!(
        until_findings(&live),
        vec![("s1".to_string(), 3, never_shows(2, 3))]
    );
    // A patch merges full annotations: a new id and a replaced one.
    live.api_ok("annotation.add", spot("s2", 4, None));
    live.api_ok(
        "scene.patch",
        json!({"annotations": [spot("s2", 4, Some(1)), spot("s3", 5, Some(4))]}),
    );
    assert_eq!(annotation(&live, "s2")["until"], json!(1));
    assert_eq!(annotation(&live, "s3")["until"], json!(4));
    let mut ids: Vec<String> = until_findings(&live).into_iter().map(|f| f.0).collect();
    ids.sort();
    assert_eq!(ids, ["s1", "s2", "s3"]);
}

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

#[test]
fn a_supervised_add_with_until_before_its_step_is_refused_when_accepted() {
    let live = Live::from_code(CODE, "rs", "until-supervised");
    live.command("supervise on");
    let rx = send(&live, "annotation.add", spot("s1", 3, Some(2)));
    let end = std::time::Instant::now() + Duration::from_secs(10);
    while live.with(|a| a.proposals.len()) < 1 {
        assert!(std::time::Instant::now() < end, "no change to review");
        std::thread::sleep(Duration::from_millis(10));
    }
    let revision = live.with(|a| a.revision);
    live.command("review");
    live.keys(&[KeyCode::Char('y')]);
    let r = rx.recv_timeout(Duration::from_secs(10)).expect("an answer");
    let e = r
        .error
        .unwrap_or_else(|| panic!("accepted: {:?}", r.result));
    assert_eq!(
        format!("{}: {}", e.code, e.message),
        format!("invalid_argument: annotation s1: {}", never_shows(2, 3))
    );
    assert!(
        document(&live)["annotations"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(live.with(|a| a.revision), revision);
}
