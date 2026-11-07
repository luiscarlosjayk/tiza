//! A save writes the schema that the document needs: 1 while it uses
//! only fields of schema 1, so that Tiza 0.2 opens it, and 2 with an
//! `until` on an annotation that is not a spotlight, with `center`, or
//! with `diff` on a patch. When that `until`, `center` or `diff` goes,
//! the next save writes 1 again. A schema past the one this version reads
//! is refused, also one that is past `u32::MAX`.

use serde_json::{Value, json};
use tiza_qa::{Live, scratch, tiza};

/// The `schema` of the file that the instance saved.
fn saved_schema(live: &Live) -> Value {
    let path = live.with(|app| app.save_path.clone());
    let file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    file["schema"].clone()
}

#[test]
fn a_save_writes_the_schema_that_the_document_needs() {
    let live = Live::from_code("fn main() {\n    let a = 1;\n}\n", "rs", "schema-needed");
    live.api_ok(
        "annotation.add",
        json!({"id": "n1", "type": "note", "line": 2, "text": "a", "step": 1}),
    );
    assert_eq!(saved_schema(&live), json!(1));
    // A spotlight had `until` in schema 1.
    live.api_ok(
        "annotation.add",
        json!({"id": "s1", "type": "spotlight", "from_line": 2, "step": 1, "until": 3}),
    );
    assert_eq!(saved_schema(&live), json!(1));
    live.api_ok("annotation.update", json!({"id": "n1", "until": 2}));
    assert_eq!(saved_schema(&live), json!(2));
    live.api_ok("annotation.update", json!({"id": "n1", "until": null}));
    assert_eq!(saved_schema(&live), json!(1));
    // `center` is new in schema 2 too.
    live.api_ok("scene.patch", json!({"center": true}));
    assert_eq!(saved_schema(&live), json!(2));
    live.api_ok("scene.patch", json!({"center": false}));
    assert_eq!(saved_schema(&live), json!(1));
    // And `diff` on a patch: Tiza 0.2.1 would drop it on a save.
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "replace", "find": "let a",
               "content": "    let a = 2;", "diff": true}),
    );
    assert_eq!(saved_schema(&live), json!(2));
    live.api_ok("move.delete", json!({"index": 1}));
    assert_eq!(saved_schema(&live), json!(1));
}

#[test]
fn a_schema_past_u32_is_refused() {
    let dir = scratch("schema-u32");
    let doc = dir.join("big.tiza");
    // 2^32 + 2: as a u32 it would be 2, a schema that this version reads.
    std::fs::write(
        &doc,
        r#"{"schema":4294967298,"source":{"language":"rs"},"code":["x"],"annotations":[]}"#,
    )
    .unwrap();
    for args in [
        vec!["check", doc.to_str().unwrap()],
        vec!["render", doc.to_str().unwrap(), "--out", "-"],
    ] {
        let out = tiza(&args);
        let all = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!out.status.success(), "{args:?}: {all}");
        assert!(all.contains("schema 4294967298"), "{args:?}: {all}");
    }
}
