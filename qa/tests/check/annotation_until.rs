//! `tiza check` and the `until` of an annotation that is not a spotlight.
//! After its `until`, an annotation is not on the screen: a move that
//! removes its text, or a window that leaves it out, gives no warning
//! there, and a `scroll` to it or a spotlight of its id gets one. A step
//! where only an annotation goes is a step with something new, when its
//! take changes. An `until` before the step is the error `until` of each
//! type, with the text of the API.

use serde_json::{Value, json};

use super::spotlight_end::{check_json, checked_against_takes, document, note, with_code};

fn until(mut a: Value, until: u32) -> Value {
    a["until"] = json!(until);
    a
}

/// A highlight on `v2` of line 3, at step 1.
fn highlight_v2() -> Value {
    json!({"id": "h1", "step": 1, "type": "highlight",
        "span": {"line": 3, "start": 8, "end": 10}})
}

/// At step 2, line 3 loses the text of the highlight.
fn replace_v2() -> Value {
    json!([{"at_step": 2, "action": "code.patch", "op": "replace",
        "find": "let v2 = 2;", "content": "    let w = 2;"}])
}

#[test]
fn a_move_after_until_gives_no_unanchored() {
    // Without `until`, the highlight stays to the end, and the move
    // removes its text: the warning that the `until` makes go.
    let file = document("until-anchor-none", json!([highlight_v2()]), replace_v2());
    let (_, findings) = check_json(&file);
    assert_eq!(with_code(&findings, "unanchored").len(), 1, "{findings:?}");
    let file = document(
        "until-anchor",
        json!([until(highlight_v2(), 1)]),
        replace_v2(),
    );
    let (code, findings) = check_json(&file);
    assert_eq!(code, 0, "{findings:?}");
    assert!(
        with_code(&findings, "unanchored").is_empty(),
        "{findings:?}"
    );
}

#[test]
fn a_step_where_only_a_note_goes_is_new() {
    let file = document(
        "until-goes",
        json!([until(note("n1", 1, 2), 1), note("n2", 3, 4)]),
        json!([]),
    );
    assert_eq!(checked_against_takes(&file), Vec::<u64>::new());
}

#[test]
fn a_note_that_goes_off_the_screen_changes_no_take() {
    // At steps 2 and 3 the screen shows other.rs: the note of the main
    // file goes at step 3 where no one sees it, so step 3 has nothing new.
    let file = document(
        "until-goes-hidden",
        json!([until(note("n1", 1, 2), 2), note("n2", 4, 4)]),
        json!([{"at_step": 2, "action": "code.link", "file": "other.rs"}]),
    );
    assert_eq!(checked_against_takes(&file), [3]);
}

#[test]
fn an_until_before_the_step_is_an_error_of_each_type() {
    let file = document(
        "until-before",
        json!([
            until(
                json!({"id": "b1", "step": 3, "type": "box", "from_line": 2, "to_line": 4}),
                2
            ),
            until(note("n1", 2, 5), 1),
        ]),
        json!([]),
    );
    let (code, findings) = check_json(&file);
    assert_eq!(code, 1, "{findings:?}");
    let errors: Vec<(String, String)> = with_code(&findings, "until")
        .iter()
        .map(|f| {
            assert_eq!(f["level"], "error", "{f}");
            (
                f["id"].as_str().unwrap().to_string(),
                f["message"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        errors,
        [
            (
                "b1".to_string(),
                "give until 3 or later, or remove it (until 2 is before step 3: the box never shows)"
                    .to_string()
            ),
            (
                "n1".to_string(),
                "give until 2 or later, or remove it (until 1 is before step 2: the note never shows)"
                    .to_string()
            ),
        ]
    );
}

#[test]
fn a_window_after_until_gives_no_outside_window() {
    let annotations = |u: Option<u32>| {
        let n = json!({"id": "n1", "step": 1, "type": "note", "line": 3,
            "text": "n1", "file": "other.rs"});
        json!([match u {
            Some(u) => until(n, u),
            None => n,
        }])
    };
    // The window shows lines 1-3 of other.rs; at step 2 a line goes in at
    // the top, and pushes the line of the note out of it.
    let moves = json!([
        {"at_step": 1, "action": "code.link", "file": "other.rs", "lines": "1-3"},
        {"at_step": 2, "action": "code.patch", "op": "insert", "position": "1", "content": "// top"}
    ]);
    let file = document("until-window-none", annotations(None), moves.clone());
    let (_, findings) = check_json(&file);
    assert_eq!(
        with_code(&findings, "outside_window").len(),
        1,
        "{findings:?}"
    );
    let file = document("until-window", annotations(Some(1)), moves);
    let (_, findings) = check_json(&file);
    assert!(
        with_code(&findings, "outside_window").is_empty(),
        "{findings:?}"
    );
}

#[test]
fn a_scroll_to_one_that_went_is_a_warning() {
    let file = document(
        "until-scroll",
        json!([until(note("n1", 1, 12), 2), note("n2", 3, 2)]),
        json!([{"at_step": 3, "action": "scroll", "annotation": "n1"}]),
    );
    let (_, findings) = check_json(&file);
    let moves = with_code(&findings, "move");
    assert!(
        moves.iter().any(|f| f["message"]
            .as_str()
            .unwrap()
            .contains("annotation «n1» shows only at steps 1-2, not at step 3")),
        "{findings:?}"
    );
}

#[test]
fn a_spotlight_of_one_that_went_is_a_warning() {
    let file = document(
        "until-spot-ids",
        json!([
            until(note("n1", 1, 2), 1),
            {"id": "s1", "step": 2, "type": "spotlight", "ids": ["n1"]}
        ]),
        json!([]),
    );
    let (_, findings) = check_json(&file);
    let warnings: Vec<String> = with_code(&findings, "until")
        .iter()
        .map(|f| format!("{} {}: {}", f["level"], f["id"], f["message"]))
        .collect();
    assert_eq!(
        warnings,
        [
            r#""warning" "s1": "ids names n1, which shows only at step 1: the spotlight does not keep it in focus at step 2""#
        ]
    );
}

#[test]
fn a_spotlight_that_went_gives_no_unanchored() {
    // A spotlight without `until` shows only at its step: the line that
    // goes at step 2 is not its line any more.
    let delete = json!([{"at_step": 2, "action": "code.patch", "op": "delete",
        "find": "let v2 = 2;"}]);
    let spot = json!({"id": "s1", "step": 1, "type": "spotlight",
        "from_line": 3, "to_line": 3});
    let file = document("until-spot-gone", json!([spot, note("n2", 3, 5)]), delete);
    let (_, findings) = check_json(&file);
    assert!(
        with_code(&findings, "unanchored").is_empty(),
        "{findings:?}"
    );
}

#[test]
fn a_scroll_to_a_spotlight_after_its_step_is_taken() {
    // A spotlight without `until` keeps its lines after its step: a
    // `scroll` to it there is fine, as in Tiza 0.2.
    let spot = json!({"id": "s1", "step": 1, "type": "spotlight",
        "from_line": 12, "to_line": 13});
    let file = document(
        "until-scroll-spot",
        json!([spot, note("n2", 3, 2)]),
        json!([{"at_step": 3, "action": "scroll", "annotation": "s1"}]),
    );
    let (_, findings) = check_json(&file);
    assert!(with_code(&findings, "move").is_empty(), "{findings:?}");
}
