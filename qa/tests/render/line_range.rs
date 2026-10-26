//! Opening a file with a line range.
//!
//! Acceptance criteria:
//! - AC1 `file.rs:12-48`, `file.rs:12..48` and `file.rs:12` open that range with the file's numbering.
//! - AC2 `A > B` is an error; a range past the end is clamped with a notice.
//! - AC3 text after the last `:` that starts with a digit is a range, also
//!   for `:ref` and `:link` at step 0.

use tiza_qa::{Live, example, scratch, tiza, tiza_ok};

#[test]
fn range_syntaxes() {
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
fn bad_and_clamped_ranges() {
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
fn malformed_numeric_ranges_are_errors() {
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
    // Text after the last `:` that starts with a digit is a range, also
    // with other characters in it: never a part of the file name.
    for bad in ["3-x", "1e3", "7-9x"] {
        let out = tiza(&["render", &format!("{}:{bad}", f.display()), "--out", "-"]);
        assert!(!out.status.success(), "{bad} accepted");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains(&format!("malformed line range `{bad}`: use A, A-B or A..B")),
            "{bad}: {err}"
        );
    }
}

/// Text after a `:` that does not start with a digit is part of the name;
/// a file that is there opens, also when its name has `:` and a digit.
#[test]
fn a_colon_in_a_file_name_is_not_a_range() {
    let dir = scratch("colon-name");
    for name in ["a:b.rs", "notes 12:00.md", "app-10:30:45.log"] {
        let file = dir.join(name);
        std::fs::write(&file, "fn a() {}\nfn b() {}\n").unwrap();
        let out = tiza_ok(&["render", file.to_str().unwrap(), "--out", "-"]);
        assert_eq!(out.lines().count(), 2, "{name}: {out}");
    }
}

/// `:ref` and `:link` at step 0 read the range as `tiza FILE:A-B` does: a
/// bad range is an error, and the document does not change.
#[test]
fn ref_and_link_at_step_0_refuse_a_bad_range() {
    let live = Live::from_code("fn main() {}\n", "rs", "ref-bad-range");
    let dir = live.with(|app| app.save_path.parent().unwrap().to_path_buf());
    std::fs::write(dir.join("other.rs"), "fn a() {}\nfn b() {}\nfn c() {}\n").unwrap();
    for (cmd, says) in [
        (
            "ref other.rs:3-1",
            "range 3-1: the first line must not be after the last",
        ),
        (
            "ref other.rs:2-x",
            "malformed line range `2-x`: use A, A-B or A..B",
        ),
        ("link other.rs:0-2", "line ranges start at 1"),
        (
            "link other.rs:1e3",
            "malformed line range `1e3`: use A, A-B or A..B",
        ),
    ] {
        let before = live.with(|app| (app.doc.code.clone(), app.doc.code_src.clone()));
        live.command(cmd);
        let (message, error) = live.message().unwrap_or_default();
        assert!(error && message.contains(says), ":{cmd}: {message}");
        let after = live.with(|app| (app.doc.code.clone(), app.doc.code_src.clone()));
        assert_eq!(after, before, ":{cmd} changed the document");
    }
    // A file with `:` and a digit in its name is that file.
    std::fs::write(dir.join("at 10:30.rs"), "fn x() {}\n").unwrap();
    live.command("ref at 10:30.rs");
    let code = live.with(|app| app.doc.code.clone());
    assert_eq!(code, ["fn x() {}"], "{:?}", live.message());
    // A good range still works.
    live.command("ref other.rs:2-3");
    let code = live.with(|app| app.doc.code.clone());
    assert_eq!(code, ["fn b() {}", "fn c() {}"]);
}
