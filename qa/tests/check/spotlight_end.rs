//! `tiza check` and the end of a spotlight: the step where a spotlight
//! ends is not a step with nothing new when its take changes there, with
//! or without `until`. When the take stays the same (another spotlight
//! keeps the same lines bright, or the file of the spotlight is not shown),
//! the step has nothing new. A spotlight whose `until` is before its
//! `step` never shows: an error. Each test also checks every step against
//! the takes: a step with no annotation and no move of its own has
//! `empty_step` exactly when it draws as the step before it.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::scratch;

/// `fn main() {`, `let v1 = 1;` … `let v12 = 12;`, `}`: lines 1 to 14.
fn code() -> Vec<String> {
    std::iter::once("fn main() {".to_string())
        .chain((1..=12).map(|i| format!("    let v{i} = {i};")))
        .chain(std::iter::once("}".to_string()))
        .collect()
}

fn note(id: &str, step: u32, line: u32) -> Value {
    json!({"id": id, "step": step, "type": "note", "line": line, "text": id})
}

fn spot(id: &str, step: u32, lines: (u32, u32), until: Option<u32>) -> Value {
    let mut s = json!({"id": id, "step": step, "type": "spotlight",
        "from_line": lines.0, "to_line": lines.1});
    if let Some(u) = until {
        s["until"] = json!(u);
    }
    s
}

/// A document with the code above, `annotations` and `moves`, next to a
/// file `other.rs` of four lines.
fn document(name: &str, annotations: Value, moves: Value) -> PathBuf {
    let dir = scratch(name);
    std::fs::write(
        dir.join("other.rs"),
        "fn other() {\n    let o1 = 1;\n    let o2 = 2;\n}\n",
    )
    .unwrap();
    let doc = json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code(),
        "annotations": annotations, "moves": moves
    });
    let path = dir.join("doc.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    path
}

/// `tiza check --json`: (exit code, the whole report).
fn check_report(file: &Path) -> (i32, Value) {
    let o = tiza_qa::command()
        .args(["check", file.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    let out = String::from_utf8_lossy(&o.stdout);
    let v: Value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"));
    (o.status.code().unwrap_or(-1), v)
}

/// `tiza check --json`: (exit code, findings).
fn check_json(file: &Path) -> (i32, Vec<Value>) {
    let (code, v) = check_report(file);
    (code, v["findings"].as_array().unwrap().clone())
}

/// The steps of the `empty_step` warnings.
fn empty_steps(findings: &[Value]) -> Vec<u64> {
    findings
        .iter()
        .filter(|f| f["code"] == "empty_step")
        .map(|f| f["step"].as_u64().unwrap())
        .collect()
}

fn with_code(findings: &[Value], code: &str) -> Vec<Value> {
    findings
        .iter()
        .filter(|f| f["code"] == code)
        .cloned()
        .collect()
}

/// The take of each step from step 0, in ANSI, from one `tiza render
/// --all-steps`: all the steps with one geometry, as `tiza check` lays
/// them out, at the width that `tiza check` uses by default.
fn takes(file: &Path) -> Vec<String> {
    let dir = file.with_file_name("takes");
    let o = tiza_qa::command()
        .args([
            "render",
            file.to_str().unwrap(),
            "--all-steps",
            "--format",
            "ansi",
        ])
        .arg("--out")
        .arg(format!("{}/", dir.display()))
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    // `step-00.ansi`, `step-01.ansi`…
    paths.sort();
    paths
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect()
}

/// Checks `file` and checks each step against the takes: a step with no
/// annotation and no move of its own has `empty_step` exactly when it
/// draws as the step before it. Returns the steps of `empty_step`.
fn checked_against_takes(file: &Path) -> Vec<u64> {
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    let starts: Vec<u64> = doc["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["step"].as_u64().unwrap())
        .chain(
            doc["moves"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["at_step"].as_u64().unwrap()),
        )
        .collect();
    let last = starts.iter().copied().max().unwrap();
    let (_, findings) = check_json(file);
    let empty = empty_steps(&findings);
    let takes = takes(file);
    assert_eq!(takes.len() as u64, last + 1, "one take for each step");
    for step in (1..=last).filter(|s| !starts.contains(s)) {
        let same = takes[step as usize - 1] == takes[step as usize];
        assert_eq!(
            empty.contains(&step),
            same,
            "step {step}: the take is {} the take before it, but check says {}",
            if same { "the same as" } else { "not" },
            if empty.contains(&step) {
                "nothing new"
            } else {
                "it is new"
            }
        );
    }
    empty
}

#[test]
fn the_step_after_until_is_new() {
    let file = document(
        "spot-end-until",
        json!([
            note("n1", 1, 2),
            spot("S1", 2, (3, 4), Some(4)),
            note("n8", 8, 9)
        ]),
        json!([]),
    );
    let (code, _) = check_json(&file);
    assert_eq!(code, 0);
    // Step 5 takes the dim away; 3, 4, 6 and 7 change nothing.
    assert_eq!(checked_against_takes(&file), [3, 4, 6, 7]);
}

#[test]
fn the_step_after_a_spotlight_without_until_is_new() {
    let file = document(
        "spot-end-one-step",
        json!([
            note("n1", 1, 2),
            spot("S1", 2, (3, 4), None),
            note("n5", 5, 9)
        ]),
        json!([]),
    );
    assert_eq!(checked_against_takes(&file), [4]);
}

#[test]
fn an_until_at_its_own_step_is_one_step() {
    let file = document(
        "spot-end-until-same",
        json!([
            note("n1", 1, 2),
            spot("S1", 2, (3, 4), Some(2)),
            note("n5", 5, 9)
        ]),
        json!([]),
    );
    let (code, findings) = check_json(&file);
    assert_eq!(code, 0, "{findings:?}");
    assert!(with_code(&findings, "until").is_empty(), "{findings:?}");
    assert_eq!(checked_against_takes(&file), [4]);
}

#[test]
fn two_spotlights_that_end_at_different_steps() {
    // S1 ends at step 4 while S2 stays, on other lines: the lines of S1 go
    // dim. S2 ends at step 6.
    let file = document(
        "spot-end-two",
        json!([
            note("n1", 1, 2),
            spot("S1", 2, (3, 4), Some(3)),
            spot("S2", 2, (6, 7), Some(5)),
            note("n8", 8, 9)
        ]),
        json!([]),
    );
    assert_eq!(checked_against_takes(&file), [3, 5, 7]);
}

#[test]
fn a_spotlight_that_another_covers_ends_with_nothing_new() {
    // S2 keeps the same lines bright when S1 ends at step 4.
    let file = document(
        "spot-end-covered",
        json!([
            note("n1", 1, 2),
            spot("S1", 2, (3, 4), Some(3)),
            spot("S2", 2, (3, 4), Some(5)),
            note("n8", 8, 9)
        ]),
        json!([]),
    );
    assert_eq!(checked_against_takes(&file), [3, 4, 5, 7]);
}

#[test]
fn an_end_that_keeps_the_same_bright_code_has_nothing_new() {
    // S1 also names h3, which is in its lines: when it ends, S2 keeps the
    // same lines and h3 bright.
    let named = document(
        "spot-end-named-inside",
        json!([
            note("n1", 1, 2),
            {"id": "h3", "step": 1, "type": "highlight", "span": {"line": 3, "start": 4, "end": 7}},
            {"id": "S1", "step": 2, "type": "spotlight", "from_line": 3, "to_line": 4,
                "ids": ["h3"], "until": 3},
            spot("S2", 2, (3, 4), Some(5)),
            note("n8", 8, 9)
        ]),
        json!([]),
    );
    assert_eq!(checked_against_takes(&named), [3, 4, 5, 7]);
    // A spotlight of all the code dims nothing: its end changes nothing.
    // (Its start at step 2 has no warning: an annotation starts there.)
    let all = document(
        "spot-end-all-code",
        json!([
            note("n1", 1, 2),
            spot("S1", 2, (1, 14), Some(3)),
            note("n8", 8, 9)
        ]),
        json!([]),
    );
    assert_eq!(checked_against_takes(&all), [3, 4, 5, 6, 7]);
}

#[test]
fn the_end_of_a_spotlight_with_only_ids_is_new() {
    let file = document(
        "spot-end-ids-only",
        json!([
            note("n1", 1, 2),
            {"id": "S1", "step": 2, "type": "spotlight", "ids": ["n1"], "until": 3},
            note("n8", 8, 9)
        ]),
        json!([]),
    );
    assert_eq!(checked_against_takes(&file), [3, 5, 6, 7]);
}

#[test]
fn a_patch_between_the_start_and_the_end_moves_the_lines() {
    // The patch at step 2 puts two lines on top: S1 (lines 3-4 at step 1)
    // is on lines 5-6 after it. S2 comes at step 3 on lines 3-4 of that
    // code, so the end of S1 at step 6 makes lines 5-6 dim.
    let patch = json!([{"at_step": 2, "action": "code.patch", "op": "insert",
        "position": "1", "content": "// a\n// b"}]);
    let other_lines = document(
        "spot-end-patch-other",
        json!([
            spot("S1", 1, (3, 4), Some(5)),
            spot("S2", 3, (3, 4), Some(8)),
            note("n9", 9, 9)
        ]),
        patch.clone(),
    );
    assert_eq!(checked_against_takes(&other_lines), [4, 5, 7, 8]);
    // S2 on lines 5-6 of that code: the same lines as S1 there.
    let same_lines = document(
        "spot-end-patch-same",
        json!([
            spot("S1", 1, (3, 4), Some(5)),
            spot("S2", 3, (5, 6), Some(8)),
            note("n9", 9, 9)
        ]),
        patch,
    );
    assert_eq!(checked_against_takes(&same_lines), [4, 5, 6, 7, 8]);
}

#[test]
fn a_spotlight_whose_file_is_not_shown_ends_with_nothing_new() {
    // From step 3 the screen shows other.rs: S1 of main.rs dims nothing
    // there, so its end at step 5 changes nothing.
    let file = document(
        "spot-end-hidden-file",
        json!([
            note("n1", 1, 2),
            spot("S1", 2, (3, 4), Some(4)),
            {"id": "n6", "step": 6, "type": "note", "line": 2, "text": "n6", "file": "other.rs"}
        ]),
        json!([{"at_step": 3, "action": "code.link", "file": "other.rs"}]),
    );
    assert_eq!(checked_against_takes(&file), [4, 5]);
}

#[test]
fn a_spotlight_of_a_linked_file_on_screen_ends_with_a_change() {
    let mut s2 = spot("S2", 2, (2, 3), Some(3));
    s2["file"] = json!("other.rs");
    let file = document(
        "spot-end-linked-file",
        json!([
            note("n1", 1, 2),
            s2,
            {"id": "n5", "step": 5, "type": "note", "line": 2, "text": "n5", "file": "other.rs"}
        ]),
        json!([{"at_step": 2, "action": "code.link", "file": "other.rs"}]),
    );
    assert_eq!(checked_against_takes(&file), [3]);
}

#[test]
fn a_spotlight_at_step_0() {
    // Active at steps 0 and 1: step 1 draws as step 0, step 2 is new.
    let until = document(
        "spot-end-step0-until",
        json!([spot("S0", 0, (3, 4), Some(1)), note("n4", 4, 9)]),
        json!([]),
    );
    assert_eq!(checked_against_takes(&until), [1, 3]);
    // Without `until`, active only at step 0: step 1 is new.
    let alone = document(
        "spot-end-step0",
        json!([spot("S0", 0, (3, 4), None), note("n4", 4, 9)]),
        json!([]),
    );
    assert_eq!(checked_against_takes(&alone), [2, 3]);
}

#[test]
fn a_spotlight_to_the_last_step_or_past_it_adds_no_step() {
    // The take ends at the last step: the spotlight does not end in it.
    let last = document(
        "spot-end-last",
        json!([
            note("n1", 1, 2),
            spot("S1", 2, (3, 4), Some(4)),
            note("n4", 4, 9)
        ]),
        json!([]),
    );
    let (code, _) = check_json(&last);
    assert_eq!(code, 0);
    assert_eq!(checked_against_takes(&last), [3]);
    let past = document(
        "spot-end-past",
        json!([
            note("n1", 1, 2),
            spot("S1", 2, (3, 4), Some(10)),
            note("n5", 5, 9)
        ]),
        json!([]),
    );
    let (code, findings) = check_json(&past);
    assert_eq!(code, 0, "{findings:?}");
    assert!(
        findings.iter().all(|f| f["level"] == "warning"),
        "{findings:?}"
    );
    assert_eq!(checked_against_takes(&past), [3, 4]);
}

#[test]
fn a_label_at_the_step_after_until_is_checked_for_clipping() {
    // Today a dimmed mark in the margin shows its legend only, so the long
    // label of m1 comes at step 3, when S1 ends, and S2 dims it again at
    // step 4. The check must lay out step 3 to find it clipped at 100
    // columns. If a dimmed mark keeps its label, it is clipped from step 1,
    // and this test still passes, but no longer needs step 3.
    let file = document(
        "spot-end-clipped-label",
        json!([
            {"id": "m1", "step": 1, "type": "mark", "kind": "borrow_mut",
                "span": {"line": 6, "start": 8, "end": 10},
                "label": "a label long enough that it is clipped at a narrow width of the check, for sure yes"},
            note("n7", 1, 7),
            spot("S1", 1, (3, 4), Some(2)),
            spot("S2", 4, (3, 4), None)
        ]),
        json!([]),
    );
    let (_, report) = check_report(&file);
    let findings = report["findings"].as_array().unwrap();
    let clipped = with_code(findings, "clipped_label");
    assert_eq!(clipped.len(), 1, "{findings:?}");
    assert_eq!(clipped[0]["id"], "m1");
    let clean = report["min_clean_width"].as_u64().unwrap();
    assert!(clean > 100, "min_clean_width {clean}");
    assert!(takes(&file)[3].contains('…'));
}

#[test]
fn an_until_before_its_step_is_an_error() {
    let file = document(
        "spot-end-until-before",
        json!([
            note("n1", 1, 2),
            spot("S1", 3, (3, 4), Some(2)),
            note("n5", 5, 9)
        ]),
        json!([]),
    );
    let (code, findings) = check_json(&file);
    assert_eq!(code, 1, "{findings:?}");
    let until = with_code(&findings, "until");
    assert_eq!(until.len(), 1, "{findings:?}");
    assert_eq!(until[0]["level"], "error");
    assert_eq!(until[0]["id"], "S1");
    assert_eq!(until[0]["step"], 3);
    assert_eq!(until[0]["line"], 3);
    assert_eq!(
        until[0]["message"],
        "until 2 is before its step 3: the spotlight never shows. Give an until of 3 or later, or remove it."
    );
    // It never dims: step 4 draws as step 3.
    assert_eq!(checked_against_takes(&file), [2, 4]);
}

#[test]
fn an_until_before_its_step_gives_no_other_finding_of_the_spotlight() {
    // At step 3 the screen shows other.rs, not the file of S1: the error
    // says why S1 never shows, with no `not_shown` as well.
    let file = document(
        "spot-end-until-before-other-file",
        json!([
            note("n1", 1, 2),
            spot("S1", 3, (3, 4), Some(2)),
            {"id": "n5", "step": 5, "type": "note", "line": 2, "text": "n5", "file": "other.rs"}
        ]),
        json!([{"at_step": 2, "action": "code.link", "file": "other.rs"}]),
    );
    let (_, findings) = check_json(&file);
    let of_s1: Vec<_> = findings.iter().filter(|f| f["id"] == "S1").collect();
    assert_eq!(of_s1.len(), 1, "{findings:?}");
    assert_eq!(of_s1[0]["code"], "until");
}
