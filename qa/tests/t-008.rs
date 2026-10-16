//! t-008 — Opening a file with a line range.
//!
//! Acceptance criteria:
//! - AC1 `file.rs:12-48`, `file.rs:12..48` and `file.rs:12` open that range with the file's numbering.
//! - AC2 `A > B` is an error; a range past the end is clamped with a notice.

use codedraw_qa::{codedraw, codedraw_ok, example};

#[test]
fn ac1_range_syntaxes() {
    let f = example("src/stack.rs");
    for spec in [
        format!("{}:7-9", f.display()),
        format!("{}:7..9", f.display()),
    ] {
        let out = codedraw_ok(&["render", &spec, "--out", "-"]);
        assert!(
            out.lines().next().unwrap().trim_start().starts_with("7 "),
            "{spec}: {out}"
        );
        assert_eq!(out.lines().count(), 3, "{spec}: {out}");
    }
    let out = codedraw_ok(&["render", &format!("{}:7", f.display()), "--out", "-"]);
    assert_eq!(out.lines().count(), 1);
}

#[test]
fn ac2_bad_and_clamped_ranges() {
    let f = example("src/stack.rs");
    let out = codedraw(&["render", &format!("{}:30-7", f.display()), "--out", "-"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("first line must not be after"));
    let out = codedraw(&["render", &format!("{}:25-999", f.display()), "--out", "-"]);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("clamped"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
