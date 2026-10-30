//! Three follow leftovers: after `:ref other.rs` follow went on watching
//! the old file (and the code on screen stayed the old file's); a
//! `scene.put` of another document left «⟳ following» over code that is not
//! the file's; and a paste identical to the file with follow off embedded
//! the code for nothing.
//!
//! Acceptance criteria:
//! - AC1 After `:ref other.rs` (or `:link` at step 0) follow reads the
//!   file the document now references: a save of the old file reloads
//!   nothing, a save of the new one reloads it, and `:follow on` later
//!   reloads the new one.
//! - AC2 A `scene.put` of another document while follow is on turns follow
//!   off and says why; the open document's own copy keeps it on.
//! - AC3 A paste identical to the file with follow off keeps `code: {file}`,
//!   as with follow on.
//! - AC4 After each case the instance agrees with a fresh load of the
//!   autosave (the same `check`, the same `render --all-steps`).
//! - AC5 This file; CHANGELOG (Fixed).
//!
//! Corner cases: `:ref` to a file that does not exist gives the error it
//! gave and changes nothing (follow included); `:ref` at a step with moves
//! applied to a file with other code is refused like a paste.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_qa::{Live, scratch};

const MAIN: &str = "fn main() {\n    let a = 1;\n}\n";
const MAIN2: &str = "fn main() {\n    let a = 10;\n}\n";
const OTHER: &str = "fn other() {\n    let b = 2;\n}\n";
const OTHER2: &str = "fn other() {\n    let b = 20;\n}\n";

const WAIT: Duration = Duration::from_secs(10);

/// `main.rs` and `other.rs` in a scratch folder; `main.rs` opened as the TUI
/// opens it (its sidecar `main.rs.tiza`), with a note on line 1.
fn opened(name: &str) -> (Live, PathBuf, PathBuf) {
    let dir = scratch(name);
    std::fs::write(dir.join("main.rs"), MAIN).unwrap();
    std::fs::write(dir.join("other.rs"), OTHER).unwrap();
    let opened = tiza_core::fs::open(dir.join("main.rs").to_str().unwrap()).unwrap();
    let tiza = opened.save_path.clone();
    let live = Live::open(opened.document, tiza.clone(), dir.join("live.sock"));
    live.command("note 1 the function");
    (live, dir, tiza)
}

fn status(live: &Live) -> Value {
    live.api_ok("follow.status", json!({}))
}

fn base(live: &Live) -> Vec<String> {
    live.with(|app| app.doc.base().to_vec())
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn message(live: &Live) -> String {
    live.message().unwrap_or_default().0
}

fn bar(live: &Live) -> String {
    live.screen(160, 30)
        .lines()
        .last()
        .unwrap_or_default()
        .to_string()
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
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

fn saved_code(tiza: &Path) -> Value {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(tiza).unwrap()).unwrap();
    v["code"].clone()
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

/// AC4: the live instance is what a fresh load of its autosave gives.
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
        r.as_object_mut().unwrap().remove("revision");
    }
    assert_eq!(live_report, disk_report, "{what}: live check vs fresh load");
    let dir = tiza.parent().unwrap();
    let sock = live.socket.to_str().unwrap().to_string();
    let live_steps = all_steps(&["--socket", &sock], &dir.join("render-live"));
    let disk_steps = all_steps(&[tiza.to_str().unwrap()], &dir.join("render-disk"));
    assert_eq!(live_steps, disk_steps, "{what}: render --all-steps");
}

// ---------- AC1 ----------

#[test]
fn ref_another_file_while_following_follows_it() {
    // Test step 1.
    let (live, dir, tiza) = opened("ref");
    live.command("follow on");
    let reads = status(&live)["reads"].as_u64().expect("reads");
    live.command("ref other.rs");
    // A new watcher for other.rs: the counts go on (the read of other.rs
    // is one more).
    assert!(status(&live)["reads"].as_u64().unwrap() > reads);
    // The document reads other.rs now, so the screen shows its code.
    assert_eq!(base(&live), lines(OTHER), "{}", message(&live));
    assert_eq!(saved_code(&tiza), json!({"file": "other.rs"}));
    let st = status(&live);
    assert_eq!(st["enabled"], true, "{st}");
    assert!(st["path"].as_str().unwrap().ends_with("other.rs"), "{st}");
    assert!(
        message(&live).ends_with("following other.rs"),
        "{}",
        message(&live)
    );
    same_as_fresh(&live, &tiza, ":ref other.rs");
    // A save of the old file reloads nothing. Then a save of other.rs with
    // the same text: its read comes after the events of main.rs, so when it
    // is done, a reload of main.rs would be on screen.
    let reloads = live.follow_seen().expect("following").reloads;
    save(&dir.join("main.rs"), MAIN2);
    save(&dir.join("other.rs"), OTHER);
    let seen = live.wait_follow_read(&dir.join("other.rs"));
    assert_eq!(base(&live), lines(OTHER));
    assert_eq!(seen.reloads, reloads);
    // A save of the new one does.
    save(&dir.join("other.rs"), OTHER2);
    wait_for("other.rs reloaded", || base(&live) == lines(OTHER2));
    same_as_fresh(&live, &tiza, "other.rs reloaded");
}

#[test]
fn u_of_the_ref_follows_the_old_file_again() {
    let (live, dir, tiza) = opened("ref-u");
    live.command("follow on");
    live.command("ref other.rs");
    assert_eq!(base(&live), lines(OTHER));
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(base(&live), lines(MAIN));
    assert_eq!(saved_code(&tiza), json!({"file": "main.rs"}));
    let st = status(&live);
    assert_eq!(st["enabled"], true, "{st}");
    assert!(st["path"].as_str().unwrap().ends_with("main.rs"), "{st}");
    same_as_fresh(&live, &tiza, "u of the :ref");
    // Redo: other.rs again, and back.
    live.keys(&[crossterm::event::KeyCode::Char('U')]);
    assert_eq!(base(&live), lines(OTHER));
    let st = status(&live);
    assert_eq!(st["enabled"], true, "{st}");
    assert!(st["path"].as_str().unwrap().ends_with("other.rs"), "{st}");
    same_as_fresh(&live, &tiza, "U of the :ref");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    save(&dir.join("main.rs"), MAIN2);
    wait_for("main.rs reloaded", || base(&live) == lines(MAIN2));
    same_as_fresh(&live, &tiza, "main.rs reloaded after the u");
}

#[test]
fn follow_on_after_ref_and_a_paste_reloads_the_new_file() {
    // A paste over main.rs while following (follow off, it remembers
    // main.rs), `:ref other.rs`, then another paste with follow off:
    // `:follow on` reloads other.rs, the file the document referenced last.
    let (live, _dir, tiza) = opened("ref-paste");
    crate::by_hand(&live);
    live.command("follow on");
    live.with(|app| app.replace_code("fn pasted() {}\n"))
        .unwrap();
    assert_eq!(status(&live)["enabled"], false);
    live.command("ref other.rs");
    assert_eq!(base(&live), lines(OTHER));
    live.with(|app| app.replace_code("fn again() {}\n"))
        .unwrap();
    live.command("follow on");
    assert_eq!(base(&live), lines(OTHER), "{}", message(&live));
    let st = status(&live);
    assert!(st["path"].as_str().unwrap().ends_with("other.rs"), "{st}");
    same_as_fresh(&live, &tiza, ":follow on after the second paste");
}

#[test]
fn link_at_step_0_is_the_same() {
    let (live, dir, tiza) = opened("link");
    live.command("follow on");
    live.command("step 0");
    live.command("link other.rs");
    assert_eq!(base(&live), lines(OTHER));
    save(&dir.join("other.rs"), OTHER2);
    wait_for("other.rs reloaded", || base(&live) == lines(OTHER2));
    same_as_fresh(&live, &tiza, ":link other.rs at step 0");
}

#[test]
fn ref_to_the_same_code_keeps_everything() {
    // other.rs holds main.rs's code: only the reference changes.
    let (live, dir, tiza) = opened("same-code");
    std::fs::write(dir.join("other.rs"), MAIN).unwrap();
    live.command("follow on");
    let annotations = live.with(|app| app.doc.annotations.len());
    live.command("ref other.rs");
    assert_eq!(base(&live), lines(MAIN));
    assert_eq!(live.with(|app| app.doc.annotations.len()), annotations);
    assert_eq!(saved_code(&tiza), json!({"file": "other.rs"}));
    save(&dir.join("other.rs"), OTHER2);
    wait_for("other.rs reloaded", || base(&live) == lines(OTHER2));
    same_as_fresh(&live, &tiza, "same code, then a save");
}

// ---------- AC2 ----------

#[test]
fn a_put_of_another_document_turns_follow_off() {
    // Test step 2.
    let (live, dir, _tiza) = opened("put");
    crate::by_hand(&live);
    live.command("follow on");
    let other = json!({
        "schema": 1,
        "source": {"path": "elsewhere.rs", "language": "rs", "first_line": 1},
        "code": ["fn elsewhere() {}"],
        "annotations": [],
        "step": 0
    });
    live.api_ok("scene.put", json!({"document": other.clone()}));
    assert_eq!(status(&live)["enabled"], false);
    let msg = message(&live);
    assert!(
        msg.starts_with("follow off: the code on screen is not main.rs"),
        "{msg}"
    );
    assert!(!bar(&live).contains("⟳ following"), "{}", bar(&live));
    // A save of main.rs no longer reaches the screen: no watcher is left to
    // read it.
    assert!(live.follow_seen().is_none(), "a watcher is left");
    save(&dir.join("main.rs"), MAIN2);
    assert_eq!(base(&live), vec!["fn elsewhere() {}".to_string()]);
}

#[test]
fn with_autosave_already_off_the_put_says_follow_off() {
    let (live, _dir, _tiza) = opened("put-nosave");
    crate::by_hand(&live);
    live.command("follow on");
    live.command("autosave off");
    let other = json!({
        "schema": 1,
        "source": {"path": "elsewhere.rs", "language": "rs", "first_line": 1},
        "code": ["fn elsewhere() {}"],
        "annotations": [],
        "step": 0
    });
    live.api_ok("scene.put", json!({"document": other}));
    assert_eq!(status(&live)["enabled"], false);
    assert_eq!(
        message(&live),
        "another document loaded; follow off: the code on screen is not main.rs"
    );
}

#[test]
fn a_put_of_the_documents_own_copy_keeps_following() {
    let (live, dir, tiza) = opened("put-same");
    live.command("follow on");
    let scene = live.api_ok("scene.get", json!({}));
    live.api_ok("scene.put", json!({"document": scene["document"]}));
    assert_eq!(status(&live)["enabled"], true);
    save(&dir.join("main.rs"), MAIN2);
    wait_for("main.rs reloaded", || base(&live) == lines(MAIN2));
    same_as_fresh(&live, &tiza, "own copy put, then a save");
}

// ---------- AC3 ----------

#[test]
fn an_identical_paste_with_follow_off_stays_a_reference() {
    // Test step 3.
    let (live, _dir, tiza) = opened("paste");
    assert_eq!(status(&live)["enabled"], false);
    live.with(|app| app.replace_code(MAIN)).unwrap();
    assert!(live.with(|app| !app.doc.is_embedded()));
    assert_eq!(saved_code(&tiza), json!({"file": "main.rs"}));
    same_as_fresh(&live, &tiza, "identical paste, follow off");
}

#[test]
fn a_paste_of_what_the_file_holds_now_stays_a_reference() {
    // The file changed on disk while follow was off; a paste of its new
    // text is the file's code: still a reference, now showing that text.
    let (live, dir, tiza) = opened("paste-new");
    std::fs::write(dir.join("main.rs"), MAIN2).unwrap();
    live.with(|app| app.replace_code(MAIN2)).unwrap();
    assert_eq!(base(&live), lines(MAIN2));
    assert!(live.with(|app| !app.doc.is_embedded()));
    assert_eq!(saved_code(&tiza), json!({"file": "main.rs"}));
    same_as_fresh(&live, &tiza, "paste of the file's new text");
}

#[test]
fn a_paste_of_older_code_with_follow_off_is_embedded() {
    // Not the file's code now (the file changed on disk): embedded, as before.
    let (live, dir, tiza) = opened("paste-old");
    std::fs::write(dir.join("main.rs"), MAIN2).unwrap();
    live.with(|app| app.replace_code(MAIN)).unwrap();
    assert!(live.with(|app| app.doc.is_embedded()));
    same_as_fresh(&live, &tiza, "paste of older code");
}

// ---------- corner cases ----------

#[test]
fn ref_to_a_missing_file_changes_nothing() {
    let (live, _dir, tiza) = opened("missing");
    crate::by_hand(&live);
    live.command("follow on");
    let before = live.with(|app| app.doc.code_src.clone());
    live.command("ref nowhere.rs");
    let (msg, loud) = live.message().unwrap();
    assert!(loud, "{msg}");
    assert_eq!(live.with(|app| app.doc.code_src.clone()), before);
    let st = status(&live);
    assert!(st["path"].as_str().unwrap().ends_with("main.rs"), "{st}");
    assert_eq!(st["enabled"], true);
    same_as_fresh(&live, &tiza, "after the refused :ref");
}

#[test]
fn ref_to_other_code_with_moves_applied_is_refused() {
    let (live, _dir, tiza) = opened("moves");
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "replace", "find": "let a = 1;", "content": "    let a = 2;"}),
    );
    live.command("step 1");
    live.command("ref other.rs");
    let (msg, loud) = live.message().unwrap();
    assert!(loud && msg.contains("go back to step 0 first"), "{msg}");
    assert_eq!(saved_code(&tiza), json!({"file": "main.rs"}));
    same_as_fresh(&live, &tiza, ":ref refused at step 1");
}

// ---------- review ----------

#[test]
fn ref_to_another_range_of_the_same_file_then_a_paste() {
    // The watch stays on main.rs; what it loaded last is the range now on
    // screen, so a paste of the whole file is not «what the file holds».
    let (live, _dir, tiza) = opened("range");
    crate::by_hand(&live);
    live.command("follow on");
    live.command("step 0");
    live.command("ref main.rs:2-3");
    assert_eq!(base(&live), lines("    let a = 1;\n}\n"));
    assert_eq!(live.with(|app| app.doc.base_first()), 2);
    same_as_fresh(&live, &tiza, ":ref main.rs:2-3");
    live.with(|app| app.replace_code(MAIN)).unwrap();
    live.command("step 1");
    live.command("note 3 closing brace");
    same_as_fresh(&live, &tiza, "the whole file pasted over the range");
}

#[test]
fn ref_to_the_whole_file_from_a_range_then_a_paste() {
    let dir = scratch("range-whole");
    std::fs::write(dir.join("main.rs"), MAIN).unwrap();
    let opened = tiza_core::fs::open(&format!("{}:2-3", dir.join("main.rs").display())).unwrap();
    let tiza = opened.save_path.clone();
    let live = Live::open(opened.document, tiza.clone(), dir.join("live.sock"));
    crate::by_hand(&live);
    live.command("follow on");
    live.command("ref main.rs");
    assert_eq!(base(&live), lines(MAIN));
    same_as_fresh(&live, &tiza, ":ref main.rs");
    live.with(|app| app.replace_code("    let a = 1;\n}\n"))
        .unwrap();
    same_as_fresh(&live, &tiza, "the old range pasted over the whole file");
}

#[test]
fn ref_to_a_file_a_code_link_shows_is_refused() {
    let (live, _dir, tiza) = opened("linked");
    live.command("step 1");
    live.command("link other.rs");
    live.command("note 2 on other");
    live.command("step 0");
    live.command("ref other.rs");
    let (msg, loud) = live.message().unwrap();
    assert!(
        loud && msg.contains("other.rs is shown by a code.link at step 1"),
        "{msg}"
    );
    assert_eq!(saved_code(&tiza), json!({"file": "main.rs"}));
    same_as_fresh(&live, &tiza, ":ref to a linked file refused");
}

#[test]
fn with_follow_never_on_a_paste_has_nothing_to_follow() {
    // As before: an unrelated undo does not make `:follow on` reload a file.
    let (live, _dir, _tiza) = opened("never");
    live.command("note 2 x");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    live.command("step 0");
    live.with(|app| app.replace_code("fn pasted() {}\n"))
        .unwrap();
    let st = status(&live);
    assert_eq!(st["enabled"], false, "{st}");
    assert!(st["path"].is_null(), "{st}");
    live.command("follow on");
    let (msg, loud) = live.message().unwrap();
    assert!(loud && msg.starts_with("nothing to follow"), "{msg}");
}

#[test]
fn u_of_a_ref_over_a_paste_follows_the_first_file_again() {
    // The paste was over main.rs; `:ref other.rs` and its `u` leave it so.
    let (live, _dir, tiza) = opened("paste-ref-u");
    crate::by_hand(&live);
    live.command("follow on");
    live.command("step 0");
    live.with(|app| app.replace_code("fn pasted() {}\n"))
        .unwrap();
    live.command("ref other.rs");
    assert_eq!(base(&live), lines(OTHER));
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(base(&live), lines("fn pasted() {}\n"));
    live.command("follow on");
    assert_eq!(base(&live), lines(MAIN), "{}", message(&live));
    let st = status(&live);
    assert!(st["path"].as_str().unwrap().ends_with("main.rs"), "{st}");
    same_as_fresh(&live, &tiza, ":follow on after the u");
}

#[test]
fn a_redo_of_the_ref_names_the_file_on_screen() {
    let (live, dir, _tiza) = opened("redo-words");
    live.command("follow on");
    live.command("step 0");
    live.command("ref other.rs");
    // A save of the old file, then one of other.rs with the same text: when
    // that one is read, the events of main.rs are done. Neither reloads.
    let reloads = live.follow_seen().expect("following").reloads;
    save(&dir.join("main.rs"), MAIN2);
    save(&dir.join("other.rs"), OTHER);
    assert_eq!(
        live.wait_follow_read(&dir.join("other.rs")).reloads,
        reloads
    );
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    live.keys(&[crossterm::event::KeyCode::Char('U')]);
    let msg = message(&live);
    assert!(msg.ends_with(":follow on reloads other.rs"), "{msg}");
}

#[test]
fn a_symlink_to_the_followed_file_keeps_reloading() {
    let (live, dir, tiza) = opened("symlink");
    std::os::unix::fs::symlink(dir.join("main.rs"), dir.join("link.rs")).unwrap();
    live.command("follow on");
    live.command("ref link.rs");
    assert_eq!(saved_code(&tiza), json!({"file": "link.rs"}));
    save(&dir.join("main.rs"), MAIN2);
    wait_for("the reload through the link", || {
        base(&live) == lines(MAIN2)
    });
    same_as_fresh(&live, &tiza, "reloaded through the link");
}

#[test]
fn link_without_a_path_on_a_reference_keeps_it() {
    let dir = scratch("link-noarg");
    std::fs::write(dir.join("main.rs"), MAIN).unwrap();
    let opened = tiza_core::fs::open(&format!("{}:1-2", dir.join("main.rs").display())).unwrap();
    let tiza = opened.save_path.clone();
    let live = Live::open(opened.document, tiza.clone(), dir.join("live.sock"));
    live.command("note 1 x");
    live.command("step 0");
    let before = saved_code(&tiza);
    live.command("link");
    assert_eq!(message(&live), "the .tiza already references main.rs:1-2");
    assert_eq!(saved_code(&tiza), before);
    assert_eq!(base(&live).len(), 2);
}
