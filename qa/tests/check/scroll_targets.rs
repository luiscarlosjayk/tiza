//! The target of a `scroll` move in `tiza check` and on the socket: one
//! that does not resolve is a warning (it does not place the view), a `find`
//! that matches more than once goes to the first match with a warning
//! unless `occurrence` names one, the fields of the target are checked as
//! the document loads, and `move.add` refuses only a target that does not
//! resolve.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_core::moves::Camera;
use tiza_qa::{Live, scratch};

/// Twelve lines; `let x` is on lines 3 and 7, and `v5()` twice on line 5.
fn code() -> Vec<String> {
    (1..=12)
        .map(|i| match i {
            3 | 7 => format!("    let x = {i};"),
            5 => "    v5(); v5();".to_string(),
            _ => format!("    v{i}();"),
        })
        .collect()
}

/// A document with the code above, a box `b2` at step 2 and `moves`.
fn document(name: &str, moves: Value) -> PathBuf {
    let path = scratch(name).join("doc.tiza");
    let doc = json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code(),
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 1, "text": "start"},
            {"id": "b2", "step": 2, "type": "box", "from_line": 9, "to_line": 10}
        ],
        "moves": moves
    });
    std::fs::write(&path, doc.to_string()).unwrap();
    path
}

fn check_json(file: &Path) -> (i32, Vec<Value>) {
    let o = tiza_qa::command()
        .args(["check", file.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    let v: Value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"));
    (
        o.status.code().unwrap_or(-1),
        v["findings"].as_array().unwrap().clone(),
    )
}

fn moves_of(findings: &[Value]) -> Vec<(String, String, String)> {
    findings
        .iter()
        .filter(|f| f["code"] == "move")
        .map(|f| {
            (
                f["level"].as_str().unwrap().to_string(),
                f["id"].as_str().unwrap().to_string(),
                f["message"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// The first line of the target of the `scroll` that places the view at
/// `step`.
fn target_at(file: &Path, step: u32) -> Option<u32> {
    let doc = tiza_core::fs::load_document(file).unwrap();
    match doc.view_at(step).camera {
        Camera::Scroll(cam) => cam.lines.map(|(a, _)| a),
        other => panic!("step {step}: {other:?}"),
    }
}

#[test]
fn a_target_that_does_not_resolve_is_a_warning_with_its_move() {
    let file = document(
        "scroll-miss",
        json!([
            {"at_step": 1, "action": "scroll", "line": 99},
            {"at_step": 2, "action": "scroll", "find": "nope"},
            {"at_step": 3, "action": "scroll", "annotation": "zz"},
            {"at_step": 4, "action": "scroll", "find": "let x", "occurrence": 3},
            {"at_step": 5, "action": "scroll", "line": 5}
        ]),
    );
    let (code, findings) = check_json(&file);
    assert_eq!(code, 0, "warnings only: {findings:?}");
    let moves = moves_of(&findings);
    let ids: Vec<&str> = moves.iter().map(|(_, id, _)| id.as_str()).collect();
    assert_eq!(
        ids,
        ["move #1", "move #2", "move #3", "move #4"],
        "{moves:?}"
    );
    for (level, _, msg) in &moves {
        assert_eq!(level, "warning");
        assert!(msg.ends_with("so it does not place the view"), "{msg}");
    }
    assert!(moves[0].2.contains("line 99"), "{}", moves[0].2);
    assert!(moves[2].2.contains("«zz»"), "{}", moves[2].2);
    assert!(moves[3].2.contains("occurrence 3"), "{}", moves[3].2);
    // A step with only a `scroll` is a camera step, not an empty step.
    assert!(
        !findings
            .iter()
            .any(|f| f["code"] == "empty_step" && f["step"] == 5),
        "{findings:?}"
    );
}

#[test]
fn a_find_that_matches_twice_goes_to_the_first_and_check_says_so() {
    let file = document(
        "scroll-twice",
        json!([{"at_step": 2, "action": "scroll", "find": "let x"}]),
    );
    let (_, findings) = check_json(&file);
    let moves = moves_of(&findings);
    assert_eq!(moves.len(), 1, "{moves:?}");
    assert_eq!(moves[0].0, "warning");
    assert!(
        moves[0]
            .2
            .contains("matches 2 times; the view goes to the first"),
        "{}",
        moves[0].2
    );
    assert_eq!(target_at(&file, 2), Some(3));
}

#[test]
fn an_occurrence_chooses_the_match_with_no_warning() {
    for (n, line) in [(1, 3), (2, 7)] {
        let file = document(
            &format!("scroll-occurrence-{n}"),
            json!([{"at_step": 2, "action": "scroll", "find": "let x", "occurrence": n}]),
        );
        let (_, findings) = check_json(&file);
        assert!(
            moves_of(&findings).is_empty(),
            "occurrence {n}: {findings:?}"
        );
        assert_eq!(target_at(&file, 2), Some(line), "occurrence {n}");
    }
}

#[test]
fn the_fields_of_a_target_are_checked_as_the_document_loads() {
    let file = document(
        "scroll-fields",
        json!([
            {"at_step": 1, "action": "scroll", "line": 2, "find": "v2"},
            {"at_step": 2, "action": "scroll", "line": 2, "occurrence": 1},
            {"at_step": 3, "action": "scroll", "find": "v2", "occurrence": 0},
            {"at_step": 4, "action": "scroll"}
        ]),
    );
    let (code, findings) = check_json(&file);
    assert_eq!(code, 1, "{findings:?}");
    let moves = moves_of(&findings);
    let errors: Vec<(&str, &str)> = moves
        .iter()
        .filter(|(level, _, _)| level == "error")
        .map(|(_, id, msg)| (id.as_str(), msg.as_str()))
        .collect();
    assert_eq!(errors.len(), 4, "{moves:?}");
    assert!(
        errors[0]
            .1
            .contains("exactly one of line, find or annotation")
    );
    assert!(errors[1].1.contains("occurrence goes only with find"));
    assert!(errors[2].1.contains("1-based"));
    assert!(
        errors[3]
            .1
            .contains("exactly one of line, find or annotation")
    );
}

#[test]
fn move_add_refuses_only_a_target_that_does_not_resolve() {
    let code = code().join("\n");
    let live = Live::from_code(&code, "rs", "scroll-move-add");
    // A text that matches twice: the first match.
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "scroll", "find": "let x"}),
    );
    // An annotation of a later step.
    live.api_ok(
        "annotation.add",
        json!({"type": "box", "step": 3, "from_line": 9, "to_line": 10, "id": "b3"}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "scroll", "annotation": "b3"}),
    );
    for bad in [
        json!({"at_step": 2, "action": "scroll", "line": 99}),
        json!({"at_step": 2, "action": "scroll", "find": "nope"}),
        json!({"at_step": 2, "action": "scroll", "annotation": "zz"}),
    ] {
        let r = live.api("move.add", bad.clone());
        assert!(r.error.is_some(), "{bad}: accepted");
    }
}

#[test]
fn matches_on_one_line_give_no_warning() {
    // `v5()` is twice on line 5 and on no other line.
    let file = document(
        "scroll-one-line",
        json!([{"at_step": 2, "action": "scroll", "find": "v5()"}]),
    );
    let (_, findings) = check_json(&file);
    assert!(moves_of(&findings).is_empty(), "{findings:?}");
    let file = document(
        "scroll-one-line-twice",
        json!([{"at_step": 2, "action": "scroll", "find": "    "}]),
    );
    let (_, findings) = check_json(&file);
    assert_eq!(moves_of(&findings).len(), 1, "on many lines: {findings:?}");
}

#[test]
fn a_line_that_a_later_move_of_the_step_removes_is_a_warning() {
    let file = document(
        "scroll-removed-later",
        json!([
            {"at_step": 2, "action": "scroll", "line": 5},
            {"at_step": 2, "action": "code.patch", "op": "delete", "position": "5"}
        ]),
    );
    let (_, findings) = check_json(&file);
    let moves = moves_of(&findings);
    assert_eq!(moves.len(), 1, "{moves:?}");
    assert!(moves[0].2.contains("removes its line 5"), "{}", moves[0].2);
}

#[test]
fn a_patch_keeps_a_line_out_of_the_code_and_refuses_to_remove_a_scroll_line() {
    // The file has a `scroll` to a line that is not in the code.
    let file = document(
        "scroll-carry",
        json!([{"at_step": 2, "action": "scroll", "line": 99}]),
    );
    let doc = tiza_core::fs::load_document(&file).unwrap();
    let dir = file.parent().unwrap().to_path_buf();
    let live = Live::open(doc, file.clone(), dir.join("s.sock"));
    // A line that is not in the code does not stop a patch.
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "// top"}),
    );
    let moves = live.api_ok("move.list", json!({}));
    assert_eq!(moves["moves"][0]["line"], 99, "kept as written: {moves}");
    // A patch that removes the line of a later `scroll` is refused.
    live.api_ok(
        "move.add",
        json!({"at_step": 3, "action": "scroll", "line": 6}),
    );
    let r = live.api(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "delete", "position": "6"}),
    );
    let err = format!("{:?}", r.error);
    assert!(err.contains("removes the line it scrolls to (6)"), "{err}");
}

#[test]
fn a_find_out_of_the_window_of_a_code_link_names_the_window() {
    // Step 2 shows lines 100-160 of `long.rs`: `fn f30()` is in the file,
    // out of the window.
    let dir = scratch("scroll-find-window");
    let long: String = (1..=200).map(|i| format!("fn f{i}() {{}}\n")).collect();
    std::fs::write(dir.join("long.rs"), long).unwrap();
    let doc = json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code(),
        "annotations": [{"id": "n1", "step": 1, "type": "note", "line": 1, "text": "start"}],
        "moves": [
            {"at_step": 2, "action": "code.link", "file": "long.rs", "lines": "100-160"},
            {"at_step": 2, "action": "scroll", "find": "fn f30()"}
        ]
    });
    let file = dir.join("doc.tiza");
    std::fs::write(&file, doc.to_string()).unwrap();
    let (_, findings) = check_json(&file);
    let moves = moves_of(&findings);
    assert_eq!(moves.len(), 1, "{moves:?}");
    assert!(
        moves[0].2.contains("(lines 100-160)"),
        "the window: {}",
        moves[0].2
    );
}
