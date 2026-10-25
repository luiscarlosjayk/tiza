//! t-040 — `-` as stdin everywhere and the published JSON Schema.
//!
//! Acceptance criteria:
//! - AC1 `tiza schema document` prints the JSON Schema of the `.tiza`
//!   format; `tiza schema api` the schema of every socket method (params and
//!   result) plus the request/response envelopes. Both are committed under
//!   `schema/` and a unit test fails when they go stale.
//! - AC2 `api.schema` over the socket returns the same API schema.
//! - AC3 `-` reads stdin: `tiza render -` (code with `--lang`, or a `.tiza`
//!   document), `tiza -` opens stdin in the TUI, `tiza api METHOD -`
//!   reads the params from stdin; `--out -` writes to stdout.
//! - AC4 Every bundled example and QA fixture validates against the document
//!   schema (checked with a JSON Schema validator in CI via Python; here: the
//!   schema names every shape and the required fields).

use serde_json::json;
use std::io::Write;
use std::process::{Command, Stdio};
use tiza_qa::{Live, bin, fixture, tiza_ok};

fn run_with_stdin(args: &[&str], input: &str) -> std::process::Output {
    let mut child = Command::new(bin())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn ac1_schemas_are_printed_and_committed() {
    let doc: serde_json::Value = serde_json::from_str(&tiza_ok(&["schema", "document"])).unwrap();
    assert_eq!(doc["title"], "Tiza document (.tiza)");
    let s = doc.to_string();
    for t in [
        "highlight",
        "mark",
        "box",
        "arrow",
        "note",
        "callout",
        "spotlight",
    ] {
        assert!(s.contains(&format!("\"{t}\"")), "{t} missing");
    }
    let api: serde_json::Value = serde_json::from_str(&tiza_ok(&["schema", "api"])).unwrap();
    assert_eq!(api["protocol"], 1);
    let methods: Vec<&str> = api["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["method"].as_str().unwrap())
        .collect();
    for m in [
        "annotation.add",
        "annotation.update",
        "scene.clear",
        "api.schema",
        "render.text",
    ] {
        assert!(methods.contains(&m), "{m} missing from the API schema");
    }
    // The replies of annotation.add/update say where the annotation is
    // (moved here from t-155).
    for name in ["annotation.add", "annotation.update"] {
        let method = api["methods"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["method"] == name)
            .unwrap_or_else(|| panic!("{name} missing from the API schema"));
        for field in ["shown_step", "visible", "hidden", "step_title"] {
            assert!(
                !method["result"]["properties"][field].is_null(),
                "the result of {name} lacks {field}"
            );
        }
    }
    // Committed copies are byte-identical in content.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../schema");
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("tiza.schema.json")).unwrap())
            .unwrap();
    assert_eq!(
        on_disk, doc,
        "schema/tiza.schema.json is stale: cargo run -- schema document > schema/tiza.schema.json"
    );
    let on_disk: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("api.schema.json")).unwrap())
            .unwrap();
    assert_eq!(on_disk, api, "schema/api.schema.json is stale");
}

#[test]
fn ac2_api_schema_over_the_socket() {
    let live = Live::from_code("fn a() {}\n", "rs", "t040-socket");
    let v = live.api_ok("api.schema", json!({}));
    assert_eq!(v["protocol"], 1);
    assert!(v["methods"].as_array().unwrap().len() >= 19);
    let add = v["methods"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["method"] == "annotation.add")
        .unwrap();
    assert!(add["params"].to_string().contains("\"callout\""));
}

#[test]
fn ac3_dash_reads_stdin() {
    // Code through stdin, language from --lang.
    let out = run_with_stdin(
        &["render", "-", "--lang", "rs", "--out", "-"],
        "fn a() {}\n",
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("fn a() {}"));
    // A document through stdin (embedded code).
    let doc = std::fs::read_to_string(fixture("t-092/callouts.tiza")).unwrap();
    let out = run_with_stdin(
        &["render", "-", "--out", "-", "--step", "1", "--width", "114"],
        &doc,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("╭─ move ─"));
    // Empty stdin is an error, not a blank render.
    let out = run_with_stdin(&["render", "-", "--out", "-"], "   \n");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("empty stdin"));
    // `tiza api METHOD -` takes the params from stdin.
    let live = Live::from_code("fn a() {}\n", "rs", "t040-api");
    let out = run_with_stdin(
        &[
            "api",
            "annotation.add",
            "-",
            "--socket",
            live.socket.to_str().unwrap(),
        ],
        r#"{"type":"note","line":1,"text":"from stdin"}"#,
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let list = live.api_ok("scene.list", json!({}));
    assert!(list.to_string().contains("from stdin"));
}

#[test]
fn ac4_examples_and_fixtures_match_the_schema_shape() {
    // Structural check without a validator crate: every annotation's `type`
    // is one of the schema's variants and carries the required fields.
    let doc: serde_json::Value = serde_json::from_str(&tiza_ok(&["schema", "document"])).unwrap();
    let variants = doc.to_string();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = std::fs::read_dir(root.join("../examples")).unwrap().chain(
        std::fs::read_dir(root.join("fixtures"))
            .unwrap()
            .flat_map(|d| std::fs::read_dir(d.unwrap().path()).unwrap()),
    );
    let mut n = 0;
    for f in files {
        let p = f.unwrap().path();
        if p.extension().and_then(|e| e.to_str()) != Some("tiza") {
            continue;
        }
        let d: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        for a in d["annotations"].as_array().unwrap() {
            let t = a["type"].as_str().unwrap();
            assert!(
                variants.contains(&format!("\"{t}\"")),
                "{}: unknown type {t}",
                p.display()
            );
            assert!(
                a["id"].is_string() && a["step"].is_number(),
                "{}: id/step",
                p.display()
            );
        }
        n += 1;
    }
    assert!(n >= 6, "{n} documents checked");
}
