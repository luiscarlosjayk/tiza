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
//! Rework 1 (QA 21:50 on 08d0da6, with t-174's rework point 2):
//! - R1 A closed value that is none of its names (a color, `border`,
//!   `corners`, `stroke`, `route`, a head, `emphasis`, `placement`, a mark
//!   `kind`, `theme`, a move's `action`/`op`/`typing`/`position`/`lines`,
//!   a method or command name) is answered with the valid names only, never
//!   with what was sent, redact or not: the 31 MCP tools, the socket
//!   methods and the `:` bar, also for a lowercase piece (`mango-quartz`)
//!   of a masked lowercase value (`zebra-mango-quartz`).
//! - R2 serde's list of valid names (`expected one of `highlight`, `box`,
//!   …`) is not filtered; only the value received is.
//! - R3 A target that matches nothing reads like `tiza check`: `move #2,
//!   step 3: find «…» matches nothing in the code at this step` (quoted
//!   text without redacts), on load and from `move.add`.
//! - Decisions: `annotation … does not exist` stays «…» on a document with
//!   redacts (quoting ids that share no piece with the value would be the
//!   id oracle of t-179); `:list` elides move targets like markers.
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
/// The load error of the card's repro, in the words of `tiza check`
/// (rework, point 3).
const MOVE2_ELIDED: &str = "move #2, step 3: find «…» matches nothing in the code at this step";

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
    assert!(e.contains(MOVE2_ELIDED), "{e}");
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
    assert!(
        e.contains("move #1, step 2: find «NOWHERE123» matches nothing in the code at this step"),
        "{e}"
    );
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
    assert!(out.contains(MOVE2_ELIDED), "{out}");
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

// ---------- Rework 1 (QA 21:50, coordinator 20:15; t-174 rework point 2) ----------
//
// A closed value (a parameter with a fixed set of names) that is not one
// of them is answered with the valid names and never with what was sent,
// redact or not, on every channel: the 31 MCP tools, the socket methods
// and the `:` bar. Three instances: no redact, the card's redact, and a
// masked value made of lowercase words (`zebra-mango-quartz`, sending
// `mango-quartz`: the message's lowercase-word exception let it through).

/// What is sent as a closed value: a plain canary, a piece of the card's
/// value and a lowercase piece of [`LOWER`].
const CANARY: &str = "Zq7Canary9x";
const LOWER: &str = "zebra-mango-quartz";
const LOWER_PIECE: &str = "mango-quartz";

/// Every MCP tool (the test fails when one is added, so its parameters get
/// classified below).
const TOOLS: [&str; 31] = [
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
    "redact",
    "render_text",
    "set_focus",
    "set_follow",
    "set_step",
    "set_theme",
    "status",
    "update_annotation",
];

/// Parameters that take free text (a target, a label, an id, a path):
/// their echo is the subject of AC1 (targets) and t-174 (labels), and they
/// are swept with values by t-175's canary.
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

/// A document whose line 2 is `token=<value>`, masked (or not), with a
/// highlight h1, a box b1 and an arrow a1 to update.
fn live_valued(value: &str, redacted: bool, name: &str) -> Live {
    let dir = scratch(name);
    let mut anns = vec![
        highlight(),
        json!({"id": "b1", "step": 1, "type": "box", "from_line": 1, "to_line": 4}),
        json!({"id": "a1", "step": 1, "type": "arrow", "from": {"line": 1, "col": 0}, "to": {"line": 4, "col": 0}}),
    ];
    if redacted {
        let end = 6 + value.chars().count();
        anns.insert(
            0,
            json!({"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 6, "end": end}}),
        );
    }
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

/// The three instances and what each sends as a closed value.
fn closed_instances(tag: &str) -> Vec<(Live, Vec<&'static str>, &'static str)> {
    vec![
        (
            live_valued(VALUE, false, &format!("t167-{tag}-plain")),
            vec![CANARY],
            VALUE,
        ),
        (
            live_valued(VALUE, true, &format!("t167-{tag}-redact")),
            vec![CANARY, "SECRETxyz789"],
            VALUE,
        ),
        (
            live_valued(LOWER, true, &format!("t167-{tag}-lower")),
            vec![LOWER_PIECE, CANARY],
            LOWER,
        ),
    ]
}

/// Neither `sent` nor any piece of 4 characters of it (nor of the masked
/// value) is in `msg`.
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
    let target = live(false, "t167-rw2-live");
    let sock = target.socket.to_str().unwrap().to_string();
    let types = "expected one of `highlight`, `mark`, `box`, `arrow`, `note`, `callout`, `redact`, `spotlight`";
    let actions = "expected one of `code.patch`, `code.link`, `cursor`, `select`, `wait`";
    let ops = "expected one of `insert`, `replace`, `delete`";
    // The document says `box`, `note` and `redact` itself: they stay.
    let extra = vec![
        json!({"id": "b2", "step": 1, "type": "box", "from_line": 1, "to_line": 2}),
        json!({"id": "n2", "step": 1, "type": "note", "line": 1, "text": "x"}),
    ];
    for (name, ann, mv, want) in [
        (
            "type",
            Some(json!({"id": "z1", "step": 1, "type": "SECRETxyz789", "line": 1})),
            None,
            types,
        ),
        (
            "action",
            None,
            Some(json!({"at_step": 2, "action": "SECRETxyz789"})),
            actions,
        ),
        (
            "op",
            None,
            Some(patch(
                json!({"op": "SECRETxyz789", "find": "port", "content": "x"}),
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
        let mut doc = document(true, mv.into_iter().collect());
        let anns = doc["annotations"].as_array_mut().unwrap();
        anns.extend(extra.clone());
        anns.extend(ann);
        let p = write(&dir, name, &doc);
        let ps = p.to_str().unwrap();
        let e = stderr(&["render", ps, "--out", "-"]);
        assert!(e.contains(want), "{name}: render: {e}");
        assert!(e.contains("unknown variant «…»"), "{name}: render: {e}");
        assert_clean(&format!("{name}: render"), &e);
        let out = tiza(&["check", ps]);
        let report = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        assert!(report.contains(want), "{name}: check: {report}");
        let e = stderr(&["--socket", &sock, "scene", "put", ps]);
        assert!(e.contains(want), "{name}: tiza scene put: {e}");
        let (_, msg) = err(&target, "scene.put", json!({"document": doc}));
        assert!(msg.contains(want), "{name}: scene.put: {msg}");
    }
    // Over the socket, on an instance with redacts: the patch says `box`.
    let live = live(true, "t167-rw2-socket");
    let (_, msg) = err(
        &live,
        "scene.patch",
        json!({"annotations": [
            {"id": "b3", "step": 1, "type": "box", "from_line": 1, "to_line": 2},
            {"id": "z3", "step": 1, "type": "SECRETxyz789", "line": 1}
        ]}),
    );
    assert!(msg.contains(types), "{msg}");
    assert_clean("scene.patch", &msg);
}

// ---------- Rework 3: the load error in the words of `tiza check` ----------

#[test]
fn rework3_a_move_that_matches_nothing_reads_like_tiza_check() {
    let dir = scratch("t167-rw3");
    for (redacted, find, quoted) in [(true, VALUE, ELIDED), (false, "NOWHERE123", "«NOWHERE123»")]
    {
        let m = patch(json!({"op": "replace", "find": find, "content": "x"}));
        let p = write(&dir, &format!("r{redacted}"), &document(redacted, vec![m]));
        let ps = p.to_str().unwrap();
        let want = format!(
            "could not open {ps}: move #1, step 2: find {quoted} matches nothing in the code at this step"
        );
        let e = stderr(&["render", ps, "--out", "-"]);
        assert!(e.contains(&want), "{e}");
        assert!(!e.contains("text \""), "{e}");
        let out = tiza(&["check", ps]);
        let report = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            report.contains(&format!(
                "move #1, step 2: find {quoted} matches nothing in the code at this step"
            )),
            "{report}"
        );
        // The socket's move.add, same words.
        let live = live(redacted, &format!("t167-rw3-{redacted}"));
        let mut mv = patch(json!({"op": "replace", "find": find, "content": "x"}));
        mv["at_step"] = json!(1);
        let (code, msg) = err(&live, "move.add", mv);
        assert_eq!(code, "text_not_found");
        assert_eq!(
            msg,
            format!("move #1, step 1: find {quoted} matches nothing in the code at this step")
        );
    }
    // A marker that matches nothing is a phrase too, not a quoted text.
    let live = live(false, "t167-rw3-marker");
    let (_, msg) = err(
        &live,
        "annotation.add",
        json!({"type": "box", "find_start": "NOWHERE123", "find_end": "port"}),
    );
    assert_eq!(msg, "start marker «NOWHERE123» matches nothing in the code");
}

// ---------- Rework, optional: `:list` and the id of a missing annotation ----------

#[test]
fn rework_optional_list_elides_move_targets_like_markers() {
    let live = live(true, "t167-rwo");
    live.api_ok(
        "move.add",
        patch(json!({"at_step": 2, "op": "replace", "find": "region=eu-west-1", "content": "region=x"})),
    );
    live.command("list");
    let screen = live.screen(120, 30);
    assert!(screen.contains("code.patch replace «…»"), "{screen}");
    let row = screen.lines().find(|l| l.contains("code.patch")).unwrap();
    assert!(!row.contains("region"), "{row}");
    let moves = live.api_ok("move.list", json!({})).to_string();
    assert!(moves.contains("replace «…»"), "{moves}");
    // Decision: a missing id stays «…» on a document with redacts —
    // quoting it unless it shares a piece with the value would answer
    // which pieces do (the id oracle t-179 tracks).
    let (code, msg) = err(
        &live,
        "annotation.update",
        json!({"id": "zz9", "color": "red"}),
    );
    assert_eq!(code, "not_found");
    assert!(!msg.contains("zz9"), "{msg}");
}

// ---------- Review of 4a492db (coordinator, adversarial) ----------
//
// Each test answers «a piece of the masked value» and «text that is not
// one» with the same code and the same message: anything else is an
// oracle. Every one fails on 4a492db.

/// The value of the numeric review (R3): digits an agent can send.
const DIGITS: &str = "kq48213977zx";

/// `(code, message)` of a call that must fail.
fn failure(live: &Live, method: &str, params: Value) -> (String, String) {
    err(live, method, params)
}

/// The same call with `piece` and with `other` in it (by `{}`) answers
/// alike, and neither is in the answer.
#[track_caller]
fn alike(live: &Live, method: &str, template: &Value, piece: &str, other: &str, value: &str) {
    let mut out = Vec::new();
    for x in [piece, other] {
        let text = template.to_string().replace("{}", x);
        let params: Value = serde_json::from_str(&text).unwrap();
        let r = live.api(method, params.clone());
        let e = r
            .error
            .unwrap_or_else(|| panic!("{method} {params} succeeded"));
        assert_no_echo(&format!("{method} {params}"), &e.message, x, value);
        out.push((e.code, e.message));
    }
    assert_eq!(
        out[0], out[1],
        "{method} {template}: «{piece}» and «{other}» answer differently"
    );
}

/// [`alike`] through the MCP.
#[track_caller]
fn alike_mcp(
    mcp: &mut McpClient,
    tool: &str,
    template: &Value,
    piece: &str,
    other: &str,
    value: &str,
) {
    let mut out = Vec::new();
    for x in [piece, other] {
        let args: Value = serde_json::from_str(&template.to_string().replace("{}", x)).unwrap();
        let (is_err, text, raw) = mcp.call(tool, args.clone());
        assert!(is_err, "{tool} {args}: accepted ({raw})");
        assert_no_echo(&format!("MCP {tool} {args}"), &text, x, value);
        out.push(text);
    }
    assert_eq!(
        out[0], out[1],
        "MCP {tool} {template}: «{piece}» and «{other}» answer differently"
    );
}

#[test]
fn review_r1_the_end_marker_is_searched_under_the_mask() {
    let live = live_valued(VALUE, true, "t167-rv1");
    for shape in [
        json!({"type": "box", "find_start": "[api]", "find_end": "\"{}\""}),
        json!({"type": "spotlight", "find_start": "[api]", "find_end": "\"{}\""}),
        json!({"type": "callout", "text": "c", "find_start": "[api]", "find_end": "\"{}\""}),
    ] {
        // `"{}"` stands for a JSON string: the template is text.
        let t: Value =
            serde_json::from_str(&shape.to_string().replace("\"\\\"{}\\\"\"", "\"{}\"")).unwrap();
        alike(&live, "annotation.add", &t, "SECRETxyz", "NOTHEREzz", VALUE);
        // The same end on the same line as the start: `token=` then the value.
        let mut same = t.clone();
        same["find_start"] = json!("token=");
        alike(
            &live,
            "annotation.add",
            &same,
            "SECRETxyz",
            "NOTHEREzz",
            VALUE,
        );
    }
    // Boxes whose markers come in a scene.patch: `check` reads them alike.
    live.api_ok(
        "scene.patch",
        json!({"annotations": [
            {"id": "m1", "step": 1, "type": "box", "from_line": 1, "to_line": 2,
             "markers": {"start": "[api]", "end": "SECRETxyz"}},
            {"id": "m2", "step": 1, "type": "box", "from_line": 1, "to_line": 2,
             "markers": {"start": "[api]", "end": "NOTHEREzz"}}
        ]}),
    );
    let report = live.api_ok("check", json!({}));
    let said = |id: &str| -> Vec<String> {
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["id"] == id)
            .map(|f| f["message"].as_str().unwrap().to_string())
            .collect()
    };
    assert!(!said("m2").is_empty(), "{report}");
    assert_eq!(said("m1"), said("m2"), "{report}");
    assert_clean("check", &report.to_string());
    live.api_ok(
        "scene.patch",
        json!({"annotations": [{"id": "m1", "deleted": true}, {"id": "m2", "deleted": true}]}),
    );
    // And the `:` bar's markers.
    let mut bar = Vec::new();
    for x in ["SECRETxyz", "NOTHEREzz"] {
        live.command(&format!("box @'[api]'..@'{x}'"));
        let m = format!("{:?}", live.message());
        assert_no_echo(":box", &m, x, VALUE);
        bar.push(m);
    }
    assert_eq!(bar[0], bar[1]);
    let mut mcp = McpClient::start(&live.socket);
    for tool in ["add_box", "add_spotlight"] {
        alike_mcp(
            &mut mcp,
            tool,
            &json!({"find_start": "[api]", "find_end": "{}"}),
            "SECRETxyz",
            "NOTHEREzz",
            VALUE,
        );
    }
    alike_mcp(
        &mut mcp,
        "add_callout",
        &json!({"find_start": "[api]", "find_end": "{}", "text": "c"}),
        "SECRETxyz",
        "NOTHEREzz",
        VALUE,
    );
    // Text outside the mask is still found: the end marker works.
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "box", "find_start": "[api]", "find_end": "port"}),
    );
    assert!(r["id"].is_string(), "{r}");
    // Without redacts the end marker finds that text as before.
    let plain = live_valued(VALUE, false, "t167-rv1-plain");
    plain.api_ok(
        "annotation.add",
        json!({"type": "box", "find_start": "[api]", "find_end": "SECRETxyz"}),
    );
}

#[test]
fn review_r2_object_keys_are_written_text() {
    for (live, piece, other, value) in [
        (
            live_valued(VALUE, true, "t167-rv2"),
            "123SECRETx",
            "999NOTHINx",
            VALUE,
        ),
        (
            live_valued(LOWER, true, "t167-rv2-lower"),
            LOWER_PIECE,
            "papaya-kiwi",
            LOWER,
        ),
    ] {
        for (method, template) in [
            ("pointer.set", json!({"{}": 1})),
            ("scene.get", json!({"{}": 1})),
            ("scene.patch", json!({"{}": 1})),
            ("annotation.update", json!({"id": "h1", "move": {"{}": 1}})),
            (
                "move.add",
                patch(
                    json!({"at_step": 1, "op": "replace", "find": "port=8080", "content": "port=1",
                             "rebind": {"{}": {"at": {"line": 4}}}}),
                ),
            ),
        ] {
            alike(&live, method, &template, piece, other, value);
        }
        let mut mcp = McpClient::start(&live.socket);
        alike_mcp(
            &mut mcp,
            "update_annotation",
            &json!({"id": "h1", "{}": 1}),
            piece,
            other,
            value,
        );
        alike_mcp(
            &mut mcp,
            "update_annotation",
            &json!({"id": "h1", "{}": 1}),
            VALUE,
            other,
            value,
        );
    }
}

#[test]
fn review_r3_numbers_the_agent_sent_are_written_text() {
    let live = live_valued(DIGITS, true, "t167-rv3");
    for (piece, other) in [("4821", "5555"), ("48213977", "55555555")] {
        for (method, template) in [
            ("move.delete", json!({"index": "N"})),
            ("step.goto", json!({"id": "N"})),
            (
                "annotation.add",
                json!({"type": "note", "line": "N", "text": "x"}),
            ),
            (
                "annotation.add",
                json!({"type": "highlight", "at": {"line": "N"}}),
            ),
            (
                "annotation.add",
                json!({"type": "box", "from_line": 1, "to_line": "N"}),
            ),
            (
                "annotation.add",
                json!({"type": "highlight", "at": {"line": 2, "col": "N"}}),
            ),
            ("pointer.set", json!({"line": "N"})),
            ("pointer.set", json!({"from": 1, "to": "N"})),
            ("focus.set", json!({"from_line": "N", "to_line": "N"})),
            ("render.text", json!({"width": "N"})),
            ("scene.patch", json!({"margin": "N"})),
        ] {
            // `"N"` is a JSON number in the request.
            let t = template.to_string().replace("\"N\"", "{}");
            let mut out = Vec::new();
            for x in [piece, other] {
                let params: Value = serde_json::from_str(&t.replace("{}", x)).unwrap();
                let (code, msg) = failure(&live, method, params.clone());
                assert_no_echo(&format!("{method} {params}"), &msg, x, DIGITS);
                out.push((code, msg));
            }
            assert_eq!(
                out[0], out[1],
                "{method} {t}: {piece} and {other} answer differently"
            );
        }
        let mut mcp = McpClient::start(&live.socket);
        for (tool, template) in [
            ("delete_move", "{\"index\": N}"),
            ("goto_step", "{\"id\": N}"),
            ("add_note", "{\"line\": N, \"text\": \"x\"}"),
            ("add_box", "{\"from_line\": 1, \"to_line\": N}"),
            ("point_at", "{\"line\": N}"),
            ("set_focus", "{\"from_line\": N, \"to_line\": N}"),
            ("highlight", "{\"at\": {\"line\": N}}"),
        ] {
            let mut out = Vec::new();
            for x in [piece, other] {
                let args: Value = serde_json::from_str(&template.replace('N', x)).unwrap();
                let (is_err, text, raw) = mcp.call(tool, args.clone());
                assert!(is_err, "{tool} {args}: accepted ({raw})");
                assert_no_echo(&format!("MCP {tool} {args}"), &text, x, DIGITS);
                out.push(text);
            }
            assert_eq!(
                out[0], out[1],
                "MCP {tool}: {piece} and {other} answer differently"
            );
        }
        for cmd in ["goto N", "note N x", "focus N-N", "hl N"] {
            let mut out = Vec::new();
            for x in [piece, other] {
                live.command(&cmd.replace('N', x));
                let m = format!("{:?}", live.message());
                assert!(m.contains("true"), ":{cmd} was not an error: {m}");
                assert_no_echo(&format!(":{cmd}"), &m, x, DIGITS);
                out.push(m);
            }
            assert_eq!(
                out[0], out[1],
                ":{cmd}: {piece} and {other} answer differently"
            );
        }
    }
    // Without redacts the numbers are said as before.
    let plain = live_valued(DIGITS, false, "t167-rv3-plain");
    let (_, msg) = err(&plain, "move.delete", json!({"index": 5555}));
    assert_eq!(msg, "move #5555 does not exist");
    let (code, msg) = err(
        &plain,
        "annotation.add",
        json!({"type": "note", "line": 55555555, "text": "x"}),
    );
    assert_eq!(code, "out_of_range");
    assert_eq!(
        msg,
        "line 55555555 does not exist (the document spans 1 to 4)"
    );
    // focus.set clamps the end like `:focus` and says no number it was sent.
    let r = plain.api_ok("focus.set", json!({"from_line": 2, "to_line": 55555555}));
    assert_eq!(r["focus"], json!([2, 4]));
}

#[test]
fn review_r4_messages() {
    let live = live_valued(VALUE, true, "t167-rv4");
    // One move prefix: `move #N, step S: …`.
    let (_, msg) = err(
        &live,
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "replace", "position": "9", "content": "x"}),
    );
    assert!(msg.starts_with("move #1, step 2: "), "{msg}");
    let (_, msg) = err(
        &live,
        "move.add",
        patch(
            json!({"op": "replace", "find": "port=8080", "content": "port=1",
                     "rebind": {"zz1": {"at": {"line": 4}}}}),
        ),
    );
    assert!(
        msg.starts_with("move #1, step 2: rebind names unknown annotation"),
        "{msg}"
    );
    // A move that does not exist is not an annotation.
    let (code, msg) = err(&live, "move.delete", json!({"index": 3}));
    assert_eq!(
        (code.as_str(), msg.as_str()),
        ("not_found", "move #3 does not exist")
    );
    // A mark's inline style color is checked, without echo.
    for c in [CANARY, "SECRETxyz789"] {
        let (code, msg) = err(
            &live,
            "annotation.add",
            json!({"type": "mark", "kind": "move", "at": {"line": 4}, "style": {"color": c}}),
        );
        assert_eq!(code, "invalid_argument");
        assert_no_echo("mark style.color", &msg, c, VALUE);
        assert!(msg.contains("unknown color"), "{msg}");
        let mut mcp = McpClient::start(&live.socket);
        let (is_err, text, _) = mcp.call(
            "add_mark",
            json!({"kind": "move", "at": {"line": 4}, "style": {"color": c}}),
        );
        assert!(is_err, "{text}");
        assert_no_echo("MCP add_mark style.color", &text, c, VALUE);
    }
    // `:export` says which option is wrong, without its value.
    let dir = scratch("t167-rv4-export");
    let out = dir.join("x.txt");
    for (opt, word) in [("step", "step="), ("width", "width="), ("theme", "theme=")] {
        live.command(&format!("export {} {opt}=SECRETxyz789", out.display()));
        let m = format!("{:?}", live.message());
        assert!(m.contains(word), ":export {opt}=: {m}");
        assert_clean(":export", &m);
    }
    live.command(&format!("export {} zzSECRETxyz=1", out.display()));
    let m = format!("{:?}", live.message());
    assert!(m.contains("unknown option (step, width, theme)"), "{m}");
    assert_clean(":export", &m);
}

#[test]
fn review_r5_every_mcp_tool_and_socket_method_with_an_unknown_key() {
    let live = live_valued(VALUE, true, "t167-rv5");
    let mut mcp = McpClient::start(&live.socket);
    for name in TOOLS {
        // What would take the redact away or change what is shown.
        if matches!(
            name,
            "clear_annotations" | "delete_annotation" | "set_follow"
        ) {
            continue;
        }
        let mut out = Vec::new();
        for k in ["123SECRETx", "999NOTHINx"] {
            let (is_err, text, _) = mcp.call(name, json!({ k: 1 }));
            assert_no_echo(&format!("MCP {name} {{{k}}}"), &text, k, VALUE);
            // A call that goes through answers with the scene: compare
            // only that it went through.
            out.push(if is_err { text } else { "ok".into() });
        }
        assert_eq!(
            out[0], out[1],
            "MCP {name}: an unknown key answers differently"
        );
    }
    let methods = live.api_ok("api.methods", json!({}));
    let methods: Vec<String> = methods["methods"]
        .as_array()
        .or_else(|| methods.as_array())
        .expect("api.methods")
        .iter()
        .map(|m| {
            m.as_str()
                .map(String::from)
                .unwrap_or_else(|| m["method"].as_str().unwrap().to_string())
        })
        .collect();
    assert!(methods.len() >= 30, "{methods:?}");
    for m in methods {
        if matches!(
            m.as_str(),
            "scene.clear" | "scene.put" | "supervise.set" | "follow.set"
        ) {
            continue;
        }
        let mut out = Vec::new();
        for k in ["123SECRETx", "999NOTHINx"] {
            let r = live.api(&m, json!({ k: 1 }));
            let e = r.error.map(|e| (e.code, e.message));
            let e = e.or(Some(("ok".into(), String::new())));
            if let Some((_, msg)) = &e {
                assert_no_echo(&format!("{m} {{{k}}}"), msg, k, VALUE);
            }
            out.push(e);
        }
        assert_eq!(out[0], out[1], "{m}: an unknown key answers differently");
    }
}

#[test]
fn ac3_changelog() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let changelog = std::fs::read_to_string(root.join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("(t-167)"));
}
