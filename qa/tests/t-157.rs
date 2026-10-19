//! t-157 — A move added at an earlier step retargets a later `position`
//! move: its content (with a secret) can land on a line no redact covers.
//!
//! Acceptance criteria:
//! - AC1 Fail closed: when a move's content holds the value under a redact,
//!   even as the start or a part of a longer token, the whole token is
//!   masked wherever the content lands (eight `•`, WIDENED); when the
//!   token's end is unclear (`=`, `:`), the rest of the line. A piece of 4+
//!   characters of the value counts as a copy (the rule of case 7).
//! - AC2 Never silent: adding or deleting a move that changes which line a
//!   later `code.patch` hits names the retargeted move (bar notice,
//!   `notices` of move.add/move.delete, MCP text) without quoting masked
//!   text: «move #1, step 2 now replaces L2 «# config» instead of
//!   «token=…»: check it».
//! - AC3 Every view at every step: screen (authoring and presentation),
//!   code.get, render.text, scene.get (the moves' `content` included),
//!   scene.list, move.list, the MCP (`list_moves`, `get_code`,
//!   `tiza://scene`) and the exports (txt/md/ansi/svg/png/gif/cast/tape).
//!   The saved .tiza keeps the authored content.
//! - AC4 This file (the 15 cases of the card); CHANGELOG, README, agent
//!   guide.
//!
//! Corner cases: moves targeted by text are not retargeted; linked files.

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_core::fs;
use tiza_qa::{Live, McpClient, scratch, tiza};

const SECRET: &str = "abc123secret";
const FILE: &str = "# config\ntoken=abc123secret\nport=8080\n";
const LONG: &str = "token=abc123secretLONGERvalue77";

/// Every 6-character piece of `value` with a digit or a capital in it (the
/// plain word «secret» can appear in prose and in messages).
fn pieces(value: &str) -> Vec<String> {
    let c: Vec<char> = value.chars().collect();
    (0..=c.len().saturating_sub(6))
        .map(|i| c[i..(i + 6).min(c.len())].iter().collect::<String>())
        .filter(|p| {
            p.chars()
                .any(|ch| ch.is_ascii_digit() || ch.is_ascii_uppercase())
        })
        .collect()
}

#[track_caller]
fn assert_no(values: &[&str], what: &str, s: &str) {
    for v in values {
        for p in pieces(v) {
            assert!(!s.contains(&p), "{what} shows «{p}» of «{v}»:\n{s}");
        }
    }
}

fn code_get(live: &Live) -> String {
    live.api_ok("code.get", json!({}))["text"]
        .as_str()
        .unwrap()
        .to_string()
}

fn goto(live: &Live, step: u32) {
    live.api_ok("step.set", json!({"step": step}));
    live.with(|a| a.finish_typing());
}

/// Everything the screen, an agent or the socket can read at this step:
/// the authoring screen and the presentation screen too.
fn readers(live: &Live) -> String {
    let mut all = String::new();
    all += &code_get(live);
    all += live.api_ok("render.text", json!({"width": 110}))["text"]
        .as_str()
        .unwrap();
    all += &live.api_ok("scene.get", json!({})).to_string();
    all += &live.api_ok("scene.list", json!({})).to_string();
    all += &live.api_ok("move.list", json!({})).to_string();
    all += &live.api_ok("status", json!({})).to_string();
    all += &live.screen(120, 20);
    live.with(|a| a.present = true);
    all += &live.screen(120, 20);
    live.with(|a| a.present = false);
    all
}

/// [`readers`] at every step, plus the MCP tools and resources at each.
#[track_caller]
fn assert_every_step(live: &Live, values: &[&str], what: &str) {
    let max = live.with(|a| a.doc.max_step());
    let mut mcp = McpClient::start(&live.socket);
    for s in 0..=max {
        goto(live, s);
        assert_no(values, &format!("{what}, step {s}"), &readers(live));
        let scene = mcp.request("resources/read", json!({"uri": "tiza://scene"}));
        assert_no(
            values,
            &format!("{what}, tiza://scene, step {s}"),
            &scene.to_string(),
        );
        for tool in ["list_moves", "get_code"] {
            let (err, out, _) = mcp.call(tool, json!({}));
            assert!(!err, "{tool}: {out}");
            assert_no(values, &format!("{what}, MCP {tool}, step {s}"), &out);
        }
    }
}

/// The code at `step` (masked, what code.get returns), one entry per line.
fn lines_at(live: &Live, step: u32) -> Vec<String> {
    goto(live, step);
    code_get(live)
        .lines()
        .map(|l| l.get(6..).unwrap_or("").to_string())
        .collect()
}

fn notices(reply: &serde_json::Value) -> Vec<String> {
    reply["notices"]
        .as_array()
        .map(|v| v.iter().map(|n| n.as_str().unwrap().to_string()).collect())
        .unwrap_or_default()
}

/// The normal document of Claude's note: a source file opened live, a
/// redact on the value (L2 cols 6..18).
fn normal(name: &str, code: &str) -> (Live, std::path::PathBuf) {
    let dir = scratch(name);
    let src = dir.join("config.ini");
    std::fs::write(&src, code).unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("config.ini.tiza"), dir.join("live.sock"));
    live.command("redact 2@6..18");
    let (msg, err) = live.message().unwrap();
    assert!(!err, "{msg}");
    (live, dir)
}

fn replace(step: u32, position: &str, content: &str) -> serde_json::Value {
    json!({"at_step": step, "action": "code.patch", "op": "replace", "position": position,
           "content": content})
}

fn insert(step: u32, position: &str, content: &str) -> serde_json::Value {
    json!({"at_step": step, "action": "code.patch", "op": "insert", "position": position,
           "content": content})
}

/// Card case 2's moves on `live`: the later `replace position "2"` with
/// `content`, then the earlier insert that retargets it. Returns the
/// insert's reply.
fn retarget(live: &Live, content: &str) -> serde_json::Value {
    live.api_ok("move.add", replace(2, "2", content));
    live.api_ok("move.add", insert(1, "1", "# payments"))
}

const NOTICE: &str = "move #1, step 2 now replaces L2 «# config» instead of «token=…»: check it";

#[test]
fn ac3_case01_the_description_in_a_real_terminal() {
    // The steps of the description, with the real binary in a pty: a
    // `redact_from_step` document, `token=abc123secret` redacted at step
    // 2, a step-2 move that replaces L2 with a longer value, then a step-1
    // move inserting `# header` at L1 → at step 2 nothing of the tail on
    // screen, in code.get or render.text, and a notice names move #1.
    let dir = scratch("t157-pty");
    let doc = dir.join("doc.tiza");
    std::fs::write(
        &doc,
        json!({"schema": 1, "code": ["# config", "token=abc123secret", "port=8080"],
               "source": {"language": "ini"}, "redact_from_step": true})
        .to_string(),
    )
    .unwrap();
    let sock = dir.join("t157.sock");
    let out = tiza_qa::pty_script(
        &dir,
        "desc.py",
        r##"
import json
binary, doc, sock = sys.argv[1], sys.argv[2], sys.argv[3]
def api(method, params):
    r = subprocess.run([binary, "--socket", sock, "api", method, json.dumps(params)],
                       capture_output=True, text=True, timeout=10)
    print("API", method, r.returncode, r.stdout.strip().replace("\n", " "), r.stderr.strip(), flush=True)
    return r.stdout
s = Pty([binary, "--socket", sock, "--no-save", doc], 30, 120)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 10, "first frame"):
    api("step.set", {"step": 2})
    api("annotation.add", {"type": "redact", "step": 2, "at": {"line": 2, "col": 6, "end": 18}})
    # The redact masks from step 2 (`redact_from_step`), not step 1 on
    # screen: the TUI says so on an alert row (t-174); a key dismisses it.
    s.wait_for(lambda out: b"does not mask step 1" in out, 10, "redact alert")
    s.send(b"\x1b")
    s.settle(0.3)
    api("move.add", {"at_step": 2, "action": "code.patch", "op": "replace", "position": "2",
                     "content": "token=abc123secretLONGER-tail99"})
    api("move.add", {"at_step": 1, "action": "code.patch", "op": "insert", "position": "1",
                     "content": "# header"})
    api("step.set", {"step": 2})
    s.wait_for(lambda out: b"now replaces L2" in out, 10, "bar notice")
    api("code.get", {})
    api("render.text", {"width": 110})
    # `u` then `U` (rework): redoing the insert retargets move #1 again and
    # the bar says so again. Spaces may come as cursor moves, so the text
    # is compared without them.
    squeezed = lambda since: lambda out: plain(out[since:]).replace(" ", "")
    mark = len(s.out)
    s.send(b"u")
    s.wait_for(lambda out: "undone:move#2added" in squeezed(mark)(out), 10, "undo")
    mark = len(s.out)
    s.send(b"U")
    if s.wait_for(lambda out: "move#1,step2nowreplacesL2" in squeezed(mark)(out), 10,
                  "bar notice after redo"):
        print("REDO_NOTICE", flush=True)
    s.send(b"P")
    s.wait_for(lambda out: plain(out).count("token=") >= 1, 5, "presentation")
    s.settle(0.3)
s.kill()
s.finish(5)
print("SCREEN", plain(s.out).replace("\n", " "))
"##,
        &[
            tiza_qa::bin().to_str().unwrap(),
            doc.to_str().unwrap(),
            sock.to_str().unwrap(),
        ],
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    // (The value itself shows at steps 0-1: `redact_from_step` masks it
    // from its own step, by design.)
    assert_no(&["LONGER-tail99"], "pty run", &out);
    assert!(
        out.contains("move #1, step 2 now replaces L2 «# config» instead of «token=…»"),
        "{out}"
    );
    assert!(out.contains("token=••••••••"), "{out}");
    assert!(out.contains("REDO_NOTICE"), "{out}");
}

#[test]
fn harness_the_pty_is_drained_while_the_body_calls_the_api() {
    // Rework of t-157: case 1 failed on macOS because nobody read the pty
    // while the body ran `tiza api` with `subprocess.run`; Tiza blocked
    // writing a frame into the full pty buffer and stopped answering its
    // socket. The same on Linux with frames bigger than its buffer: a
    // 200×400 terminal full of long lines, never read by the body, and
    // API calls that redraw it whole (`theme.set toggle`). With the reader thread of
    // qa/src/pty.py every call answers (with the old helper, the first one
    // timed out at 10 s).
    let dir = scratch("t157-drain");
    let doc = dir.join("big.tiza");
    let code: Vec<String> = (1..=300)
        .map(|i| format!("line {i} = {}", "x".repeat(150)))
        .collect();
    std::fs::write(
        &doc,
        json!({"schema": 1, "code": code, "source": {"language": "ini"}}).to_string(),
    )
    .unwrap();
    let sock = dir.join("drain.sock");
    let out = tiza_qa::pty_script(
        &dir,
        "drain.py",
        r##"
import json
binary, doc, sock = sys.argv[1], sys.argv[2], sys.argv[3]
def api(method, params):
    try:
        r = subprocess.run([binary, "--socket", sock, "api", method, json.dumps(params)],
                           capture_output=True, text=True, timeout=10)
    except subprocess.TimeoutExpired:
        print("API_TIMEOUT", method, flush=True)
        return False
    print("API", method, r.returncode, flush=True)
    return r.returncode == 0
s = Pty([binary, "--socket", sock, "--no-save", doc], 200, 400)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 10, "first frame"):
    for i in range(6):
        if not (api("theme.set", {"theme": "toggle"}) and api("code.get", {})):
            break
    else:
        print("ALL_ANSWERED", flush=True)
s.kill()
s.finish(5)
print("READ", len(s.out))
"##,
        &[
            tiza_qa::bin().to_str().unwrap(),
            doc.to_str().unwrap(),
            sock.to_str().unwrap(),
        ],
    );
    assert!(
        out.contains("ALL_ANSWERED") && !out.contains("API_TIMEOUT"),
        "{out}"
    );
}

#[test]
fn ac1_case02_a_normal_document_masks_the_whole_new_value() {
    let (live, dir) = normal("t157-normal", FILE);
    live.api_ok("move.add", replace(2, "2", LONG));
    assert_eq!(lines_at(&live, 2)[1], "token=••••••••");
    let reply = live.api_ok("move.add", insert(1, "1", "# payments"));
    assert_eq!(notices(&reply), vec![NOTICE.to_string()]);
    assert!(
        live.with(|a| a.redact_alerts.clone())
            .contains(&NOTICE.to_string()),
        "bar notice"
    );
    assert!(live.screen(120, 20).contains("now replaces L2 «# config»"));
    // Step 2: the retargeted move wrote over `# config`; its value is
    // masked whole there, and the redact still covers its own line.
    assert_eq!(
        lines_at(&live, 2),
        vec![
            "# payments",
            "token=••••••••",
            "token=••••••••",
            "port=8080"
        ]
    );
    // t-174 AC9 (Luis): the redact followed its own text; the copy is
    // masked, but that is no WIDENED.
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(!list.contains("WIDENED"), "{list}");
    // The loud step notice when the presenter reaches step 2.
    goto(&live, 1);
    live.keys(&[KeyCode::Char('n')]);
    let alerts = live.with(|a| a.redact_alerts.clone()).join(" ");
    assert!(
        // t-174 (Luis, A5): the redact followed its text; the copy the
        // move wrote is said line by line, not as a range L2-3.
        alerts.contains("redact a1: masked a copy of its value on L2 (move #1)"),
        "{alerts}"
    );
    // scene.get at every step: the moves' content has no tail.
    for s in 0..=2 {
        goto(&live, s);
        let scene = live.api_ok("scene.get", json!({}));
        assert_no(
            &["LONGERvalue77"],
            &format!("scene.get step {s}"),
            &scene.to_string(),
        );
        assert_eq!(
            scene["document"]["moves"][0]["content"],
            json!("token=••••••••")
        );
    }
    assert_every_step(&live, &["LONGERvalue77", SECRET], "normal document");
    // The .tiza keeps what was written.
    live.command("w");
    let saved = std::fs::read_to_string(dir.join("config.ini.tiza")).unwrap();
    assert!(saved.contains(LONG), "{saved}");
}

#[test]
fn ac3_case03_every_export_of_the_saved_document() {
    let (live, dir) = normal("t157-exports", FILE);
    retarget(&live, LONG);
    live.command("w");
    let doc = dir.join("config.ini.tiza");
    let values = ["LONGERvalue77", SECRET];
    let mut exts = vec!["txt", "md", "ansi", "svg", "png", "gif", "cast", "tape"];
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        exts.push("mp4");
    }
    for ext in exts {
        let out = dir.join(format!("a.{ext}"));
        let mut args = vec![
            "render",
            doc.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ];
        if matches!(ext, "txt" | "md" | "ansi" | "svg" | "png") {
            args.extend(["--step", "2"]);
        }
        if ext == "svg" {
            // The embedded fonts are binary noise; the drawn text is what counts.
            args.push("--no-embed-fonts");
        }
        let o = tiza(&args);
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(o.status.success(), "{ext}: {err}");
        assert_no(&values, &format!("{ext} stderr"), &err);
        assert_eq!(
            err.matches("redact a1: masked a copy of its value on L2 (move #1)")
                .count(),
            1,
            "{ext}: said once: {err}"
        );
        if !matches!(ext, "png" | "gif" | "mp4") {
            let text = std::fs::read_to_string(&out).unwrap();
            assert_no(&values, ext, &text);
            if ext != "tape" {
                assert!(text.contains("••••••••"), "{ext}");
            }
        }
    }
    // PNG and GIF are pixels: every frame they draw, as text.
    let d = fs::open(doc.to_str().unwrap()).unwrap().document;
    let frames = tiza_core::anim::frames(
        &d,
        &tiza_core::theme::Theme::dark(),
        &tiza_core::anim::AnimOptions::default(),
    );
    assert!(frames.len() >= 3);
    for f in &frames {
        assert_no(
            &values,
            &format!("frame of step {}", f.step),
            &f.canvas.to_plain_text(),
        );
    }
}

#[test]
fn ac1_case04_the_earlier_move_deletes_a_line() {
    let (live, _) = normal("t157-delete", FILE);
    live.api_ok("move.add", replace(2, "2", LONG));
    let reply = live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "position": "1"}),
    );
    assert_eq!(
        notices(&reply),
        vec!["move #1, step 2 now replaces L2 «port=8080» instead of «token=…»: check it"]
    );
    assert_eq!(lines_at(&live, 2), vec!["token=••••••••", "token=••••••••"]);
    assert_every_step(&live, &["LONGERvalue77", SECRET], "delete above");
}

#[test]
fn ac1_case05_one_line_becomes_two_and_a_range_target() {
    let (live, _) = normal("t157-split", FILE);
    live.api_ok("move.add", replace(2, "2", LONG));
    let reply = live.api_ok("move.add", replace(1, "1", "# config\n# payments"));
    assert_eq!(
        notices(&reply),
        vec!["move #1, step 2 now replaces L2 «# payments» instead of «token=…»: check it"]
    );
    assert_eq!(lines_at(&live, 2)[1], "token=••••••••");
    assert_every_step(&live, &["LONGERvalue77", SECRET], "one line becomes two");

    // A range target on the later move.
    let (live, _) = normal(
        "t157-range",
        "# config\ntoken=abc123secret\nport=8080\nend\n",
    );
    live.api_ok(
        "move.add",
        replace(2, "2-3", "token=abc123secretLONGERvalue77\nport=9090"),
    );
    let reply = live.api_ok("move.add", replace(1, "1", "# config\n# payments"));
    assert_eq!(
        notices(&reply),
        vec!["move #1, step 2 now replaces L2-3 «# payments…» instead of «token=…»: check it"]
    );
    let at2 = lines_at(&live, 2);
    assert!(at2.contains(&"token=••••••••".to_string()), "{at2:?}");
    assert_every_step(&live, &["LONGERvalue77", SECRET], "range target");
}

#[test]
fn ac1_case06_the_value_in_other_shapes() {
    // In the middle of a token, quoted, twice on a line, on two lines of a
    // multi-line content, and split across two lines (pieces of 4+).
    let cases: [(&str, &[&str], &[&str]); 5] = [
        (
            "x=PREabc123secretPOST",
            &["x=••••••••"],
            &["PREabc123secretPOST"],
        ),
        (
            r#"key="abc123secret-v2""#,
            &[r#"key="••••••••""#],
            &["abc123secret-v2"],
        ),
        (
            "a=abc123secretX b=abc123secretY",
            &["a=•••••••• b=••••••••"],
            &["abc123secretX", "abc123secretY"],
        ),
        (
            "t1=abc123secretA\nt2=abc123secretB",
            &["t1=••••••••", "t2=••••••••"],
            &["abc123secretA", "abc123secretB"],
        ),
        (
            "part1=abc123\npart2=secretXYZ",
            &["part1=••••••••", "part2=••••••••"],
            &["secretXYZ", "abc123"],
        ),
    ];
    for (k, (content, want, values)) in cases.iter().enumerate() {
        let (live, _) = normal(&format!("t157-shape{k}"), FILE);
        let reply = retarget(&live, content);
        assert_eq!(notices(&reply), vec![NOTICE.to_string()], "{content}");
        let at2 = lines_at(&live, 2);
        for (j, w) in want.iter().enumerate() {
            assert_eq!(at2[1 + j], *w, "{content}: {at2:?}");
        }
        let mut all: Vec<&str> = values.to_vec();
        all.push(SECRET);
        assert_every_step(&live, &all, content);
    }
}

#[test]
fn ac1_case07_a_piece_of_the_value() {
    // The rule: a piece of 4+ characters of the value (one of them a
    // letter or digit) is a copy — its whole token is masked. `abc123` is
    // 6 of 12 characters → masked; `abc` (3) is not a copy.
    let (live, _) = normal("t157-piece", FILE);
    let reply = retarget(&live, "id=abc123 ref=abc-x");
    assert_eq!(notices(&reply), vec![NOTICE.to_string()]);
    assert_eq!(lines_at(&live, 2)[1], "id=•••••••• ref=abc-x");
    assert_every_step(&live, &[SECRET, "abc123"], "a piece");
    // Exactly four characters (`cret`) count too.
    let (live, _) = normal("t157-piece4", FILE);
    retarget(&live, "name=Xcret9");
    assert_eq!(lines_at(&live, 2)[1], "name=••••••••");
}

#[test]
fn ac2_case08_a_typed_later_move() {
    let (live, dir) = normal("t157-typed", FILE);
    let mut m = replace(2, "2", LONG);
    m["typing"] = json!("char");
    m["speed_ms"] = json!(30);
    live.api_ok("move.add", m);
    let reply = live.api_ok("move.add", insert(1, "1", "# payments"));
    assert_eq!(notices(&reply), vec![NOTICE.to_string()]);
    goto(&live, 1);
    live.keys(&[KeyCode::Char('n')]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut seen = 0;
    loop {
        let typing = live.with(|a| a.is_typing());
        let shown = live.screen(120, 12) + &code_get(&live);
        assert_no(&["LONGERvalue77", SECRET], "a typing frame", &shown);
        seen += 1;
        if !typing {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "typing never ended");
    }
    assert!(seen > 1, "no intermediate frame was observed");
    assert_eq!(lines_at(&live, 2)[1], "token=••••••••");
    // Every frame of the GIF and the .cast.
    live.command("w");
    let file = dir.join("config.ini.tiza");
    let d = fs::open(file.to_str().unwrap()).unwrap().document;
    let frames = tiza_core::anim::frames(
        &d,
        &tiza_core::theme::Theme::dark(),
        &tiza_core::anim::AnimOptions::default(),
    );
    assert!(frames.iter().filter(|f| f.typing).count() > 10);
    for f in &frames {
        assert_no(
            &["LONGERvalue77", SECRET],
            "GIF frame",
            &f.canvas.to_plain_text(),
        );
    }
    let cast = dir.join("a.cast");
    let o = tiza(&[
        "render",
        file.to_str().unwrap(),
        "--out",
        cast.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_no(
        &["LONGERvalue77", SECRET],
        ".cast",
        &std::fs::read_to_string(&cast).unwrap(),
    );
}

#[test]
fn ac1_case09_a_redact_made_with_redact_auto() {
    let dir = scratch("t157-auto");
    let src = dir.join("app.env");
    std::fs::write(&src, "# config\nAPI_KEY=abc123secret\nport=8080\n").unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("app.env.tiza"), dir.join("live.sock"));
    live.command("redact auto");
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.contains("1 masked"), "{msg}");
    live.api_ok(
        "move.add",
        replace(2, "2", "API_KEY=abc123secretLONGERvalue77"),
    );
    let reply = live.api_ok("move.add", insert(1, "1", "# payments"));
    assert_eq!(
        notices(&reply),
        vec!["move #1, step 2 now replaces L2 «# config» instead of «API_KEY=…»: check it"]
    );
    assert_eq!(lines_at(&live, 2)[1], "API_KEY=••••••••");
    assert_every_step(&live, &["LONGERvalue77", SECRET], ":redact auto");
}

#[test]
fn ac1_case10_two_redacts_with_two_values_on_the_line() {
    let (live, _) = normal(
        "t157-two",
        "# config\ntoken=abc123secret key=zz99top7\nport=8080\n",
    );
    live.command("redact 2@23..31");
    assert!(!live.message().unwrap().1);
    live.api_ok(
        "move.add",
        replace(2, "2", "token=abc123secretLONG1 key=zz99top7LONG2"),
    );
    assert_eq!(lines_at(&live, 2)[1], "token=•••••••• key=••••••••");
    live.api_ok("move.add", insert(1, "1", "# payments"));
    assert_eq!(lines_at(&live, 2)[1], "token=•••••••• key=••••••••");
    assert_every_step(
        &live,
        &["abc123secretLONG1", "zz99top7LONG2"],
        "two redacts",
    );
}

#[test]
fn ac2_case11_undo_and_redo() {
    let (live, _) = normal("t157-undo", FILE);
    retarget(&live, LONG);
    assert_eq!(lines_at(&live, 2)[1], "token=••••••••");
    assert!(!live.with(|a| a.redact_alerts.is_empty()));
    live.keys(&[KeyCode::Char('u')]);
    // The later move hits its old line again; the notice is gone.
    assert!(live.with(|a| a.redact_alerts.is_empty()));
    assert_eq!(
        lines_at(&live, 2),
        vec!["# config", "token=••••••••", "port=8080"]
    );
    assert_every_step(&live, &["LONGERvalue77", SECRET], "after undo");
    live.keys(&[KeyCode::Char('U')]);
    assert_eq!(
        lines_at(&live, 2),
        vec![
            "# payments",
            "token=••••••••",
            "token=••••••••",
            "port=8080"
        ]
    );
    assert_every_step(&live, &["LONGERvalue77", SECRET], "after redo");
    // Redoing shifts the code under move #1 again: the presenter is told
    // again, on the bar, as move.add told them (rework of t-157).
    // (Visiting step 2 above added the widened redact's own alert.)
    assert!(
        live.with(|a| a.redact_alerts.clone())
            .contains(&NOTICE.to_string())
    );
    let screen = live.screen(80, 20);
    assert!(screen.contains("now replaces L2 «# config»"), "{screen}");
    assert_no(&["LONGERvalue77", SECRET], "screen after redo", &screen);
    // Any key hides it, as after move.add; undo again clears it.
    live.keys(&[KeyCode::Char('u')]);
    assert!(live.with(|a| a.redact_alerts.is_empty()));
    // Redoing a deletion that retargets says it too: delete the insert
    // (move #1 goes back to `token=…`), undo, redo.
    live.keys(&[KeyCode::Char('U')]);
    live.keys(&[KeyCode::Esc]);
    assert!(live.with(|a| a.redact_alerts.is_empty()));
    let reply = live.api_ok("move.delete", json!({"index": 2}));
    let back = "move #1, step 2 now replaces L2 «token=…» instead of «# config»: check it";
    assert_eq!(notices(&reply), vec![back.to_string()]);
    live.keys(&[KeyCode::Char('u')]);
    assert!(live.with(|a| a.redact_alerts.is_empty()));
    live.keys(&[KeyCode::Char('U')]);
    assert!(
        live.with(|a| a.redact_alerts.clone())
            .contains(&back.to_string())
    );
    assert_every_step(
        &live,
        &["LONGERvalue77", SECRET],
        "after redoing the delete",
    );
    // Redoing an edit that retargets nothing says nothing.
    live.keys(&[KeyCode::Esc]);
    live.api_ok(
        "annotation.add",
        json!({"type": "box", "from_line": 3, "to_line": 3}),
    );
    live.keys(&[KeyCode::Char('u')]);
    live.keys(&[KeyCode::Char('U')]);
    assert!(live.with(|a| a.redact_alerts.is_empty()));
}

#[test]
fn ac2_case12_an_agent_retargets_it_over_mcp() {
    let (live, _) = normal("t157-mcp", FILE);
    live.api_ok("move.add", replace(2, "2", LONG));
    let mut mcp = McpClient::start(&live.socket);
    let (err, out, _) = mcp.call(
        "add_move",
        json!({"at_step": 1, "op": "insert", "position": "1", "content": "# payments"}),
    );
    assert!(!err, "{out}");
    assert!(out.contains(NOTICE), "{out}");
    assert_no(&["LONGERvalue77", SECRET], "add_move result", &out);
    // The presenter sees it on the bar.
    assert!(
        live.with(|a| a.redact_alerts.clone())
            .contains(&NOTICE.to_string())
    );
    assert!(live.screen(120, 20).contains("now replaces L2"));
    // Deleting it over MCP retargets move #1 back, said too.
    let (err, out, _) = mcp.call("delete_move", json!({"index": 2}));
    assert!(!err, "{out}");
    assert!(
        out.contains("move #1, step 2 now replaces L2 «token=…» instead of «# config»: check it"),
        "{out}"
    );
    assert_every_step(&live, &["LONGERvalue77", SECRET], "MCP");
}

#[test]
fn ac3_case13_save_quit_and_reopen() {
    let (live, dir) = normal("t157-reopen", FILE);
    retarget(&live, LONG);
    live.command("w");
    live.keys(&[KeyCode::Char('q')]);
    drop(live);
    let file = dir.join("config.ini.tiza");
    let saved = std::fs::read_to_string(&file).unwrap();
    assert!(saved.contains(LONG), "the .tiza keeps the authored content");
    let doc = fs::open(file.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, file.clone(), dir.join("again.sock"));
    assert_eq!(
        lines_at(&live, 2),
        vec![
            "# payments",
            "token=••••••••",
            "token=••••••••",
            "port=8080"
        ]
    );
    assert_every_step(&live, &["LONGERvalue77", SECRET], "reopened");
    // `tiza render` of the reopened file at each step.
    for s in ["0", "1", "2"] {
        let o = tiza(&["render", file.to_str().unwrap(), "--step", s, "--out", "-"]);
        assert!(o.status.success());
        assert_no(
            &["LONGERvalue77", SECRET],
            &format!("render --step {s}"),
            &String::from_utf8_lossy(&o.stdout),
        );
    }
    // `tiza check` of the reopened file (t-101): no error, and the redact
    // the retargeted move widened is a warning with the TUI's own text,
    // without the secret. (The retarget itself is said when the move that
    // causes it is added or deleted; a saved file has no «before» to
    // compare with, and t-101's acceptance criteria do not ask for it.)
    for args in [
        vec!["check", file.to_str().unwrap()],
        vec!["check", file.to_str().unwrap(), "--json"],
    ] {
        let o = tiza(&args);
        let out = String::from_utf8_lossy(&o.stdout).into_owned();
        assert!(o.status.success(), "{out}");
        assert!(
            out.contains("redact a1: masked a copy of its value on L2 (move #1)"),
            "{out}"
        );
        assert_no(&["LONGERvalue77", SECRET], "tiza check", &out);
        assert_no(
            &["LONGERvalue77", SECRET],
            "tiza check stderr",
            &String::from_utf8_lossy(&o.stderr),
        );
    }
    let o = tiza(&["check", file.to_str().unwrap(), "--strict"]);
    assert!(!o.status.success(), "the warning fails --strict");
}

#[test]
fn ac1_case14_a_linked_file_with_its_own_redact() {
    let dir = scratch("t157-link");
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(dir.join("config.ini"), FILE).unwrap();
    let doc = tiza_core::Document::from_code("fn main() {}\n", "rs", Some("main.rs".into()));
    let live = Live::open(doc, dir.join("main.rs.tiza"), dir.join("live.sock"));
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.link", "file": "config.ini"}),
    );
    goto(&live, 1);
    live.command("redact 2@6..18");
    assert!(!live.message().unwrap().1);
    live.api_ok("move.add", replace(2, "2", LONG));
    let reply = live.api_ok("move.add", insert(1, "1", "# payments"));
    assert_eq!(
        notices(&reply),
        vec![
            "move #2, step 2 now replaces L2 of config.ini «# config» instead of «token=…»: check it"
        ]
    );
    assert_eq!(
        lines_at(&live, 2),
        vec![
            "# payments",
            "token=••••••••",
            "token=••••••••",
            "port=8080"
        ]
    );
    assert_every_step(&live, &["LONGERvalue77", SECRET], "linked file");
}

#[test]
fn ac2_case15_negative_cases() {
    // A move targeted by text is not retargeted: no notice, no extra mask.
    let (live, _) = normal("t157-find", FILE);
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "replace", "find": "token=",
               "content": LONG}),
    );
    let reply = live.api_ok("move.add", insert(1, "1", "# payments"));
    assert!(notices(&reply).is_empty(), "{reply}");
    assert!(live.with(|a| a.redact_alerts.is_empty()));
    assert_eq!(
        lines_at(&live, 2),
        vec!["# payments", "# config", "token=••••••••", "port=8080"]
    );
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(!list.contains("WIDENED"), "{list}");

    // A retarget onto a line with no secret anywhere: the notice only.
    let (live, _) = normal(
        "t157-plain",
        "# config\ntoken=abc123secret\nport=8080\ndebug=true\n",
    );
    live.api_ok("move.add", replace(2, "4", "debug=false"));
    let reply = live.api_ok("move.add", insert(1, "1", "# payments"));
    assert_eq!(
        notices(&reply),
        vec!["move #1, step 2 now replaces L4 «port=8080» instead of «debug=true»: check it"]
    );
    assert_eq!(
        lines_at(&live, 2),
        vec![
            "# payments",
            "# config",
            "token=••••••••",
            "debug=false",
            "debug=true"
        ]
    );
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(!list.contains("WIDENED"), "{list}");

    // Without any redact the retarget is still said.
    let live = Live::from_code("a\nb\nc\n", "txt", "t157-noredact");
    live.api_ok("move.add", replace(2, "2", "B"));
    let reply = live.api_ok("move.add", insert(1, "1", "top"));
    assert_eq!(
        notices(&reply),
        vec!["move #1, step 2 now replaces L2 «a» instead of «b»: check it"]
    );
    // An earlier move that keeps the line count and the target line: no notice.
    let reply = live.api_ok("move.add", replace(1, "3", "C"));
    assert!(notices(&reply).is_empty(), "{reply}");
}
