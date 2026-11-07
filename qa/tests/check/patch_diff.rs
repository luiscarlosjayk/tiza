//! `tiza check` and a `code.patch` move with `diff`. The step after the
//! move takes the `+` and the `−` away: it is a step with something new,
//! but only when the take shows a sign there. A move with `diff` that
//! shows no sign gets the warning `move`, with the reason: it writes the
//! same line again, or it writes or removes lines outside the window of
//! its step.

use serde_json::{Value, json};

use super::spotlight_end::{check_json, checked_against_takes, document, note, with_code};

#[test]
fn the_step_after_a_diff_is_new() {
    let file = document(
        "diff-step-after",
        json!([note("n4", 4, 2)]),
        json!([{"at_step": 1, "action": "code.patch", "op": "replace",
            "find": "let v3 = 3;", "content": "    let v3 = 30;", "diff": true}]),
    );
    let (code, findings) = check_json(&file);
    assert_eq!(code, 0, "{findings:?}");
    // Step 2 takes the `+` away; step 3 changes nothing.
    assert_eq!(checked_against_takes(&file), [3]);
    assert!(with_code(&findings, "move").is_empty(), "{findings:?}");
}

#[test]
fn a_diff_that_marks_no_line_is_a_warning() {
    let file = document(
        "diff-no-line",
        json!([note("n3", 3, 2)]),
        json!([
            // The same line again.
            {"at_step": 1, "action": "code.patch", "op": "replace",
                "find": "let v3 = 3;", "content": "    let v3 = 3;", "diff": true},
            // A line that changes: no warning.
            {"at_step": 2, "action": "code.patch", "op": "replace",
                "find": "let v4 = 4;", "content": "    let v4 = 40;", "diff": true}
        ]),
    );
    let (_, findings) = check_json(&file);
    let moves = with_code(&findings, "move");
    assert_eq!(moves.len(), 1, "{findings:?}");
    assert_eq!(moves[0]["level"], "warning");
    assert_eq!(moves[0]["id"], "move #1");
    assert_eq!(moves[0]["step"], 1);
    assert!(
        moves[0]["message"]
            .as_str()
            .unwrap()
            .contains("it writes the same lines again"),
        "{moves:?}"
    );
}

#[test]
fn the_step_after_a_diff_that_shows_no_sign_is_empty() {
    let file = document(
        "diff-no-sign-after",
        json!([note("n3", 3, 2)]),
        json!([{"at_step": 1, "action": "code.patch", "op": "replace",
            "find": "let v3 = 3;", "content": "    let v3 = 3;", "diff": true}]),
    );
    // Step 1 shows no `+`, so nothing goes at step 2.
    assert_eq!(checked_against_takes(&file), [2]);
}

/// The message of the one `move` warning of a document with `moves`, and
/// a note at step 3.
fn diff_warning(name: &str, moves: serde_json::Value) -> String {
    let file = document(name, json!([note("n3", 3, 2)]), moves);
    let (_, findings) = check_json(&file);
    let moves = with_code(&findings, "move");
    assert_eq!(moves.len(), 1, "{findings:?}");
    assert_eq!(moves[0]["step"], 1);
    moves[0]["message"].as_str().unwrap().to_string()
}

#[test]
fn a_diff_that_only_removes_lines_shows_them_and_is_no_warning() {
    let file = document(
        "diff-delete",
        json!([note("n4", 4, 2)]),
        json!([{"at_step": 1, "action": "code.patch", "op": "delete",
            "find": "let v3 = 3;", "diff": true}]),
    );
    let (code, findings) = check_json(&file);
    assert_eq!(code, 0, "{findings:?}");
    assert!(with_code(&findings, "move").is_empty(), "{findings:?}");
    // Step 2 takes the removed line away: something new.
    assert_eq!(checked_against_takes(&file), [3]);
}

#[test]
fn a_diff_that_removes_lines_outside_the_window_is_a_warning() {
    // Line 3 of other.rs is outside the window 1-1 that step 1 shows.
    let m = diff_warning(
        "diff-delete-window",
        json!([
            {"at_step": 1, "action": "code.link", "file": "other.rs", "lines": "1-1"},
            {"at_step": 1, "action": "code.patch", "op": "delete",
                "find": "let o2 = 2;", "diff": true}
        ]),
    );
    assert!(
        m.contains("the lines that it removes are outside the lines that its step shows (1-1)"),
        "{m}"
    );
}

#[test]
fn a_diff_outside_the_window_of_its_step_is_a_warning() {
    // Line 3 of other.rs is outside the window 1-2 that step 1 shows.
    let m = diff_warning(
        "diff-window",
        json!([
            {"at_step": 1, "action": "code.link", "file": "other.rs", "lines": "1-2"},
            {"at_step": 1, "action": "code.patch", "op": "replace",
                "find": "let o2 = 2;", "content": "    let o2 = 20;", "diff": true}
        ]),
    );
    assert!(
        m.contains("outside the lines that its step shows (1-2)"),
        "{m}"
    );
}

#[test]
fn a_removed_line_that_is_cut_is_a_warning_of_its_step() {
    let dir = tiza_qa::scratch("diff-cut-removed");
    let long = format!("    let s = \"{}\";", "x".repeat(150));
    let doc = json!({
        "schema": 2, "source": {"language": "rs", "first_line": 1},
        "code": ["fn main() {", long, "    run();", "}"],
        "annotations": [],
        "moves": [{"at_step": 1, "action": "code.patch", "op": "delete",
            "find": "let s", "diff": true}]
    });
    let file = dir.join("doc.tiza");
    std::fs::write(&file, doc.to_string()).unwrap();
    let (_, findings) = check_json(&file);
    let cut = with_code(&findings, "clipped_code");
    let removed: Vec<&Value> = cut
        .iter()
        .filter(|f| f["message"].as_str().unwrap().contains("removed line"))
        .collect();
    assert_eq!(removed.len(), 1, "{findings:?}");
    let m = removed[0]["message"].as_str().unwrap();
    assert!(
        m.contains("a removed line of the diff at step 1 is cut with «…»: before L2"),
        "{m}"
    );
    assert_eq!(removed[0]["step"], 1);
    assert_eq!(removed[0]["line"], 2);
    // The line itself is cut at step 0, where it is code.
    assert_eq!(cut.len(), 2, "{findings:?}");
}
