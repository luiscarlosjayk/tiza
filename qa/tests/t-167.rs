//! t-167 — Errors never repeat a closed value: a parameter with a fixed
//! set of names (a color, `border`, a type, a move's `op`, a method…) that
//! is none of them is answered with the valid names only, never with what
//! was sent — the MCP tools, the socket methods, the `:` bar and supervised
//! mode (R1); serde's list of valid names stays whole (R2); MCP arguments
//! that do not deserialize repeat no value; a move that matches nothing
//! reads like `tiza check` (R3); `focus.set` checks its lines.
//!
//! What an agent sent as a free text (a `find`, a step id) is quoted as it
//! wrote it.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_core::protocol::{Request, Response};
use tiza_qa::{Live, McpClient, scratch, tiza};

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

// ---------- what an agent or the document wrote is quoted ----------

#[test]
fn load_errors_quote_the_target() {
    let dir = scratch("t167-plain-load");
    let m = patch(json!({"op": "replace", "find": "NOWHERE123", "content": "x"}));
    let p = write(&dir, "nowhere", &document(vec![m]));
    let e = stderr(&["render", p.to_str().unwrap(), "--out", "-"]);
    assert!(
        e.contains("move #1, step 2: find «NOWHERE123» matches nothing in the code at this step"),
        "{e}"
    );
    let m = patch(
        json!({"op": "replace", "find_start": "region", "find_end": "Q7valueXyz789", "content": "x"}),
    );
    let p = write(&dir, "order", &document(vec![m]));
    let e = stderr(&["render", p.to_str().unwrap(), "--out", "-"]);
    assert!(e.contains("find_end «Q7valueXyz789»"), "{e}");
}

#[test]
fn socket_errors_quote_the_target() {
    let live = live("t167-socket-plain");
    let (_, msg) = err(
        &live,
        "annotation.add",
        json!({"type": "highlight", "at": {"find": "NOWHERE123"}}),
    );
    assert!(msg.contains("NOWHERE123"), "{msg}");
    let (_, msg) = err(&live, "pointer.set", json!({"find": "NOWHERE123"}));
    assert!(msg.contains("«NOWHERE123»"), "{msg}");
    let (_, msg) = err(&live, "step.goto", json!({"id": "nowhere"}));
    assert!(msg.contains("\"nowhere\""), "{msg}");
}

// ---------- supervised mode ----------

fn request(method: &str, params: Value) -> Request {
    Request {
        id: json!(1),
        method: method.into(),
        params,
    }
}

/// Parks `req` as a proposal at the clock `at`; the reply comes on the
/// returned channel once the presenter decides.
fn propose(live: &Live, req: Request, at: Instant) -> mpsc::Receiver<Response> {
    let (tx, rx) = mpsc::channel();
    live.with(move |a| a.handle_ipc_at(req, tx, at));
    rx
}

fn reply_text(rx: &mpsc::Receiver<Response>) -> String {
    let r = rx.recv_timeout(Duration::from_secs(10)).expect("a reply");
    serde_json::to_string(&r).unwrap()
}

#[test]
fn supervised_summaries_quote_the_target() {
    let live = live("t167-supervised-plain");
    live.command("supervise on");
    let t0 = Instant::now();
    let _rx = propose(
        &live,
        request(
            "annotation.add",
            json!({"type": "highlight", "at": {"find": VALUE}, "label": "the key"}),
        ),
        t0,
    );
    let _rx2 = propose(&live, request("step.goto", json!({"id": "intro"})), t0);
    let s: Vec<String> = live.with(|a| a.proposals.iter().map(|p| p.summary.clone()).collect());
    assert_eq!(s[0], format!("add highlight «{VALUE}» «the key»"));
    assert_eq!(s[1], "go to step intro");
    live.with(|a| a.reject_all_proposals());
}

// ---------- Review of 812ff18 (adversarial reviewer) ----------

#[test]
fn review2_mcp_parameters_that_do_not_deserialize_quote_no_value() {
    // Since t-189 an `invalid type` names the value sent (it is not a
    // closed value): `invalid type: string "…", expected u32`. Any other
    // message that does not deserialize still quotes nothing.
    let live = live("t167-r2");
    let mut mcp = McpClient::start(&live.socket);
    for (tool, args) in [
        ("highlight", json!({"at": "Q7valueXyz789"})),
        ("add_box", json!({"from_line": "Q7valueXyz789"})),
        ("point_at", json!({"line": "Q7valueXyz789"})),
    ] {
        let (is_err, text, _) = mcp.call(tool, args.clone());
        assert!(is_err, "{tool} {args}: {text}");
        assert!(text.contains("failed to deserialize"), "{text}");
        if text.contains("invalid type: ") {
            assert!(
                text.contains("string \"Q7valueXyz789\""),
                "MCP {tool} {args}: {text}"
            );
        } else {
            assert_clean(&format!("MCP {tool} {args}"), &text);
        }
    }
}

#[test]
fn review5_supervised_type_and_method_are_not_echoed() {
    let live = live("t167-r5");
    live.command("supervise on");
    let t0 = Instant::now();
    let a = propose(
        &live,
        request("annotation.add", json!({"type": VALUE, "at": {"line": 2}})),
        t0,
    );
    let b = propose(&live, request(VALUE, json!({})), t0);
    let c = propose(&live, request("Q7valueXyz789", json!({})), t0);
    let s: Vec<String> = live.with(|a| a.proposals.iter().map(|p| p.summary.clone()).collect());
    for x in &s {
        assert_clean("summary", x);
    }
    assert_eq!(s[0], "add «…» L2");
    let status = live.api_ok("supervise.status", json!({}));
    assert_clean("supervise.status", &status.to_string());
    // The code itself is on screen: the review list must not repeat it.
    let screen = live.screen(120, 30).replace(&format!("token={VALUE}"), "");
    assert_clean("the review list", &screen);
    let m1 = live.with(|a| a.accept_proposal()).unwrap();
    let m2 = live.with(|a| a.reject_proposal()).unwrap();
    let m3 = live.with(|a| a.accept_proposal()).unwrap();
    for (m, rx) in [(m1, a), (m2, b), (m3, c)] {
        assert_clean("decision", &m);
        assert_clean("reply", &reply_text(&rx));
    }
    // Known types and methods read as before.
    let _d = propose(&live, request("theme.set", json!({"theme": "light"})), t0);
    assert_eq!(live.with(|a| a.proposals[0].summary.clone()), "theme light");
    live.with(|a| a.reject_all_proposals());
}

// ---------- Rework 1 (QA 21:50, coordinator 20:15) ----------
//
// A closed value (a parameter with a fixed set of names) that is not one
// of them is answered with the valid names and never with what was sent,
// on every channel: the 30 MCP tools, the socket methods and the `:` bar.
// Two instances: the card's value, and a value made of lowercase words
// (`zebra-mango-quartz`, sending `mango-quartz`).

/// What is sent as a closed value: a plain canary, a piece of the card's
/// value and a lowercase piece of [`LOWER`].
const CANARY: &str = "Zq7Canary9x";
const LOWER: &str = "zebra-mango-quartz";
const LOWER_PIECE: &str = "mango-quartz";

/// Every MCP tool (the test fails when one is added, so its parameters get
/// classified below).
const TOOLS: [&str; 30] = [
    "add_arrow",
    "add_box",
    "add_callout",
    "add_mark",
    "add_move",
    "add_note",
    "add_spotlight",
    "check",
    "clear_annotations",
    "clear_pointer",
    "define_style",
    "delete_annotation",
    "delete_move",
    "export_image",
    "get_code",
    "goto_step",
    "highlight",
    "list_annotations",
    "list_kinds",
    "list_moves",
    "next_step",
    "point_at",
    "prev_step",
    "render_text",
    "set_focus",
    "set_follow",
    "set_step",
    "set_theme",
    "status",
    "update_annotation",
];

/// Parameters that take free text (a target, a label, an id, a path):
/// quoted as written.
const FREE: [&str; 21] = [
    "label",
    "text",
    "title",
    "id",
    "find",
    "find_start",
    "find_end",
    "after",
    "before",
    "content",
    "file",
    "path",
    "name",
    "glyph",
    "legend",
    "ids",
    "at",
    "from",
    "to",
    "rebind",
    "style",
];

/// (tool, closed parameter, a valid call it is put into).
fn mcp_closed(dir: &Path) -> Vec<(&'static str, &'static str, Value)> {
    let png = dir.join("x.txt").to_str().unwrap().to_string();
    let arrow = json!({"from": {"line": 1}, "to": {"line": 4}});
    let boxed = json!({"from_line": 1, "to_line": 4});
    let callout = json!({"from_line": 1, "to_line": 4, "text": "hi"});
    let mark = json!({"kind": "move", "at": {"line": 4}});
    let mv = json!({"at_step": 2, "op": "insert", "position": "4", "content": "x=1"});
    let hl = json!({"at": {"line": 4}});
    let mut out = Vec::new();
    for p in ["color", "stroke", "route", "head_start", "head_end"] {
        out.push(("add_arrow", p, arrow.clone()));
    }
    for p in ["color", "border", "corners"] {
        out.push(("add_box", p, boxed.clone()));
    }
    for p in ["color", "border", "corners", "placement"] {
        out.push(("add_callout", p, callout.clone()));
    }
    for p in ["color", "emphasis", "kind"] {
        out.push(("add_mark", p, mark.clone()));
    }
    for p in ["action", "op", "typing", "position", "lines"] {
        let mut base = mv.clone();
        if p == "lines" {
            base = json!({"at_step": 2, "action": "code.link", "file": "other.ini"});
        }
        out.push(("add_move", p, base));
    }
    out.push(("add_note", "color", json!({"line": 1, "text": "n"})));
    out.push((
        "define_style",
        "color",
        json!({"name": "st1", "glyph": "*"}),
    ));
    out.push(("export_image", "theme", json!({"path": png})));
    for p in ["color", "emphasis"] {
        out.push(("highlight", p, hl.clone()));
    }
    out.push(("set_theme", "theme", json!({})));
    for (id, ps) in [
        ("h1", &["color", "emphasis"][..]),
        ("b1", &["border", "corners"][..]),
        ("a1", &["stroke", "route", "head_start", "head_end"][..]),
    ] {
        for p in ps {
            out.push(("update_annotation", p, json!({"id": id})));
        }
    }
    out
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

/// The two instances and what each sends as a closed value.
fn closed_instances(tag: &str) -> Vec<(Live, Vec<&'static str>, &'static str)> {
    vec![
        (
            live_valued(VALUE, &format!("t167-{tag}-plain")),
            vec![CANARY, "Q7valueXyz789"],
            VALUE,
        ),
        (
            live_valued(LOWER, &format!("t167-{tag}-lower")),
            vec![LOWER_PIECE, CANARY],
            LOWER,
        ),
    ]
}

/// Neither `sent` nor any piece of 4 characters of it (nor of the
/// document's value) is in `msg`.
#[track_caller]
fn assert_no_echo(what: &str, msg: &str, sent: &str, value: &str) {
    for s in [sent, value] {
        let c: Vec<char> = s.chars().collect();
        for i in 0..=c.len().saturating_sub(4) {
            let p: String = c[i..(i + 4).min(c.len())].iter().collect();
            assert!(!msg.contains(&p), "{what} repeats «{p}» of «{s}»:\n{msg}");
        }
    }
}

#[test]
fn rework1_every_mcp_tool_answers_a_closed_value_without_repeating_it() {
    let dir = scratch("t167-rw1-mcp");
    for (live, sent, value) in closed_instances("rw1-mcp") {
        let mut mcp = McpClient::start(&live.socket);
        // Every tool and every string parameter is classified.
        let tools = mcp.tools();
        let tools = tools.as_array().expect("tools");
        let mut names: Vec<String> = tools
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        names.sort();
        assert_eq!(
            names,
            TOOLS.to_vec(),
            "the MCP tools changed: classify the new one"
        );
        let closed = mcp_closed(&dir);
        for t in tools {
            let name = t["name"].as_str().unwrap();
            let props = t["inputSchema"]["properties"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            for (p, schema) in &props {
                let stringy = schema.to_string().contains("\"string\"")
                    || schema.get("$ref").is_some()
                    || schema.get("anyOf").is_some();
                if !stringy {
                    continue;
                }
                let known = FREE.contains(&p.as_str())
                    || closed.iter().any(|(tl, cp, _)| *tl == name && cp == p)
                    || (name == "goto_step" && p == "id");
                assert!(known, "{name}.{p}: classify it as free text or closed");
            }
        }
        for (tool, param, base) in &closed {
            for v in &sent {
                let mut args = base.clone();
                args[*param] = json!(v);
                let (is_err, text, raw) = mcp.call(tool, args.clone());
                assert!(is_err, "{tool} {args}: accepted ({raw})");
                assert_no_echo(&format!("MCP {tool}.{param}"), &text, v, value);
                assert!(
                    text.contains("unknown") || text.contains("use ") || text.contains("expected"),
                    "{tool}.{param}: the valid names are not said: {text}"
                );
            }
        }
        // An unknown resource: the URI asked is not repeated either.
        let r = mcp.request(
            "resources/read",
            json!({"uri": format!("tiza://{}", sent[0])}),
        );
        assert_no_echo("resources/read", &r.to_string(), sent[0], value);
    }
}

/// (method, params with `V` where the closed value goes).
fn socket_closed() -> Vec<(&'static str, Value)> {
    let v = json!("V");
    vec![
        ("annotation.add", json!({"type": v, "at": {"line": 4}})),
        (
            "annotation.add",
            json!({"type": "highlight", "at": {"line": 4}, "color": v}),
        ),
        (
            "annotation.add",
            json!({"type": "highlight", "at": {"line": 4}, "emphasis": v}),
        ),
        (
            "annotation.add",
            json!({"type": "mark", "kind": v, "at": {"line": 4}}),
        ),
        (
            "annotation.add",
            json!({"type": "mark", "kind": "move", "at": {"line": 4}, "emphasis": v}),
        ),
        (
            "annotation.add",
            json!({"type": "arrow", "from": {"line": 1}, "to": {"line": 4}, "stroke": v}),
        ),
        (
            "annotation.add",
            json!({"type": "arrow", "from": {"line": 1}, "to": {"line": 4}, "route": v}),
        ),
        (
            "annotation.add",
            json!({"type": "arrow", "from": {"line": 1}, "to": {"line": 4}, "heads": {"start": v}}),
        ),
        (
            "annotation.add",
            json!({"type": "arrow", "from": {"line": 1}, "to": {"line": 4}, "heads": {"end": v}}),
        ),
        (
            "annotation.add",
            json!({"type": "box", "from_line": 1, "to_line": 4, "border": v}),
        ),
        (
            "annotation.add",
            json!({"type": "box", "from_line": 1, "to_line": 4, "corners": v}),
        ),
        (
            "annotation.add",
            json!({"type": "callout", "from_line": 1, "to_line": 4, "text": "x", "placement": v}),
        ),
        ("annotation.update", json!({"id": "h1", "color": v})),
        ("annotation.update", json!({"id": "h1", "emphasis": v})),
        ("annotation.update", json!({"id": "b1", "border": v})),
        ("annotation.update", json!({"id": "b1", "corners": v})),
        ("annotation.update", json!({"id": "a1", "stroke": v})),
        ("annotation.update", json!({"id": "a1", "route": v})),
        (
            "annotation.update",
            json!({"id": "a1", "heads": {"start": v}}),
        ),
        (
            "annotation.update",
            json!({"id": "a1", "heads": {"end": v}}),
        ),
        (
            "annotation.update",
            json!({"id": "h1", "shape": {"type": v, "at": {"line": 4}}}),
        ),
        ("scene.patch", json!({"theme": v})),
        (
            "scene.patch",
            json!({"annotations": [{"id": "n9", "step": 1, "type": v, "line": 1}]}),
        ),
        (
            "scene.patch",
            json!({"annotations": [{"id": "n9", "step": 1, "type": "box", "from_line": 1, "to_line": 4, "border": v}]}),
        ),
        ("move.add", json!({"at_step": 2, "action": v})),
        (
            "move.add",
            json!({"at_step": 2, "action": "code.patch", "op": v, "position": "4", "content": "x"}),
        ),
        (
            "move.add",
            json!({"at_step": 2, "action": "code.patch", "op": "insert", "position": "4", "content": "x", "typing": v}),
        ),
        (
            "move.add",
            json!({"at_step": 2, "action": "code.patch", "op": "insert", "position": v, "content": "x"}),
        ),
        (
            "move.add",
            json!({"at_step": 2, "action": "code.link", "file": "other.ini", "lines": v}),
        ),
        ("theme.set", json!({"theme": v})),
        (
            "style.set",
            json!({"name": "st1", "glyph": "*", "color": v}),
        ),
        ("V", json!({})),
    ]
}

/// `V` in `params` (and as the method) replaced by `sent`.
fn with_value(method: &str, params: &Value, sent: &str) -> (String, Value) {
    let p: Value = serde_json::from_str(
        &params
            .to_string()
            .replace("\"V\"", &json!(sent).to_string()),
    )
    .unwrap();
    (method.replace('V', sent), p)
}

#[test]
fn rework1_socket_methods_answer_a_closed_value_without_repeating_it() {
    for (live, sent, value) in closed_instances("rw1-socket") {
        for (method, params) in socket_closed() {
            for s in &sent {
                let (m, p) = with_value(method, &params, s);
                let (_, msg) = err(&live, &m, p.clone());
                assert_no_echo(&format!("{m} {params}"), &msg, s, value);
            }
        }
        // scene.put of a document with a closed value that is not one.
        let mut doc = live.api_ok("scene.get", json!({}))["document"].clone();
        for s in &sent {
            doc["theme"] = json!(s);
            let (_, msg) = err(&live, "scene.put", json!({"document": doc.clone()}));
            assert_no_echo("scene.put theme", &msg, s, value);
            assert!(msg.contains("`dark`, `light`"), "{msg}");
        }
    }
}

#[test]
fn rework1_the_bar_answers_a_closed_value_without_repeating_it() {
    for (live, sent, value) in closed_instances("rw1-bar") {
        for s in &sent {
            for cmd in [
                format!("theme {s}"),
                format!("hl 4 emphasis={s}"),
                format!("box 1-4 border={s}"),
                format!("box 1-4 corners={s}"),
                format!("box 1-4 fill={s}"),
                format!("arrow 1 4 stroke={s}"),
                format!("arrow 1 4 route={s}"),
                format!("arrow 1 4 start={s}"),
                format!("arrow 1 4 end={s}"),
                format!("style st1 glyph=* color={s}"),
                format!("mark {s} 4"),
                format!("hl 4 {s}=1"),
                format!("follow {s}"),
                s.to_string(),
            ] {
                live.command(&cmd);
                let m = format!("{:?}", live.message());
                assert!(m.contains("true"), ":{cmd} was not an error: {m}");
                assert_no_echo(&format!(":{cmd}"), &m, s, value);
            }
        }
    }
}

// ---------- Rework 2: serde's list of valid names stays whole ----------

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

// ---------- Rework 3: the load error in the words of `tiza check` ----------

#[test]
fn rework3_a_move_that_matches_nothing_reads_like_tiza_check() {
    let dir = scratch("t167-rw3");
    let m = patch(json!({"op": "replace", "find": "NOWHERE123", "content": "x"}));
    let p = write(&dir, "nowhere", &document(vec![m]));
    let ps = p.to_str().unwrap();
    let want = format!(
        "could not open {ps}: move #1, step 2: find «NOWHERE123» matches nothing in the code at this step"
    );
    let e = stderr(&["render", ps, "--out", "-"]);
    assert!(e.contains(&want), "{e}");
    assert!(!e.contains("text \""), "{e}");
    let out = tiza(&["check", ps]);
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        report.contains(
            "move #1, step 2: find «NOWHERE123» matches nothing in the code at this step"
        ),
        "{report}"
    );
    // The socket's move.add, same words.
    let sock = live("t167-rw3-socket");
    let mut mv = patch(json!({"op": "replace", "find": "NOWHERE123", "content": "x"}));
    mv["at_step"] = json!(1);
    let (code, msg) = err(&sock, "move.add", mv);
    assert_eq!(code, "text_not_found");
    assert_eq!(
        msg,
        "move #1, step 1: find «NOWHERE123» matches nothing in the code at this step"
    );
    // A marker that matches nothing is a phrase too, not a quoted text.
    let live = live("t167-rw3-marker");
    let (_, msg) = err(
        &live,
        "annotation.add",
        json!({"type": "box", "find_start": "NOWHERE123", "find_end": "port"}),
    );
    assert_eq!(msg, "start marker «NOWHERE123» matches nothing in the code");
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

#[test]
fn ac3_changelog() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("(t-167)"));
}
