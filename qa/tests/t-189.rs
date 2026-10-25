//! t-189 — serde «invalid type» errors hid what the agent sent:
//! `invalid type: string «…», expected u32` instead of `"three"`.
//!
//! Acceptance criteria:
//! - AC1 `invalid type` errors from the socket, the MCP and loading a
//!   `.tiza` name the value that was sent (truncated to 40 characters with
//!   …), as serde writes it: `invalid type: string "three", expected u32`.
//! - AC2 Closed values stay as they are: never repeated over the socket or
//!   the MCP, the list of valid names instead.
//! - AC3 This file; the t-167 test that asserted no value for MCP
//!   arguments that do not deserialize now expects the wrong-typed value
//!   named (the others assert «…» for closed values, which keep it);
//!   CHANGELOG (Changed).
//!
//! Corner cases: a very long string value is truncated; a value with
//! control characters or newlines is escaped (serde writes it as Rust's
//! `{:?}`), never raw. A non-UTF-8 file does not load at all (`stream did
//! not contain valid UTF-8`) and quotes nothing.
//!
//! From t-167 (moved here when its no-echo checks of the redact era
//! went): MCP arguments of the wrong type name the value, serde's list of
//! valid names stays whole, and `focus.set` checks its lines.
//!
//! Review (`rev_*`): the cut cannot be dodged by a value that says
//! `unknown variant` or `", expected`; a load error is printed once.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, bin, scratch, tiza};

fn error_message(live: &Live, method: &str, params: serde_json::Value) -> String {
    let r = live.api(method, params);
    r.error
        .unwrap_or_else(|| panic!("{method} should fail: {:?}", r.result))
        .message
}

#[test]
fn ac1_socket_names_the_value_sent() {
    // Test step 1.
    let live = Live::from_code("fn main() {\n    let x = 1;\n}\n", "rs", "t189-socket");
    let m = error_message(
        &live,
        "annotation.add",
        json!({"type": "note", "line": "three", "text": "x"}),
    );
    assert!(
        m.contains("invalid type: string \"three\", expected u32"),
        "{m}"
    );
}

#[test]
fn ac1_corner_a_long_value_is_cut_and_controls_are_escaped() {
    let live = Live::from_code("fn main() {}\n", "rs", "t189-long");
    let value = format!("{}\n\u{1b}[31m", "x".repeat(60));
    let m = error_message(
        &live,
        "annotation.add",
        json!({"type": "note", "line": value, "text": "x"}),
    );
    assert!(
        m.contains(&format!("string \"{}…\", expected u32", "x".repeat(40))),
        "{m}"
    );
    assert!(!m.contains('\u{1b}') && !m.contains('\n'), "{m:?}");
    // Short enough to be whole: the escapes are shown, not sent.
    let m = error_message(
        &live,
        "annotation.add",
        json!({"type": "note", "line": "a\nb\u{1b}", "text": "x"}),
    );
    assert!(m.contains(r#"string "a\nb\u{1b}""#), "{m}");
    assert!(!m.contains('\u{1b}'), "{m:?}");
}

#[test]
fn ac1_loading_names_the_value_and_cuts_a_long_one() {
    let dir = scratch("t189-load");
    let file = dir.join("bad.tiza");
    let doc = |line: serde_json::Value| {
        json!({
            "schema": 1, "source": {"language": "rs"}, "code": ["fn main() {}"], "step": 0,
            "annotations": [{"id": "a1", "step": 0, "type": "note", "line": line, "text": "x"}]
        })
        .to_string()
    };
    std::fs::write(&file, doc(json!("three"))).unwrap();
    let out = std::process::Command::new(bin())
        .arg("check")
        .arg(&file)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains(r#"string "three", expected u32"#), "{text}");

    std::fs::write(&file, doc(json!(format!("{}\n\u{1b}[31m", "y".repeat(80))))).unwrap();
    for args in [vec!["check"], vec!["render", "--out", "-"]] {
        let out = std::process::Command::new(bin())
            .args(&args)
            .arg(&file)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            text.contains(&format!("string \"{}…\", expected u32", "y".repeat(40))),
            "{args:?}: {text}"
        );
        assert!(!text.contains('\u{1b}'), "{args:?}: {text:?}");
    }
}

#[test]
fn rev_the_cut_cannot_be_dodged() {
    let live = Live::from_code("fn main() {}\n", "rs", "t189-rev-dodge");
    for tail in ["unknown variant `x", "\", expected u32 and more"] {
        let value = format!("{}{tail}", "A".repeat(300));
        let m = error_message(
            &live,
            "annotation.add",
            json!({"type": "note", "line": value, "text": "x"}),
        );
        assert!(!m.contains(&"A".repeat(41)), "{m}");
        assert!(m.contains(&format!("{}…", "A".repeat(40))), "{m}");
    }
}

#[test]
fn rev_a_load_error_is_printed_once() {
    let dir = scratch("t189-rev-once");
    let file = dir.join("long.tiza");
    let doc = json!({
        "schema": 1, "source": {"language": "rs"}, "code": ["fn main() {}"], "step": 0,
        "annotations": [{"id": "a1", "step": 0, "type": "note", "line": "y".repeat(80), "text": "x"}]
    });
    std::fs::write(&file, doc.to_string()).unwrap();
    let out = std::process::Command::new(bin())
        .args(["render", "--out", "-"])
        .arg(&file)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains(&"y".repeat(41)), "{err}");
    assert_eq!(err.matches("invalid type").count(), 1, "{err}");
}

// ---------- From t-167 ----------

const VALUE: &str = "abc123Q7valueXyz789";

fn code() -> Vec<String> {
    vec![
        "[api]".into(),
        format!("token={VALUE}"),
        "region=eu-west-1".into(),
        "port=8080".into(),
    ]
}

fn highlight() -> Value {
    json!({"id": "h1", "step": 1, "type": "highlight", "span": {"line": 3, "start": 0, "end": 6}})
}

/// Every piece of 4 characters of the value.
fn pieces() -> Vec<String> {
    let c: Vec<char> = VALUE.chars().collect();
    (0..=c.len() - 4)
        .map(|i| c[i..i + 4].iter().collect())
        .collect()
}

#[track_caller]
fn assert_clean(what: &str, s: &str) {
    for p in pieces() {
        assert!(!s.contains(&p), "{what} shows «{p}» of the value:\n{s}");
    }
}

fn patch(extra: Value) -> Value {
    let mut m = json!({"at_step": 2, "action": "code.patch"});
    for (k, v) in extra.as_object().unwrap() {
        m[k] = v.clone();
    }
    m
}

fn document(moves: Vec<Value>) -> Value {
    let anns = vec![highlight()];
    json!({
        "schema": 1,
        "source": {"language": "ini", "first_line": 1},
        "code": code(),
        "annotations": anns,
        "moves": moves,
    })
}

fn write(dir: &Path, name: &str, doc: &Value) -> PathBuf {
    let p = dir.join(format!("{name}.tiza"));
    std::fs::write(&p, serde_json::to_string_pretty(doc).unwrap()).unwrap();
    p
}

fn stderr(args: &[&str]) -> String {
    let out = tiza(args);
    assert!(!out.status.success(), "{args:?} succeeded");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn live(name: &str) -> Live {
    let dir = scratch(name);
    let doc = tiza_core::fs::parse_document(&document(vec![]).to_string(), None).unwrap();
    Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"))
}

fn err(live: &Live, method: &str, params: Value) -> (String, String) {
    let r = live.api(method, params.clone());
    let e = r
        .error
        .unwrap_or_else(|| panic!("{method} {params} succeeded"));
    (e.code, e.message)
}

/// A document whose line 2 is `token=<value>`, with a highlight h1, a box
/// b1 and an arrow a1 to update.
fn live_valued(value: &str, name: &str) -> Live {
    let dir = scratch(name);
    let anns = vec![
        highlight(),
        json!({"id": "b1", "step": 1, "type": "box", "from_line": 1, "to_line": 4}),
        json!({"id": "a1", "step": 1, "type": "arrow", "from": {"line": 1, "col": 0}, "to": {"line": 4, "col": 0}}),
    ];
    let doc = json!({
        "schema": 1,
        "source": {"language": "ini", "first_line": 1},
        "code": ["[api]", format!("token={value}"), "region=eu-west-1", "port=8080"],
        "annotations": anns,
    });
    let doc = tiza_core::fs::parse_document(&doc.to_string(), None).unwrap();
    // A file `code.link` can show, for its `lines`.
    std::fs::write(dir.join("other.ini"), "a=1\nb=2\n").unwrap();
    Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"))
}

#[test]
fn review2_mcp_parameters_of_the_wrong_type_name_the_value() {
    // Since t-189 an `invalid type` names the value sent (it is not a
    // closed value): `invalid type: string "…", expected u32`. Any other
    // message that does not deserialize still quotes nothing.
    let live = live("t167-r2");
    let mut mcp = McpClient::start(&live.socket);
    // A string where an object or a line number goes is a wrong type:
    // named since t-189.
    for (tool, args) in [
        ("highlight", json!({"at": "Q7valueXyz789"})),
        ("add_box", json!({"from_line": "Q7valueXyz789"})),
        ("point_at", json!({"line": "Q7valueXyz789"})),
    ] {
        let (is_err, text, _) = mcp.call(tool, args.clone());
        assert!(is_err, "{tool} {args}: {text}");
        assert!(
            text.contains(
                "failed to deserialize parameters: invalid type: string \"Q7valueXyz789\""
            ),
            "MCP {tool} {args}: {text}"
        );
    }
}

#[test]
fn rework2_an_unknown_type_action_or_op_keeps_the_list_of_valid_names() {
    let dir = scratch("t167-rw2");
    let target = live("t167-rw2-live");
    let types =
        "expected one of `highlight`, `mark`, `box`, `arrow`, `note`, `callout`, `spotlight`";
    let actions = "expected one of `code.patch`, `code.link`, `cursor`, `select`, `wait`";
    let ops = "expected one of `insert`, `replace`, `delete`";
    // The document says `box` and `note` itself: they stay.
    let extra = vec![
        json!({"id": "b2", "step": 1, "type": "box", "from_line": 1, "to_line": 2}),
        json!({"id": "n2", "step": 1, "type": "note", "line": 1, "text": "x"}),
    ];
    for (name, ann, mv, want) in [
        (
            "type",
            Some(json!({"id": "z1", "step": 1, "type": "Q7valueXyz789", "line": 1})),
            None,
            types,
        ),
        (
            "action",
            None,
            Some(json!({"at_step": 2, "action": "Q7valueXyz789"})),
            actions,
        ),
        (
            "op",
            None,
            Some(patch(
                json!({"op": "Q7valueXyz789", "find": "port", "content": "x"}),
            )),
            ops,
        ),
        (
            "plain-op",
            None,
            Some(patch(json!({"op": "zap", "find": "port", "content": "x"}))),
            ops,
        ),
    ] {
        let mut doc = document(mv.into_iter().collect());
        let anns = doc["annotations"].as_array_mut().unwrap();
        anns.extend(extra.clone());
        anns.extend(ann);
        let p = write(&dir, name, &doc);
        let ps = p.to_str().unwrap();
        // On load the value is quoted as written (t-047); the list stays.
        let e = stderr(&["render", ps, "--out", "-"]);
        assert!(e.contains(want), "{name}: render: {e}");
        let out = tiza(&["check", ps]);
        let report = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        assert!(report.contains(want), "{name}: check: {report}");
        // Over the socket, never the value: only the list.
        let (_, msg) = err(&target, "scene.put", json!({"document": doc}));
        assert!(msg.contains(want), "{name}: scene.put: {msg}");
        assert!(
            msg.contains("unknown variant «…»"),
            "{name}: scene.put: {msg}"
        );
        assert_clean(&format!("{name}: scene.put"), &msg);
    }
    // The patch says `box` itself: it stays in the list.
    let live = live("t167-rw2-socket");
    let (_, msg) = err(
        &live,
        "scene.patch",
        json!({"annotations": [
            {"id": "b3", "step": 1, "type": "box", "from_line": 1, "to_line": 2},
            {"id": "z3", "step": 1, "type": "Q7valueXyz789", "line": 1}
        ]}),
    );
    assert!(msg.contains(types), "{msg}");
    assert_clean("scene.patch", &msg);
}

#[test]
fn review2_r3_focus_set_refuses_lines_that_are_not_line_numbers() {
    let live = live_valued("kq48213977zx", "t167-rw2-3");
    live.api_ok("focus.set", json!({"from_line": 2, "to_line": 3}));
    for p in [
        json!({"from_line": -1, "to_line": 3}),
        json!({"from_line": 3, "to_line": -2}),
        json!({"to_line": 4821}),
        json!({"to_line": 3}),
        json!({"from_line": 1.5}),
        json!({"from_line": "2"}),
    ] {
        let (code, msg) = err(&live, "focus.set", p.clone());
        assert_eq!(code, "invalid_argument", "{p}: {msg}");
        for n in ["-1", "-2", "4821", "1.5"] {
            assert!(!msg.contains(n), "{p}: {msg}");
        }
        assert_eq!(
            live.with(|a| a.focus),
            Some((2, 3)),
            "{p} changed the focus"
        );
    }
    let mut mcp = McpClient::start(&live.socket);
    let (is_err, text, _) = mcp.call("set_focus", json!({"to_line": 4821}));
    assert!(is_err, "{text}");
    assert!(!text.contains("4821"), "{text}");
    // Both omitted (or null) still clear it.
    live.api_ok("focus.set", json!({"from_line": null}));
    assert_eq!(live.with(|a| a.focus), None);
}
