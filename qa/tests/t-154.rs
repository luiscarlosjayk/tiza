//! t-154 — Redacts through moves, the edges t-140 left: `redact_from_step`
//! documents, a redact on a move-written line when moves change before it,
//! and secrets in move targets.
//!
//! Acceptance criteria:
//! - AC1 `redact_from_step: true`: a move added at or before a redact's
//!   step never reinterprets its columns (the coordinates are pinned apart
//!   from the activation step: `pin: 0`, the untouched file).
//! - AC2 A redact pinned to a move-written line (`pin: N`) follows that
//!   line when moves are added or deleted at earlier steps; deleting the
//!   move that wrote it drops it loudly (`redact aN dropped`).
//! - AC3 No second reading for redacts Tiza pinned itself (only for
//!   documents that did not come through add/move.add).
//! - AC4 A move target text that looks like a secret is refused like a
//!   redact anchor, without echo; a target under a mask is just not found,
//!   with the same answer as text that is not there and without echo.
//! - AC5 `:next`/`:prev` keep the redact notice.
//! - AC6 This file; CHANGELOG.
//!
//! Corner cases: linked files (`code.link`) with their own redacts; undo /
//! redo of move.add restores the pins.

use serde_json::json;
use tiza_core::fs;
use tiza_core::{Document, Shape};
use tiza_qa::{Live, scratch, tiza};

const SECRET: &str = "abc123secret";

/// Every 6-character piece of the value that holds a digit.
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
    all += &live.screen(110, 20);
    all
}

fn goto(live: &Live, step: u32) {
    live.api_ok("step.set", json!({"step": step}));
    live.with(|a| a.finish_typing());
}

/// The redact `id`: (step, line, start, end, pin, widened).
fn redact(live: &Live, id: &str) -> Option<(u32, u32, u32, u32, Option<u32>, bool)> {
    let id = id.to_string();
    live.with(move |a| {
        let r = a.doc.find_annotation(&id)?;
        match &r.shape {
            Shape::Redact {
                span, pin, widened, ..
            } => Some((r.step, span.line, span.start, span.end, *pin, *widened)),
            _ => None,
        }
    })
}

fn open_tiza(name: &str, doc: serde_json::Value) -> Live {
    let dir = scratch(name);
    let path = dir.join("doc.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    let doc = fs::open(path.to_str().unwrap()).unwrap().document;
    Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"))
}

#[test]
fn ac1_redact_from_step_keeps_its_columns_through_a_move_at_its_step() {
    // Card step 1: `redact_from_step: true`, a redact at step 2 on
    // `token=abc123secret`, then a move at step 2 that replaces that line
    // with a longer value → masked at step 2, the whole value.
    let live = open_tiza(
        "t154-from-step",
        json!({"schema": 1, "code": ["# config", "token=abc123secret", "end"],
               "source": {"language": "ini"}, "redact_from_step": true}),
    );
    live.command("step 2");
    live.command("redact 2:token=");
    let (msg, err) = live.message().unwrap();
    assert!(!err, "{msg}");
    assert_eq!(redact(&live, "a1").unwrap().0, 2, "activation step kept");
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "replace", "find": "token=",
               "content": "token=abc123secretLONGER-tail99"}),
    );
    // Pinned to the untouched file, its step still its activation step.
    let (step, line, start, end, pin, _) = redact(&live, "a1").unwrap();
    assert_eq!((step, line, start, end, pin), (2, 2, 6, 18, Some(0)));
    goto(&live, 2);
    let all = readers(&live);
    assert_no("abc123secretLONGER-tail99", "step 2", &all);
    assert!(
        code_get(&live).contains("token=••••••••"),
        "{}",
        code_get(&live)
    );
    // Opt-in mode: not masked before its step (the line is the old one).
    goto(&live, 1);
    assert!(code_get(&live).contains("token=abc123secret"));

    // A move at an EARLIER step that inserts a line above: at step 2 the
    // mask is still on the token line, the line above is untouched.
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1",
               "content": "# header"}),
    );
    goto(&live, 2);
    let code = code_get(&live);
    assert_no(
        "abc123secretLONGER-tail99",
        "step 2 after an insert above",
        &readers(&live),
    );
    assert!(
        code.contains("# config") && code.contains("token=••••••••"),
        "{code}"
    );

    // Saved and re-opened: the pin survives, the render masks it.
    live.command("w");
    let path = live.with(|a| a.save_path.clone());
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(
        saved.contains("\"pin\": 0") || saved.contains("\"pin\":0"),
        "{saved}"
    );
    let o = tiza(&[
        "render",
        path.to_str().unwrap(),
        "--step",
        "2",
        "--out",
        "-",
    ]);
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_no("abc123secretLONGER-tail99", "render --step 2", &out);
}

/// `# config` / `port=8080`; move #1 at step 2 inserts the token line
/// after `port=8080` (a text target, so it follows that line), and
/// `:redact 3:token=` marks it at step 2.
fn token_written_by_a_move(name: &str) -> Live {
    let live = Live::from_code("# config\nport=8080\n", "ini", name);
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "insert", "after": "port=8080",
               "content": format!("token={SECRET}")}),
    );
    goto(&live, 2);
    live.command("redact 3:token=");
    let (msg, err) = live.message().unwrap();
    assert!(!err, "{msg}");
    live
}

#[test]
fn ac2_a_redact_on_a_move_written_line_follows_moves_added_before_it() {
    // Card step 2: `:redact` on a line a step-2 move inserted, then a move
    // at step 1 inserting a line above → still masked at step 2 on the
    // right line, the line that took its old number is not masked.
    let live = token_written_by_a_move("t154-follow");
    let (step, line, _, _, pin, _) = redact(&live, "a1").unwrap();
    assert_eq!((step, line, pin), (2, 3, Some(1)), "pinned to move #1");
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1",
               "content": "// header"}),
    );
    // Its offset in move #1's lines is kept: right after move #1 it is L4.
    let (_, line, start, end, pin, widened) = redact(&live, "a1").unwrap();
    assert_eq!((line, start, end, pin, widened), (4, 6, 18, Some(1), false));
    goto(&live, 2);
    let all = readers(&live);
    assert_no(SECRET, "step 2", &all);
    let code = code_get(&live);
    assert!(
        code.contains("port=8080") && code.contains("token=••••••••"),
        "{code}"
    );
    assert!(!all.contains("WIDENED"), "followed exactly:\n{all}");

    // Deleting a move at an earlier step: the line moves back up, exact.
    live.api_ok("move.delete", json!({"index": 2}));
    assert_eq!(redact(&live, "a1").unwrap().1, 3);
    goto(&live, 2);
    assert_no(SECRET, "step 2 after deleting the header", &readers(&live));
    assert!(code_get(&live).contains("port=8080"));

    // Deleting the move that wrote the line: dropped, loudly.
    let r = live.api_ok("move.delete", json!({"index": 1}));
    let notices = r["notices"].to_string();
    assert!(notices.contains("redact a1 dropped"), "{r}");
    assert_no(SECRET, "the notice", &notices);
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains("redact a1 dropped"), "{msg}");
    assert!(redact(&live, "a1").is_none());
    // `u` brings the move and the redact back, pinned again.
    live.command("u");
    assert_eq!(redact(&live, "a1").unwrap().4, Some(1));
    goto(&live, 2);
    assert_no(SECRET, "after undo", &readers(&live));
}

#[test]
fn ac3_no_second_reading_for_a_redact_tiza_pinned() {
    // A move rewrites L1 in place with a new value; the presenter marks the
    // new value at step 1. Tiza pinned it to move #1: step 0 shows the old
    // line as it is (nothing there was asked to be masked).
    let old = r#"{"token": "public-demo1", "user": "ana"}"#;
    let new = r#"{"token": "abc123secret", "user": "ana"}"#;
    let live = Live::from_code(&format!("{old}\n"), "json", "t154-one-reading");
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "replace", "position": "1",
               "content": new}),
    );
    goto(&live, 1);
    live.command("redact 1@11..23");
    assert!(!live.message().unwrap().1);
    assert_eq!(redact(&live, "a1").unwrap().4, Some(1));
    goto(&live, 1);
    assert_no(SECRET, "step 1", &readers(&live));
    goto(&live, 0);
    let code = code_get(&live);
    assert!(
        code.contains(old) && !code.contains("••"),
        "no mask on the old line: {code}"
    );

    // The same redact written by hand (no pin): both readings, as in t-140.
    let live = open_tiza(
        "t154-two-readings",
        json!({"schema": 1, "code": [old], "source": {"language": "json"},
               "annotations": [{"id": "a1", "type": "redact", "span": {"line": 1, "start": 11, "end": 23}}],
               "moves": [{"at_step": 1, "action": "code.patch", "op": "replace", "position": "1", "content": new}]}),
    );
    goto(&live, 0);
    assert!(code_get(&live).contains("••••••••"), "{}", code_get(&live));
    goto(&live, 1);
    assert_no(SECRET, "hand-written step 1", &readers(&live));
}

#[test]
fn ac4_move_targets_never_save_or_echo_a_secret() {
    let live = token_written_by_a_move("t154-targets");
    // Card step 3: a `find` of the masked value is refused without echo,
    // with the same answer as a text that is not there (no oracle).
    let masked = live.api(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "delete", "find": SECRET}),
    );
    let absent = live.api(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "delete", "find": "zzz999qqq"}),
    );
    let (m, a) = (masked.error.unwrap(), absent.error.unwrap());
    assert_eq!(m.code, "text_not_found");
    assert_eq!((m.code.clone(), m.message.clone()), (a.code, a.message));
    assert_no(SECRET, "the error", &m.message);
    // A part of the value: the same.
    let part = live.api(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "insert", "after": &SECRET[2..9], "content": "x"}),
    );
    let p = part.error.unwrap();
    assert_eq!(p.code, "text_not_found");
    assert_no(SECRET, "the error", &p.message);

    // A target that looks like a secret by itself: refused like a redact
    // anchor, in every target field, never echoed.
    let token = "sk_live_51Hx9QeLkT2mNpR8vWzYa";
    for params in [
        json!({"at_step": 3, "action": "code.patch", "op": "delete", "find": token}),
        json!({"at_step": 3, "action": "code.patch", "op": "insert", "after": token, "content": "x"}),
        json!({"at_step": 3, "action": "code.patch", "op": "insert", "before": token, "content": "x"}),
        json!({"at_step": 3, "action": "code.patch", "op": "delete", "find_start": "# config", "find_end": token}),
        json!({"at_step": 3, "action": "select", "from": {"find": token}, "to": {"find": "port"}}),
        json!({"at_step": 3, "action": "code.patch", "op": "replace", "position": "2", "content": "port=1",
               "rebind": {"a1": {"at": {"find": token}}}}),
    ] {
        let r = live.api("move.add", params.clone());
        let e = r.error.unwrap_or_else(|| panic!("accepted: {params}"));
        assert_eq!(e.code, "invalid_argument", "{params}");
        assert!(e.message.contains("looks like a secret"), "{}", e.message);
        assert!(!e.message.contains("sk_live"), "{}", e.message);
    }
    assert_eq!(live.with(|a| a.doc.moves.len()), 1, "nothing was added");
    // An ordinary target still works.
    live.api_ok(
        "move.add",
        json!({"at_step": 3, "action": "code.patch", "op": "replace", "find": "port=8080", "content": "port=9090"}),
    );

    // Without redacts a long identifier is an ordinary target.
    let live = Live::from_code(
        "let a = HashMapOfStringToVecOfBytes::new();\n",
        "rs",
        "t154-no-redacts",
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "replace",
               "find": "HashMapOfStringToVecOfBytes::new()", "content": "let a = 1;"}),
    );
}

#[test]
fn ac5_next_and_prev_keep_the_redact_notice() {
    // A move at step 1 renames the key before the value (the redact
    // widens: loud notice), and one at step 2 appends a line.
    let line = r#"{"token": "abc123secret", "user": "ana"}"#;
    let dir = scratch("t154-notice");
    std::fs::write(dir.join("config.json"), format!("{line}\n")).unwrap();
    let doc = fs::open(dir.join("config.json").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("config.json.tiza"), dir.join("live.sock"));
    live.command("redact 1@11..23");
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "replace", "position": "1",
               "content": r#"{"apiToken": "abc123secret", "user": "ana"}"#}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "insert", "position": "2", "content": "end"}),
    );
    goto(&live, 0);
    live.command("next");
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("redact a1 could not follow its text through move #1"),
        "{msg}"
    );
    assert_eq!(live.with(|a| a.doc.step), 1);
    // A step without a notice still says where it is.
    live.command("next");
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.starts_with("step 2/"), "{msg}");
    live.command("prev");
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains("redact a1 could not follow"), "{msg}");
    goto(&live, 0);
    live.command("step 1");
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains("redact a1 could not follow"), "{msg}");
}

#[test]
fn corner_undo_and_redo_of_move_add_restore_the_pins() {
    let live = token_written_by_a_move("t154-undo");
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1",
               "content": "// header"}),
    );
    assert_eq!(redact(&live, "a1").unwrap().1, 4);
    live.command("u");
    assert_eq!(live.with(|a| a.doc.moves.len()), 1);
    assert_eq!(redact(&live, "a1").unwrap().1, 3, "undo: back on L3");
    goto(&live, 2);
    assert_no(SECRET, "after undo", &readers(&live));
    assert!(code_get(&live).contains("port=8080"));
    live.command("redo");
    assert_eq!(redact(&live, "a1").unwrap().1, 4, "redo: L4 again");
    goto(&live, 2);
    assert_no(SECRET, "after redo", &readers(&live));
    assert!(code_get(&live).contains("port=8080"));
}

#[test]
fn corner_a_linked_file_with_its_own_redact() {
    // main.rs links queue.env at step 1; move #2, step 2 writes the token
    // in queue.env, marked there at step 2; a move at step 1 (after the
    // link) then inserts a line at the top of queue.env.
    let dir = scratch("t154-link");
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    std::fs::write(dir.join("queue.env"), "QUEUE=jobs\n").unwrap();
    let doc = Document::from_code("fn main() {}\n", "rs", Some("main.rs".into()));
    let live = Live::open(doc, dir.join("main.rs.tiza"), dir.join("live.sock"));
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.link", "file": "queue.env"}),
    );
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "insert", "after": "QUEUE=jobs",
               "content": format!("TOKEN={SECRET}")}),
    );
    goto(&live, 2);
    live.command("redact 2:TOKEN=");
    let (msg, err) = live.message().unwrap();
    assert!(!err, "{msg}");
    assert_eq!(redact(&live, "a1").unwrap().4, Some(2));
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1",
               "content": "# queue settings"}),
    );
    assert_eq!(redact(&live, "a1").unwrap().1, 3);
    goto(&live, 2);
    let code = code_get(&live);
    assert_no(SECRET, "linked file at step 2", &readers(&live));
    assert!(
        code.contains("QUEUE=jobs") && code.contains("TOKEN=••••••••"),
        "{code}"
    );
    // Deleting the move that wrote it: dropped, loudly.
    let r = live.api_ok("move.delete", json!({"index": 2}));
    assert!(
        r["notices"].to_string().contains("redact a1 dropped"),
        "{r}"
    );
}
