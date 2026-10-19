//! t-183 — Minimal redact (option 2): a mask the presenter owns, on the
//! file's own code, and nothing that tracks the value anywhere else.
//!
//! Acceptance criteria (PHASE 2):
//! - AC1 A redact is an annotation over columns of the BASE code; one place
//!   (layout) masks it in the TUI and every export (eight bullets).
//! - AC2 Presenter: `:redact` adds, `x` deletes, `u` undoes, `:redact off`
//!   unmasks only his screen.
//! - AC3 Agents (socket/MCP) may ADD redacts; they may not delete, narrow,
//!   move or retype one, change the text of a masked line or replace the
//!   code (only the masked copy from scene.get is accepted); the refusal is
//!   one constant message that never depends on the value.
//! - AC4 Moves (anyone) may not touch masked lines: refused with a clear
//!   message.
//! - AC5 Saving with embedded code writes the base code masked; a file
//!   reference keeps the reference.
//! - AC6 Follow: if a masked line changes on reload, the whole line is
//!   masked (fail closed).
//! - AC7 (in `t-167.rs`) closed values are never echoed in errors.
//! - AC8 README and agent guide say the mask covers the code only.
//!
//! Corner cases: documents saved by the version before t-183 still open (or
//! refuse clearly); a linked file (`code.link`) with its own redact follows
//! the same rules (decision D1).

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_core::fs;
use tiza_core::protocol::Response;
use tiza_core::redact::{AGENT_REFUSAL, MASKED_LINE};
use tiza_qa::{Live, McpClient, scratch, tiza, tiza_ok};

/// The value under the main mask, and two more on one line.
const VALUE: &str = "Zq7vK9wPx3Lm";
const LEFT: &str = "Hb4tR8yN";
const RIGHT: &str = "Mc2WdQ7x";
/// A value masked with `keep=4`: only `81Tq` may show.
const KEPT: &str = "Pk3nVw81Tq";

fn env_text() -> String {
    format!("# service config\nTOKEN={VALUE}\nPAIR=a:{LEFT} b:{RIGHT}\nNAME=demo\nKEY={KEPT}\n")
}

/// Every run of four characters of the values a mask hides (of `KEPT`, all
/// but its kept tail).
fn pieces() -> Vec<String> {
    let mut out = Vec::new();
    for v in [VALUE, LEFT, RIGHT, &KEPT[..KEPT.len() - 4]] {
        let c: Vec<char> = v.chars().collect();
        for w in c.windows(4) {
            out.push(w.iter().collect());
        }
    }
    out
}

fn assert_clean(what: &str, s: &str) {
    for p in pieces() {
        assert!(!s.contains(&p), "{what} shows `{p}`:\n{s}");
    }
}

/// [`assert_clean`] for a document that masks only `VALUE`.
fn assert_no_value(what: &str, s: &str) {
    let c: Vec<char> = VALUE.chars().collect();
    for w in c.windows(4) {
        let p: String = w.iter().collect();
        assert!(!s.contains(&p), "{what} shows `{p}`:\n{s}");
    }
}

/// `app.env` opened as a file reference, its sidecar next to it.
fn opened(name: &str) -> (Live, std::path::PathBuf, std::path::PathBuf) {
    let dir = scratch(name);
    let src = dir.join("app.env");
    std::fs::write(&src, env_text()).unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let sidecar = dir.join("app.env.tiza");
    let live = Live::open(doc, sidecar.clone(), dir.join("live.sock"));
    (live, src, sidecar)
}

/// The presenter's masks: `TOKEN=` onwards, the two values of `PAIR` by
/// columns, `KEY=` onwards with `keep=4`.
fn mask_all(live: &Live) {
    for cmd in [
        "redact 2:TOKEN=",
        "redact 3@7..15",
        "redact 3@18..26",
        "redact 5:KEY= keep=4",
    ] {
        live.command(cmd);
        let (msg, err) = live.message().unwrap();
        assert!(!err, "{cmd}: {msg}");
    }
}

fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or_default().to_string()
}

fn code_get(live: &Live) -> String {
    text(&live.api_ok("code.get", json!({})), "text")
}

fn render_text(live: &Live) -> String {
    text(&live.api_ok("render.text", json!({"width": 100})), "text")
}

/// Everything an agent or a recording reads from the live instance.
fn readers(live: &Live) -> Vec<(&'static str, String)> {
    vec![
        ("code.get", code_get(live)),
        ("render.text", render_text(live)),
        ("scene.get", live.api_ok("scene.get", json!({})).to_string()),
        (
            "scene.list",
            live.api_ok("scene.list", json!({})).to_string(),
        ),
        ("move.list", live.api_ok("move.list", json!({})).to_string()),
        ("status", live.api_ok("status", json!({})).to_string()),
        ("screen", live.screen(110, 20)),
    ]
}

fn assert_readers_clean(live: &Live, what: &str) {
    for (surface, s) in readers(live) {
        assert_clean(&format!("{what}: {surface}"), &s);
    }
}

/// The one refusal an agent gets (AC3): same code, same words, byte for byte.
fn refused(what: &str, r: Response) {
    let e = r
        .error
        .unwrap_or_else(|| panic!("{what}: accepted, it must be refused"));
    assert_eq!(e.code, "invalid_argument", "{what}: {}", e.message);
    assert_eq!(e.message, AGENT_REFUSAL, "{what}");
}

#[test]
fn ac1_one_place_masks_every_surface_and_every_export() {
    let (live, _src, sidecar) = opened("t183-ac1");
    // A step with something on it, so the document has steps 0 to 3.
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 4, "text": "the name", "step": 3}),
    );
    // Created at step 3: masks from step 0, on the file's own columns.
    live.api_ok("step.set", json!({"step": 3}));
    mask_all(&live);
    let text = render_text(&live);
    assert!(text.contains("TOKEN=••••••••"), "{text}");
    assert!(
        text.contains("PAIR=a:•••••••• b:••••••••"),
        "two masks on one line: {text}"
    );
    assert!(text.contains("KEY=••••••••81Tq"), "keep=4: {text}");
    assert!(text.contains("NAME=demo"), "untouched: {text}");
    for step in [3, 1, 0] {
        live.api_ok("step.set", json!({"step": step}));
        assert_readers_clean(&live, &format!("step {step}"));
        assert!(code_get(&live).contains("TOKEN=••••••••"), "step {step}");
    }
    // The MCP tools and resources read the same masked code.
    let mut mcp = McpClient::start(&live.socket);
    for tool in ["get_code", "list_annotations", "status", "render_text"] {
        let (err, out, _) = mcp.call(tool, json!({}));
        assert!(!err, "{tool}: {out}");
        assert_clean(&format!("MCP {tool}"), &out);
    }
    for uri in [
        "tiza://code",
        "tiza://scene",
        "tiza://render",
        "tiza://render.md",
    ] {
        let r = mcp.request("resources/read", json!({"uri": uri}));
        assert!(r["error"].is_null(), "{uri}: {r}");
        assert_clean(&format!("MCP {uri}"), &r.to_string());
    }
    // Every export of the saved document, drawn by the same layout.
    live.command("w");
    let dir = sidecar.parent().unwrap();
    for ext in ["txt", "md", "ansi", "svg", "cast", "tape"] {
        let out = dir.join(format!("x.{ext}"));
        tiza_ok(&[
            "render",
            sidecar.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--width",
            "100",
        ]);
        let s = std::fs::read_to_string(&out).unwrap();
        assert_clean(ext, &s);
        if ext != "tape" {
            assert!(s.contains("••••••••"), "{ext}:\n{s}");
        }
    }
    for ext in ["png", "gif"] {
        let out = dir.join(format!("x.{ext}"));
        let o = tiza(&[
            "render",
            sidecar.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--scale",
            "1",
        ]);
        assert!(
            o.status.success(),
            "{ext}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(std::fs::metadata(&out).unwrap().len() > 500, "{ext}");
        assert_clean(
            &format!("{ext} stderr"),
            &String::from_utf8_lossy(&o.stderr),
        );
    }
    // The sidecar holds columns only, never the text under them.
    assert_clean("sidecar", &std::fs::read_to_string(&sidecar).unwrap());
}

#[test]
fn ac2_presenter_adds_deletes_undoes_and_unmasks_only_his_screen() {
    let dir = scratch("t183-ac2");
    let src = dir.join("app.env");
    std::fs::write(&src, env_text()).unwrap();
    let sock = dir.join("t183.sock");
    let out = tiza_qa::pty_script(
        &dir,
        "presenter.py",
        &format!(
            r##"
import json
binary, src, sock = sys.argv[1], sys.argv[2], sys.argv[3]
VALUE = "{VALUE}"
def api(method, params):
    r = subprocess.run([binary, "--socket", sock, "api", method, json.dumps(params)],
                       capture_output=True, text=True, timeout=10)
    print("API", method, r.returncode, r.stdout.strip().replace("\n", " "), r.stderr.strip(), flush=True)
since = lambda mark, text: lambda out: text in plain(out[mark:])
s = Pty([binary, "--socket", sock, "--no-save", src], 30, 110)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 10, "first frame"):
    s.send(b":redact 2:TOKEN=\r")
    if s.wait_for(lambda out: "TOKEN=••••••••" in plain(out), 10, "mask"):
        print("MASKED", flush=True)
    api("code.get", {{}})
    mark = len(s.out)
    s.send(b"\t")
    s.settle(0.3)
    s.send(b"x")
    if s.wait_for(since(mark, VALUE), 10, "deleted"):
        print("DELETED", flush=True)
    mark = len(s.out)
    s.send(b"u")
    if s.wait_for(since(mark, "••••••••"), 10, "undone"):
        print("UNDONE", flush=True)
    api("code.get", {{}})
    mark = len(s.out)
    s.send(b":redact off\r")
    if s.wait_for(lambda out: VALUE in plain(out[mark:]) and "UNMASKED" in plain(out[mark:]), 10, "off"):
        print("OFF", flush=True)
    api("code.get", {{}})
    api("render.text", {{"width": 100}})
    api("scene.get", {{}})
s.kill()
s.finish(5)
"##
        ),
        &[
            tiza_qa::bin().to_str().unwrap(),
            src.to_str().unwrap(),
            sock.to_str().unwrap(),
        ],
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    for step in ["MASKED", "DELETED", "UNDONE", "OFF"] {
        assert!(out.contains(step), "{step} missing:\n{out}");
    }
    // Every reader keeps the mask, before the deletion, after the undo and
    // while the presenter's own screen is unmasked.
    let api: Vec<&str> = out.lines().filter(|l| l.starts_with("API ")).collect();
    assert_eq!(api.len(), 5, "{out}");
    for line in api {
        assert!(line.starts_with("API ") && line.contains(" 0 "), "{line}");
        assert!(!line.contains(VALUE), "a reader shows the value: {line}");
        assert!(line.contains("••••••••"), "{line}");
    }
}

#[test]
fn ac3_agents_may_add_redacts_but_never_take_one_away() {
    let (live, _src, _) = opened("t183-ac3");
    live.command("redact 2:TOKEN=");
    assert!(!live.message().unwrap().1);
    let copy = live.api_ok("scene.get", json!({}))["document"].clone();
    // Deleting, narrowing, moving, retyping, relabelling.
    refused(
        "annotation.delete",
        live.api("annotation.delete", json!({"id": "a1"})),
    );
    for (what, p) in [
        (
            "narrow",
            json!({"id": "a1", "shape": {"type": "redact", "at": {"line": 2, "col": 6, "end": 9}}}),
        ),
        (
            "move",
            json!({"id": "a1", "shape": {"type": "redact", "at": {"line": 4, "col": 0, "end": 4}}}),
        ),
        (
            "retype",
            json!({"id": "a1", "shape": {"type": "highlight", "at": {"line": 2, "col": 6, "end": 18}}}),
        ),
        ("label", json!({"id": "a1", "label": "the token"})),
        ("step", json!({"id": "a1", "step": 2})),
    ] {
        refused(what, live.api("annotation.update", p));
    }
    // A clear leaves the redacts, and says so.
    live.api_ok(
        "annotation.add",
        json!({"id": "n1", "type": "note", "line": 4, "text": "the name"}),
    );
    let r = live.api_ok("scene.clear", json!({}));
    assert_eq!(r["removed"], 1, "{r}");
    assert_eq!(r["kept_redacts"], 1, "{r}");
    assert!(
        text(&r, "note").contains("only the presenter removes a mask"),
        "{r}"
    );
    assert!(code_get(&live).contains("TOKEN=••••••••"));
    // scene.put: only the masked copy, every redact as it went.
    let put = |doc: &Value| live.api("scene.put", json!({"document": doc}));
    let lines: Vec<String> = env_text().lines().map(str::to_string).collect();
    let with_line = |l2: Option<&str>| -> Value {
        let mut d = copy.clone();
        let mut code: Vec<Value> = d["code"].as_array().unwrap().clone();
        match l2 {
            Some(t) => code[1] = json!(t),
            None => {
                code.remove(1);
            }
        }
        d["code"] = json!(code);
        d
    };
    // The real code, a guess, another value, the line left out: one answer.
    let real = {
        let mut d = copy.clone();
        d["code"] = json!(lines.clone());
        d
    };
    refused("put of the real code", put(&real));
    refused(
        "put with a guess",
        put(&with_line(Some(&format!("TOKEN={VALUE}")))),
    );
    refused(
        "put with another value",
        put(&with_line(Some("TOKEN=guess-1234"))),
    );
    refused("put without the masked line", put(&with_line(None)));
    let other = json!({"schema": 1, "source": {"language": "rs"}, "code": ["fn main() {}"]});
    refused("put of another file", put(&other));
    let mut without = copy.clone();
    without["annotations"] = json!([]);
    refused("put without its redact", put(&without));
    let mut narrower = copy.clone();
    let end = narrower["annotations"][0]["span"]["end"].as_u64().unwrap();
    narrower["annotations"][0]["span"]["end"] = json!(end - 1);
    refused("put with the redact narrower", put(&narrower));
    // scene.patch: deleting or changing a redact.
    refused(
        "patch deleted",
        live.api(
            "scene.patch",
            json!({"annotations": [{"id": "a1", "deleted": true}]}),
        ),
    );
    let mut narrow = copy["annotations"][0].clone();
    let end = narrow["span"]["end"].as_u64().unwrap();
    narrow["span"]["end"] = json!(end - 2);
    refused(
        "patch narrower",
        live.api("scene.patch", json!({"annotations": [narrow]})),
    );
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    assert!(code_get(&live).contains("TOKEN=••••••••"));
    // What an agent may do: send the copy back as it came, add redacts.
    live.api_ok("scene.put", json!({"document": copy}));
    live.api_ok(
        "scene.patch",
        json!({"annotations": [copy["annotations"][0].clone()], "title": "service"}),
    );
    let mut more = live.api_ok("scene.get", json!({}))["document"].clone();
    more["annotations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "r2", "type": "redact", "step": 0, "span": {"line": 3, "start": 7, "end": 15}}));
    live.api_ok("scene.put", json!({"document": more}));
    live.api_ok(
        "annotation.add",
        json!({"id": "r3", "type": "redact", "at": {"line": 3, "col": 18, "end": 26}}),
    );
    // Over MCP: the same refusal, word for word; adding works.
    let mut mcp = McpClient::start(&live.socket);
    let (err, out, _) = mcp.call("redact", json!({"at": {"line": 5, "find": "KEY="}}));
    assert!(!err, "{out}");
    for (tool, args) in [
        ("delete_annotation", json!({"id": "a1"})),
        ("update_annotation", json!({"id": "r2", "label": "x"})),
    ] {
        let (err, out, _) = mcp.call(tool, args);
        assert!(err && out.contains(AGENT_REFUSAL), "{tool}: {out}");
    }
    let (err, out, _) = mcp.call("clear_annotations", json!({}));
    assert!(!err && out.contains("kept_redacts"), "{out}");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 4);
    // The presenter is not an agent: `x` on a selected redact removes it.
    live.keys(&[KeyCode::Tab, KeyCode::Char('x')]);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 3);
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 4);
    assert!(!render_text(&live).contains(VALUE));
}

/// A `code.patch` move at `step`.
fn patch(step: u32, op: &str, extra: Value) -> Value {
    let mut m = json!({"at_step": step, "action": "code.patch", "op": op});
    for (k, v) in extra.as_object().unwrap() {
        m[k] = v.clone();
    }
    m
}

#[test]
fn ac4_no_move_rewrites_or_deletes_a_masked_line() {
    let (live, _src, _) = opened("t183-ac4");
    live.command("redact 2:TOKEN=");
    assert!(!live.message().unwrap().1);
    for (what, m) in [
        (
            "replace",
            patch(
                1,
                "replace",
                json!({"position": "2", "content": "TOKEN=new"}),
            ),
        ),
        ("delete", patch(1, "delete", json!({"position": "2"}))),
        (
            "typed",
            patch(
                1,
                "replace",
                json!({"position": "2", "content": "TOKEN=typed", "typing": "char"}),
            ),
        ),
        (
            "find before the value",
            patch(1, "delete", json!({"find": "TOKEN="})),
        ),
        (
            "a range over it",
            patch(
                1,
                "delete",
                json!({"find_start": "# service", "find_end": "PAIR="}),
            ),
        ),
    ] {
        let r = live.api("move.add", m);
        let e = r.error.unwrap_or_else(|| panic!("{what}: accepted"));
        assert_eq!(e.code, "invalid_argument", "{what}: {}", e.message);
        assert!(
            e.message.contains("it would change L2") && e.message.contains(MASKED_LINE),
            "{what}: {}",
            e.message
        );
        assert_no_value(what, &e.message);
    }
    assert_eq!(live.with(|a| a.doc.moves.len()), 0);
    // Inserting above shifts the mask with its line.
    live.api_ok(
        "move.add",
        patch(
            1,
            "insert",
            json!({"position": "1", "content": "# inserted"}),
        ),
    );
    live.api_ok("step.set", json!({"step": 1}));
    assert!(
        code_get(&live).contains("   3  TOKEN=••••••••"),
        "{}",
        code_get(&live)
    );
    assert!(
        !live
            .api_ok("scene.list", json!({}))
            .to_string()
            .contains("WIDENED")
    );
    // A line typed elsewhere never shows the value, frame by frame.
    live.api_ok(
        "move.add",
        patch(
            2,
            "replace",
            json!({"position": "5", "content": "NAME=typed-name", "typing": "char", "speed_ms": 15}),
        ),
    );
    live.api_ok("step.set", json!({"step": 1}));
    live.keys(&[KeyCode::Char('n')]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut frames = 0;
    loop {
        let typing = live.with(|a| a.is_typing());
        let shown = live.screen(110, 14) + &code_get(&live);
        assert_no_value("a typing frame", &shown);
        assert!(shown.contains("TOKEN=••••••••"), "{shown}");
        frames += 1;
        if !typing {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "typing never ended");
    }
    assert!(frames > 1, "no typing frame was seen");
    // Deleting a move that would retarget a later one onto the masked line.
    live.api_ok(
        "move.add",
        patch(
            3,
            "replace",
            json!({"position": "2", "content": "# config"}),
        ),
    );
    let r = live.api("move.delete", json!({"index": 1}));
    let e = r.error.expect("move.delete retargets move #3 onto L2");
    assert!(e.message.contains(MASKED_LINE), "{}", e.message);
    assert_eq!(live.with(|a| a.doc.moves.len()), 3);
    // A redact on a line a move changes: refused like the move.
    live.api_ok("step.set", json!({"step": 0}));
    live.command("redact 1");
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains(MASKED_LINE), "{msg}");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    // A document that has such a move does not open: a clear error.
    let dir = scratch("t183-ac4-doc");
    let doc = dir.join("doc.tiza");
    std::fs::write(
        &doc,
        json!({"schema": 1, "source": {"language": "ini"}, "code": env_text().lines().collect::<Vec<_>>(),
               "annotations": [{"id": "r1", "type": "redact", "step": 0, "span": {"line": 2, "start": 6, "end": 18}}],
               "moves": [patch(1, "replace", json!({"position": "2", "content": "TOKEN=x"}))]})
        .to_string(),
    )
    .unwrap();
    let e = fs::load_document(&doc).unwrap_err().to_string();
    assert!(
        e.contains("move #1, step 1: it would change L2") && e.contains(MASKED_LINE),
        "{e}"
    );
    let o = tiza(&["render", doc.to_str().unwrap(), "--out", "-"]);
    assert!(!o.status.success());
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(stderr.contains(MASKED_LINE), "{stderr}");
    assert_no_value("render stderr", &stderr);
}

#[test]
fn ac5_an_embedded_save_is_masked_and_a_reference_stays_a_reference() {
    // Embedded: the base code is written masked; reopened, the same screen.
    let (live, _src, sidecar) = opened("t183-ac5-embed");
    mask_all(&live);
    live.command("embed");
    live.command("w");
    let saved = std::fs::read_to_string(&sidecar).unwrap();
    assert_clean("embedded sidecar", &saved);
    assert!(saved.contains("TOKEN=••••••••"), "{saved}");
    let before = render_text(&live);
    let doc = fs::load_document(&sidecar).unwrap();
    let again = Live::open(
        doc,
        sidecar.clone(),
        scratch("t183-ac5-reopen").join("live.sock"),
    );
    assert_eq!(render_text(&again), before, "reopened, the same screen");
    // A file reference stays one through scene.get → scene.put and a save.
    let (live, _src, sidecar) = opened("t183-ac5-ref");
    mask_all(&live);
    let copy = live.api_ok("scene.get", json!({}))["document"].clone();
    assert_clean("scene.get", &copy.to_string());
    live.api_ok("scene.put", json!({"document": copy}));
    live.command("w");
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
    assert_eq!(saved["code"]["file"], "app.env", "{saved}");
    assert!(!saved.to_string().contains("••••"), "{saved}");
    assert!(
        live.with(|a| a.doc.code[1].clone()).ends_with(VALUE),
        "the instance still follows the real file"
    );
    assert!(code_get(&live).contains("TOKEN=••••••••"));
}

#[test]
fn corner_a_linked_file_with_its_own_redact() {
    // D1: a linked file's redact is on that file's own code, with the same
    // rules: moves shift it, never rewrite its line; a save masks it.
    let dir = scratch("t183-link");
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(
        dir.join("queue.env"),
        format!("QUEUE=jobs\nTOKEN={VALUE}\n"),
    )
    .unwrap();
    let doc = tiza_core::Document::from_code("fn main() {}\n", "rs", Some("main.rs".into()));
    let live = Live::open(doc, dir.join("main.rs.tiza"), dir.join("live.sock"));
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.link", "file": "queue.env"}),
    );
    live.api_ok("step.set", json!({"step": 1}));
    live.command("redact 2:TOKEN=");
    let (msg, err) = live.message().unwrap();
    assert!(!err, "{msg}");
    live.api_ok(
        "move.add",
        patch(
            1,
            "insert",
            json!({"position": "1", "content": "# queue settings"}),
        ),
    );
    live.api_ok("step.set", json!({"step": 1}));
    assert!(
        code_get(&live).contains("   3  TOKEN=••••••••"),
        "{}",
        code_get(&live)
    );
    for (surface, s) in readers(&live) {
        assert_no_value(&format!("linked file: {surface}"), &s);
    }
    let r = live.api(
        "move.add",
        patch(
            1,
            "replace",
            json!({"find": "TOKEN=", "content": "TOKEN=other"}),
        ),
    );
    assert!(r.error.unwrap().message.contains(MASKED_LINE));
    live.command("embed");
    live.command("w");
    let saved = std::fs::read_to_string(dir.join("main.rs.tiza")).unwrap();
    assert_no_value("saved with the linked file frozen", &saved);
    assert!(saved.contains("TOKEN=••••••••"), "{saved}");
}
