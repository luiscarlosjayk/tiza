//! t-164 — A reload or a paste that shifts the lines under a move marks it
//! (stale) and skips it, with a notice; it never moves it in silence.
//!
//! Acceptance criteria:
//! - AC1 When the step-0 code of a file changes under its moves (a follow
//!   reload, or a paste at a step where no patch of that file is applied),
//!   every move is checked again on the new code. A move is stale when the
//!   lines of its `position` target no longer hold the text they held before
//!   the change (out of range counts), or when a `find`/`select`/`rebind`
//!   target now matches nothing or more than once. No approximate moving of
//!   moves (ADR 0007 §3e).
//! - AC2 A stale move is skipped when stepping (steps keep their numbers),
//!   shows as `(stale: <why>)` in `scene.list`, carries `stale: true` and
//!   `stale_reason` in `move.list` and MCP `list_moves`; `status` says how
//!   many moves are stale.
//! - AC3 One notice per change names the stale moves («1 move no longer
//!   matches the code: #1 (line 2 changed); it is skipped — fix it before
//!   the next run»). A change that leaves every move matching says nothing
//!   new; one that makes a move match again says so («move #1 matches the
//!   code again»).
//! - AC4 Nothing is deleted: the `.tiza` keeps the moves as they were; a
//!   later reload, paste or undo that makes a move match again clears the
//!   mark; undoing the reload or paste restores the exact previous state.
//! - AC6 This file; CHANGELOG; ADR 0007 §7.
//!
//! (AC5 and the redaction steps of the card went with the redaction, t-183.)
//!
//! Follow only watches the main file today (watching a linked file is
//! t-106), so the linked-file case checks `tiza_core::stale` directly with
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

#[test]
fn ac1_a_reload_that_inserts_above_a_position_move_marks_it_stale_and_fixing_it_clears_it() {
    let (live, dir) = replace_line_2("t164-ac1");
    follow(&live);
    assert_eq!(code_at(&live, 2)[1], "host = example.com");
    // Step 1: a line on top. The text of line 2 is now line 3: stale, not followed.
    reload(&live, &dir, &format!("# payments\n{INI}"), 1);
    let (msg, loud) = live.message().unwrap();
    assert_eq!(
        msg,
        "1 move no longer matches the code: #1 (line 2 changed); it is skipped — fix it before the next run · config.ini reloaded (#1): 0 annotations kept"
    );
    assert!(loud, "a move that stopped matching is said loudly");
    assert_eq!(
        stale_list(&live),
        vec!["move #1 [step 2] code.patch replace L2 1 line (stale: line 2 changed)"]
    );
    assert_eq!(stale_count(&live), 1);
    let at2 = code_at(&live, 2);
    assert_eq!(at2[1], "[server]", "step 2 applies no move");
    assert_eq!(at2[2], "host = localhost");
    assert_eq!(
        live.api_ok("status", json!({}))["max_step"],
        2,
        "steps keep their numbers"
    );
    // Step 2: take the line out again: the mark goes, the bar says so.
    reload(&live, &dir, INI, 2);
    assert_eq!(
        message(&live),
        "move #1 matches the code again · config.ini reloaded (#2): 0 annotations kept"
    );
    assert!(stale_list(&live).is_empty());
    assert_eq!(stale_count(&live), 0);
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
fn ac3_a_paste_that_deletes_the_first_line_marks_it_and_u_restores_the_exact_state() {
    let (live, _dir) = replace_line_2("t164-paste");
    let before = live.with(|app| app.doc.clone());
    paste(&live, INI.split_once('\n').unwrap().1);
    let (msg, loud) = live.message().unwrap();
    assert!(
        msg.starts_with(
            "1 move no longer matches the code: #1 (line 2 changed); it is skipped — fix it before the next run · 4 lines pasted"
        ),
        "{msg}"
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
fn corner_a_change_that_touches_no_target_and_an_identical_paste_say_nothing() {
    let (live, dir) = replace_line_2("t164-quiet");
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
fn ac2_move_list_and_mcp_list_moves_say_stale_and_why() {
    let (live, _dir) = replace_line_2("t164-list");
    paste(&live, &format!("# payments\n{INI}"));
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
    let doc = live.api_ok("scene.get", json!({}));
    assert!(!doc.to_string().contains("stale"), "{doc}");
}

// ---------------------------------------------------------------- targets

#[test]
fn corner_a_find_that_now_matches_twice_is_stale_and_deleting_the_copy_clears_it() {
    let (live, dir) = demo(
        "t164-find",
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
            "move #1 [step 1] code.patch replace «port = 8080» 1 line (stale: find «port = 8080» matches 2 times (L3, L6); a move target must match exactly once (add occurrence: N))"
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
fn corner_an_occurrence_past_the_matches_is_stale() {
    let (live, _dir) = demo(
        "t164-occ",
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
fn corner_reindenting_the_target_is_stale_whitespace_and_undoing_it_in_the_editor_clears_it() {
    let (live, dir) = replace_line_2("t164-ws");
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
fn corner_line_endings_and_a_final_newline_change_nothing() {
    let (live, dir) = replace_line_2("t164-crlf");
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
fn corner_ranges_out_of_range_inserts_cursors_selections_and_waits() {
    let (live, _dir) = demo(
        "t164-kinds",
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
    assert!(
        why[2]
            .as_str()
            .unwrap()
            .starts_with("line 4 does not exist"),
        "{ml}"
    );
    assert!(why[3].as_str().unwrap().contains("«8080»"), "{ml}");
    assert_eq!(why[4], Value::Null, "{ml}");
    assert!(
        message(&live).starts_with("4 moves no longer match the code: #1 (line 4 is past the end of the code (3 lines)), #2 (lines 2-3 changed), #3 (line 4 does not exist"),
        "{}",
        message(&live)
    );
}

#[test]
fn corner_a_move_whose_target_a_stale_one_wrote_is_stale_too_and_both_clear() {
    let (live, dir) = demo(
        "t164-chain",
        INI,
        json!([
            {"at_step": 1, "action": "code.patch", "op": "replace", "position": "3", "content": "port = 9090"},
            {"at_step": 2, "action": "code.patch", "op": "replace", "find": "port = 9090", "content": "port = 443"}
        ]),
        json!([]),
        json!({}),
    );
    follow(&live);
    reload(&live, &dir, &format!("; top\n{INI}"), 1);
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
fn corner_an_annotation_on_text_a_stale_move_writes_is_unanchored_never_drawn_elsewhere() {
    let (live, dir) = demo(
        "t164-ann",
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
    reload(&live, &dir, &format!("# payments\n{INI}"), 1);
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
fn corner_a_step_whose_only_move_is_stale_is_still_stepped_through_and_types_nothing() {
    let (live, _dir) = demo(
        "t164-typing",
        INI,
        json!([{"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "; a\n; b", "typing": "line", "speed_ms": 500}]),
        json!([]),
        json!({}),
    );
    paste(&live, &format!("; top\n{INI}"));
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
fn corner_a_loop_says_it_once_per_change_not_once_per_round() {
    let (live, _dir) = demo(
        "t164-loop",
        INI,
        json!([{"at_step": 2, "action": "code.patch", "op": "replace", "position": "2", "content": "host = example.com"}]),
        json!([]),
        json!({"loop": true, "steps": [
            {"step": 0, "auto_advance_ms": 100},
            {"step": 1, "auto_advance_ms": 100},
            {"step": 2, "auto_advance_ms": 100}
        ]}),
    );
    paste(&live, &format!("# payments\n{INI}"));
    let notice = message(&live);
    assert!(notice.contains("no longer matches"), "{notice}");
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
    let notices = seen
        .iter()
        .filter(|m| m.contains("no longer match"))
        .count();
    assert!(notices <= 1, "{seen:?}");
    assert_eq!(stale_count(&live), 1);
}

// ---------------------------------------------------------------- files

#[test]
fn corner_a_linked_file_changed_on_its_own_only_marks_its_own_moves() {
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
        [
            "// queue",
            "struct Queue;",
            "impl Queue {",
            "    fn push() {}",
            "}",
        ]
        .map(String::from)
        .to_vec(),
    );
    let notice = tiza_core::stale::revalidate(Some(&before), &mut doc).unwrap();
    assert_eq!(
        notice,
        "1 move no longer matches the code: #3 (line 3 changed); it is skipped — fix it before the next run"
    );
    assert!(doc.moves[0].stale.is_none(), "main.rs's move untouched");
    assert_eq!(
        doc.moves[2].stale.as_ref().unwrap().file,
        Some("src/queue.rs".to_string())
    );
    assert_eq!(doc.code_at(1).unwrap().0[1], "    go();");
}

#[test]
fn corner_restart_render_and_check_load_strictly_and_autosave_keeps_the_moves() {
    let (live, dir) = demo(
        "t164-strict",
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
fn corner_agents_add_and_delete_moves_and_get_put_round_trips_with_stale_moves() {
    let (live, dir) = replace_line_2("t164-agents");
    follow(&live);
    reload(&live, &dir, &format!("# payments\n{INI}"), 1);
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
fn corner_a_big_file_with_thirty_moves_revalidates_without_a_pause() {
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
    let (live, _dir) = demo("t164-big", &code, json!(moves), json!([]), json!({}));
    let mut worst = Duration::ZERO;
    for round in 0..10 {
        let text = if round % 2 == 0 {
            format!("// edit {round}\n{code}")
        } else {
            code.replace("    n2998", &format!("    n2998 + {round}"))
        };
        let took = live.with(move |app| {
            let t = Instant::now();
            app.replace_code(&text).unwrap();
            t.elapsed()
        });
        worst = worst.max(took);
    }
    eprintln!("t-164 big file: worst paste + revalidation of 10: {worst:?}");
    assert!(worst < Duration::from_secs(2), "{worst:?}");
    assert_eq!(
        stale_count(&live),
        0,
        "the last paste leaves every move matching"
    );
}
