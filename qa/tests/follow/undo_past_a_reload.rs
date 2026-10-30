//! With follow on, `u` past a reload brings back the old code: the
//! screen must not claim to follow a file that holds other code, and the
//! autosave must not pair that code's annotations with the file.
//!
//! Acceptance criteria:
//! - AC1 With follow on, an undo or a redo that puts on screen code the
//!   followed file does not hold turns follow off and says so once, in
//!   the words of a paste (`follow off: the code on screen is not main.rs; :follow
//!   on reloads main.rs · undone: …`); `follow.status` gives `enabled:
//!   false` and that `reason`. An undo that lands on the file's code leaves
//!   follow on.
//! - AC2 `:follow on` from there reloads the file as a normal reload (one
//!   undo entry), so `u` shows that older code again and says so again.
//! - AC3 Reopening such a document: `:follow on` reloads the file it was
//!   following. Not covered here: it needs the sidecar to remember the
//!   file, a format change waiting for a decision.
//! - AC5 The save after such an undo never pairs the annotations and moves
//!   made for the code on screen with a file that holds other code: the
//!   code is saved embedded, as after a paste.
//!
//! Corner cases: a redo forward onto the file's code leaves follow off and
//! says how to turn it on; with moves applied the undo is embedded and
//! follow goes off at once, and `:follow on` waits for step 0; a file
//! removed while follow is off: `:follow on` waits for it, and when it
//! comes back it reloads (also after a paste); an undo of a paste after the
//! file changed on disk is embedded too.
//!
//! Adversarial review (`rev_*`): `scene.get` → `scene.put` after such an
//! undo, or after a paste, keeps the code embedded; an undo while follow
//! waits for a removed file keeps the file to follow; `:follow off` keeps
//! it too, so `:follow on` reloads it; the `reason` describes the screen
//! after every change (a paste on top, the file edited back, a redo back to
//! the paste).
//!
//! After every step the live instance is what a fresh load of its autosave
//! gives: the same `check` report and the same `render --all-steps`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_core::Document;
use tiza_qa::{Live, scratch};

const CODE: &str = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
const SEVEN: &str = "fn main() {\n    let x = 7;\n    println!(\"{x}\");\n}\n";
const NOTICE: &str = "follow off: the code on screen is not main.rs; :follow on reloads main.rs";
const REASON: &str = "the code on screen is not main.rs; :follow on reloads main.rs";

const WAIT: Duration = Duration::from_secs(10);

/// A source file next to its sidecar, opened by reference, following.
fn followed(name: &str, code: &str) -> (Live, PathBuf, PathBuf) {
    let dir = scratch(name);
    let src = std::path::absolute(dir.join("main.rs")).unwrap();
    std::fs::write(&src, code).unwrap();
    let mut doc = Document::from_code(code, "rs", Some(src.display().to_string()));
    doc.reference_file(src.display().to_string(), None);
    doc.step = 0;
    let tiza = dir.join("main.rs.tiza");
    let live = Live::open(doc, tiza.clone(), dir.join("live.sock"));
    live.command("follow on");
    assert_eq!(live.message().unwrap().0, "following main.rs");
    (live, src, tiza)
}

fn status(live: &Live) -> Value {
    live.api_ok("follow.status", json!({}))
}

fn code(live: &Live) -> Vec<String> {
    live.with(|app| app.doc.code.clone())
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn message(live: &Live) -> String {
    live.message().unwrap_or_default().0
}

fn bar(live: &Live) -> String {
    let screen = live.screen(160, 30);
    screen.lines().last().unwrap_or_default().to_string()
}

fn reloads(live: &Live) -> u64 {
    status(live)["reloads"].as_u64().unwrap_or(0)
}

fn wait_reloads(live: &Live, n: u64) {
    let start = Instant::now();
    while reloads(live) < n {
        assert!(start.elapsed() < WAIT, "timed out waiting for reload #{n}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Saves like an editor: a temp file renamed over the target.
fn save(path: &Path, content: &str) {
    let tmp = path.with_extension("rs.tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

fn check_file(path: &Path) -> (i32, Value) {
    let out = tiza_qa::command()
        .args(["check", "--json"])
        .arg(path)
        .output()
        .unwrap();
    let v = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)));
    (out.status.code().unwrap_or(-1), v)
}

/// `tiza render SOURCE --all-steps --format txt` (SOURCE: a file, or the
/// live instance with `--socket`): one text per step.
fn all_steps(args: &[&str], out: &Path) -> Vec<String> {
    let _ = std::fs::remove_dir_all(out);
    let r = tiza_qa::command()
        .arg("render")
        .args(args)
        .args(["--all-steps", "--format", "txt", "--width", "100", "--out"])
        .arg(format!("{}/", out.display()))
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "render {args:?}: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    let mut files: Vec<PathBuf> = std::fs::read_dir(out)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    files
        .iter()
        .map(|f| std::fs::read_to_string(f).unwrap())
        .collect()
}

/// The live instance is what a fresh load of its autosaved `.tiza` gives:
/// the same `check` report (and `tiza check` exits 0) and the same
/// `render --all-steps`.
fn same_as_fresh(live: &Live, tiza: &Path, what: &str) {
    live.with(|app| app.finish_typing());
    assert!(
        live.with(|app| !app.dirty),
        "{what}: the autosave did not run"
    );
    let mut live_report = live.api_ok("check", json!({}));
    let (rc, mut disk_report) = check_file(tiza);
    assert_eq!(rc, 0, "{what}: tiza check on the autosave: {disk_report}");
    for r in [&mut live_report, &mut disk_report] {
        r.as_object_mut().unwrap().remove("narrower");
    }
    // The live reply also names the revision it checked.
    live_report.as_object_mut().unwrap().remove("revision");
    assert_eq!(live_report, disk_report, "{what}: live check vs fresh load");
    let dir = tiza.parent().unwrap();
    let sock = live.socket.to_str().unwrap().to_string();
    let live_steps = all_steps(&["--socket", &sock], &dir.join("render-live"));
    let disk_steps = all_steps(&[tiza.to_str().unwrap()], &dir.join("render-disk"));
    assert_eq!(live_steps, disk_steps, "{what}: render --all-steps");
}

/// What the autosave holds as `code`.
fn saved_code(tiza: &Path) -> Value {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(tiza).unwrap()).unwrap();
    v["code"].clone()
}

/// Follow on, a highlight on the `1` of `x = 1`, and a save of main.rs
/// with `x = 7`: the reload drops the highlight, so it is an undo step.
fn reloaded_over_a_highlight(name: &str) -> (Live, PathBuf, PathBuf) {
    let (live, src, tiza) = followed(name, CODE);
    live.command("hl 2:1 one");
    assert_eq!(live.with(|app| app.doc.annotations.len()), 1);
    save(&src, SEVEN);
    wait_reloads(&live, 1);
    assert_eq!(code(&live), lines(SEVEN));
    assert_eq!(live.with(|app| app.doc.annotations.len()), 0);
    same_as_fresh(&live, &tiza, "after the reload");
    (live, src, tiza)
}

#[test]
fn u_past_the_reload_turns_follow_off_once_and_saves_embedded() {
    let (live, _src, tiza) = reloaded_over_a_highlight("step1");
    assert!(bar(&live).contains("⟳ following"), "{}", bar(&live));

    // First `u`: the reload. x = 1 and the highlight are back; main.rs
    // holds x = 7, so follow goes off and says so.
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(live.with(|app| app.doc.annotations.len()), 1);
    assert_eq!(message(&live), format!("{NOTICE} · undone: reload #1"));
    let st = status(&live);
    assert_eq!(st["enabled"], false, "{st}");
    assert_eq!(st["reason"], REASON, "{st}");
    let b = bar(&live);
    assert!(!b.contains("following") && b.contains(NOTICE), "{b}");
    live.command("follow");
    assert_eq!(message(&live), format!("follow off: {REASON}"));
    // AC5: saved embedded, never `code: {file}` with x = 1's highlight.
    assert_eq!(saved_code(&tiza)[1], "    let x = 1;");
    same_as_fresh(&live, &tiza, "after u of the reload");

    // Second `u`: the highlight. Still x = 1 and not main.rs's; nothing
    // new to say about follow (said once).
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(live.with(|app| app.doc.annotations.len()), 0);
    let msg = message(&live);
    assert!(msg.starts_with("undone: "), "{msg}");
    assert!(!msg.contains("follow"), "{msg}");
    assert_eq!(status(&live)["enabled"], false);
    assert_eq!(status(&live)["reason"], REASON);
    assert!(saved_code(&tiza).is_array(), "{}", saved_code(&tiza));
    same_as_fresh(&live, &tiza, "after the second u");

    // A save in the editor does not reach the screen: follow is off, and
    // no watcher is left to read it.
    assert!(live.follow_seen().is_none(), "a watcher is left");
    save(&tiza.with_file_name("main.rs"), &SEVEN.replace('7', "9"));
    assert_eq!(code(&live), lines(CODE));
}

#[test]
fn u_of_a_note_from_before_a_harmless_reload_turns_follow_off() {
    // A reload that moves no annotation leaves no undo step: the note's own
    // undo step holds the code from before it.
    let (live, src, tiza) = followed("harmless", CODE);
    live.command("note 1 entry");
    save(&src, SEVEN);
    wait_reloads(&live, 1);
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(message(&live), format!("{NOTICE} · undone: note a1"));
    assert_eq!(status(&live)["enabled"], false);
    same_as_fresh(&live, &tiza, "after u of the note");
    // `u` names what it undid for a `:clear` too (moved here from the reload on save tests).
    live.command("note 1 one");
    live.command("note 2 two");
    live.command("clear");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(message(&live), "undone: 2 annotations cleared");
}

#[test]
fn follow_on_reloads_as_one_undo_step_and_u_turns_it_off_again() {
    let (live, _src, tiza) = reloaded_over_a_highlight("step2");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(status(&live)["enabled"], false);

    live.command("follow on");
    assert_eq!(message(&live), "following main.rs");
    assert_eq!(code(&live), lines(SEVEN));
    let st = status(&live);
    assert_eq!(st["enabled"], true, "{st}");
    assert!(st.get("reason").is_none(), "{st}");
    assert!(!live.with(|app| app.doc.is_embedded()));
    same_as_fresh(&live, &tiza, "after :follow on");

    // One undo step: `u` gives x = 1 back, and follow goes off again.
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(message(&live), format!("{NOTICE} · undone: reload #1"));
    assert_eq!(status(&live)["enabled"], false);
    assert_eq!(status(&live)["reason"], REASON);
    same_as_fresh(&live, &tiza, "after u of :follow on");
}

#[test]
fn u_of_a_note_made_after_the_reload_keeps_following() {
    let (live, src, tiza) = followed("step3", CODE);
    save(&src, SEVEN);
    wait_reloads(&live, 1);
    live.command("note 1 entry");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(message(&live), "undone: note a1");
    assert_eq!(code(&live), lines(SEVEN));
    let st = status(&live);
    assert_eq!(st["enabled"], true, "{st}");
    assert!(!live.with(|app| app.doc.is_embedded()));
    assert!(saved_code(&tiza).is_object(), "{}", saved_code(&tiza));
    same_as_fresh(&live, &tiza, "after u of the later note");
    // And it keeps following.
    save(&src, CODE);
    wait_reloads(&live, 2);
    assert_eq!(code(&live), lines(CODE));
}

/// a1 highlights `t.len()` at step 1; the step-2 patch rewrites line 3 and
/// its `rebind` puts a1 on `t.len()` after it (the case QA 1 reproduced).
const REBIND: &str = r#"{
  "schema": 1,
  "source": { "path": "main.rs", "language": "rs", "first_line": 1 },
  "code": { "file": "main.rs" },
  "step": 0,
  "moves": [
    { "at_step": 2, "action": "code.patch", "op": "replace", "find": "n + 1", "content": "    n * 2",
      "rebind": { "a1": { "at": { "find": "t.len()" } } } }
  ],
  "annotations": [
    { "id": "a1", "step": 1, "type": "highlight", "span": { "line": 2, "start": 12, "end": 19 }, "label": "length" }
  ]
}"#;

const REBIND_SRC: &str = "fn f(t: &str) -> usize {\n    let n = t.len();\n    n + 1\n}\n";

/// The rebind document opened by reference to its main.rs, following.
fn rebind_followed(name: &str) -> (Live, PathBuf, PathBuf) {
    let dir = scratch(name);
    let src = std::path::absolute(dir.join("main.rs")).unwrap();
    std::fs::write(&src, REBIND_SRC).unwrap();
    let tiza = dir.join("main.rs.tiza");
    std::fs::write(&tiza, REBIND).unwrap();
    let doc = tiza_core::fs::load_document(&tiza).expect("it opens");
    let live = Live::open(doc, tiza.clone(), dir.join("live.sock"));
    live.command("follow on");
    assert_eq!(message(&live), "following main.rs");
    (live, src, tiza)
}

#[test]
fn u_after_a_reload_that_dropped_a_rebound_annotation_saves_a_clean_file() {
    let (live, src, tiza) = rebind_followed("step5");

    save(&src, &REBIND_SRC.replace("t.len()", "t.chars().count()"));
    wait_reloads(&live, 1);
    assert_eq!(live.with(|app| app.doc.annotations.len()), 0);
    assert!(
        message(&live).contains("move #1's rebind of a1 is now ignored"),
        "{}",
        message(&live)
    );

    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(live.with(|app| app.doc.annotations.len()), 1);
    assert_eq!(code(&live)[1], "    let n = t.len();");
    assert!(message(&live).starts_with(NOTICE), "{}", message(&live));
    // The autosave opens, checks clean and draws a1 on `t.len()`.
    assert!(saved_code(&tiza).is_array(), "{}", saved_code(&tiza));
    let fresh = tiza_core::fs::load_document(&tiza).expect("the autosave opens");
    assert_eq!(fresh.code[1], "    let n = t.len();");
    same_as_fresh(&live, &tiza, "after u of the reload");
    live.command("step 1");
    let screen = live.screen(100, 12);
    assert!(screen.contains("let n = t.len();"), "{screen}");
    assert!(screen.contains("length"), "{screen}");
}

#[test]
fn redo_onto_the_files_code_leaves_follow_off_and_says_how() {
    let (live, src, tiza) = reloaded_over_a_highlight("redo");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    live.keys(&[crossterm::event::KeyCode::Char('U')]);
    assert_eq!(code(&live), lines(SEVEN));
    assert_eq!(
        message(&live),
        "redone: reload #1; follow is off: :follow on reloads main.rs"
    );
    let st = status(&live);
    assert_eq!(st["enabled"], false, "{st}");
    assert!(st.get("reason").is_none(), "{st}");
    // The file's code again: a reference again.
    assert!(saved_code(&tiza).is_object(), "{}", saved_code(&tiza));
    same_as_fresh(&live, &tiza, "after U");
    // Follow stays off until the author turns it on: no watcher is left.
    assert!(live.follow_seen().is_none(), "a watcher is left");
    save(&src, CODE);
    assert_eq!(code(&live), lines(SEVEN));
    // Now main.rs holds x = 1 again: `u` to x = 1 shows what the file
    // holds (kept embedded, as the undo step was).
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(CODE));
    same_as_fresh(&live, &tiza, "u onto what the file holds now");
    // And a redo onto x = 7, which main.rs no longer holds: embedded.
    live.keys(&[crossterm::event::KeyCode::Char('U')]);
    assert_eq!(code(&live), lines(SEVEN));
    assert!(live.with(|app| app.doc.is_embedded()));
    assert_eq!(status(&live)["reason"], REASON);
    same_as_fresh(&live, &tiza, "U onto what the file no longer holds");
}

#[test]
fn moves_applied_the_undo_is_embedded_and_follow_on_waits_for_step_0() {
    let (live, src, tiza) = followed("moves", CODE);
    live.api_ok(
        "move.add",
        json!({"at_step": 2, "action": "code.patch", "op": "insert", "position": "4",
               "content": "    // printed"}),
    );
    live.command("step 0");
    live.command("hl 2:1 one");
    live.command("step 0");
    save(&src, SEVEN);
    wait_reloads(&live, 1);
    assert_eq!(code(&live), lines(SEVEN));
    same_as_fresh(&live, &tiza, "after the reload");

    live.command("step 2");
    assert_eq!(code(&live)[3], "    // printed");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(live.with(|app| app.doc.base()[1].clone()), "    let x = 1;");
    assert_eq!(code(&live)[3], "    // printed");
    assert_eq!(message(&live), format!("{NOTICE} · undone: reload #1"));
    assert_eq!(status(&live)["enabled"], false);
    same_as_fresh(&live, &tiza, "u at step 2");

    // Follow turns on and waits for a step with no move applied (ADR 0007
    // §7; it used to be refused): the file loads over the undone code there.
    live.command("follow on");
    assert_eq!(message(&live), "following main.rs; step 1 loads a save");
    assert_eq!(status(&live)["enabled"], true);
    assert_eq!(code(&live)[3], "    // printed");
    same_as_fresh(&live, &tiza, ":follow on at step 2");
    live.command("step 0");
    live.command("follow on");
    assert_eq!(code(&live), lines(SEVEN));
    assert_eq!(status(&live)["enabled"], true);
    same_as_fresh(&live, &tiza, ":follow on at step 0");
}

#[test]
fn a_file_removed_while_follow_is_off_reloads_when_it_comes_back() {
    let (live, src, tiza) = reloaded_over_a_highlight("removed");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert!(live.with(|app| app.doc.is_embedded()));
    std::fs::remove_file(&src).unwrap();
    live.command("follow on");
    wait_for("the file reported missing", || {
        status(&live)["missing"] == true
    });
    assert!(
        bar(&live).contains("following (file removed; waiting)"),
        "{}",
        bar(&live)
    );
    // Still the older code, saved embedded.
    assert_eq!(code(&live), lines(CODE));
    same_as_fresh(&live, &tiza, "while main.rs is missing");

    // The file comes back: a normal reload, one undo step.
    save(&src, SEVEN);
    wait_reloads(&live, 1);
    assert_eq!(code(&live), lines(SEVEN));
    assert_eq!(status(&live)["missing"], false);
    assert!(!live.with(|app| app.doc.is_embedded()));
    same_as_fresh(&live, &tiza, "after the file came back");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(status(&live)["enabled"], false);
}

#[test]
fn u_of_a_paste_after_the_file_changed_is_embedded() {
    let (live, src, tiza) = followed("paste-u", CODE);
    live.with(|app| app.replace_code("fn other() {}\n").unwrap());
    // The paste turned follow off: no watcher is left to read the save.
    assert!(live.follow_seen().is_none(), "a watcher is left");
    save(&src, SEVEN);
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(
        message(&live),
        format!("undone: paste; follow is off: {REASON}")
    );
    assert_eq!(status(&live)["reason"], REASON);
    same_as_fresh(&live, &tiza, "u of the paste");
    live.command("follow on");
    assert_eq!(code(&live), lines(SEVEN));
    same_as_fresh(&live, &tiza, ":follow on");
}

fn get(live: &Live) -> Value {
    live.api_ok("scene.get", json!({}))["document"].clone()
}

fn put(live: &Live, document: &Value) -> Value {
    let r = live.api("scene.put", json!({ "document": document }));
    assert!(r.error.is_none(), "put refused: {:?}", r.error);
    r.result.unwrap_or(Value::Null)
}

/// `scene.get` → `scene.put` of the same document, then of it with a new
/// title: the code on screen stays embedded, autosave stays on, and each
/// time the live instance is what a fresh load of the autosave gives.
fn get_put_keeps_the_screen(live: &Live, tiza: &Path, what: &str) {
    let shown = code(live);
    let doc = get(live);
    put(live, &doc);
    let msg = message(live);
    assert!(!msg.contains("changed on disk"), "{what}: {msg}");
    assert_eq!(code(live), shown, "{what}");
    assert!(live.with(|app| app.doc.is_embedded()), "{what}");
    assert!(saved_code(tiza).is_array(), "{what}: {}", saved_code(tiza));
    same_as_fresh(live, tiza, &format!("{what}: unchanged put"));
    let mut edited = doc;
    edited["title"] = json!("after a put");
    put(live, &edited);
    let msg = message(live);
    assert!(!msg.contains("not saved"), "{what}: {msg}");
    assert_eq!(live.api_ok("status", json!({}))["autosave"], true, "{what}");
    assert!(saved_code(tiza).is_array(), "{what}: {}", saved_code(tiza));
    same_as_fresh(live, tiza, &format!("{what}: put with a new title"));
    // The file the code replaced is still what `:follow on` reloads.
    assert!(
        status(live)["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("main.rs")),
        "{what}"
    );
}

#[test]
fn get_put_after_the_step5_u_keeps_the_code_embedded() {
    let (live, src, tiza) = rebind_followed("put-step5");
    save(&src, &REBIND_SRC.replace("t.len()", "t.chars().count()"));
    wait_reloads(&live, 1);
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(live.with(|app| app.doc.annotations.len()), 1);
    get_put_keeps_the_screen(&live, &tiza, "after u of the reload");
    let fresh = tiza_core::fs::load_document(&tiza).expect("the autosave opens");
    assert_eq!(fresh.code[1], "    let n = t.len();");
}

#[test]
fn get_put_after_a_paste_keeps_the_code_embedded() {
    let (live, _src, tiza) = followed("put-paste", CODE);
    live.command("hl 3:println one");
    live.with(|app| {
        app.replace_code("fn other() {\n    println!(\"hi\");\n}\n")
            .unwrap()
    });
    get_put_keeps_the_screen(&live, &tiza, "after a paste");
    live.command("follow on");
    assert_eq!(code(&live), lines(CODE));
}

#[test]
fn u_while_waiting_for_a_missing_file_keeps_the_file_to_follow() {
    let (live, src, tiza) = followed("wait-u", CODE);
    live.with(|app| app.replace_code("fn other() {}\n").unwrap());
    std::fs::remove_file(&src).unwrap();
    live.command("follow on");
    wait_for("the file reported missing", || {
        status(&live)["missing"] == true
    });
    live.command("note 1 hi");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    let msg = message(&live);
    assert!(
        msg.starts_with(
            "follow off: the code is the paste again, not main.rs; :follow on reloads main.rs"
        ),
        "{msg}"
    );
    let st = status(&live);
    assert_eq!(st["enabled"], false, "{st}");
    assert!(
        st["path"].as_str().is_some_and(|p| p.ends_with("main.rs")),
        "{st}"
    );
    live.command("follow on");
    assert!(
        !message(&live).contains("nothing to follow"),
        "{}",
        message(&live)
    );
    assert_eq!(status(&live)["enabled"], true);
    save(&src, SEVEN);
    wait_reloads(&live, 1);
    assert_eq!(code(&live), lines(SEVEN));
    assert_eq!(status(&live)["missing"], false);
    same_as_fresh(&live, &tiza, "after the file came back");
    // One undo step: the paste comes back (moved here from the corner of a
    // paste and a file that comes back).
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), vec!["fn other() {}"]);
    assert!(
        message(&live).contains("the code is the paste again"),
        "{}",
        message(&live)
    );
}

#[test]
fn follow_off_keeps_the_file_follow_on_reloads() {
    let (live, _src, tiza) = reloaded_over_a_highlight("off-on");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    live.command("follow off");
    assert_eq!(message(&live), "follow off");
    assert_eq!(status(&live)["reason"], REASON);
    live.command("follow on");
    assert_eq!(message(&live), "following main.rs");
    assert_eq!(code(&live), lines(SEVEN));
    same_as_fresh(&live, &tiza, ":follow off, :follow on");
    // The same through the socket (what `set_follow` sends).
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    live.api_ok("follow.set", json!({"enabled": false}));
    let r = live.api_ok("follow.set", json!({"enabled": true}));
    assert_eq!(r["enabled"], true, "{r}");
    assert_eq!(code(&live), lines(SEVEN));
    same_as_fresh(&live, &tiza, "follow.set false, true");
}

#[test]
fn the_reason_says_what_the_screen_shows_after_every_change() {
    let (live, src, tiza) = reloaded_over_a_highlight("reason");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(status(&live)["reason"], REASON);
    // A paste on top: pasted.
    live.with(|app| app.replace_code("fn other() {}\n").unwrap());
    assert_eq!(
        status(&live)["reason"],
        "the code is pasted, not main.rs; :follow on reloads main.rs"
    );
    // `u`: the older version again.
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(status(&live)["reason"], REASON);
    // The file edited back to what the screen shows: nothing to say, and
    // `:follow on` follows it without changing the screen.
    save(&src, CODE);
    let st = status(&live);
    assert!(st.get("reason").is_none(), "{st}");
    live.command("follow");
    assert_eq!(
        message(&live),
        "follow off (:follow on to reload when the file is saved)"
    );
    live.command("follow on");
    assert_eq!(code(&live), lines(CODE));
    assert!(!live.with(|app| app.doc.is_embedded()));
    same_as_fresh(&live, &tiza, ":follow on over code the file holds again");
}

#[test]
fn a_redo_back_to_the_paste_says_paste_not_version() {
    // Paste, save main.rs, `:follow on` (reload), `u` (the paste), `u` (the
    // code before the paste, which main.rs no longer holds), `U` (the paste).
    let (live, src, tiza) = followed("redo-paste", CODE);
    live.with(|app| app.replace_code("fn other() {}\n").unwrap());
    save(&src, SEVEN);
    live.command("follow on");
    assert_eq!(code(&live), lines(SEVEN));
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), vec!["fn other() {}"]);
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(status(&live)["reason"], REASON);
    live.keys(&[crossterm::event::KeyCode::Char('U')]);
    assert_eq!(code(&live), vec!["fn other() {}"]);
    assert_eq!(
        status(&live)["reason"],
        "the code is the paste again, not main.rs; :follow on reloads main.rs"
    );
    // Follow was off already; the reason changed, so the bar says it once.
    assert_eq!(
        message(&live),
        "redone: paste; follow is off: the code is the paste again, not main.rs; :follow on reloads main.rs"
    );
    same_as_fresh(&live, &tiza, "U back to the paste");
}

#[test]
fn u_from_the_files_code_onto_an_embedded_version_says_why_once() {
    // A note and a highlight, a reload that drops the highlight; `u` ×3
    // (reload, highlight, note), `U` ×3 back to the reload: the file's
    // code with follow off. The next `u` lands on a snapshot that is
    // already embedded (x = 1): follow is off and the bar says why.
    let (live, src, tiza) = followed("u-embedded", CODE);
    live.command("note 1 entry");
    live.command("hl 2:1 one");
    save(&src, SEVEN);
    wait_reloads(&live, 1);
    for _ in 0..3 {
        live.keys(&[crossterm::event::KeyCode::Char('u')]);
    }
    for _ in 0..3 {
        live.keys(&[crossterm::event::KeyCode::Char('U')]);
    }
    assert_eq!(code(&live), lines(SEVEN));
    assert_eq!(
        message(&live),
        "redone: reload #1; follow is off: :follow on reloads main.rs"
    );
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(
        message(&live),
        format!("undone: reload #1; follow is off: {REASON}")
    );
    assert_eq!(status(&live)["reason"], REASON);
    same_as_fresh(&live, &tiza, "u onto the embedded version");
    // Once: a further `u` onto the same kind of screen says nothing new.
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert!(!message(&live).contains("follow"), "{}", message(&live));
}
