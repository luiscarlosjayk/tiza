//! t-140 — Redacts fail closed through moves: a `code.patch` that rewrites
//! a redacted line follows the value or widens the mask, never leaves it in
//! its old columns; move content is masked wherever it is read back.
//!
//! Acceptance criteria:
//! - AC1 A move (`code.patch` insert/replace/delete, instant or typed) that
//!   changes a line holding a redact: the redact follows its text with the
//!   rules of a reload (t-117) — the text around it when intact; otherwise
//!   the rest of the line from where its text no longer matches (whole line
//!   if the column is gone), or every new line of the replaced range when
//!   the line count changes; dropped only when its line is deleted with
//!   nothing in its place. Never by plain column shift.
//! - AC2 The same while typing: no frame (TUI, GIF, .cast) shows a
//!   character of the value, partial frames included.
//! - AC3 Going back restores the step-0 redact exactly; ten round trips
//!   leave it identical.
//! - AC4 WIDENED in `:list`/`scene.list` (`widened: true` in scene.get) at
//!   that step, a loud notice when the presenter reaches it, `render --step
//!   N` warns once on stderr.
//! - AC5 Move content masked in `scene.get`, `move.list`, the MCP
//!   (`tiza://scene`, `list_moves`); the `.tiza` keeps the authored content.
//! - AC6 `move.add` with content that repeats the value under a redact is
//!   refused (`invalid_argument`, value never echoed) unless it lands under
//!   a mask after the patch.
//! - AC7 Docs: README, agent guide, CHANGELOG.

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_core::Document;
use tiza_core::fs;
use tiza_qa::{Live, McpClient, scratch, tiza};

const LINE: &str = r#"{"token": "abc123secret", "user": "ana"}"#;
const MASKED: &str = r#"{"token": "••••••••", "user": "ana"}"#;

/// Every 6-character piece of the value that holds a digit (the plain
/// word "secret" can appear in prose).
fn pieces(value: &str) -> Vec<String> {
    let c: Vec<char> = value.chars().collect();
    (0..=c.len().saturating_sub(6))
        .map(|i| c[i..i + 6].iter().collect::<String>())
        .filter(|p| p.chars().any(|ch| ch.is_ascii_digit()))
        .collect()
}

fn assert_no(value: &str, what: &str, s: &str) {
    for p in pieces(value) {
        assert!(!s.contains(&p), "{what} shows «{p}» of the value:\n{s}");
    }
}

/// `config.json` with the line, opened live, redact on the value (cols 11..23).
fn opened(name: &str, code: &str) -> (Live, std::path::PathBuf) {
    let dir = scratch(name);
    let src = dir.join("config.json");
    std::fs::write(&src, code).unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("config.json.tiza"), dir.join("live.sock"));
    live.command("redact 1@11..23");
    let (msg, err) = live.message().unwrap();
    assert!(!err, "{msg}");
    (live, dir)
}

fn replace_line(content: &str) -> serde_json::Value {
    json!({"at_step": 1, "action": "code.patch", "op": "replace", "position": "1", "content": content})
}

fn code_get(live: &Live) -> String {
    live.api_ok("code.get", json!({}))["text"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Everything an agent or the screen can read at the current step.
fn readers(live: &Live) -> String {
    let mut all = String::new();
    all += &code_get(live);
    all += live.api_ok("render.text", json!({"width": 100}))["text"]
        .as_str()
        .unwrap();
    all += &live.api_ok("scene.get", json!({})).to_string();
    all += &live.api_ok("scene.list", json!({})).to_string();
    all += &live.api_ok("move.list", json!({})).to_string();
    all += &live.api_ok("status", json!({})).to_string();
    all += &live.screen(110, 20);
    all
}

/// Goes to `step` the way the presenter does: from step 0, `n` by `n`
/// (the TUI opens at step 1, where a move of step 1 already applies).
fn step_to(live: &Live, step: u32) {
    live.api_ok("step.set", json!({"step": 0}));
    for _ in 0..step {
        live.keys(&[KeyCode::Char('n')]);
        live.with(|a| a.finish_typing());
    }
}

#[test]
fn ac1_the_five_cases_never_show_the_value() {
    // (1) a longer value: the text around it is intact → the mask follows,
    // exact (still eight bullets, no widening).
    let (live, _) = opened("t140-case1", &format!("{LINE}\n"));
    live.api_ok(
        "move.add",
        replace_line(r#"{"token": "abc123LONGER99", "user": "ana"}"#),
    );
    step_to(&live, 1);
    let all = readers(&live);
    assert_no("abc123LONGER99", "case 1", &all);
    assert!(code_get(&live).contains(MASKED), "{}", code_get(&live));
    assert!(!all.contains("WIDENED"), "followed, not widened:\n{all}");

    // (2) the key renamed, same value: the text before it changed → the
    // rest of the line from where it differs, WIDENED.
    let (live, _) = opened("t140-case2", &format!("{LINE}\n"));
    live.api_ok(
        "move.add",
        replace_line(r#"{"apiToken": "abc123secret", "user": "ana"}"#),
    );
    step_to(&live, 1);
    let all = readers(&live);
    assert_no("abc123secret", "case 2", &all);
    assert!(
        code_get(&live).contains("{\"••••••••"),
        "{}",
        code_get(&live)
    );
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(list.contains("WIDENED"), "{list}");
    let scene = live.api_ok("scene.get", json!({}));
    assert_eq!(scene["document"]["annotations"][0]["widened"], json!(true));
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("redact a1 could not follow its text through move #1"),
        "{msg}"
    );

    // (3) the line split in two: the line count changed → both new lines,
    // whole.
    let (live, _) = opened("t140-case3", &format!("{LINE}\nnext\n"));
    live.api_ok(
        "move.add",
        replace_line("{\"token\": \"abc123secret\",\n \"user\": \"ana\"}"),
    );
    step_to(&live, 1);
    let all = readers(&live);
    assert_no("abc123secret", "case 3", &all);
    let code = code_get(&live);
    assert!(code.contains("   1  ••••••••\n"), "{code}");
    assert!(code.contains("   2   ••••••••\n"), "{code}");
    assert!(code.contains("   3  next"), "{code}");

    // (5) the same value at the same columns, the text after it edited:
    // the screen was already masked; now the content travels masked too in
    // scene.get / move.list / the MCP. (Mid-line, the text on both sides
    // anchors the mask: one side changed, so the rest of the line from the
    // value on is masked — fail closed, as a reload does.)
    let (live, dir) = opened("t140-case5", &format!("{LINE}\n"));
    live.api_ok(
        "move.add",
        replace_line(r#"{"token": "abc123secret", "user": "bob"}"#),
    );
    step_to(&live, 1);
    let all = readers(&live);
    assert_no("abc123secret", "case 5", &all);
    assert!(
        code_get(&live).contains("   1  {\"token\": \"••••••••\n"),
        "{}",
        code_get(&live)
    );
    let moves = live.api_ok("move.list", json!({}));
    assert_eq!(
        moves["moves"][0]["content"],
        json!(r#"{"token": "••••••••"#)
    );
    let scene = live.api_ok("scene.get", json!({}));
    assert_eq!(
        scene["document"]["moves"][0]["content"],
        json!(r#"{"token": "••••••••"#)
    );
    let mut mcp = McpClient::start(&live.socket);
    let scene = mcp.request("resources/read", json!({"uri": "tiza://scene"}));
    assert_no("abc123secret", "tiza://scene", &scene.to_string());
    assert!(scene.to_string().contains("••••••••"), "{scene}");
    let (err, out, _) = mcp.call("list_moves", json!({}));
    assert!(!err, "{out}");
    assert_no("abc123secret", "list_moves", &out);
    // The .tiza keeps the authored content (it is the presenter's file).
    live.command("w");
    let saved = std::fs::read_to_string(dir.join("config.json.tiza")).unwrap();
    assert!(saved.contains("bob"), "{saved}");
}

#[test]
fn ac2_typing_never_shows_a_character_of_the_value() {
    // (4) typed character by character, with `keep=4`: every partial frame
    // is masked — no tail of a half-typed value (that would be its start)
    // — in the TUI while it types, and in every frame of the GIF and the
    // .cast. Only the finished value shows its last 4, as `keep` asks.
    const NEW: &str = "abc123LONGERVALUE99";
    let dir = scratch("t140-typing");
    let src = dir.join("config.json");
    std::fs::write(&src, format!("{LINE}\n")).unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("config.json.tiza"), dir.join("live.sock"));
    live.api_ok(
        "annotation.add",
        json!({"type": "redact", "at": {"line": 1, "col": 11, "end": 23}, "keep": 4}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "replace", "position": "1",
               "content": format!(r#"{{"token": "{NEW}", "user": "ana"}}"#),
               "typing": "char", "speed_ms": 20}),
    );
    // Any 4 characters of the value but its last 4.
    let leaks = |s: &str| -> Vec<String> {
        let c: Vec<char> = NEW.chars().collect();
        (0..c.len() - 4)
            .map(|i| c[i..i + 4].iter().collect::<String>())
            .filter(|p| s.contains(p.as_str()))
            .collect()
    };
    live.api_ok("step.set", json!({"step": 0}));
    live.keys(&[KeyCode::Char('n')]);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut seen = 0;
    loop {
        let typing = live.with(|a| a.is_typing());
        let shown = live.screen(110, 12) + &code_get(&live);
        assert!(
            leaks(&shown).is_empty(),
            "a typing frame leaks {:?}:\n{shown}",
            leaks(&shown)
        );
        seen += 1;
        if !typing {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "typing never ended");
    }
    assert!(seen > 1, "no intermediate frame was observed");
    assert!(
        code_get(&live).contains(r#"{"token": "••••••••UE99", "user": "ana"}"#),
        "{}",
        code_get(&live)
    );
    // Every frame of the recordings, computed from the same plan.
    live.command("w");
    let tiza_file = dir.join("config.json.tiza");
    let doc = fs::open(tiza_file.to_str().unwrap()).unwrap().document;
    let frames = tiza_core::anim::frames(
        &doc,
        &tiza_core::theme::Theme::dark(),
        &tiza_core::anim::AnimOptions::default(),
    );
    assert!(frames.iter().filter(|f| f.typing).count() > 10);
    for f in &frames {
        let text = f.canvas.to_plain_text();
        assert!(leaks(&text).is_empty(), "frame of step {}:\n{text}", f.step);
    }
    let cast = dir.join("demo.cast");
    let o = tiza(&[
        "render",
        tiza_file.to_str().unwrap(),
        "--out",
        cast.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let cast = std::fs::read_to_string(&cast).unwrap();
    assert!(leaks(&cast).is_empty(), ".cast leaks {:?}", leaks(&cast));
}

#[test]
fn ac3_going_back_restores_the_step_zero_mask_exactly() {
    let (live, _) = opened("t140-back", &format!("{LINE}\n"));
    let before = live.with(|a| a.doc.annotations.clone());
    live.api_ok(
        "move.add",
        replace_line(r#"{"apiToken": "abc123secret", "user": "ana"}"#),
    );
    step_to(&live, 0);
    let at0 = code_get(&live);
    assert!(at0.contains(MASKED), "{at0}");
    for _ in 0..10 {
        live.keys(&[KeyCode::Char('n')]);
        live.with(|a| a.finish_typing());
        assert_no("abc123secret", "step 1", &code_get(&live));
        live.keys(&[KeyCode::Char('p')]);
        assert_eq!(code_get(&live), at0);
    }
    // The redact itself: same columns, now pinned to step 0 (the code its
    // columns refer to); not widened.
    let after = live.with(|a| a.doc.annotations.clone());
    assert_eq!(after.len(), 1);
    assert_eq!(after[0].shape, before[0].shape);
    assert_eq!(after[0].step, 0);
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(!list.contains("WIDENED"), "{list}");
}

#[test]
fn ac4_render_step_warns_once_and_masks_every_format() {
    let (live, dir) = opened("t140-render", &format!("{LINE}\n"));
    live.api_ok(
        "move.add",
        replace_line(r#"{"apiToken": "abc123secret", "user": "ana"}"#),
    );
    live.command("w");
    let doc = dir.join("config.json.tiza");
    for (out, step) in [("a.txt", "1"), ("a.svg", "1"), ("a.gif", "1")] {
        let path = dir.join(out);
        let o = tiza(&[
            "render",
            doc.to_str().unwrap(),
            "--step",
            step,
            "--out",
            path.to_str().unwrap(),
        ]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let err = String::from_utf8_lossy(&o.stderr);
        assert_eq!(
            err.matches("could not follow its text through move #1")
                .count(),
            1,
            "{out}: {err}"
        );
        assert_no("abc123secret", "stderr", &err);
        if !out.ends_with(".gif") {
            assert_no(
                "abc123secret",
                out,
                &std::fs::read_to_string(&path).unwrap(),
            );
        }
    }
    // Step 0: nothing widened yet, nothing said.
    let o = tiza(&["render", doc.to_str().unwrap(), "--step", "0", "--out", "-"]);
    assert!(String::from_utf8_lossy(&o.stderr).is_empty());
    assert!(String::from_utf8_lossy(&o.stdout).contains(MASKED));
}

#[test]
fn ac6_move_content_that_copies_the_value_is_refused_without_echo() {
    let (live, _) = opened("t140-refuse", &format!("{LINE}\nend\n"));
    // A new line that repeats the value where no mask is.
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "2",
               "content": "// old token: abc123secret"}),
    );
    let e = r.error.expect("refused");
    assert_eq!(e.code, "invalid_argument");
    assert!(e.message.contains("redact a1"), "{}", e.message);
    assert_no("abc123secret", "the error", &e.message);
    assert!(live.with(|a| a.doc.moves.is_empty()));
    // Landing under the mask (same value in place) is allowed.
    let r = live.api(
        "move.add",
        replace_line(r#"{"token": "abc123secret", "user": "bob"}"#),
    );
    assert!(r.error.is_none(), "{:?}", r.error);
}

#[test]
fn corner_insert_above_delete_and_two_redacts_on_a_line() {
    // A line inserted above: the mask moves down with its line, exact.
    let (live, _) = opened("t140-insert", &format!("{LINE}\n"));
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "// config"}),
    );
    step_to(&live, 1);
    let code = code_get(&live);
    assert!(code.contains(&format!("   2  {MASKED}")), "{code}");
    assert!(
        !live
            .api_ok("scene.list", json!({}))
            .to_string()
            .contains("WIDENED")
    );

    // The redacted line deleted: dropped, with the notice.
    let (live, _) = opened("t140-delete", &format!("{LINE}\nend\n"));
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "position": "1"}),
    );
    step_to(&live, 1);
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("redact a1 dropped: move #1 deleted its line"),
        "{msg}"
    );
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(list.contains("(unanchored)"), "{list}");

    // A replace that keeps the line identical: exact, not widened.
    let (live, _) = opened("t140-same", &format!("{LINE}\n"));
    live.api_ok("move.add", replace_line(LINE));
    step_to(&live, 1);
    assert!(code_get(&live).contains(MASKED));
    assert!(
        !live
            .api_ok("scene.list", json!({}))
            .to_string()
            .contains("WIDENED")
    );

    // Two redacts on one line, the line rewritten: one mask, no
    // duplicate ids, nothing of either value.
    let two = r#"{"token": "abc123secret", "pass": "hunter2xyz9"}"#;
    let (live, _) = opened("t140-two", &format!("{two}\n"));
    live.command("redact 1@32..43");
    assert!(!live.message().unwrap().1);
    live.api_ok(
        "move.add",
        replace_line(r#"{"tok": "abc123secret", "pwd": "hunter2xyz9"}"#),
    );
    step_to(&live, 1);
    let all = readers(&live);
    assert_no("abc123secret", "two redacts", &all);
    assert_no("hunter2xyz9", "two redacts", &all);
    let code = code_get(&live);
    assert_eq!(code.matches("••••••••").count(), 1, "{code}");
    let ids: Vec<String> = live.with(|a| {
        a.doc
            .view_at(a.doc.step)
            .doc
            .annotations
            .iter()
            .map(|x| x.id.clone())
            .collect()
    });
    let mut dedup = ids.clone();
    dedup.dedup();
    assert_eq!(ids, dedup);
}

#[test]
fn corner_rebind_must_cover_the_value() {
    // A rebind onto a span that covers the value is taken; one that does
    // not is ignored and the redact widens.
    let (live, _) = opened("t140-rebind", &format!("{LINE}\n"));
    let mut m = replace_line(r#"{"secretToken": "abc123secret", "user": "ana"}"#);
    m["rebind"] = json!({"a1": {"at": {"line": 1, "col": 16}}});
    live.api_ok("move.add", m);
    step_to(&live, 1);
    assert_no("abc123secret", "rebind", &readers(&live));
    let (live, _) = opened("t140-rebind2", &format!("{LINE}\n"));
    let mut m = replace_line(r#"{"secretToken": "abc123secret", "user": "ana"}"#);
    m["rebind"] = json!({"a1": {"at": {"line": 1, "find": "user"}}});
    live.api_ok("move.add", m);
    step_to(&live, 1);
    assert_no("abc123secret", "rebind elsewhere", &readers(&live));
    assert!(
        live.api_ok("scene.list", json!({}))
            .to_string()
            .contains("WIDENED")
    );
}

#[test]
fn corner_auto_redact_on_patched_code_and_a_linked_file() {
    // `:redact auto` at a step whose move wrote the secret: the mask is
    // pinned to that step (the line does not exist before it).
    let live = Live::from_code("NAME=demo\n", "env", "t140-auto");
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "2",
               "content": "PASSWORD=hunter2hunter2"}),
    );
    step_to(&live, 1);
    live.command("redact auto");
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.contains("1 masked"), "{msg}");
    assert_no("hunter2hunter2", "auto at step 1", &readers(&live));
    assert_eq!(live.with(|a| a.doc.annotations[0].step), 1);
    // A later move that rewrites that line: followed / widened from there.
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "replace", "position": "2",
               "content": "DB_PASSWORD=hunter2hunter2"}),
    );
    step_to(&live, 2);
    assert_no("hunter2hunter2", "auto at step 2", &readers(&live));
    step_to(&live, 0);
    assert!(code_get(&live).contains("NAME=demo"));

    // A linked file with its own redact and a move on that file.
    let dir = scratch("t140-link");
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(dir.join("secrets.env"), format!("{LINE}\n")).unwrap();
    let doc = Document::from_code("fn main() {}\n", "rs", Some("main.rs".into()));
    let live = Live::open(doc, dir.join("main.rs.tiza"), dir.join("live.sock"));
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.link", "file": "secrets.env"}),
    );
    step_to(&live, 1);
    live.command("redact 1@11..23");
    assert!(!live.message().unwrap().1);
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "replace", "position": "1",
               "content": r#"{"apiToken": "abc123secret", "user": "ana"}"#}),
    );
    step_to(&live, 2);
    assert_no("abc123secret", "linked file", &readers(&live));
    step_to(&live, 1);
    assert!(code_get(&live).contains(MASKED), "{}", code_get(&live));
}

#[test]
fn corner_a_hand_written_document_is_read_both_ways() {
    // A `.tiza` written by hand, by an agent or before t-140: a redact with
    // the default step 1 and a move at step 1 that rewrites its line. Its
    // columns could refer to the file or to the code after the move: both
    // are masked (fail closed), at every step, on every format.
    let dir = scratch("t140-legacy");
    for (name, content) in [
        ("longer", r#"{"token": "abc123LONGER99", "user": "ana"}"#),
        ("renamed", r#"{"apiToken": "abc123secret", "user": "ana"}"#),
        ("above", "// config"),
    ] {
        let op = if name == "above" { "insert" } else { "replace" };
        let doc = json!({
            "schema": 1,
            "code": [LINE],
            "source": {"language": "json"},
            "annotations": [{"id": "a1", "type": "redact", "span": {"line": 1, "start": 11, "end": 23}}],
            "moves": [{"at_step": 1, "action": "code.patch", "op": op, "position": "1", "content": content}]
        });
        let path = dir.join(format!("{name}.tiza"));
        std::fs::write(&path, doc.to_string()).unwrap();
        for step in ["0", "1"] {
            let o = tiza(&[
                "render",
                path.to_str().unwrap(),
                "--step",
                step,
                "--out",
                "-",
            ]);
            let out = String::from_utf8_lossy(&o.stdout);
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
            assert_no("abc123LONGER99", &format!("{name} step {step}"), &out);
            assert_no("abc123secret", &format!("{name} step {step}"), &out);
            assert!(out.contains("••••••••"), "{name} step {step}:\n{out}");
        }
    }
}
