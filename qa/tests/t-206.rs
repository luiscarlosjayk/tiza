//! t-206 — With follow on and moves applied (step ≥ 1), saving the file in
//! the editor left a `.tiza` that did not open, and going back to step 0
//! did not reload.
//!
//! The reload waits for step 0 (ADR 0007 §7), but the autosave kept
//! `code: {file}` with the annotations and `rebind` entries made for the
//! code on screen, so a fresh load read the file's new text under them:
//! `tiza check` failed (`rebind «a1» «t.len()» matches 0 times after the
//! patch`) and the TUI refused the file, or, without a rebind, drew on other
//! text in silence. Back at step 0 the bar kept «⟳ following» over code the
//! file no longer had, until the next change on disk.
//!
//! Acceptance criteria:
//! - AC1 While a reload is deferred because moves are applied, what is
//!   saved always agrees with a fresh load: the sidecar is saved with the
//!   code on screen embedded (as t-193/t-202 do) or not saved, never
//!   `code: {file}` against a file that now holds other code. `tiza check`
//!   on the autosave = the screen, and the .tiza always opens.
//! - AC2 Going back to step 0 with a deferred change reloads it at once as
//!   a normal reload (one undo entry, re-anchoring, the moves' stale rule),
//!   or turns follow off saying why; never «⟳ following» over code the file
//!   no longer has.
//! - AC3 Quitting (`:q`, Ctrl-C, terminal gone per t-205) while the reload
//!   is still deferred leaves a .tiza that opens and shows what the screen
//!   showed.
//! - AC4 This file, with the repro above, with and without a rebind,
//!   quitting at step 2 and after going back to step 0; CHANGELOG (Fixed).
//!
//! Corner cases: two editor saves while at step 2 (only the last one counts
//! when going back; a file saved back as it was is no reload); the file
//! removed while at step 2; a paste at step ≥ 1 (refused, as always);
//! `--no-save` (nothing written, and the live rule still holds).
//!
//! After every step the live instance is what a fresh load of its autosave
//! gives: the same `check` report and the same `render --all-steps`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_qa::{Live, bin, scratch};

/// QA 1's fixture (pruebas/t199/qa1/paste): `a1` highlights `t.len()` from
/// step 1, the step-2 move replaces `n + 1`.
const OLD: &str = "fn f(t: &str) -> usize {\n    let n = t.len();\n    n + 1\n}\n";
/// The editor's save: `t.len()` is gone.
const NEW: &str = "fn f(t: &str) -> usize {\n    let n = t.chars().count();\n    n + 1\n}\n";
const OTHER: &str = "fn f(t: &str) -> usize {\n    let n = t.bytes().len();\n    n + 1\n}\n";
const WAITS: &str =
    "main.rs changed on disk but moves are applied at step 2: go back to step 0 to reload";

const WAIT: Duration = Duration::from_secs(10);

/// `main.rs` (OLD) and its sidecar, opened by reference at step 0,
/// following. `rebind`: the step-2 move rebinds `a1` to `t.len()`.
fn followed(name: &str, rebind: bool) -> (Live, PathBuf, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, OLD).unwrap();
    let mut mv = json!({
        "at_step": 2, "action": "code.patch", "op": "replace",
        "find": "n + 1", "content": "    n * 2"
    });
    if rebind {
        mv["rebind"] = json!({"a1": {"at": {"find": "t.len()"}}});
    }
    let doc = json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "annotations": [{
            "id": "a1", "step": 1, "type": "highlight",
            "span": {"line": 2, "start": 12, "end": 19}, "label": "length"
        }],
        "step": 0,
        "moves": [mv],
    });
    let tiza = dir.join("main.rs.tiza");
    std::fs::write(&tiza, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(doc, tiza.clone(), dir.join("live.sock"));
    live.command("follow on");
    assert_eq!(live.message().unwrap().0, "following main.rs");
    (live, src, tiza)
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
    let screen = live.screen(160, 30);
    screen.lines().last().unwrap_or_default().to_string()
}

fn reloads(live: &Live) -> u64 {
    status(live)["reloads"].as_u64().unwrap_or(0)
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

/// At step 2, an editor save of `content`: the reload waits for step 0.
fn save_at_step_2(live: &Live, src: &Path, content: &str) {
    live.with(|app| app.message = None);
    save(src, content);
    wait_for("the reload to wait for step 0", || message(live) == WAITS);
}

fn check_file(path: &Path) -> (i32, Value) {
    let out = std::process::Command::new(bin())
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
    let r = std::process::Command::new(bin())
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
/// `render --all-steps`. Quitting now (`:q`, Ctrl-C, the terminal gone)
/// leaves exactly this file: every change is on disk already (AC3).
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

fn the_repro(name: &str, rebind: bool) {
    // Test step 1: at step 2 the editor saves main.rs; the reload waits,
    // and the autosave keeps the code on screen, embedded.
    let (live, src, tiza) = followed(name, rebind);
    live.command("step 2");
    same_as_fresh(&live, &tiza, "at step 2");
    save_at_step_2(&live, &src, NEW);
    assert_eq!(base(&live), lines(OLD), "the screen keeps the old code");
    assert!(saved_code(&tiza).is_array(), "{}", saved_code(&tiza));
    same_as_fresh(&live, &tiza, "the reload waits at step 2");
    assert!(bar(&live).contains("⟳ following"), "{}", bar(&live));

    // Test step 2: back at step 0 the change is reloaded at once, as a
    // normal reload: `a1` is dropped (its text is gone) and `u` gives it back.
    live.command("step 0");
    assert_eq!(base(&live), lines(NEW));
    assert_eq!(reloads(&live), 1);
    let msg = message(&live);
    assert!(msg.contains("main.rs reloaded (#1)"), "{msg}");
    assert!(msg.contains("dropped (text gone; u undoes)"), "{msg}");
    if rebind {
        // The moves' stale rule: the rebind of a dropped annotation.
        assert!(
            msg.contains("move #1's rebind of a1 is now ignored"),
            "{msg}"
        );
    }
    assert_eq!(live.with(|app| app.doc.annotations.len()), 0);
    assert_eq!(saved_code(&tiza), json!({"file": "main.rs"}));
    same_as_fresh(&live, &tiza, "back at step 0");
    assert!(bar(&live).contains("⟳ following"), "{}", bar(&live));

    // One undo entry: `u` brings the old code and a1 back, embedded, and
    // follow goes off saying why (t-202).
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(base(&live), lines(OLD));
    assert_eq!(live.with(|app| app.doc.annotations.len()), 1);
    assert!(
        message(&live).starts_with("follow off: the code on screen is not main.rs"),
        "{}",
        message(&live)
    );
    same_as_fresh(&live, &tiza, "u after the reload");
}

#[test]
fn ac1_ac2_the_repro_with_a_rebind() {
    the_repro("t206-rebind", true);
}

#[test]
fn ac1_ac2_the_repro_without_a_rebind() {
    // Without a rebind the fresh load did not fail: it drew `a1` on other
    // text in silence.
    the_repro("t206-no-rebind", false);
}

#[test]
fn ac3_quitting_at_step_2_leaves_the_screen() {
    // Test step 3: what is on disk while the reload waits is what quitting
    // leaves; a fresh load opens it at step 2 with the old code.
    let (live, src, tiza) = followed("t206-quit-step2", true);
    live.command("step 2");
    save_at_step_2(&live, &src, NEW);
    same_as_fresh(&live, &tiza, "quit at step 2");
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    assert_eq!(doc.base(), lines(OLD).as_slice());
    assert!(live.with(|app| app.unsaved_summary().is_none()));
    drop(live);
    // The file is still what the editor saved.
    assert_eq!(std::fs::read_to_string(&src).unwrap(), NEW);
}

#[test]
fn corner_two_saves_at_step_2_only_the_last_counts() {
    let (live, src, tiza) = followed("t206-two-saves", true);
    live.command("step 2");
    save_at_step_2(&live, &src, NEW);
    save_at_step_2(&live, &src, OTHER);
    same_as_fresh(&live, &tiza, "two saves at step 2");
    live.command("step 0");
    assert_eq!(base(&live), lines(OTHER));
    assert_eq!(reloads(&live), 1);
    same_as_fresh(&live, &tiza, "back at step 0");
}

#[test]
fn corner_saved_back_as_it_was_is_no_reload() {
    // A save and then one back to the code on screen: at step 0 there is
    // nothing to reload, and the document references the file again.
    let (live, src, tiza) = followed("t206-saved-back", true);
    live.command("step 2");
    save_at_step_2(&live, &src, NEW);
    save(&src, OLD);
    wait_for("the file read back", || {
        live.with(|app| app.source_file().is_some())
            && std::fs::read_to_string(&src).unwrap() == OLD
    });
    live.command("step 0");
    assert_eq!(base(&live), lines(OLD));
    assert_eq!(reloads(&live), 0);
    assert_eq!(live.with(|app| app.doc.annotations.len()), 1);
    assert_eq!(saved_code(&tiza), json!({"file": "main.rs"}));
    assert_eq!(status(&live)["enabled"], true);
    same_as_fresh(&live, &tiza, "saved back, step 0");
}

#[test]
fn corner_the_file_removed_at_step_2_and_back_with_other_code() {
    let (live, src, tiza) = followed("t206-removed", true);
    live.command("step 2");
    std::fs::remove_file(&src).unwrap();
    wait_for("the removal", || {
        message(&live) == "main.rs removed; waiting for it to come back"
    });
    save_at_step_2(&live, &src, NEW);
    same_as_fresh(&live, &tiza, "back with other code at step 2");
    live.command("step 0");
    assert_eq!(base(&live), lines(NEW));
    same_as_fresh(&live, &tiza, "back at step 0");
}

#[test]
fn corner_a_paste_at_step_2_is_refused() {
    let (live, _src, tiza) = followed("t206-paste", true);
    live.command("step 2");
    let err = live
        .with(|app| app.replace_code("fn g() {}\n"))
        .unwrap_err()
        .to_string();
    assert!(err.contains("go back to step 0 first"), "{err}");
    same_as_fresh(&live, &tiza, "paste refused");
}

#[test]
fn corner_no_save_writes_nothing_and_the_live_rule_holds() {
    let (live, src, tiza) = followed("t206-no-save", true);
    live.with(|app| app.autosave = false);
    let before = std::fs::read(&tiza).unwrap();
    live.command("step 2");
    save_at_step_2(&live, &src, NEW);
    assert_eq!(base(&live), lines(OLD));
    live.command("step 0");
    assert_eq!(base(&live), lines(NEW));
    assert_eq!(reloads(&live), 1);
    assert_eq!(std::fs::read(&tiza).unwrap(), before);
}
