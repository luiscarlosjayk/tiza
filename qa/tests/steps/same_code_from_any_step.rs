//! The code of a step is the same whatever step you come from. Each step
//! is reached one step at a time from step 0 first: that is the code each
//! jump must give. Then from every step, `:step N`, `step.set` and
//! `step.goto` go to every step, also to the empty step after the last
//! one, which only `:step` opens. The empty step has the code of the last
//! step, and its typing complete, as a jump to the last step.

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_qa::{Live, scratch};

const CODE: &str = "fn main() {\n    let t = String::from(\"hi\");\n    let n = len(&t);\n    println!(\"{n}\");\n}\n";

/// The last step with a move or an annotation; the empty step is one more.
const LAST: u32 = 3;

/// A patch at step 2, and two at step 3 (the last): one that types,
/// slowly, and one that does not. A note at each step.
fn document(name: &str, present: bool) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    for (step, line) in [(1, 1), (2, 2), (3, 4)] {
        live.api_ok(
            "annotation.add",
            json!({"type": "note", "line": line, "text": format!("s{step}"), "step": step}),
        );
    }
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "insert",
            "after": "let t", "content": "    let m = 0;"}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "replace",
            "find": "let n = len(&t);", "content": "    let n = t.len();"}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "insert",
            "after": "println!", "content": "    println!(\"{m}\");",
            "typing": "char", "speed_ms": 3_600_000}),
    );
    if present {
        live.keys(&[KeyCode::Char('P')]);
        assert!(live.with(|a| a.present), "P starts presentation mode");
    }
    live.command("step 0");
    live
}

/// What an agent and `tiza scene text` see of the code at the step on
/// screen: `code.get` and `render.text`.
fn seen(live: &Live) -> (u32, Value, Value) {
    (
        live.with(|a| a.doc.step),
        live.api_ok("code.get", json!({}))["text"].clone(),
        live.api_ok("render.text", json!({}))["text"].clone(),
    )
}

/// From step 0, one step at a time to `step`, each typing finished.
fn walk_to(live: &Live, step: u32) {
    live.command("step 0");
    for s in 1..=step {
        live.command(&format!("step {s}"));
        live.with(|a| a.finish_typing());
    }
    assert_eq!(live.with(|a| a.doc.step), step);
}

/// The code of each step from 0 to the empty step, one step at a time.
fn oracle(live: &Live) -> Vec<(u32, Value, Value)> {
    (0..=LAST + 1)
        .map(|s| {
            walk_to(live, s);
            seen(live)
        })
        .collect()
}

fn every_jump_gives_the_code_of_its_step(present: bool) {
    let live = document(
        if present {
            "jump-present"
        } else {
            "jump-normal"
        },
        present,
    );
    let want = oracle(&live);
    assert!(
        want[LAST as usize].1.as_str().unwrap().contains("t.len()"),
        "the last step has its patch: {:?}",
        want[LAST as usize].1
    );
    assert_eq!(want[LAST as usize + 1].1, want[LAST as usize].1);
    for from in 0..=LAST + 1 {
        for to in 0..=LAST + 1 {
            walk_to(&live, from);
            live.command(&format!("step {to}"));
            if to == LAST + 1 {
                assert!(
                    !live.with(|a| a.is_typing()),
                    "from {from}, the empty step types (present: {present})"
                );
            }
            live.with(|a| a.finish_typing());
            assert_eq!(
                seen(&live),
                want[to as usize],
                ":step {to} from step {from} (present: {present})"
            );
        }
    }
    // The API goes no further than the last step, with its code.
    for from in 0..=LAST + 1 {
        for to in 0..=LAST + 1 {
            walk_to(&live, from);
            live.api_ok("step.set", json!({"step": to}));
            live.with(|a| a.finish_typing());
            let at = if from == LAST + 1 && to >= from {
                // On the empty step, a step.set to it or later stays.
                LAST + 1
            } else {
                to.min(LAST)
            };
            assert_eq!(
                seen(&live),
                want[at as usize],
                "step.set {to} from step {from} (present: {present})"
            );
            walk_to(&live, from);
            live.api_ok("step.goto", json!({"id": to.min(LAST)}));
            assert_eq!(
                seen(&live),
                want[to.min(LAST) as usize],
                "step.goto {} from step {from} (present: {present})",
                to.min(LAST)
            );
        }
    }
}

#[test]
fn the_code_of_a_step_is_the_same_from_every_step() {
    every_jump_gives_the_code_of_its_step(false);
}

#[test]
fn the_code_of_a_step_is_the_same_from_every_step_in_presentation_mode() {
    every_jump_gives_the_code_of_its_step(true);
}

#[test]
fn on_the_empty_step_an_annotation_finds_the_text_of_the_last_patch() {
    let live = document("empty-step-find", false);
    walk_to(&live, 2);
    live.command(&format!("step {}", LAST + 1));
    let (msg, err) = live.message().unwrap();
    assert!(
        !err && msg.contains("empty, new annotations go here"),
        "{msg}"
    );
    live.api_ok(
        "annotation.add",
        json!({"type": "highlight", "at": {"find": "t.len()"}}),
    );
    // `tiza scene text` of the live instance gives the code of the last
    // patch too.
    let out = tiza_qa::tiza(&["scene", "text", "--socket", live.socket.to_str().unwrap()]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success() && text.contains("t.len()"), "{text}");
    assert!(!text.contains("len(&t)"), "{text}");
}

#[test]
fn the_empty_step_shows_the_file_of_the_last_step() {
    let dir = scratch("empty-step-link");
    std::fs::write(dir.join("other.rs"), "fn other() {\n    let o = 1;\n}\n").unwrap();
    let live = Live::open(
        tiza_core::model::Document::from_code(CODE, "rs", None),
        dir.join("doc.tiza"),
        dir.join("live.sock"),
    );
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "main", "step": 1}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.link", "file": "other.rs"}),
    );
    let view = |live: &Live| live.api_ok("status", json!({}))["view"].clone();
    walk_to(&live, 2);
    let want = (view(&live), seen(&live));
    for from in 0..=1 {
        walk_to(&live, from);
        live.command("step 3");
        assert_eq!(live.with(|a| a.doc.step), 3);
        assert_eq!(
            (view(&live), seen(&live).1),
            (want.0.clone(), want.1.1.clone()),
            "from {from}"
        );
    }
}

#[test]
fn a_move_of_the_last_step_that_does_not_apply_keeps_the_last_step() {
    let live = document("empty-step-loud", false);
    walk_to(&live, 2);
    // A move of the last step that does not apply to its code. A request
    // refuses such a move, so the test writes it in the document.
    live.with(|a| {
        a.doc.moves[1] = serde_json::from_value(json!({"at_step": LAST,
            "action": "code.patch", "op": "replace",
            "find": "not in the code", "content": "    let x = 0;"}))
        .unwrap();
    });
    live.command(&format!("step {}", LAST + 1));
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.starts_with("moves:"), "{msg}");
    assert_eq!(live.with(|a| a.doc.step), LAST, "{msg}");
}
