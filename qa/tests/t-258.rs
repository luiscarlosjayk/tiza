//! Two socket requests could lose the work of the presenter: a
//! `scene.clear` whose `step` Tiza could not read cleared every
//! annotation, and a `scene.put` with a newer `schema` was saved in a file
//! that this version cannot open.
//!
//! Acceptance criteria:
//! - AC1 `scene.clear` refuses a `step` that is not a whole number, 0 or
//!   more (`"2"`, `-1`, `1.5`, `null`, 2^32 + 2, which `as u32` made step
//!   2), parameters that are not an object (`null`, `[]`) and an unknown
//!   key, with `invalid_argument`,
//!   and changes nothing. `{}` clears every annotation and `{"step": N}`
//!   the annotations of step N, as before. In
//!   supervised mode the refusal comes at once: the request is not parked.
//!   The MCP tool with no step still clears every annotation; it refuses
//!   an unknown key and a `null` step.
//! - AC2 `scene.put` refuses a `schema` newer than this version with
//!   `unsupported_schema`, before the document changes or is saved. An
//!   older `schema` is saved as the current one. A file and a `scene.put`
//!   give the same error, with the schema as it was written.
//! - AC3 In supervised mode, a method that does not exist is answered at
//!   once (`invalid_argument`), not parked as a change to review.
//!   Parking follows `METHODS`: each method there is one that the instance
//!   knows, and a name that is not there gets `unknown method`.
//! - AC4 A request that changes nothing (`scene.patch {}`, an
//!   `annotation.update` with only its `id`) leaves `revision` as it is. A
//!   request that changes something makes it grow (by 1 or more).
//! - AC5 The result schema of `scene.clear`, `code.get`, `style.set` and
//!   `theme.set` names every field of the reply.
//! - AC6 The help of `tiza scene patch` and the `scene.patch` description
//!   name every field that `scene.patch` takes.
//! - AC7 `tiza FILE:A-B` opens `FILE.A-B.tiza` from an earlier session, found
//!   under the clamped lines, and says so. Another range is another
//!   document. A `FILE.A-B.tiza` that does not load is an error, and the
//!   file stays as it is. `tiza render FILE:A-B` still shows only the
//!   lines, without that document.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};
use tiza_core::protocol::Response;
use tiza_qa::{Live, McpClient, tiza_ok};

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n}\n";

/// An instance with three notes, on more than one step.
fn three_notes(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    for (line, step) in [(1, 0), (2, 1), (3, 2)] {
        live.api_ok(
            "annotation.add",
            json!({"type": "note", "line": line, "text": format!("n{line}"), "step": step}),
        );
    }
    live
}

/// The number of annotations and the revision.
fn state(live: &Live) -> (u64, u64) {
    let s = live.api_ok("status", json!({}));
    (
        s["annotations"].as_u64().unwrap(),
        s["revision"].as_u64().unwrap(),
    )
}

/// A call that must be answered within 10 s (a parked request waits up to
/// 60 s).
fn api_within(live: &Live, method: &str, params: Value) -> Response {
    let (tx, rx) = mpsc::channel();
    let socket = live.socket.clone();
    let method = method.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(tiza_tui::ipc::call(&socket, &method, params));
    });
    rx.recv_timeout(Duration::from_secs(10))
        .expect("no answer in 10 s: the request was parked")
        .expect("socket call")
}

const BAD_CLEARS: [&str; 10] = [
    r#"{"step": "2"}"#,
    r#"{"step": -1}"#,
    r#"{"step": 1.5}"#,
    r#"{"step": null}"#,
    r#"{"step": 4294967296}"#,
    r#"{"step": 4294967298}"#,
    "null",
    "[]",
    "[null]",
    r#"{"stp": 2}"#,
];

#[test]
fn ac1_a_clear_with_a_step_tiza_cannot_read_changes_nothing() {
    let live = three_notes("t258-clear");
    let before = state(&live);
    assert_eq!(before.0, 3);
    for bad in BAD_CLEARS {
        let params: Value = serde_json::from_str(bad).unwrap();
        let r = live.api("scene.clear", params);
        let e = r
            .error
            .unwrap_or_else(|| panic!("{bad}: accepted: {:?}", r.result));
        assert_eq!(e.code, "invalid_argument", "{bad}: {}", e.message);
        assert_eq!(state(&live), before, "{bad} changed the scene");
    }
    // The good forms, as before: one step, then the rest.
    let doc = live.api_ok("scene.get", json!({}))["document"].clone();
    let at_1 = doc["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|a| a["step"] == json!(1))
        .count() as u64;
    assert!(at_1 > 0 && at_1 < 3, "{doc}");
    let r = live.api_ok("scene.clear", json!({"step": 1}));
    assert_eq!(
        (r["removed"].as_u64(), r["remaining"].as_u64()),
        (Some(at_1), Some(3 - at_1))
    );
    let r = live.api_ok("scene.clear", json!({}));
    assert_eq!(
        (r["removed"].as_u64(), r["remaining"].as_u64()),
        (Some(3 - at_1), Some(0))
    );
}

#[test]
fn ac1_supervised_mode_refuses_a_bad_clear_at_once() {
    let live = three_notes("t258-supervised");
    live.api_ok("supervise.set", json!({"enabled": true}));
    let before = state(&live);
    for bad in BAD_CLEARS {
        let params: Value = serde_json::from_str(bad).unwrap();
        let r = api_within(&live, "scene.clear", params);
        let e = r
            .error
            .unwrap_or_else(|| panic!("{bad}: accepted: {:?}", r.result));
        assert_eq!(e.code, "invalid_argument", "{bad}: {}", e.message);
    }
    let pending = live.api_ok("supervise.status", json!({}));
    assert_eq!(pending["pending"], json!([]), "{pending}");
    assert_eq!(state(&live), before);
}

#[test]
fn ac1_the_mcp_tool_with_no_step_still_clears_every_annotation() {
    let live = three_notes("t258-mcp");
    let mut mcp = McpClient::start(&live.socket);
    for bad in [json!({"stp": 2}), json!({"step": null})] {
        let (err, text, _) = mcp.call("clear_annotations", bad.clone());
        assert!(err, "{bad} accepted: {text}");
        assert_eq!(state(&live).0, 3, "{bad} cleared: {text}");
    }
    let (err, text, _) = mcp.call("clear_annotations", json!({}));
    assert!(!err, "{text}");
    assert_eq!(state(&live).0, 0, "{text}");
}

/// The sidecar that `live` autosaves to: its bytes and its time.
fn saved(live: &Live) -> (PathBuf, Vec<u8>, std::time::SystemTime) {
    let path = live.with(|app| app.save_path.clone());
    let bytes = std::fs::read(&path).unwrap();
    let time = std::fs::metadata(&path).unwrap().modified().unwrap();
    (path, bytes, time)
}

#[test]
fn ac2_a_put_with_a_newer_schema_changes_and_saves_nothing() {
    let live = three_notes("t258-put");
    let scene = live.api_ok("scene.get", json!({}));
    let before = state(&live);
    let (path, bytes, time) = saved(&live);
    let mut doc = scene["document"].clone();
    doc["schema"] = json!(99);
    doc["annotations"] = json!([]);
    let r = live.api("scene.put", json!({"document": doc}));
    let e = r
        .error
        .unwrap_or_else(|| panic!("accepted: {:?}", r.result));
    assert_eq!(e.code, "unsupported_schema", "{}", e.message);
    assert_eq!(state(&live), before);
    assert_eq!(
        live.api_ok("scene.get", json!({}))["document"],
        scene["document"]
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), time);
    // In supervised mode too, at once.
    live.api_ok("supervise.set", json!({"enabled": true}));
    let before = state(&live);
    let r = api_within(&live, "scene.put", json!({"document": doc}));
    assert_eq!(
        r.error.map(|e| e.code).as_deref(),
        Some("unsupported_schema")
    );
    assert_eq!(state(&live), before);
}

#[test]
fn ac2_a_put_with_an_older_schema_is_saved_as_the_current_one() {
    let live = three_notes("t258-put-old");
    let mut doc = live.api_ok("scene.get", json!({}))["document"].clone();
    doc["schema"] = json!(0);
    doc["title"] = json!("older");
    live.api_ok("scene.put", json!({"document": doc}));
    let now = live.api_ok("scene.get", json!({}))["document"].clone();
    assert_eq!(now["schema"], json!(tiza_core::model::SCHEMA_VERSION));
    let (path, bytes, _) = saved(&live);
    let file: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        file["schema"],
        json!(tiza_core::model::SCHEMA_VERSION),
        "{}",
        path.display()
    );
    assert_eq!(file["title"], json!("older"));
}

#[test]
fn ac2_a_file_and_a_put_give_the_same_error_for_a_newer_schema() {
    let live = three_notes("t258-same-error");
    let doc = live.api_ok("scene.get", json!({}))["document"].clone();
    // 2^32 + 2 would read as schema 2 if it were cut to 32 bits.
    for schema in [json!(99), json!(4_294_967_298u64)] {
        let mut d = doc.clone();
        d["schema"] = schema.clone();
        let file = tiza_core::fs::parse_document(&d.to_string(), None).unwrap_err();
        assert_eq!(file.code(), "unsupported_schema");
        assert!(file.to_string().contains(&schema.to_string()), "{file}");
        let r = live.api("scene.put", json!({"document": d}));
        let e = r.error.unwrap_or_else(|| panic!("{schema}: accepted"));
        assert_eq!(
            (e.code.as_str(), e.message),
            ("unsupported_schema", file.to_string())
        );
    }
}

#[test]
fn ac3_supervised_mode_answers_an_unknown_method_at_once() {
    let live = three_notes("t258-unknown");
    live.api_ok("supervise.set", json!({"enabled": true}));
    let r = api_within(&live, "nope.method", json!({}));
    let e = r
        .error
        .unwrap_or_else(|| panic!("accepted: {:?}", r.result));
    assert_eq!(e.code, "invalid_argument", "{}", e.message);
    let pending = live.api_ok("supervise.status", json!({}));
    assert_eq!(pending["pending"], json!([]), "{pending}");
}

#[test]
fn ac4_a_request_that_changes_nothing_keeps_the_revision() {
    let live = three_notes("t258-revision");
    let id = live.api_ok("scene.get", json!({}))["document"]["annotations"][0]["id"].clone();
    let (_, before) = state(&live);
    live.api_ok("scene.patch", json!({}));
    assert_eq!(state(&live).1, before, "scene.patch {{}}");
    live.api_ok("annotation.update", json!({"id": id}));
    assert_eq!(state(&live).1, before, "annotation.update with only its id");
    // A change still makes it grow.
    live.api_ok("scene.patch", json!({"title": "changed"}));
    let (_, after) = state(&live);
    assert!(after > before, "{before} -> {after}");
    live.api_ok("annotation.update", json!({"id": id, "label": "x"}));
    assert!(state(&live).1 > after);
}

#[test]
fn ac5_the_result_schemas_name_every_field_of_the_reply() {
    let live = three_notes("t258-results");
    let api = live.api_ok("api.schema", json!({}));
    for (method, params) in [
        ("scene.clear", json!({"step": 2})),
        ("code.get", json!({})),
        (
            "style.set",
            json!({"name": "hot", "glyph": "*", "legend": "hot"}),
        ),
        ("theme.set", json!({"theme": "light"})),
    ] {
        let reply = live.api_ok(method, params);
        let schema = api["methods"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["method"] == method)
            .unwrap_or_else(|| panic!("{method} not in api.schema"));
        let known = schema["result"]["properties"].as_object().unwrap();
        for key in reply.as_object().unwrap().keys() {
            assert!(
                known.contains_key(key),
                "{method}: `{key}` is not in its result schema"
            );
        }
    }
}

#[test]
fn ac6_the_patch_help_and_description_name_every_field() {
    let api: Value = serde_json::from_str(&tiza_ok(&["schema", "api"])).unwrap();
    let patch = api["methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["method"] == "scene.patch")
        .unwrap();
    let description = patch["description"].as_str().unwrap();
    let help = tiza_ok(&["scene", "patch", "--help"]);
    let fields: Vec<&String> = patch["params"]["properties"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert!(fields.len() >= 10, "{fields:?}");
    for f in fields {
        assert!(
            description.contains(f.as_str()),
            "the description lacks `{f}`"
        );
        assert!(
            help.contains(f.as_str()),
            "`tiza scene patch --help` lacks `{f}`"
        );
    }
}

#[test]
fn ac3_every_method_in_methods_is_one_the_instance_knows() {
    let live = Live::from_code(CODE, "rs", "t258-methods");
    let unknown = |r: &Response| {
        r.error
            .as_ref()
            .is_some_and(|e| e.message.starts_with("unknown method"))
    };
    for (method, _) in tiza_core::protocol::METHODS {
        let r = live.api(method, json!({}));
        assert!(
            !unknown(&r),
            "{method} is in METHODS, and the instance does not know it"
        );
    }
    assert!(unknown(&live.api("nope.method", json!({}))));
}

#[test]
fn ac7_a_range_opens_its_document_from_an_earlier_session() {
    use tiza_core::fs;
    let dir = tiza_qa::scratch("t258-range");
    let file = dir.join("three.txt");
    std::fs::write(&file, "one\ntwo\nthree\n").unwrap();
    let spec = |r: &str| format!("{}:{r}", file.display());
    // Session 1: a note on lines 2-3, autosaved.
    let o = fs::open(&spec("2-3")).unwrap();
    assert!(!o.existing);
    let save = dir.join("three.txt.2-3.tiza");
    assert_eq!(o.save_path, save);
    {
        let live = Live::open(o.document, o.save_path, dir.join("a.sock"));
        live.api_ok(
            "annotation.add",
            json!({"type": "note", "line": 2, "text": "kept"}),
        );
    }
    // Session 2, the same range and one that clamps to it: the note is there.
    for (n, r) in ["2-3", "2..9"].iter().enumerate() {
        let o = fs::open(&spec(r)).unwrap();
        assert!(o.existing, "{r}");
        assert_eq!(o.save_path, save, "{r}");
        assert!(
            o.notice
                .as_deref()
                .unwrap_or("")
                .contains("opened the existing"),
            "{r}: {:?}",
            o.notice
        );
        let live = Live::open(o.document, o.save_path, dir.join(format!("b{n}.sock")));
        assert_eq!(state(&live).0, 1, "{r}");
    }
    // `tiza render` shows only the lines of a range, without its document.
    let out = std::process::Command::new(tiza_qa::bin())
        .args(["render", &spec("2-3"), "--format", "txt"])
        .output()
        .unwrap();
    let (text, err) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{err}");
    assert!(text.contains("two") && !text.contains("kept"), "{text}");
    assert!(!err.contains("opened the existing"), "{err}");
    // Another range is another document.
    let o = fs::open(&spec("1-2")).unwrap();
    assert!(!o.existing && o.document.annotations.is_empty());
    assert_eq!(o.save_path, dir.join("three.txt.1-2.tiza"));
    // A document there that does not load: an error, and the file stays.
    std::fs::write(&save, "not json").unwrap();
    assert!(fs::open(&spec("2-3")).is_err());
    assert_eq!(std::fs::read_to_string(&save).unwrap(), "not json");
}
