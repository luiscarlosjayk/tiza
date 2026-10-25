//! `.tiza` format robustness (QA-agent bugs: empty code, BOM).
//!
//! Acceptance criteria:
//! - AC1 An empty source file, `"code": []` and an empty stdin never panic: the
//!   document has one empty line.
//! - AC2 A BOM at the start of a `.tiza` or a source file is accepted.
//! - AC3 A newer `schema` and an unknown annotation `type` are explicit errors.
//! - AC4 Unknown fields are ignored on load (they are not preserved on save; documented).
//!
//! Corner case: a `.tiza` with an annotation type Tiza does not know does
//! not load — TUI, `render`, `check`, `scene.put` — and nothing of its code
//! is printed on the way.

use serde_json::json;
use tiza_qa::{Live, scratch, tiza, tiza_ok};

#[test]
fn empty_code_renders_one_blank_line() {
    let dir = scratch("empty");
    let empty = dir.join("empty.rs");
    std::fs::write(&empty, "").unwrap();
    let out = tiza_ok(&["render", empty.to_str().unwrap(), "--out", "-"]);
    assert_eq!(out.trim(), "1", "{out:?}");
    let doc = dir.join("empty.tiza");
    std::fs::write(
        &doc,
        r#"{"schema":1,"source":{"language":"rs"},"code":[],"annotations":[]}"#,
    )
    .unwrap();
    let out = tiza_ok(&["render", doc.to_str().unwrap(), "--out", "-"]);
    assert_eq!(out.trim(), "1");
    let png = dir.join("empty.png");
    tiza_ok(&[
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
fn bom_is_accepted() {
    let dir = scratch("bom");
    let doc = dir.join("bom.tiza");
    std::fs::write(&doc, "\u{feff}{\"schema\":1,\"source\":{\"language\":\"rs\"},\"code\":[\"fn a() {}\"],\"annotations\":[]}").unwrap();
    let out = tiza_ok(&["render", doc.to_str().unwrap(), "--out", "-"]);
    assert!(out.contains("fn a() {}"), "{out}");
    let src = dir.join("bom.rs");
    std::fs::write(&src, "\u{feff}fn b() {}\n").unwrap();
    let out = tiza_ok(&["render", src.to_str().unwrap(), "--out", "-"]);
    assert!(
        out.contains(" 1   fn b() {}") || out.contains("fn b() {}"),
        "{out}"
    );
    assert!(!out.contains('\u{feff}'));
}

#[test]
fn newer_schema_and_unknown_type_are_errors() {
    let dir = scratch("errors");
    let doc = dir.join("new.tiza");
    std::fs::write(
        &doc,
        r#"{"schema":2,"source":{"language":"rs"},"code":["x"],"annotations":[]}"#,
    )
    .unwrap();
    let out = tiza(&["render", doc.to_str().unwrap(), "--out", "-"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("schema"));
    let doc = dir.join("type.tiza");
    std::fs::write(&doc, r#"{"schema":1,"source":{"language":"rs"},"code":["x"],"annotations":[{"id":"a","step":1,"type":"blob","line":1}]}"#).unwrap();
    let out = tiza(&["render", doc.to_str().unwrap(), "--out", "-"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("blob"));
}

#[test]
fn an_unknown_annotation_type_does_not_load_and_prints_no_code() {
    let dir = scratch("unknown-type");
    let src = dir.join("app.env");
    std::fs::write(&src, "TOKEN=demo-token-1\nPORT=8080\n").unwrap();
    let sidecar = dir.join("app.env.tiza");
    let doc = json!({
        "schema": 1,
        "source": {"language": "sh", "first_line": 1},
        "code": {"file": "app.env"},
        "annotations": [
            {"id": "z1", "step": 1, "type": "blob", "span": {"line": 1, "start": 6, "end": 18}},
            {"id": "n1", "step": 1, "type": "note", "line": 2, "text": "the port"}
        ]
    });
    std::fs::write(&sidecar, doc.to_string()).unwrap();
    // Opening the source file picks the sidecar: refused, as the TUI does.
    let e = tiza_core::fs::open(src.to_str().unwrap())
        .map(|_| ())
        .expect_err("a sidecar with an unknown type does not open");
    assert!(e.to_string().contains("unknown variant"), "{e}");
    // `render` and `check`: exit 1, the type named, no code shown.
    for args in [
        vec!["render", sidecar.to_str().unwrap(), "--out", "-"],
        vec!["render", src.to_str().unwrap(), "--out", "-"],
        vec!["check", sidecar.to_str().unwrap()],
        vec!["check", sidecar.to_str().unwrap(), "--json"],
    ] {
        let out = tiza(&args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        let text = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        assert!(text.contains("unknown variant `blob`"), "{args:?}: {text}");
        assert!(
            !text.contains("demo-token"),
            "{args:?}: the code is shown: {text}"
        );
    }
    // Over the socket the same type is refused without repeating it.
    let live = Live::from_code("TOKEN=demo-token-1\nPORT=8080\n", "sh", "unknown-live");
    let r = live.api("scene.put", json!({"document": doc}));
    let e = r.error.expect("refused");
    assert!(e.message.contains("unknown variant «…»"), "{e:?}");
    assert!(!e.message.contains("blob"), "{e:?}");
    let r = live.api(
        "annotation.add",
        json!({"type": "blob", "at": {"line": 1, "find": "TOKEN="}}),
    );
    assert!(r.error.is_some());
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0);
}
