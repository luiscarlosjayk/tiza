//! t-008 — Opening a file with a line range.
//!
//! Acceptance criteria:
//! - AC1 `file.rs:12-48`, `file.rs:12..48` and `file.rs:12` open that range with the file's numbering.
//! - AC2 `A > B` is an error; a range past the end is clamped with a notice.

use tiza_qa::{example, tiza, tiza_ok};

#[test]
fn ac1_range_syntaxes() {
    let f = example("src/stack.rs");
    for spec in [
        format!("{}:7-9", f.display()),
        format!("{}:7..9", f.display()),
    ] {
        let out = tiza_ok(&["render", &spec, "--out", "-"]);
        assert!(
            out.lines().next().unwrap().trim_start().starts_with("7 "),
            "{spec}: {out}"
        );
        assert_eq!(out.lines().count(), 3, "{spec}: {out}");
    }
    let out = tiza_ok(&["render", &format!("{}:7", f.display()), "--out", "-"]);
    assert_eq!(out.lines().count(), 1);
}

#[test]
fn ac2_bad_and_clamped_ranges() {
    let f = example("src/stack.rs");
    let out = tiza(&["render", &format!("{}:30-7", f.display()), "--out", "-"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("first line must not be after"));
    let out = tiza(&["render", &format!("{}:25-999", f.display()), "--out", "-"]);
    assert!(out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("clamped"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn qa_malformed_numeric_ranges_are_errors() {
    let f = example("src/stack.rs");
    for bad in ["12-", "-5", "12..", "1-2-3", "0"] {
        let out = tiza(&["render", &format!("{}:{bad}", f.display()), "--out", "-"]);
        assert!(!out.status.success(), "{bad} accepted");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("malformed line range") || err.contains("start at 1"),
            "{bad}: {err}"
        );
    }
}
