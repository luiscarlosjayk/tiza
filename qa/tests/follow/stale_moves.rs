//! A reload or a paste that shifts the lines under a move marks it
//! (stale) and skips it, with a notice; it never moves it in silence.
//!
//! Acceptance criteria:
//! - AC1 When the step-0 code of a file changes under its moves (a follow
//!   reload, or a paste at a step where no patch of that file is applied),
//!   every move is checked again on the new code. Line numbers (`position`,
//!   a `cursor` line, a `select` by line) follow the change only where that
//!   is exact (Luis, 2026-10-20): the old and new code's common start and
//!   end keep their lines, the end shifted by what the change added or
//!   removed; a target between them (the changed block, or several edits)
//!   is stale even on the same text, and so is one whose lines no longer
//!   hold their text (out of range counts). A `find`/`select`/`rebind`
//!   target that now matches nothing or more than once is stale. No
//!   approximate moving of moves (ADR 0007 §3e).
//! - AC2 A stale move is skipped when stepping (steps keep their numbers),
//!   shows as `(stale: <why>)` in `scene.list`, carries `stale: true` and
//!   `stale_reason` in `move.list` and MCP `list_moves`; `status` says how
//!   many moves are stale.
//! - AC3 One notice per change names the stale moves («stale move
//!   skipped (u undoes or re-add): #1 (L2 changed)»). A change
//!   that leaves every move matching says nothing new; one that makes a
//!   move match again says so («move #1 matches the code again»).
//! - AC4 Nothing is deleted: the `.tiza` keeps a stale move as it was; a
//!   later reload, paste or undo that makes a move match again clears the
//!   mark; undoing the reload or paste restores the exact previous state.
//! - AC6 This file; CHANGELOG; ADR 0007 §7.
//!
//! Follow only watches the main file today (watching a linked file is
//! for later), so the linked-file case checks `tiza_core::stale` directly with
//! the linked file's code changed.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, scratch, tiza};

const INI: &str = "[server]\nhost = localhost\nport = 8080\n[db]\nuser = admin\n";

const WAIT: Duration = Duration::from_secs(10);

/// `config.ini` next to `config.ini.tiza`, which reads it by reference, with
/// `moves`, `annotations` and any other field of `extra`, opened at step 0.
fn demo(name: &str, code: &str, moves: Value, annotations: Value, extra: Value) -> (Live, PathBuf) {
    let dir = scratch(name);
    std::fs::write(dir.join("config.ini"), code).unwrap();
    let mut doc = json!({
        "schema": 1,
        "source": {"language": "ini"},
        "code": {"file": "config.ini"},
        "step": 0,
        "moves": moves,
        "annotations": annotations,
    });
    for (k, v) in extra.as_object().unwrap() {
        doc[k] = v.clone();
    }
    let path = dir.join("config.ini.tiza");
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    let d = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(d, path, dir.join("s.sock"));
    (live, dir)
}

/// The card's demo: a `replace position "2"` at step 2.
fn replace_line_2(name: &str) -> (Live, PathBuf) {
    demo(
        name,
        INI,
        json!([{"at_step": 2, "action": "code.patch", "op": "replace", "position": "2", "content": "host = example.com"}]),
        json!([]),
        json!({}),
    )
}

fn save_like_zed(path: &Path, content: &str) {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

fn reloads(live: &Live) -> u64 {
    live.api_ok("follow.status", json!({}))["reloads"]
        .as_u64()
        .unwrap_or(0)
}

/// Saves `content` over config.ini and waits for reload number `n`.
fn reload(live: &Live, dir: &Path, content: &str, n: u64) {
    save_like_zed(&dir.join("config.ini"), content);
    let start = Instant::now();
    while reloads(live) < n {
        assert!(start.elapsed() < WAIT, "timed out waiting for reload #{n}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn follow(live: &Live) {
    live.command("follow on");
    assert_eq!(live.message().unwrap().0, "following config.ini");
}

fn paste(live: &Live, text: &str) {
    let text = text.to_string();
    live.with(move |app| app.replace_code(&text)).unwrap();
}

fn list(live: &Live) -> Vec<String> {
    live.api_ok("scene.list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn stale_list(live: &Live) -> Vec<String> {
    list(live)
        .into_iter()
        .filter(|l| l.starts_with("move #"))
        .collect()
}

fn stale_count(live: &Live) -> u64 {
    live.api_ok("status", json!({}))["stale_moves"]
        .as_u64()
        .unwrap_or(0)
}

/// The code shown at `step`, then back to step 0.
fn code_at(live: &Live, step: u32) -> Vec<String> {
    live.api_ok("step.set", json!({"step": step}));
    let code = live.with(|app| app.doc.code.clone());
    live.api_ok("step.set", json!({"step": 0}));
    code
}

fn message(live: &Live) -> String {
    live.message().map(|m| m.0).unwrap_or_default()
}

// ---------------------------------------------------------------- AC1–AC3

/// INI with its line 2, the one the card's move replaces, edited.
fn host_edited() -> String {
    INI.replace("host = localhost", "host = db1")
}

fn position_of(live: &Live, index: usize) -> Value {
    live.api_ok("move.list", json!({}))["moves"][index]["position"].clone()
}

#[test]
fn a_reload_that_inserts_above_a_position_move_shifts_it_and_one_that_edits_its_line_marks_it() {
    let (live, dir) = replace_line_2("ac1");
    follow(&live);
    assert_eq!(code_at(&live, 2)[1], "host = example.com");
    // A line on top: one block above the target, which follows it exactly.
    reload(&live, &dir, &format!("# payments\n{INI}"), 1);
    assert_eq!(
        message(&live),
        "config.ini reloaded (#1): 0 annotations kept"
    );
    assert_eq!(position_of(&live, 0), "3");
    assert_eq!(code_at(&live, 2)[2], "host = example.com");
    assert_eq!(stale_count(&live), 0);
    reload(&live, &dir, INI, 2);
    assert_eq!(position_of(&live, 0), "2");
    // Its own line edited: stale, said loudly, skipped.
    reload(&live, &dir, &host_edited(), 3);
    let (msg, loud) = live.message().unwrap();
    assert_eq!(
        msg,
        "stale move skipped (u undoes or re-add): #1 (L2 changed) · config.ini reloaded (#3): 0 annotations kept"
    );
    assert!(loud, "a move that stopped matching is said loudly");
    assert_eq!(
        stale_list(&live),
        vec!["move #1 [step 2] code.patch replace L2 1 line (stale: line 2 changed)"]
    );
    assert_eq!(stale_count(&live), 1);
    assert_eq!(code_at(&live, 2)[1], "host = db1", "step 2 applies no move");
    assert_eq!(
        live.api_ok("status", json!({}))["max_step"],
        2,
        "steps keep their numbers"
    );
    // The line back: the mark goes, the bar says so.
    reload(&live, &dir, INI, 4);
    assert_eq!(
        message(&live),
        "move #1 matches the code again · config.ini reloaded (#4): 0 annotations kept"
    );
    assert!(stale_list(&live).is_empty());
    assert!(
        !live
            .api_ok("status", json!({}))
            .as_object()
            .unwrap()
            .contains_key("stale_moves"),
        "no stale move, no field"
    );
    assert_eq!(code_at(&live, 2)[1], "host = example.com");
}

#[test]
fn a_paste_that_deletes_the_first_line_shifts_it_one_that_edits_it_marks_it_and_u_restores() {
    let (live, _dir) = replace_line_2("paste");
    let before = live.with(|app| app.doc.clone());
    paste(&live, INI.split_once('\n').unwrap().1);
    assert_eq!(message(&live), "4 lines pasted (0 annotations kept)");
    assert_eq!(position_of(&live, 0), "1");
    assert_eq!(code_at(&live, 2)[0], "host = example.com");
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(
        live.with(|app| app.doc.clone()),
        before,
        "u gives back the document"
    );
    paste(&live, &host_edited());
    let (msg, loud) = live.message().unwrap();
    assert_eq!(
        msg,
        "stale move skipped (u undoes or re-add): #1 (L2 changed) · 5 lines pasted (0 annotations kept)"
    );
    assert!(loud);
    assert_eq!(stale_count(&live), 1);
    live.keys(&[KeyCode::Char('u')]);
    let after = live.with(|app| app.doc.clone());
    assert_eq!(after, before, "u gives back the document, marks included");
    assert_eq!(stale_count(&live), 0);
    assert_eq!(code_at(&live, 2)[1], "host = example.com");
    // Redo brings the mark back with the paste.
    live.keys(&[KeyCode::Char('U')]);
    assert_eq!(stale_count(&live), 1);
}

#[test]
fn a_change_that_touches_no_target_and_an_identical_paste_say_nothing() {
    let (live, dir) = replace_line_2("quiet");
    follow(&live);
    reload(&live, &dir, &INI.replace("admin", "root"), 1);
    assert_eq!(
        message(&live),
        "config.ini reloaded (#1): 0 annotations kept"
    );
    assert_eq!(stale_count(&live), 0);
    let now = INI.replace("admin", "root");
    paste(&live, &now);
    assert_eq!(message(&live), "5 lines pasted (0 annotations kept)");
    assert_eq!(stale_count(&live), 0);
}

#[test]
fn move_list_and_mcp_list_moves_say_stale_and_why() {
    let (live, _dir) = replace_line_2("list");
    paste(&live, &host_edited());
    let ml = live.api_ok("move.list", json!({}));
    assert_eq!(ml["moves"][0]["stale"], true, "{ml}");
    assert_eq!(ml["moves"][0]["stale_reason"], "line 2 changed", "{ml}");
    assert_eq!(ml["moves"][0]["position"], "2", "the move is as written");
    assert!(
        ml["moves"][0]["summary"]
            .as_str()
            .unwrap()
            .ends_with("(stale: line 2 changed)")
    );
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, _) = mcp.call("list_moves", json!({}));
    assert!(!err, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["moves"][0]["stale"], true, "{text}");
    assert_eq!(v["moves"][0]["stale_reason"], "line 2 changed");
    let (_, text, _) = mcp.call("list_annotations", json!({}));
    assert!(text.contains("(stale: line 2 changed)"), "{text}");
    // `scene.get` does not carry the mark: it is computed, never data.
    let doc = live.api_ok("scene.get", json!({}))["document"].clone();
    assert!(!doc.to_string().contains("stale"), "{doc}");
}

// ---------------------------------------------------------------- targets

#[test]
fn a_find_that_now_matches_twice_is_stale_and_deleting_the_copy_clears_it() {
    let (live, dir) = demo(
        "find",
        INI,
        json!([{"at_step": 1, "action": "code.patch", "op": "replace", "find": "port = 8080", "content": "port = 9090"}]),
        json!([]),
        json!({}),
    );
    follow(&live);
    reload(&live, &dir, &format!("{INI}port = 8080\n"), 1);
    let s = stale_list(&live);
    assert_eq!(
        s,
        vec![
            "move #1 [step 1] code.patch replace «port = 8080» 1 line (stale: find «port = 8080» matches 2 times (L3, L6))"
        ],
    );
    reload(&live, &dir, INI, 2);
    assert!(message(&live).starts_with("move #1 matches the code again"));
    assert_eq!(code_at(&live, 1)[2], "port = 9090");
    // A find whose text is gone, and an occurrence past the matches.
    paste(&live, &INI.replace("port = 8080", "port = 1"));
    assert!(
        stale_list(&live)[0].ends_with("(stale: find «port = 8080» matches nothing in the code)"),
        "{:?}",
        stale_list(&live)
    );
}

#[test]
fn an_occurrence_past_the_matches_is_stale() {
    let (live, _dir) = demo(
        "occ",
        "a = 1\nb = 1\nc = 1\n",
        json!([{"at_step": 1, "action": "code.patch", "op": "delete", "find": "= 1", "occurrence": 3}]),
        json!([]),
        json!({}),
    );
    paste(&live, "a = 1\nb = 1\nc = 2\n");
    let s = stale_list(&live);
    assert!(
        s[0].ends_with("(stale: find «= 1»: occurrence 3 asked, only 2 matches at this step)"),
        "{s:?}"
    );
}

#[test]
fn reindenting_the_target_is_stale_whitespace_and_undoing_it_in_the_editor_clears_it() {
    let (live, dir) = replace_line_2("ws");
    follow(&live);
    reload(&live, &dir, &INI.replace("host =", "    host ="), 1);
    assert_eq!(
        stale_list(&live),
        vec!["move #1 [step 2] code.patch replace L2 1 line (stale: line 2 changed (whitespace))"]
    );
    reload(&live, &dir, INI, 2);
    assert_eq!(stale_count(&live), 0);
}

#[test]
fn line_endings_and_a_final_newline_change_nothing() {
    let (live, dir) = replace_line_2("crlf");
    follow(&live);
    // The reload reads lines as before: CRLF and a missing final newline
    // are the same code, so nothing reloads and nothing goes stale.
    save_like_zed(&dir.join("config.ini"), &INI.replace('\n', "\r\n"));
    std::thread::sleep(Duration::from_millis(700));
    save_like_zed(&dir.join("config.ini"), INI.trim_end());
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(reloads(&live), 0);
    assert_eq!(stale_count(&live), 0);
    paste(&live, &INI.replace('\n', "\r\n"));
    assert_eq!(message(&live), "5 lines pasted (0 annotations kept)");
    assert_eq!(stale_count(&live), 0);
}

#[test]
fn ranges_out_of_range_inserts_cursors_selections_and_waits() {
    let (live, _dir) = demo(
        "kinds",
        INI,
        json!([
            {"at_step": 1, "action": "code.patch", "op": "delete", "position": "4-5"},
            {"at_step": 2, "action": "code.patch", "op": "insert", "position": "3", "content": "tls = on"},
            {"at_step": 3, "action": "cursor", "line": 4},
            {"at_step": 3, "action": "select", "from": {"find": "[server]"}, "to": {"find": "8080"}},
            {"at_step": 3, "action": "wait", "ms": 10}
        ]),
        json!([]),
        json!({}),
    );
    // Only line 5 of the range 4-5 changes: the range is stale.
    paste(&live, &INI.replace("admin", "root"));
    let s = stale_list(&live);
    assert_eq!(s.len(), 1, "{s:?}");
    assert!(
        s[0].starts_with("move #1 ") && s[0].ends_with("(stale: lines 4-5 changed)"),
        "{s:?}"
    );
    // Three lines left: the range is past the end, the insert point moved
    // to the end, the cursor line is gone, the selection's text is gone;
    // a wait is never stale.
    paste(&live, "[server]\nhost = localhost\nport = 1\n");
    let ml = live.api_ok("move.list", json!({}));
    let why: Vec<Value> = ml["moves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["stale_reason"].clone())
        .collect();
    assert_eq!(
        why[0], "line 4 is past the end of the code (3 lines)",
        "{ml}"
    );
    assert_eq!(why[1], "lines 2-3 changed", "{ml}");
    assert_eq!(
        why[2], "line 4 is past the end of the code (3 lines)",
        "{ml}"
    );
    assert!(why[3].as_str().unwrap().contains("«8080»"), "{ml}");
    assert_eq!(why[4], Value::Null, "{ml}");
    assert!(
        message(&live).starts_with("4 stale moves skipped (u undoes or re-add): #1 (L4 past the end), #2 (L2-3 changed), #3 (L4 past the end), #4 (target not found)"),
        "{}",
        message(&live)
    );
}

#[test]
fn a_move_whose_target_a_stale_one_wrote_is_stale_too_and_both_clear() {
    let (live, dir) = demo(
        "chain",
        INI,
        json!([
            {"at_step": 1, "action": "code.patch", "op": "replace", "position": "3", "content": "port = 9090"},
            {"at_step": 2, "action": "code.patch", "op": "replace", "find": "port = 9090", "content": "port = 443"}
        ]),
        json!([]),
        json!({}),
    );
    follow(&live);
    reload(&live, &dir, &INI.replace("port = 8080", "port = 1"), 1);
    let ml = live.api_ok("move.list", json!({}));
    assert_eq!(ml["moves"][0]["stale_reason"], "line 3 changed");
    assert_eq!(
        ml["moves"][1]["stale_reason"],
        "find «port = 9090» matches nothing in the code"
    );
    reload(&live, &dir, INI, 2);
    assert_eq!(
        message(&live),
        "moves #1, #2 match the code again · config.ini reloaded (#2): 0 annotations kept"
    );
    assert_eq!(code_at(&live, 2)[2], "port = 443");
}

#[test]
fn an_annotation_on_text_a_stale_move_writes_is_unanchored_never_drawn_elsewhere() {
    let (live, dir) = demo(
        "ann",
        INI,
        json!([{"at_step": 2, "action": "code.patch", "op": "replace", "position": "2", "content": "host = example.com"}]),
        json!([
            {"id": "n1", "step": 2, "type": "note", "line": 2, "text": "the new host"},
            {"id": "h1", "step": 2, "type": "highlight", "span": {"line": 2, "start": 7, "end": 18}}
        ]),
        json!({}),
    );
    follow(&live);
    let drawn = |live: &Live| {
        live.api_ok("step.set", json!({"step": 2}));
        let t = live.api_ok("render.text", json!({"width": 100}))["text"]
            .as_str()
            .unwrap()
            .to_string();
        live.api_ok("step.set", json!({"step": 0}));
        t
    };
    assert!(drawn(&live).contains("the new host"));
    reload(&live, &dir, &host_edited(), 1);
    assert_eq!(stale_count(&live), 1);
    live.api_ok("step.set", json!({"step": 2}));
    let l = list(&live);
    assert!(
        l[0].ends_with("(unanchored)") && l[0].starts_with("n1"),
        "{l:?}"
    );
    assert!(
        l[1].ends_with("(unanchored)") && l[1].starts_with("h1"),
        "{l:?}"
    );
    live.api_ok("step.set", json!({"step": 0}));
    assert!(
        !drawn(&live).contains("the new host"),
        "not drawn on another line"
    );
    // Kept in the file, and back when the move matches again.
    assert_eq!(live.with(|app| app.doc.annotations.len()), 2);
    reload(&live, &dir, INI, 2);
    assert!(drawn(&live).contains("the new host"));
}

#[test]
fn a_step_whose_only_move_is_stale_is_still_stepped_through_and_types_nothing() {
    let (live, _dir) = demo(
        "typing",
        INI,
        json!([{"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "; a\n; b", "typing": "line", "speed_ms": 500}]),
        json!([]),
        json!({}),
    );
    paste(&live, &INI.replace("[server]", "[srv]"));
    assert_eq!(stale_count(&live), 1);
    live.keys(&[KeyCode::Char('n')]);
    let (step, typing) = live.with(|app| (app.doc.step, app.is_typing()));
    assert_eq!(
        (step, typing),
        (1, false),
        "the step is there; nothing types"
    );
    assert_eq!(live.with(|app| app.doc.typing_ms_at(1)), 0);
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!(live.with(|app| app.doc.step), 0);
}

#[test]
fn a_loop_says_it_once_per_change_not_once_per_round() {
    let (live, _dir) = demo(
        "loop",
        INI,
        json!([{"at_step": 2, "action": "code.patch", "op": "replace", "position": "2", "content": "host = example.com"}]),
        json!([]),
        json!({"loop": true, "steps": [
            {"step": 0, "auto_advance_ms": 100},
            {"step": 1, "auto_advance_ms": 100},
            {"step": 2, "auto_advance_ms": 100}
        ]}),
    );
    paste(&live, &host_edited());
    let notice = message(&live);
    assert!(notice.starts_with("stale move skipped"), "{notice}");
    // Three rounds of the loop: watch every message the bar shows.
    let mut seen = Vec::new();
    let mut zeros = 0;
    let mut was = 0;
    let start = Instant::now();
    while zeros < 3 {
        assert!(start.elapsed() < WAIT, "the loop did not go round");
        let (step, msg) = live.with(|app| (app.doc.step, app.message.clone()));
        if step == 0 && was != 0 {
            zeros += 1;
        }
        was = step;
        if let Some((m, _)) = msg
            && seen.last() != Some(&m)
        {
            seen.push(m);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let notices = seen.iter().filter(|m| m.contains("stale move")).count();
    assert!(notices <= 1, "{seen:?}");
    assert_eq!(stale_count(&live), 1);
}

// ---------------------------------------------------------------- files

#[test]
fn a_linked_file_changed_on_its_own_only_marks_its_own_moves() {
    let mut doc = tiza_core::fs::parse_document(
        &json!({
            "schema": 1,
            "source": {"language": "rs", "path": "/x/main.rs"},
            "code": ["fn main() {", "    run();", "}"],
            "views": {"src/queue.rs": ["struct Queue;", "impl Queue {", "    fn push() {}", "}"]},
            "moves": [
                {"at_step": 1, "action": "code.patch", "op": "replace", "position": "2", "content": "    go();"},
                {"at_step": 2, "action": "code.link", "file": "src/queue.rs"},
                {"at_step": 3, "action": "code.patch", "op": "replace", "position": "3", "content": "    fn push(&mut self) {}"}
            ]
        })
        .to_string(),
        None,
    )
    .unwrap();
    let before = doc.clone();
    doc.view_code.insert(
        "src/queue.rs".into(),
        ["struct Queue;", "impl Queue {", "    fn push(x) {}", "}"]
            .map(String::from)
            .to_vec(),
    );
    let notice = tiza_core::stale::refresh(Some(&before), &mut doc).unwrap();
    assert!(notice.loud);
    assert_eq!(
        notice.text,
        "stale move skipped (u undoes or re-add): #3 (L3 changed)"
    );
    assert!(doc.moves[0].stale.is_none(), "main.rs's move untouched");
    assert_eq!(
        doc.moves[2].stale.as_ref().unwrap().file,
        Some("src/queue.rs".to_string())
    );
    assert_eq!(doc.code_at(1).unwrap().0[1], "    go();");
}

#[test]
fn restart_render_and_check_load_strictly_and_autosave_keeps_the_moves() {
    let (live, dir) = demo(
        "strict",
        INI,
        json!([{"at_step": 1, "action": "code.patch", "op": "replace", "find": "port = 8080", "content": "port = 9090"}]),
        json!([]),
        json!({}),
    );
    let path = dir.join("config.ini.tiza");
    // Autosave of something else first, so the file is Tiza's own writing.
    live.command("title stale demo");
    let moves_of = |p: &Path| -> String {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        v["moves"].to_string()
    };
    let start = Instant::now();
    while !std::fs::read_to_string(&path)
        .unwrap()
        .contains("stale demo")
    {
        assert!(start.elapsed() < WAIT, "no autosave");
        std::thread::sleep(Duration::from_millis(20));
    }
    let before = moves_of(&path);
    follow(&live);
    reload(&live, &dir, &INI.replace("8080", "1"), 1);
    assert_eq!(stale_count(&live), 1);
    live.command("title stale demo 2");
    let start = Instant::now();
    while !std::fs::read_to_string(&path)
        .unwrap()
        .contains("stale demo 2")
    {
        assert!(start.elapsed() < WAIT, "no autosave");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(moves_of(&path), before, "the moves are saved as written");
    // Only the live instance skips it: a fresh load is strict as ever.
    let e = tiza_core::fs::open(path.to_str().unwrap())
        .err()
        .unwrap()
        .to_string();
    assert!(
        e.contains("move #1, step 1: find «port = 8080» matches nothing"),
        "{e}"
    );
    let out = tiza(&["render", path.to_str().unwrap(), "--out", "-"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("matches nothing"));
    let out = tiza(&["check", path.to_str().unwrap()]);
    assert!(!out.status.success());
    let text =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    assert!(
        text.contains("move #1, step 1") && text.contains("matches nothing"),
        "{text}"
    );
    // And the live preflight lists it the same way.
    let report = live.api_ok("check", json!({}));
    assert!(report.to_string().contains("matches nothing"), "{report}");
}

// ---------------------------------------------------------------- agents

#[test]
fn agents_add_and_delete_moves_and_get_put_round_trips_with_stale_moves() {
    let (live, dir) = replace_line_2("agents");
    follow(&live);
    reload(&live, &dir, &host_edited(), 1);
    assert_eq!(stale_count(&live), 1);
    // A put of what get gave out changes nothing and keeps the mark.
    let got = live.api_ok("scene.get", json!({}))["document"].clone();
    let rev = live.api_ok("status", json!({}))["revision"].clone();
    live.api_ok("scene.put", json!({"document": got.clone()}));
    assert_eq!(live.api_ok("status", json!({}))["revision"], rev);
    assert_eq!(stale_count(&live), 1);
    assert_eq!(live.api_ok("scene.get", json!({}))["document"], got);
    // A put that changes something else revalidates and keeps it too.
    let mut edited = got.clone();
    edited["title"] = json!("edited");
    live.api_ok("scene.put", json!({"document": edited}));
    assert_eq!(stale_count(&live), 1);
    // move.add validates on the code as it is now (the stale one skipped).
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "; added"}),
    );
    let ml = live.api_ok("move.list", json!({}));
    assert_eq!(ml["moves"][0]["stale"], true);
    assert_eq!(
        ml["moves"][0]["position"], "2",
        "a stale move is not carried"
    );
    assert_eq!(code_at(&live, 1)[0], "; added");
    // Deleting the stale move works and undoes.
    live.api_ok("move.delete", json!({"index": 1}));
    assert_eq!(stale_count(&live), 0);
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(stale_count(&live), 1);
}

// ---------------------------------------------------------------- speed

#[test]
fn a_big_file_with_thirty_moves_revalidates_without_a_pause() {
    let lines: Vec<String> = (0..3000)
        .map(|i| match i % 4 {
            0 => format!("pub fn f{i}(s: String) -> usize {{"),
            1 => format!("    let n{i} = s.len() + {i};"),
            2 => format!("    n{i}"),
            _ => "}".into(),
        })
        .collect();
    let code = lines.join("\n") + "\n";
    let moves: Vec<Value> = (0..30)
        .map(|k| {
            if k % 2 == 0 {
                json!({"at_step": k + 1, "action": "code.patch", "op": "replace", "position": format!("{}", 100 * k + 2), "content": format!("    let m{k} = 0;")})
            } else {
                json!({"at_step": k + 1, "action": "code.patch", "op": "replace", "find": format!("let n{} = ", 100 * k + 1), "content": format!("    let m{k} = 1;")})
            }
        })
        .collect();
    let (live, _dir) = demo("big", &code, json!(moves), json!([]), json!({}));
    let mut worst = Duration::ZERO;
    for round in 0..10 {
        // A line added on top and taken out again: every position follows.
        let text = if round % 2 == 0 {
            format!("// edit {round}\n{code}")
        } else {
            code.clone()
        };
        let took = live.with(move |app| {
            let t = Instant::now();
            app.replace_code(&text).unwrap();
            t.elapsed()
        });
        worst = worst.max(took);
    }
    eprintln!("stale moves, big file: worst paste + revalidation of 10: {worst:?}");
    assert!(worst < Duration::from_secs(2), "{worst:?}");
    assert_eq!(
        stale_count(&live),
        0,
        "the last paste leaves every move matching"
    );
}

// ---------------------------------------------------------------- review

/// A replace by position at step 1 and a find at step 2, both made stale
/// by one paste.
fn two_stale(name: &str) -> (Live, PathBuf) {
    let (live, dir) = demo(
        name,
        INI,
        json!([
            {"at_step": 1, "action": "code.patch", "op": "replace", "position": "2", "content": "host = example.com"},
            {"at_step": 2, "action": "code.patch", "op": "replace", "find": "user = admin", "content": "user = root"}
        ]),
        json!([]),
        json!({}),
    );
    paste(&live, &host_edited().replace("user = admin", "user = x"));
    assert_eq!(stale_count(&live), 2);
    (live, dir)
}

#[test]
fn live_exports_skip_what_the_screen_skips() {
    let (live, dir) = two_stale("export");
    let mut mcp = McpClient::start(&live.socket);
    for step in [1, 2] {
        let path = dir.join(format!("out{step}.txt"));
        let (err, text, _) = mcp.call(
            "export_image",
            json!({"path": path.to_str().unwrap(), "step": step, "width": 100}),
        );
        assert!(!err, "{text}");
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(
            !out.contains("example.com") && !out.contains("root"),
            "{out}"
        );
        assert!(
            out.contains("[server]") && out.contains("user = x"),
            "{out}"
        );
    }
    let sock = live.socket.to_str().unwrap().to_string();
    let path = dir.join("cli.txt");
    let o = tiza(&[
        "--socket",
        &sock,
        "render",
        "--step",
        "2",
        "--out",
        path.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = std::fs::read_to_string(&path).unwrap();
    assert!(
        !out.contains("example.com") && out.contains("[server]"),
        "{out}"
    );
    let got = live.api_ok("scene.get", json!({}));
    assert_eq!(got["stale"][0]["index"], 1, "{got}");
    assert!(!got["document"].to_string().contains("stale"));
}

#[test]
fn a_held_annotation_comes_back_on_what_the_move_writes_now() {
    let (live, _dir) = demo(
        "release",
        INI,
        json!([{"at_step": 1, "action": "code.patch", "op": "replace", "find": "port = 8080", "content": "port = 9090"}]),
        json!([{"id": "n1", "step": 1, "type": "note", "line": 3, "text": "new port"}]),
        json!({}),
    );
    paste(&live, &INI.replace("port = 8080", "port = 1"));
    assert_eq!(stale_count(&live), 1);
    paste(
        &live,
        "[server]\nhost = localhost\n[db]\nuser = admin\nport = 8080\n",
    );
    assert!(
        message(&live).starts_with("move #1 matches the code again"),
        "{}",
        message(&live)
    );
    let line = live.with(|app| match &app.doc.annotations[0].shape {
        tiza_core::model::Shape::Note { line, .. } => *line,
        _ => 0,
    });
    assert_eq!(line, 5, "on the line the move writes, not on [db]");
    live.api_ok("step.set", json!({"step": 1}));
    assert_eq!(list(&live), vec!["n1 [step 1] note L5 «new port»"]);
    let got = live.api_ok("scene.get", json!({}))["document"].clone();
    live.api_ok("scene.put", json!({"document": got}));
}

#[test]
fn a_put_pairs_stale_marks_by_move_not_by_place() {
    let (live, _dir) = demo(
        "pair",
        INI,
        json!([
            {"at_step": 1, "action": "code.patch", "op": "replace", "position": "2", "content": "host = example.com"},
            {"at_step": 2, "action": "code.patch", "op": "replace", "position": "4", "content": "[database]"}
        ]),
        json!([]),
        json!({}),
    );
    paste(&live, &host_edited().replace("[db]", "[d]"));
    assert_eq!(stale_count(&live), 2);
    let mut got = live.api_ok("scene.get", json!({}))["document"].clone();
    got["moves"].as_array_mut().unwrap().remove(0);
    live.api_ok("scene.put", json!({"document": got}));
    let ml = live.api_ok("move.list", json!({}));
    assert_eq!(ml["moves"][0]["stale_reason"], "line 4 changed", "{ml}");
    assert!(!code_at(&live, 2).contains(&"[database]".to_string()));
}

#[test]
fn the_live_check_lists_position_moves() {
    let (live, _dir) = replace_line_2("check");
    paste(&live, &host_edited());
    let report = live.api_ok("check", json!({}));
    assert_eq!(report["ok"], false, "{report}");
    let f = &report["findings"][0];
    assert_eq!(f["code"], "stale", "{report}");
    assert_eq!(f["id"], "move #1");
    assert!(
        f["message"].as_str().unwrap().contains("line 2 changed"),
        "{f}"
    );
}

#[test]
fn cursor_and_select_by_line_follow_their_lines_like_a_position() {
    let (live, _dir) = demo(
        "lines",
        INI,
        json!([
            {"at_step": 1, "action": "cursor", "line": 4},
            {"at_step": 1, "action": "select", "from": {"line": 5, "col": 0}, "to": {"line": 5, "col": 4}}
        ]),
        json!([]),
        json!({}),
    );
    paste(&live, &format!("# top\n{INI}"));
    let ml = live.api_ok("move.list", json!({}));
    assert_eq!(ml["moves"][0]["line"], 5, "{ml}");
    assert_eq!(ml["moves"][1]["from"]["line"], 6, "{ml}");
    assert_eq!(stale_count(&live), 0);
    live.keys(&[KeyCode::Char('u')]);
    paste(&live, &INI.replace("[db]", "[d]").replace("admin", "x"));
    let ml = live.api_ok("move.list", json!({}));
    assert_eq!(ml["moves"][0]["stale_reason"], "line 4 changed", "{ml}");
    assert_eq!(ml["moves"][1]["stale_reason"], "line 5 changed", "{ml}");
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(stale_count(&live), 0);
}

const BRACES: &str = "fn a() {\n    one();\n}\nfn b() {\n    two();\n}\n";

/// A `replace position "3"` on the `}` that closes `a`.
fn brace(name: &str) -> (Live, PathBuf) {
    demo(
        name,
        BRACES,
        json!([{"at_step": 1, "action": "code.patch", "op": "replace", "position": "3", "content": "} // end of a"}]),
        json!([]),
        json!({}),
    )
}

#[test]
fn a_reload_of_one_block_shifts_exactly_and_two_separate_edits_mark_it_even_on_the_same_text() {
    let (live, dir) = demo(
        "blocks",
        BRACES,
        json!([{"at_step": 1, "action": "code.patch", "op": "replace", "position": "3", "content": "} // end of a"}]),
        json!([]),
        json!({}),
    );
    follow(&live);
    reload(
        &live,
        &dir,
        &format!("fn z() {{\n    zero();\n}}\n{BRACES}"),
        1,
    );
    assert_eq!(position_of(&live, 0), "6");
    assert_eq!(code_at(&live, 1)[5], "} // end of a");
    reload(&live, &dir, BRACES, 2);
    assert_eq!(position_of(&live, 0), "3");
    // Line 1 and line 5 edited: line 3 is between them, its place unknown.
    reload(
        &live,
        &dir,
        &BRACES
            .replace("fn a()", "fn a2()")
            .replace("two()", "deux()"),
        3,
    );
    let ml = live.api_ok("move.list", json!({}));
    assert_eq!(
        ml["moves"][0]["stale_reason"],
        "line 3: the code around it changed, so where it went is not known",
        "{ml}"
    );
    assert_eq!(position_of(&live, 0), "3", "never moved by a guess");
    assert_eq!(code_at(&live, 1)[2], "}", "and never applied");
    // Other edits around it: still not known.
    reload(
        &live,
        &dir,
        &BRACES
            .replace("fn a()", "fn a3()")
            .replace("two()", "drei()"),
        4,
    );
    assert_eq!(stale_count(&live), 1);
    // The code exactly as it was when the move last matched (a checkout,
    // a stash, the editor's undo): the move is back, numbers and all.
    reload(&live, &dir, BRACES, 5);
    assert_eq!(stale_count(&live), 0, "{}", message(&live));
    assert!(message(&live).starts_with("move #1 matches the code again"));
    assert_eq!(live.api_ok("check", json!({}))["ok"], true);
}

#[test]
fn a_boundary_line_the_change_repeats_is_ambiguous_and_makes_the_move_stale() {
    // C2a: the common end `1`, `}` also follows the copy in fn c.
    for (position, name) in [("3", "c2a"), ("2", "c2a-body")] {
        let (live, _dir) = demo(
            name,
            "fn a() {\n    1\n}\n",
            json!([{"at_step": 1, "action": "code.patch", "op": "replace", "position": position, "content": "X"}]),
            json!([]),
            json!({}),
        );
        paste(&live, "fn a2() {\n    1\n}\nfn c() {\n    1\n}\n");
        assert_eq!(stale_count(&live), 1, "{name}: {}", message(&live));
        assert_eq!(position_of(&live, 0), position);
        assert!(!code_at(&live, 1).contains(&"X".to_string()), "{name}");
    }
    // C2c: the common start `fn a() {` is also the new function's header.
    let (live, _dir) = ab("c2c", "1");
    paste(
        &live,
        &format!(
            "fn a() {{\n    9\n}}\n{}",
            AB.replace("    2\n", "    20\n")
        ),
    );
    assert_eq!(stale_count(&live), 1, "{}", message(&live));
    assert_eq!(code_at(&live, 1)[0], "fn a() {");
    // A function duplicated above or below: either copy could be it, and
    // so could any `}` next to the change.
    let above = format!("fn a() {{\n    1\n}}\n{AB}");
    let below = format!("{AB}fn b() {{\n    2\n}}\n");
    for (code, position, after, name) in [
        (&above, "3", None, "dup-above-a"),
        (&above, "6", Some("9"), "dup-above-b"),
        (&below, "3", None, "dup-below-a"),
        (&below, "6", None, "dup-below-b"),
    ] {
        let (live, _dir) = ab(name, position);
        paste(&live, code);
        match after {
            Some(p) => {
                assert_eq!(stale_count(&live), 0, "{name}");
                assert_eq!(position_of(&live, 0), p, "{name}");
            }
            None => {
                assert_eq!(stale_count(&live), 1, "{name}");
                assert!(
                    !code_at(&live, 1).contains(&"} // end".to_string()),
                    "{name}"
                );
            }
        }
    }
    // Luis's case still follows: `fn a() {` is not in the pasted block.
    let (live, _dir) = brace("brace-still");
    paste(&live, &format!("fn z() {{\n    zero();\n}}\n{BRACES}"));
    assert_eq!(position_of(&live, 0), "6");
    assert_eq!(stale_count(&live), 0);
}

const AB: &str = "fn a() {\n    1\n}\nfn b() {\n    2\n}\n";

fn ab(name: &str, position: &str) -> (Live, PathBuf) {
    demo(
        name,
        AB,
        json!([{"at_step": 1, "action": "code.patch", "op": "replace", "position": position, "content": "} // end"}]),
        json!([]),
        json!({}),
    )
}

#[test]
fn two_edits_with_a_common_end_by_chance_mark_what_is_below_them() {
    let (live, _dir) = ab("suffix", "6");
    // `1` → `10` and a `fn c` added: only the last `}` is alike at the end.
    paste(
        &live,
        &format!(
            "{}fn c() {{\n    3\n}}\n",
            AB.replace("    1\n", "    10\n")
        ),
    );
    assert_eq!(stale_count(&live), 1, "{}", message(&live));
    assert_eq!(position_of(&live, 0), "6");
    let at1 = code_at(&live, 1);
    assert_eq!(at1[5], "}");
    assert_eq!(at1[8], "}", "fn c's brace untouched");
    // One block above the target (a line inserted in `a`): it follows.
    let (live, _dir) = ab("suffix-one", "6");
    paste(&live, &AB.replace("    1\n", "    1\n    1b\n"));
    assert_eq!(stale_count(&live), 0, "{}", message(&live));
    assert_eq!(position_of(&live, 0), "7");
    assert_eq!(code_at(&live, 1)[6], "} // end");
}

#[test]
fn a_stale_move_follows_the_change_and_is_checked_where_its_line_went() {
    let (live, _dir) = ab("stale-shift", "3");
    let broken = AB.replacen("}\n", ")\n", 1);
    paste(&live, &broken);
    assert_eq!(
        live.api_ok("move.list", json!({}))["moves"][0]["stale_reason"],
        "line 3 changed"
    );
    // A block on top: its line is now L6 and still `)`; L3 is fn z's `}`.
    let with_z = format!("fn z() {{\n    0\n}}\n{broken}");
    paste(&live, &with_z);
    assert_eq!(stale_count(&live), 1, "{}", message(&live));
    assert_eq!(
        position_of(&live, 0),
        "6",
        "it follows its line while stale"
    );
    assert_eq!(code_at(&live, 1)[2], "}", "fn z's brace untouched");
    // Its brace back: it matches again, on its own line.
    paste(&live, &format!("fn z() {{\n    0\n}}\n{AB}"));
    assert!(
        message(&live).starts_with("move #1 matches the code again"),
        "{}",
        message(&live)
    );
    let at1 = code_at(&live, 1);
    assert_eq!(at1[5], "} // end");
    assert_eq!(at1[2], "}");
}

#[test]
fn a_put_or_a_move_that_makes_a_stale_move_match_says_so_and_clears_it() {
    let (live, _dir) = demo(
        "recheck",
        "a\nb\n",
        json!([{"at_step": 2, "action": "code.patch", "op": "replace", "find": "b", "content": "B"}]),
        json!([]),
        json!({}),
    );
    paste(&live, "a\nb\nb\n");
    assert_eq!(stale_count(&live), 1);
    // A move at step 1 that deletes the copy: the find matches once again.
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "position": "3"}),
    );
    assert_eq!(stale_count(&live), 0);
    assert_eq!(message(&live), "move #1 matches the code again");
    // A put whose code makes it match again says so too.
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(stale_count(&live), 1);
    let mut got = live.api_ok("scene.get", json!({}))["document"].clone();
    got["code"] = json!(["a", "b"]);
    live.api_ok("scene.put", json!({"document": got}));
    assert_eq!(stale_count(&live), 0);
    assert!(
        message(&live).starts_with("move #1 matches the code again"),
        "{}",
        message(&live)
    );
}

#[test]
fn at_120_columns_the_bar_shows_the_hint_and_the_first_move() {
    let (live, _dir) = demo(
        "bar",
        BRACES,
        json!([
            {"at_step": 1, "action": "code.patch", "op": "replace", "position": "3", "content": "X"},
            {"at_step": 1, "action": "code.patch", "op": "replace", "find": "two();", "content": "Y"}
        ]),
        json!([]),
        json!({}),
    );
    paste(
        &live,
        &BRACES
            .replace("fn a()", "fn a2()")
            .replace("two()", "deux()"),
    );
    let msg = message(&live);
    assert!(
        msg.starts_with("2 stale moves skipped (u undoes or re-add): #1 (L3 place unknown), #2 (target not found)"),
        "{msg}"
    );
    let screen = live.screen(120, 30);
    let bar = screen.lines().last().unwrap();
    assert!(
        bar.contains("(u undoes or re-add): #1 (L3 place unknown)"),
        "{bar}"
    );
    // The whole reason stays in move.list.
    assert_eq!(
        live.api_ok("move.list", json!({}))["moves"][0]["stale_reason"],
        "line 3: the code around it changed, so where it went is not known"
    );
}

// ---------------------------------------------------------------- rework 1

/// The card's INI with three moves on line numbers, followed.
fn three_by_line(name: &str) -> (Live, PathBuf) {
    let (live, dir) = demo(
        name,
        INI,
        json!([
            {"at_step": 1, "action": "code.patch", "op": "replace", "position": "2", "content": "host = a"},
            {"at_step": 2, "action": "code.patch", "op": "replace", "position": "3", "content": "port = 1"},
            {"at_step": 3, "action": "code.patch", "op": "replace", "position": "5", "content": "user = b"}
        ]),
        json!([]),
        json!({}),
    );
    follow(&live);
    (live, dir)
}

#[test]
fn with_follow_at_120_columns_the_bar_names_every_stale_move() {
    let (live, dir) = three_by_line("bar-follow");
    // A first reload, so the bar has a time of the last reload to show.
    reload(&live, &dir, &INI.replace("[db]", "[database]"), 1);
    reload(
        &live,
        &dir,
        &INI.replace("localhost", "h")
            .replace("8080", "1")
            .replace("admin", "x"),
        2,
    );
    assert_eq!(stale_count(&live), 3);
    let screen = live.screen(120, 30);
    let bar = screen.lines().last().unwrap();
    for n in ["#1 (", "#2 (", "#3 ("] {
        assert!(bar.contains(n), "{n} missing: {bar}");
    }
    assert!(
        !bar.contains("last "),
        "no reload time while the notice is up: {bar}"
    );
    assert!(bar.contains("following"), "{bar}");
}

#[test]
fn quitting_with_stale_moves_is_refused_until_q_bang() {
    let (live, _dir) = demo(
        "quit",
        INI,
        json!([
            {"at_step": 1, "action": "code.patch", "op": "replace", "position": "2", "content": "host = a"},
            {"at_step": 2, "action": "code.patch", "op": "replace", "find": "user = admin", "content": "user = b"}
        ]),
        json!([]),
        json!({}),
    );
    paste(&live, &INI.replace("localhost", "h"));
    live.command("q");
    let (msg, loud) = live.message().unwrap();
    assert_eq!(
        msg,
        "1 stale move (#1): :q! quits anyway; next time it applies on line 2 as written"
    );
    assert!(loud);
    assert!(!live.with(|app| app.quit));
    live.keys(&[KeyCode::Char('q')]);
    assert!(!live.with(|app| app.quit), "the q key asks the same");
    live.command("wq");
    assert!(
        !live.with(|app| app.quit),
        ":wq saves but does not close over it"
    );
    paste(&live, &INI.replace("localhost", "h").replace("admin", "x"));
    live.command("q");
    assert_eq!(
        message(&live),
        "2 stale moves (#1, #2): :q! quits anyway; next time #1 applies on line 2 as written, #2 stops the document from opening"
    );
    live.command("q!");
    assert!(live.with(|app| app.quit));
    // What `:q!` and Ctrl-C print on stderr once the terminal is restored.
    assert_eq!(
        live.with(|app| tiza_core::stale::close_summary(&app.doc)),
        Some("2 stale moves (#1, #2): next time #1 applies on line 2 as written, #2 stops the document from opening".into())
    );
}

#[test]
fn the_refusal_to_quit_keeps_the_way_out_at_120_columns_with_follow() {
    let (live, dir) = demo(
        "quit-bar",
        "fn main() {\n    let a = 1;\n    let b = 2;\n    show(a, b);\n    done();\n}\n",
        json!([
            {"at_step": 1, "action": "code.patch", "op": "replace", "position": "2", "content": "    let a = 10;"},
            {"at_step": 2, "action": "code.patch", "op": "replace", "find": "let b = 2;", "content": "    let b = 20;"},
            {"at_step": 3, "action": "cursor", "line": 5}
        ]),
        json!([]),
        json!({}),
    );
    follow(&live);
    reload(
        &live,
        &dir,
        "fn main() {\n    let a = 3;\n    let b = 4;\n    show(a, b);\n    finish();\n}\n",
        1,
    );
    assert_eq!(stale_count(&live), 3);
    live.command("q");
    let bar = live.screen(120, 30).lines().last().unwrap().to_string();
    assert!(
        bar.contains("3 stale moves (#1, #2, #3): :q! quits anyway; next time #1"),
        "{bar}"
    );
    assert!(!bar.contains("last "), "{bar}");
}

#[test]
fn the_live_check_calls_a_stale_find_stale() {
    let (live, _dir) = demo(
        "check-find",
        "x = 1\nx + 1\n",
        json!([{"at_step": 1, "action": "code.patch", "op": "replace", "find": "x + 1", "content": "x + 2"}]),
        json!([]),
        json!({}),
    );
    paste(&live, "x = 1\nx + 3\n");
    let report = live.api_ok("check", json!({}));
    let mine: Vec<&Value> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["id"] == "move #1")
        .collect();
    assert_eq!(mine.len(), 1, "{report}");
    assert_eq!(mine[0]["code"], "stale", "{report}");
    let msg = mine[0]["message"].as_str().unwrap();
    assert!(
        msg.contains("the code changed under it")
            && msg.contains("Fix it before closing Tiza: a fresh load would refuse the document"),
        "{msg}"
    );
    assert_eq!(report["ok"], false);
}

#[test]
fn an_insert_at_the_end_follows_when_the_last_line_is_sure() {
    // Lines 1-3 indented; undoing it makes them repeat lines 4-6.
    let indented = "    a\n    b\n    c\na\nb\nc\nx\ny\n";
    let (live, _dir) = demo(
        "append",
        indented,
        json!([
            {"at_step": 1, "action": "code.patch", "op": "insert", "position": "9", "content": "z"},
            {"at_step": 2, "action": "cursor", "line": 8}
        ]),
        json!([]),
        json!({}),
    );
    paste(&live, "a\nb\nc\na\nb\nc\nx\ny\n");
    assert_eq!(stale_count(&live), 0, "{}", message(&live));
    assert_eq!(position_of(&live, 0), "9");
    assert_eq!(code_at(&live, 1)[8], "z");
}
