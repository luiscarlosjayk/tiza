//! t-047 — `.codedraw` format robustness (QA-agent bugs: empty code, BOM).
//!
//! Acceptance criteria:
//! - AC1 An empty source file, `"code": []` and an empty stdin never panic: the
//!   document has one empty line.
//! - AC2 A BOM at the start of a `.codedraw` or a source file is accepted.
//! - AC3 A newer `schema` and an unknown annotation `type` are explicit errors.
//! - AC4 Unknown fields are ignored on load (they are not preserved on save; documented).

use codedraw_qa::{codedraw, codedraw_ok, scratch};

#[test]
fn ac1_empty_code_renders_one_blank_line() {
    let dir = scratch("t047-empty");
    let empty = dir.join("empty.rs");
    std::fs::write(&empty, "").unwrap();
    let out = codedraw_ok(&["render", empty.to_str().unwrap(), "--out", "-"]);
    assert_eq!(out.trim(), "1", "{out:?}");
    let doc = dir.join("empty.codedraw");
    std::fs::write(
        &doc,
        r#"{"schema":1,"source":{"language":"rs"},"code":[],"annotations":[]}"#,
    )
    .unwrap();
    let out = codedraw_ok(&["render", doc.to_str().unwrap(), "--out", "-"]);
    assert_eq!(out.trim(), "1");
    let png = dir.join("empty.png");
    codedraw_ok(&[
        "render",
        doc.to_str().unwrap(),
        "--out",
        png.to_str().unwrap(),
        "--scale",
        "1",
    ]);
    assert!(png.exists());
}

#[test]
fn ac2_bom_is_accepted() {
    let dir = scratch("t047-bom");
    let doc = dir.join("bom.codedraw");
    std::fs::write(&doc, "\u{feff}{\"schema\":1,\"source\":{\"language\":\"rs\"},\"code\":[\"fn a() {}\"],\"annotations\":[]}").unwrap();
    let out = codedraw_ok(&["render", doc.to_str().unwrap(), "--out", "-"]);
    assert!(out.contains("fn a() {}"), "{out}");
    let src = dir.join("bom.rs");
    std::fs::write(&src, "\u{feff}fn b() {}\n").unwrap();
    let out = codedraw_ok(&["render", src.to_str().unwrap(), "--out", "-"]);
    assert!(
        out.contains(" 1   fn b() {}") || out.contains("fn b() {}"),
        "{out}"
    );
    assert!(!out.contains('\u{feff}'));
}

#[test]
fn ac3_newer_schema_and_unknown_type_are_errors() {
    let dir = scratch("t047-errors");
    let doc = dir.join("new.codedraw");
    std::fs::write(
        &doc,
        r#"{"schema":2,"source":{"language":"rs"},"code":["x"],"annotations":[]}"#,
    )
    .unwrap();
    let out = codedraw(&["render", doc.to_str().unwrap(), "--out", "-"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("schema"));
    let doc = dir.join("type.codedraw");
    std::fs::write(&doc, r#"{"schema":1,"source":{"language":"rs"},"code":["x"],"annotations":[{"id":"a","step":1,"type":"blob","line":1}]}"#).unwrap();
    let out = codedraw(&["render", doc.to_str().unwrap(), "--out", "-"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("blob"));
}
