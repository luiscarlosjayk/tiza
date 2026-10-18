//! t-167 — Opening or rendering a .tiza whose move targets masked text
//! printed the secret in the load error; the same echo came back from the
//! socket, the MCP and supervised mode.
//!
//! Acceptance criteria (with the QA amendments of 10:33, 12:45, 13:00 and
//! 14:00):
//! - AC1 On a document with ANY redact, no error and no echo quotes the
//!   text of a target the agent or the document wrote — `find`, `after`,
//!   `before`, `find_start`/`find_end`, `select`, `rebind`, range markers,
//!   `at.find`, `from`/`to.find`, the step id of `step.goto` / `goto_step`
//!   / `tiza goto` — not only when it falls under the mask (the difference
//!   would be an oracle): it says «…», like `tiza check` and `move.add`
//!   (t-154). Outputs: the load error (TUI and `tiza render`), `scene.put`
//!   and `tiza scene put`, `scene.patch`, `annotation.add/update`,
//!   `pointer.set`, the MCP `highlight`/`add_box`/`update_annotation`/
//!   `point_at`/`goto_step`, and all of supervised mode: the review list,
//!   `accepted pN: …`, `pN accepted but failed: …`, `rejected pN: …`, the
//!   expiry at 60 s, the t-155 tail on acceptance and
//!   `supervise.status.pending[].summary` over the socket; there no `find`
//!   is quoted and `label`/`text` pass the copy rule of t-157 (a piece of
//!   4+ characters of a masked value is taken out).
//! - AC2 Every output above (and the raw bytes of the pty) is scanned for
//!   any piece of 4+ characters of the masked value and holds none.
//! - AC3 This file; CHANGELOG (Fixed).
//!
//! Corner cases: a target that only partly overlaps the mask is «…» too;
//! a document without redacts keeps quoting its targets as before.
//!
//! The QA script this rebuilds (pruebas/t167/qa_t167.py, on Luis's Mac)
//! writes one .tiza per kind of target, with code `token=abc123SECRETxyz789`
//! and a redact on 6..24.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_core::protocol::{Request, Response};
use tiza_qa::{Live, McpClient, fixture, scratch, tiza};

const VALUE: &str = "abc123SECRETxyz789";
const ELIDED: &str = "«…»";

fn code() -> Vec<String> {
    vec![
        "[api]".into(),
        format!("token={VALUE}"),
        "region=eu-west-1".into(),
        "port=8080".into(),
    ]
}

fn redact() -> Value {
    json!({"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 6, "end": 24}})
}

fn highlight() -> Value {
    json!({"id": "h1", "step": 1, "type": "highlight", "span": {"line": 3, "start": 0, "end": 6}})
}

/// Every piece of 4 characters of the value (the t-157 rule).
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

/// One move per kind of target (and per way of meeting the mask): every
/// one fails on the masked code, so each document fails to load.
fn cases() -> Vec<(&'static str, Value)> {
    vec![
        (
            "find-whole",
            patch(json!({"op": "replace", "find": VALUE, "content": "x"})),
        ),
        (
            "find-part",
            patch(json!({"op": "replace", "find": "SECRETxyz", "content": "x"})),
        ),
        // Corner case: only part of the target is under the mask.
        (
            "find-overlap",
            patch(json!({"op": "replace", "find": "token=abc123", "content": "x"})),
        ),
        (
            "find-nowhere",
            patch(json!({"op": "replace", "find": "NOWHERE123", "content": "x"})),
        ),
        (
            "after",
            patch(json!({"op": "insert", "after": VALUE, "content": "x"})),
        ),
        (
            "before",
            patch(json!({"op": "insert", "before": "123SECRETx", "content": "x"})),
        ),
        (
            "find-start",
            patch(
                json!({"op": "replace", "find_start": "SECRETxyz789", "find_end": "port", "content": "x"}),
            ),
        ),
        (
            "find-end",
            patch(
                json!({"op": "replace", "find_start": "region", "find_end": "SECRETxyz789", "content": "x"}),
            ),
        ),
        (
            "select",
            json!({"at_step": 2, "action": "select", "from": {"find": "SECRETxyz"}, "to": {"find": "region"}}),
        ),
        (
            "rebind",
            patch(
                json!({"op": "replace", "find": "region=eu-west-1", "content": "region=x",
                         "rebind": {"h1": {"at": {"find": "SECRETxyz789"}}}}),
            ),
        ),
    ]
}

fn document(redacted: bool, moves: Vec<Value>) -> Value {
    let mut anns = vec![highlight()];
    if redacted {
        anns.insert(0, redact());
    }
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

fn live(redacted: bool, name: &str) -> Live {
    let dir = scratch(name);
    let doc = tiza_core::fs::parse_document(&document(redacted, vec![]).to_string(), None).unwrap();
    Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"))
}

fn err(live: &Live, method: &str, params: Value) -> (String, String) {
    let r = live.api(method, params.clone());
    let e = r
        .error
        .unwrap_or_else(|| panic!("{method} {params} succeeded"));
    (e.code, e.message)
}

// ---------- AC1/AC2: the load error ----------

#[test]
fn ac1_render_and_check_of_the_card_repro_say_elided() {
    let f = fixture("t-167/w-redact.tiza");
    let f = f.to_str().unwrap();
    let e = stderr(&["render", f, "--out", "-"]);
    assert!(e.contains("find «…» (move #2, step 3)"), "{e}");
    assert_clean("tiza render", &e);
    let out = tiza(&["check", f]);
    let report =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(report.contains("find «…»"), "{report}");
    assert_clean("tiza check", &report);
}

#[test]
fn ac1_every_kind_of_target_is_elided_on_load_render_check_and_scene_put() {
    let dir = scratch("t167-load");
    let target = live(false, "t167-load-live");
    let sock = target.socket.to_str().unwrap().to_string();
    let mut nowhere = String::new();
    let mut whole = String::new();
    for (name, m) in cases() {
        let doc = document(true, vec![m]);
        let p = write(&dir, name, &doc);
        let ps = p.to_str().unwrap();
        let e = stderr(&["render", ps, "--out", "-"]);
        assert!(e.contains(ELIDED), "{name}: {e}");
        assert_clean(&format!("{name}: tiza render"), &e);
        let out = tiza(&["check", ps]);
        let report = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        assert_clean(&format!("{name}: tiza check"), &report);
        // `tiza scene put` reads the file before sending it.
        let e = stderr(&["--socket", &sock, "scene", "put", ps]);
        assert_clean(&format!("{name}: tiza scene put"), &e);
        // `scene.put` over the socket, on an instance whose own document
        // has no redact: the document it brings has one.
        let (code, msg) = err(&target, "scene.put", json!({"document": doc}));
        assert!(msg.contains(ELIDED), "{name}: {msg}");
        assert_clean(&format!("{name}: scene.put"), &msg);
        assert!(!code.is_empty());
        if name == "find-nowhere" {
            nowhere = e.replace(ps, "");
        }
        if name == "find-whole" {
            whole = e.replace(ps, "");
        }
    }
    // No oracle: a target under the mask and one that is nowhere read alike.
    assert_eq!(whole, nowhere);
}

#[test]
fn corner_without_redacts_load_errors_still_quote_the_target() {
    let dir = scratch("t167-plain-load");
    let m = patch(json!({"op": "replace", "find": "NOWHERE123", "content": "x"}));
    let p = write(&dir, "nowhere", &document(false, vec![m]));
    let e = stderr(&["render", p.to_str().unwrap(), "--out", "-"]);
    assert!(e.contains("find «NOWHERE123» (move #1, step 2)"), "{e}");
    let m = patch(
        json!({"op": "replace", "find_start": "region", "find_end": "SECRETxyz789", "content": "x"}),
    );
    let p = write(&dir, "order", &document(false, vec![m]));
    let e = stderr(&["render", p.to_str().unwrap(), "--out", "-"]);
    assert!(e.contains("find_end «SECRETxyz789»"), "{e}");
}

#[test]
fn ac2_tui_load_error_in_a_pty_raw_bytes() {
    let dir = scratch("t167-pty-load");
    let f = fixture("t-167/w-redact.tiza");
    let out = tiza_qa::pty_script(
        &dir,
        "load.py",
        r##"
binary, doc = sys.argv[1], sys.argv[2]
pieces = sys.argv[3].split(",")
s = Pty([binary, "--no-save", doc], 30, 100)
s.wait_for(lambda out: b"could not open" in out, 10, "load error")
s.finish(5)
raw = s.out
leaks = [p for p in pieces if p.encode() in raw]
print("RAW_LEAKS" if leaks else "RAW_CLEAN", leaks, flush=True)
print("PLAIN", plain(raw).replace("\n", " "))
"##,
        &[
            tiza_qa::bin().to_str().unwrap(),
            f.to_str().unwrap(),
            &pieces().join(","),
        ],
    );
    assert!(out.contains("RAW_CLEAN"), "{out}");
    assert!(out.contains("find «…» (move #2, step 3)"), "{out}");
}

// ---------- AC1/AC2: socket and MCP echoes ----------

#[test]
fn ac1_socket_errors_quote_no_target() {
    let live = live(true, "t167-socket");
    let finds = [
        VALUE,
        "SECRETxyz789",
        "123SECRETx",
        "token=abc123",
        "NOWHERE123",
    ];
    let mut seen: Vec<(String, String)> = Vec::new();
    for f in finds {
        let e = err(
            &live,
            "annotation.add",
            json!({"type": "highlight", "at": {"find": f}}),
        );
        assert_clean("annotation.add highlight", &e.1);
        seen.push(e);
    }
    // The masked targets and the absent one get the same answer.
    assert!(seen.windows(2).all(|w| w[0] == w[1]), "{seen:?}");
    assert_eq!(seen[0].1, "text \"…\" not found in the code");

    let cases = [
        (
            "annotation.add",
            json!({"type": "box", "find_start": "port", "find_end": VALUE}),
        ),
        (
            "annotation.add",
            json!({"type": "box", "find_start": "SECRETxyz", "find_end": "port"}),
        ),
        (
            "annotation.add",
            json!({"type": "arrow", "from": {"find": "SECRETxyz"}, "to": {"find": "port"}}),
        ),
        (
            "annotation.update",
            json!({"id": "h1", "move": {"at": {"find": "123SECRETx"}}}),
        ),
        ("pointer.set", json!({"find": VALUE})),
        ("pointer.set", json!({"line": 2, "find": "SECRETxyz"})),
        ("step.goto", json!({"id": VALUE})),
        ("step.goto", json!({"id": "SECRETxyz789"})),
    ];
    for (method, params) in cases {
        let (_, msg) = err(&live, method, params.clone());
        assert_clean(&format!("{method} {params}"), &msg);
    }
    // Moves added over the socket (t-154 already; every kind of target).
    for (name, m) in cases_for_add() {
        let (_, msg) = err(&live, "move.add", m);
        assert_clean(&format!("move.add {name}"), &msg);
    }
    // scene.patch: an annotation the parser refuses quotes the value it
    // read (`invalid type: string "…"`), and a redact brought by the patch
    // itself.
    let (_, msg) = err(
        &live,
        "scene.patch",
        json!({"annotations": [{"id": "b9", "step": 1, "type": "box", "from_line": VALUE, "to_line": 4}]}),
    );
    assert_clean("scene.patch", &msg);
    assert!(msg.contains(ELIDED), "{msg}");
    let plain = live_plain_patch();
    assert_clean("scene.patch bringing a redact", &plain);
}

fn cases_for_add() -> Vec<(&'static str, Value)> {
    cases()
        .into_iter()
        .map(|(n, mut m)| {
            m["at_step"] = json!(1);
            (n, m)
        })
        .collect()
}

/// On an instance without redacts, a `scene.patch` that brings a redact
/// and an annotation the parser refuses: the error elides what it quotes.
fn live_plain_patch() -> String {
    let live = live(false, "t167-patch-plain");
    let (_, msg) = err(
        &live,
        "scene.patch",
        json!({"annotations": [
            redact(),
            {"id": "b9", "step": 1, "type": "box", "from_line": "SECRETxyz", "to_line": 4}
        ]}),
    );
    assert!(msg.contains(ELIDED), "{msg}");
    msg
}

#[test]
fn corner_without_redacts_socket_errors_still_quote_the_target() {
    let live = live(false, "t167-socket-plain");
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

#[test]
fn ac1_mcp_tools_quote_no_target() {
    let live = live(true, "t167-mcp");
    let mut mcp = McpClient::start(&live.socket);
    let calls = [
        ("highlight", json!({"at": {"find": "SECRETxyz789"}})),
        ("highlight", json!({"at": {"find": "NOWHERE123"}})),
        ("add_box", json!({"find_start": "port", "find_end": VALUE})),
        (
            "update_annotation",
            json!({"id": "h1", "at": {"find": "123SECRETx"}}),
        ),
        ("point_at", json!({"find": VALUE})),
        ("goto_step", json!({"id": VALUE})),
    ];
    let mut texts = Vec::new();
    for (tool, args) in calls {
        let (is_err, text, _) = mcp.call(tool, args.clone());
        assert!(is_err, "{tool} {args}: {text}");
        assert_clean(&format!("MCP {tool}"), &text);
        texts.push(text);
    }
    assert_eq!(texts[0], texts[1], "no oracle between masked and absent");
    // `tiza goto` (the CLI) on the same instance.
    let e = stderr(&["--socket", live.socket.to_str().unwrap(), "goto", VALUE]);
    assert_clean("tiza goto", &e);
    assert!(e.contains(ELIDED), "{e}");
}

// ---------- AC1/AC2: supervised mode ----------

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
fn ac1_supervised_mode_quotes_no_target() {
    let live = live(true, "t167-supervised");
    live.command("supervise on");
    let t0 = Instant::now();
    // Pending: the review list (screen) and `supervise.status` over the socket.
    let failing = propose(
        &live,
        request(
            "annotation.add",
            json!({"type": "highlight", "at": {"find": VALUE}}),
        ),
        t0,
    );
    let summary = live.with(|a| a.proposals[0].summary.clone());
    assert_eq!(summary, "add highlight «…»");
    let status = live.api_ok("supervise.status", json!({}));
    assert_eq!(status["pending"][0]["summary"], json!("add highlight «…»"));
    assert_clean("supervise.status", &status.to_string());
    let screen = live.screen(120, 30);
    assert!(screen.contains("p1  add highlight «…»"), "{screen}");
    assert_clean("the review list", &screen);

    // Accepted but failed: the bar and the agent's error.
    let msg = live.with(|a| a.accept_proposal()).unwrap();
    assert_eq!(msg, "p1 accepted but failed: add highlight «…»");
    let reply = reply_text(&failing);
    assert_clean("the failed proposal's reply", &reply);

    // Accepted: a visible `find`, a label with the value in it, on a
    // later step (the t-155 tail on acceptance).
    let ok = propose(
        &live,
        request(
            "annotation.add",
            json!({"type": "highlight", "at": {"find": "region"}, "step": 2,
                   "label": format!("was {VALUE} before")}),
        ),
        t0,
    );
    let msg = live.with(|a| a.accept_proposal()).unwrap();
    assert!(
        msg.starts_with("accepted p2: add highlight «…» «was … before»"),
        "{msg}"
    );
    assert!(msg.contains("(step 2, press n)"), "the t-155 tail: {msg}");
    assert_clean("accepted", &msg);
    let reply = reply_text(&ok);
    assert!(reply.contains("\"result\""), "{reply}");

    // Rejected: a pointer, an arrow, a step id and a note, one by one.
    for params in [
        request("pointer.set", json!({"find": "SECRETxyz"})),
        request(
            "annotation.add",
            json!({"type": "arrow", "from": {"find": "123SECRETx"}, "to": {"find": "port"}}),
        ),
        request("step.goto", json!({"id": VALUE})),
        request(
            "annotation.add",
            json!({"type": "note", "line": 3, "text": "abc123SECRET is the key"}),
        ),
        request("annotation.update", json!({"id": "h1", "label": "xyz789"})),
    ] {
        let what = params.method.clone();
        let rx = propose(&live, params, t0);
        let s = live.with(|a| a.proposals[0].summary.clone());
        assert_clean(&format!("{what} summary"), &s);
        let msg = live.with(|a| a.reject_proposal()).unwrap();
        assert!(msg.starts_with("rejected p"), "{msg}");
        assert_clean(&format!("rejected {what}"), &msg);
        assert_clean("the rejection", &reply_text(&rx));
    }

    // Expired at 60 s, on an injected clock (no sleep).
    let rx = propose(
        &live,
        request(
            "annotation.add",
            json!({"type": "highlight", "at": {"find": "SECRETxyz789"}}),
        ),
        t0,
    );
    let pending = live.api_ok("supervise.status", json!({}));
    assert_clean("supervise.status", &pending.to_string());
    let expired = live.with(move |a| {
        let changed = a.expire_proposals_at(t0 + Duration::from_secs(61));
        (changed, a.message.clone())
    });
    assert!(expired.0, "the proposal expired");
    assert_clean("the expiry message", &format!("{:?}", expired.1));
    let reply = reply_text(&rx);
    assert!(reply.contains("expired"), "{reply}");
    assert_clean("the expiry reply", &reply);
}

#[test]
fn corner_without_redacts_supervised_summaries_quote_as_before() {
    let live = live(false, "t167-supervised-plain");
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

#[test]
fn ac2_supervised_mode_in_a_pty_raw_bytes() {
    let dir = scratch("t167-pty-supervised");
    let doc = write(&dir, "doc", &document(true, vec![]));
    let sock = dir.join("live.sock");
    let out = tiza_qa::pty_script(
        &dir,
        "supervised.py",
        r##"
import json
binary, doc, sock, value = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
pieces = sys.argv[5].split(",")
outputs = []
def api(method, params):
    r = subprocess.run([binary, "--socket", sock, "api", method, json.dumps(params)],
                       capture_output=True, text=True, timeout=10)
    outputs.append(r.stdout + r.stderr)
    return r
def park(method, params):
    return subprocess.Popen([binary, "--socket", sock, "api", method, json.dumps(params)],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
squeezed = lambda since: lambda out: plain(out[since:]).replace(" ", "")
s = Pty([binary, "--socket", sock, "--no-save", doc], 30, 120)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 10, "first frame"):
    api("supervise.set", {"enabled": True})
    mark = len(s.out)
    p = park("annotation.add", {"type": "highlight", "at": {"find": value}})
    if s.wait_for(lambda out: "Agentproposals" in squeezed(mark)(out), 10, "review list"):
        print("LISTED", flush=True)
    api("supervise.status", {})
    # The bar is redrawn by cell diffs, so its text is read in the Live
    # tests; here the raw bytes after each decision are what counts.
    s.send(b"y")
    o, e = p.communicate(timeout=10)
    outputs.append(o + e)
    if "not found" in e:
        print("FAILED_REPLY", flush=True)
    s.settle(0.3)
    mark = len(s.out)
    p = park("annotation.add", {"type": "note", "line": 3, "text": "was " + value})
    if s.wait_for(lambda out: "Agentproposals" in squeezed(mark)(out), 10, "review list 2"):
        print("LISTED_2", flush=True)
    s.send(b"n")
    o, e = p.communicate(timeout=10)
    outputs.append(o + e)
    if "rejected" in e:
        print("REJECTED_REPLY", flush=True)
    s.settle(0.3)
    api("supervise.set", {"enabled": False})
    api("pointer.set", {"find": value})
    r = subprocess.run([binary, "--socket", sock, "goto", value],
                       capture_output=True, text=True, timeout=10)
    outputs.append(r.stdout + r.stderr)
    s.settle(0.3)
s.kill()
s.finish(5)
raw = s.out
leaks = [p for p in pieces if p.encode() in raw]
print("RAW_LEAKS" if leaks else "RAW_CLEAN", leaks, flush=True)
leaks = [p for p in pieces if any(p in o for o in outputs)]
print("API_LEAKS" if leaks else "API_CLEAN", leaks, flush=True)
print("OUTPUTS", " | ".join(o.replace("\n", " ") for o in outputs))
"##,
        &[
            tiza_qa::bin().to_str().unwrap(),
            doc.to_str().unwrap(),
            sock.to_str().unwrap(),
            VALUE,
            &pieces().join(","),
        ],
    );
    for want in [
        "LISTED",
        "FAILED_REPLY",
        "LISTED_2",
        "REJECTED_REPLY",
        "RAW_CLEAN",
        "API_CLEAN",
    ] {
        assert!(out.contains(want), "{want}:\n{out}");
    }
}

// ---------- Review of 812ff18 (adversarial reviewer) ----------

#[test]
fn review1_a_step_id_with_blanks_is_not_quoted_either() {
    let live = live(true, "t167-r1");
    for id in [" SECRETxyz789", "SECRETxyz789\n", "\tSECRETxyz"] {
        let (_, msg) = err(&live, "step.goto", json!({"id": id}));
        assert_clean(&format!("step.goto {id:?}"), &msg);
    }
    let mut mcp = McpClient::start(&live.socket);
    let (is_err, text, _) = mcp.call("goto_step", json!({"id": " SECRETxyz789"}));
    assert!(is_err, "{text}");
    assert_clean("MCP goto_step", &text);
    let e = stderr(&[
        "--socket",
        live.socket.to_str().unwrap(),
        "goto",
        " SECRETxyz789",
    ]);
    assert_clean("tiza goto", &e);
    live.command("goto  SECRETxyz789");
    assert_clean(":goto", &format!("{:?}", live.message()));
}

#[test]
fn review2_mcp_parameters_that_do_not_deserialize_quote_no_value() {
    let live = live(true, "t167-r2");
    let mut mcp = McpClient::start(&live.socket);
    for (tool, args) in [
        ("highlight", json!({"at": "SECRETxyz789"})),
        ("add_box", json!({"from_line": "SECRETxyz789"})),
        ("point_at", json!({"line": "SECRETxyz789"})),
    ] {
        let (is_err, text, _) = mcp.call(tool, args.clone());
        assert!(is_err, "{tool} {args}: {text}");
        assert!(text.contains("failed to deserialize"), "{text}");
        assert_clean(&format!("MCP {tool} {args}"), &text);
    }
}

#[test]
fn review3_serde_errors_on_load_quote_no_value() {
    let dir = scratch("t167-r3");
    let target = live(false, "t167-r3-live");
    let sock = target.socket.to_str().unwrap().to_string();
    for (name, m) in [
        (
            "select-string",
            json!({"at_step": 2, "action": "select", "from": "SECRETxyz", "to": {"find": "port"}}),
        ),
        (
            "rebind-string",
            patch(
                json!({"op": "replace", "find": "region=eu-west-1", "content": "region=x",
                         "rebind": {"h1": {"at": "SECRETxyz789"}}}),
            ),
        ),
        (
            "unknown-op",
            patch(json!({"op": "SECRETxyz789", "find": "port", "content": "x"})),
        ),
    ] {
        let p = write(&dir, name, &document(true, vec![m]));
        let ps = p.to_str().unwrap();
        let e = stderr(&["render", ps, "--out", "-"]);
        assert_clean(&format!("{name}: render"), &e);
        let out = tiza(&["check", ps]);
        let report = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        assert!(report.contains("format"), "{name}: {report}");
        assert_clean(&format!("{name}: check"), &report);
        let e = stderr(&["--socket", &sock, "scene", "put", ps]);
        assert_clean(&format!("{name}: tiza scene put"), &e);
    }
    // The TUI in a pty: the raw bytes.
    let p = dir.join("select-string.tiza");
    let out = tiza_qa::pty_script(
        &dir,
        "load.py",
        r##"
binary, doc = sys.argv[1], sys.argv[2]
pieces = sys.argv[3].split(",")
s = Pty([binary, "--no-save", doc], 30, 100)
s.wait_for(lambda out: b"could not open" in out, 10, "load error")
s.finish(5)
leaks = [p for p in pieces if p.encode() in s.out]
print("RAW_LEAKS" if leaks else "RAW_CLEAN", leaks, plain(s.out).replace("\n", " "), flush=True)
"##,
        &[
            tiza_qa::bin().to_str().unwrap(),
            p.to_str().unwrap(),
            &pieces().join(","),
        ],
    );
    assert!(
        out.contains("RAW_CLEAN") && out.contains("could not open"),
        "{out}"
    );
    // Without a redact the parse error quotes as before.
    let m = json!({"at_step": 2, "action": "select", "from": "SECRETxyz", "to": {"find": "port"}});
    let p = write(&dir, "plain", &document(false, vec![m]));
    let e = stderr(&["render", p.to_str().unwrap(), "--out", "-"]);
    assert!(e.contains("SECRETxyz"), "{e}");
}

#[test]
fn review4_listed_markers_are_elided_on_a_document_with_redacts() {
    let live = live(true, "t167-r4");
    live.api_ok(
        "scene.patch",
        json!({"annotations": [{"id": "b1", "step": 1, "type": "box", "from_line": 2, "to_line": 3,
                                "markers": {"start": "SECRETxyz", "end": "port"}}]}),
    );
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(list.contains("@«…»..@«…»"), "{list}");
    assert_clean("scene.list", &list);
    let mut mcp = McpClient::start(&live.socket);
    let (_, text, _) = mcp.call("list_annotations", json!({}));
    assert_clean("MCP list_annotations", &text);
    live.command("list");
    assert_clean(":list", &live.screen(120, 30));
    live.with(|a| a.selected = Some("b1".into()));
    assert_clean(
        "the selection",
        &format!("{:?}", live.with(|a| a.selected_text())),
    );
    // Without redacts the markers are listed as before.
    let plain = live_plain_markers();
    assert!(plain.contains("@«SECRETxyz»..@«port»"), "{plain}");
}

fn live_plain_markers() -> String {
    let live = live(false, "t167-r4-plain");
    live.api_ok(
        "scene.patch",
        json!({"annotations": [{"id": "b1", "step": 1, "type": "box", "from_line": 2, "to_line": 3,
                                "markers": {"start": "SECRETxyz", "end": "port"}}]}),
    );
    live.api_ok("scene.list", json!({})).to_string()
}

#[test]
fn review5_supervised_type_and_method_are_not_echoed() {
    let live = live(true, "t167-r5");
    live.command("supervise on");
    let t0 = Instant::now();
    let a = propose(
        &live,
        request("annotation.add", json!({"type": VALUE, "at": {"line": 2}})),
        t0,
    );
    let b = propose(&live, request(VALUE, json!({})), t0);
    let c = propose(&live, request("SECRETxyz789", json!({})), t0);
    let s: Vec<String> = live.with(|a| a.proposals.iter().map(|p| p.summary.clone()).collect());
    for x in &s {
        assert_clean("summary", x);
    }
    assert_eq!(s[0], "add «…» L2");
    let status = live.api_ok("supervise.status", json!({}));
    assert_clean("supervise.status", &status.to_string());
    assert_clean("the review list", &live.screen(120, 30));
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

#[test]
fn review6_the_presenters_command_errors_quote_no_target() {
    let live = live(true, "t167-r6");
    for cmd in [
        "hl 2:SECRETxyz",
        "hl SECRETxyz789",
        "box @'SECRETxyz'..@'port'",
        "arrow 2:SECRETxyz 4:port",
        "arrow 3:123SECRETx 4:port",
        "goto SECRETxyz789",
        "spot 2:abc123SECRETxyz789",
    ] {
        live.command(cmd);
        let m = live.message();
        assert!(matches!(m, Some((_, true))), ":{cmd} → {m:?}");
        assert_clean(&format!(":{cmd}"), &format!("{m:?}"));
    }
    // Without redacts the bar quotes the target as before.
    let plain = live_plain_command();
    assert!(plain.contains("NOWHERE123"), "{plain}");
}

fn live_plain_command() -> String {
    let live = live(false, "t167-r6-plain");
    live.command("hl 2:NOWHERE123");
    format!("{:?}", live.message())
}

#[test]
fn review7_pieces_quoted_any_other_way_are_taken_out() {
    let live = live(true, "t167-r7");
    let cases = [
        (
            "annotation.add",
            json!({"type": "highlight", "at": {"line": 3}, "emphasis": "SECRETxyz789"}),
        ),
        ("scene.patch", json!({"SECRETxyz789": 1})),
        (
            "annotation.add",
            json!({"type": "highlight", "at": {"line": 3}, "color": "SECRETxyz789"}),
        ),
        (
            "move.add",
            patch(
                json!({"at_step": 1, "op": "replace", "find": "port=8080", "content": "port=1",
                         "rebind": {"SECRETxyz789": {"at": {"line": 4}}}}),
            ),
        ),
        ("SECRETxyz789", json!({})),
        (
            "scene.patch",
            json!({"annotations": [{"id": "SECRETxyz789", "deleted": true}]}),
        ),
        ("annotation.delete", json!({"id": "123SECRETx"})),
    ];
    for (method, params) in cases {
        let (_, msg) = err(&live, method, params.clone());
        assert_clean(&format!("{method} {params}"), &msg);
    }
    // The message's own words stay: a value that shares a piece with them
    // does not take them out.
    let live = Live::open(
        tiza_core::fs::parse_document(
            &json!({"schema": 1, "source": {"language": "ini", "first_line": 1},
                    "code": ["# config", "token=abc123secret", "port=8080"],
                    "annotations": [{"id": "r1", "step": 1, "type": "redact",
                                     "span": {"line": 2, "start": 6, "end": 18}}]})
            .to_string(),
            None,
        )
        .unwrap(),
        scratch("t167-r7-words").join("doc.tiza"),
        scratch("t167-r7-words-sock").join("live.sock"),
    );
    let (_, msg) = err(
        &live,
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "find": "sk_live_51Hx9QeLkT2mNpR8vWzYa"}),
    );
    assert!(msg.contains("looks like a secret"), "{msg}");
}

#[test]
fn ac3_changelog() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("(t-167)"));
}
