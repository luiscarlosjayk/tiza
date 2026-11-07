//! The TUI and the `until` of an annotation that is not a spotlight.
//! Tab does not select one after its `until`, and a step change takes off
//! a selection that the new step does not draw, so that a key does not
//! act on what the screen does not show. `:until` takes each type, `-`
//! removes it, `u` undoes it, and the reply says why the annotation is
//! not on the screen at the step on screen.

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::Live;

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n}\n";

/// `n1` on line 2 at step 1 until step 1, `n3` on line 4 at step 3; step 1
/// on screen.
fn live(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.api_ok(
        "annotation.add",
        json!({"id": "n1", "type": "note", "line": 2, "text": "one", "step": 1, "until": 1}),
    );
    live.api_ok(
        "annotation.add",
        json!({"id": "n3", "type": "note", "line": 4, "text": "three", "step": 3}),
    );
    live.api_ok("step.set", json!({"step": 1}));
    live
}

fn until_of(live: &Live, id: &'static str) -> Option<u32> {
    live.with(move |a| a.doc.find_annotation(id).unwrap().until)
}

#[test]
fn tab_does_not_select_one_after_its_until() {
    let live = live("until-tab");
    live.api_ok("step.set", json!({"step": 2}));
    live.keys(&[KeyCode::Tab]);
    assert_eq!(live.with(|a| a.selected.clone()), None);
    let (msg, _) = live.message().unwrap();
    assert_eq!(
        msg,
        "no annotation on the screen at this step: L lists them all"
    );
}

#[test]
fn a_step_change_takes_off_a_selection_that_goes() {
    let live = live("until-select");
    live.keys(&[KeyCode::Tab]);
    assert_eq!(live.with(|a| a.selected.clone()).as_deref(), Some("n1"));
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(live.with(|a| a.doc.step), 2);
    assert_eq!(
        live.with(|a| a.selected.clone()),
        None,
        "n1 is not drawn at step 2"
    );
    // A selection that the next step still draws stays.
    live.api_ok("annotation.update", json!({"id": "n1", "until": null}));
    live.api_ok("step.set", json!({"step": 1}));
    live.keys(&[KeyCode::Tab]);
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(live.with(|a| a.selected.clone()).as_deref(), Some("n1"));
}

#[test]
fn until_takes_each_type_and_a_dash_removes_it() {
    let live = live("until-dash");
    live.command("until n3 4");
    assert_eq!(
        live.message(),
        Some((
            "n3 shows until step 4 (step 3, :step 3 shows it)".to_string(),
            false
        ))
    );
    assert_eq!(until_of(&live, "n3"), Some(4));
    live.command("until n3 -");
    assert_eq!(until_of(&live, "n3"), None);
    // `u` gives it back.
    live.command("u");
    assert_eq!(until_of(&live, "n3"), Some(4));
    // Nothing to remove: no change and no undo entry.
    live.command("until n3 -");
    live.command("until n3 -");
    let (msg, err) = live.message().unwrap();
    assert_eq!((msg.as_str(), err), ("n3 has no until", false));
    live.command("u");
    assert_eq!(
        until_of(&live, "n3"),
        Some(4),
        "the undo took back the first -"
    );
}

#[test]
fn the_reply_says_why_it_is_not_on_the_screen() {
    let live = live("until-tail");
    live.api_ok("step.set", json!({"step": 3}));
    live.command("until n3 3");
    assert_eq!(
        live.message(),
        Some(("n3 shows until step 3".to_string(), false))
    );
    live.command("step n3 2");
    live.command("until n3 2");
    let (msg, _) = live.message().unwrap();
    assert_eq!(
        msg,
        "n3 shows until step 2 (not shown: it shows only at step 2)"
    );
}
