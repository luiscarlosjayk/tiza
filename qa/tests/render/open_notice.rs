//! The notice when a document opens names an annotation as outside the
//! code only when its lines are outside the code: the layout then does not
//! draw it. An annotation on a line of the code that fails another check
//! (a mark kind that Tiza does not know, a span past the end of its line,
//! an empty span) gets its own notice, which tells to use `tiza check`. It
//! does not say "not drawn": Tiza draws the mark, and the part of the span
//! on the line.
//!
//! Corner cases: an annotation outside the code that also fails another
//! check is outside; several annotations with both causes.

use tiza_qa::{scratch, tiza};

/// A document of 10 lines `let vN = N;` with these annotations: its path,
/// and the stderr and stdout of `tiza render` of it.
fn render_of(name: &str, annotations: serde_json::Value) -> (std::path::PathBuf, String, String) {
    let dir = scratch(name);
    let code: Vec<String> = (1..=10).map(|i| format!("let v{i} = {i};")).collect();
    let file = dir.join("doc.tiza");
    std::fs::write(
        &file,
        serde_json::json!({
            "schema": 1,
            "source": {"language": "rs", "first_line": 1},
            "code": code,
            "annotations": annotations
        })
        .to_string(),
    )
    .unwrap();
    let out = tiza(&["render", file.to_str().unwrap(), "--out", "-"]);
    assert!(out.status.success());
    (
        file,
        String::from_utf8_lossy(&out.stderr).into_owned(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn a_mark_kind_that_tiza_does_not_know_is_not_outside_the_code() {
    let (file, err, out) = render_of(
        "notice-kind",
        serde_json::json!([
            {"id": "m1", "step": 1, "type": "mark", "kind": "moved", "span": {"line": 3, "start": 4, "end": 6}, "label": "here"}
        ]),
    );
    assert!(
        err.contains("tiza check gives the problem of 1 annotation: m1"),
        "{err}"
    );
    assert!(!err.contains("outside the code"), "{err}");
    // Tiza draws the mark, with its label.
    assert!(out.contains("here"), "{out}");
    // And `tiza check` gives the problem.
    let check = tiza(&["check", file.to_str().unwrap()]);
    let report = String::from_utf8_lossy(&check.stdout);
    assert!(
        report.contains("m1") && report.contains("unknown mark kind"),
        "{report}"
    );
}

#[test]
fn an_annotation_outside_the_code_is_named_as_outside() {
    // A box past the last line, and a mark past it that also has a kind
    // that Tiza does not know: both are outside. A mark in the code with
    // an unknown kind gets the other notice.
    // A span past the end of its line, and an empty span: on a line of the
    // code, so not outside.
    let (_, err, out) = render_of(
        "notice-mixed",
        serde_json::json!([
            {"id": "m1", "step": 1, "type": "mark", "kind": "moved", "span": {"line": 3, "start": 4, "end": 6}},
            {"id": "b1", "step": 1, "type": "box", "from_line": 50, "to_line": 60},
            {"id": "m2", "step": 1, "type": "mark", "kind": "moved", "span": {"line": 40, "start": 0, "end": 2}},
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 2, "start": 0, "end": 80}, "label": "past the end"},
            {"id": "h2", "step": 1, "type": "highlight", "span": {"line": 4, "start": 3, "end": 3}}
        ]),
    );
    assert!(
        err.contains(
            "2 annotations outside the code and not drawn: b1 (lines 1-10), m2 (lines 1-10)"
        ),
        "{err}"
    );
    assert!(
        err.contains("tiza check gives the problem of 3 annotations: m1, h1, h2"),
        "{err}"
    );
    // Tiza draws h1, the part of it on the line, with its label: the
    // notice must not call it "not drawn".
    assert!(out.contains("past the end"), "{out}");
    let not_drawn = err
        .split(';')
        .find(|p| p.contains("not drawn"))
        .unwrap_or("");
    assert!(!not_drawn.contains("h1"), "{err}");
}
