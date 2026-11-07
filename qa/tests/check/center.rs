//! `tiza check` and `"center": true`. Below the width of the block of the
//! take, the warning `center` says from which width it is centered, and
//! at that width the render moves the code. A document with `center` and
//! labels in the margin is checked as any other: the take at a wider width
//! is the same take with more blank columns on each side.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::scratch;

/// Lines 1-3, the widest is line 2 (14 columns).
const FN: [&str; 3] = ["fn main() {", "    let x = 1;", "}"];

/// A document of `FN` with no padding (the width of `check` is the width
/// of the code), a box on lines 1-3 and a note on line 2, with `center`
/// when `center`.
fn document(name: &str, center: bool) -> PathBuf {
    let path = scratch(name).join("doc.tiza");
    let doc = json!({"schema": 2, "source": {"language": "rs", "first_line": 1},
        "code": FN, "padding": [0, 0], "center": center, "annotations": [
            {"id": "b1", "step": 1, "type": "box", "from_line": 1, "to_line": 3},
            {"id": "n1", "step": 2, "type": "note", "line": 2, "text": "x is one, and it stays one"}]});
    std::fs::write(&path, doc.to_string()).unwrap();
    path
}

fn check(file: &Path, width: usize) -> (i32, Vec<Value>) {
    let o = tiza_qa::command()
        .args(["check", file.to_str().unwrap(), "--json", "--width"])
        .arg(width.to_string())
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&o.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )
    });
    (
        o.status.code().unwrap_or(-1),
        v["findings"].as_array().unwrap().clone(),
    )
}

fn centers(findings: &[Value]) -> Vec<String> {
    findings
        .iter()
        .filter(|f| f["code"] == "center")
        .map(|f| f["message"].as_str().unwrap().to_string())
        .collect()
}

/// The blank columns before `fn main` in `tiza render --width`.
fn indent(file: &Path, width: usize) -> usize {
    let o = tiza_qa::command()
        .args([
            "render",
            file.to_str().unwrap(),
            "--format",
            "txt",
            "--out",
            "-",
        ])
        .args(["--width", &width.to_string(), "--step", "1"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&o.stdout).to_string();
    let row = text.lines().find(|l| l.contains("fn main")).unwrap();
    row.find("fn main").unwrap()
}

#[test]
fn a_width_below_the_block_gets_the_warning_center() {
    let file = document("center-check-edge", true);
    // The block: the gutter (3), the box rail, two blank columns, 14
    // columns of code, a blank column, the right rail, a blank column.
    let block = 3 + 1 + 2 + 14 + 1 + 1 + 1;
    let (_, below) = check(&file, block - 1);
    assert_eq!(
        centers(&below),
        [format!(
            "content is centered from width {block}: it is {block} columns wide, and the code has {} columns here, so it stays at the left",
            block - 1
        )]
    );
    let (_, at) = check(&file, block);
    assert!(centers(&at).is_empty(), "{at:?}");
    // The render agrees: the code moves only when the block has room.
    assert_eq!(indent(&file, block - 1), indent(&file, block));
    assert_eq!(indent(&file, block + 2), indent(&file, block) + 1);
    // Without `center`, no warning.
    let (_, plain) = check(&document("center-check-off", false), block - 1);
    assert!(centers(&plain).is_empty(), "{plain:?}");
}

#[test]
fn a_centered_document_with_labels_is_checked_as_any_other() {
    // A check runs the take at the widths where a label can change: with
    // `center` the code goes right at each width, and the take is still
    // the same one (in a debug build, `check` stops if it is not).
    let centered = check(&document("center-check-labels", true), 60);
    let plain = check(&document("center-check-labels-off", false), 60);
    assert_eq!(centered.0, 0, "{:?}", centered.1);
    let codes = |f: &[Value]| -> Vec<String> {
        f.iter()
            .map(|f| f["code"].as_str().unwrap().to_string())
            .collect()
    };
    // At 60 columns the centered note has less margin: it may go under its
    // line, but nothing is cut.
    assert!(
        !codes(&centered.1).contains(&"clipped_label".to_string()),
        "{:?}",
        centered.1
    );
    assert_eq!(plain.0, 0, "{:?}", plain.1);
}
