//! Follow pauses at a step where a `code.patch` is applied, and loads the
//! change on disk when the presenter is back before the first patch.
//!
//! Acceptance criteria:
//! - AC1 Only a `code.patch` (of any file) pauses follow. `cursor`,
//!   `select`, `wait` and `code.link` do not change the text: a save there
//!   loads at once. At a paused step `follow.set` and `:follow on` start
//!   paused.
//! - AC2 A save while paused is a change that waits: only the newest text
//!   counts. One notice per pause names the step that loads it; later saves
//!   in the same pause say nothing. A save back to the code on screen drops
//!   the change in silence.
//! - AC3 At the resume step (the last step before the first patch) or
//!   earlier, by any path (`p`, `step.set`, `step.goto`, the wrap of a
//!   `loop`), the change loads at once as one normal reload, and the moves
//!   apply again with the stale rule. A jump answers with the new code. In
//!   a `loop` the notice about a stale move shows once per change.
//! - AC4 The bar says `⟳ paused`, `⟳ paused · changed` or `⟳ paused (file
//!   removed)`. Presentation mode shows no notice.
//! - AC5 `follow.status` and `status` give `paused`, `pending` and
//!   `resume_step`.
//!
//! Corner cases: two saves are one change; a file removed and back with
//! other text is one change; the empty first write of an editor that
//! truncates is never the change; a file that is not valid UTF-8 gives its
//! error once in a pause, is no change, and at the resume step keeps the
//! code with follow on; `:follow off` drops the change and says so;
//! `:follow on` loads at once or starts paused; a save while the `:` line,
//! a panel or a typed move runs waits for it; several paths at once give
//! one reload; an undo of a reload at a paused step turns follow off, as
//! before.
//!
//! Most tests read the save with `App::read_save` (what the watcher does
//! after its debounce) on the TUI thread, so that what a save did is known
//! when the call returns.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use serde_json::{Value, json};
use tiza_qa::{Live, scratch};

/// `a1` highlights `t.len()` from step 1. Step 1 moves the cursor, step 2
/// patches `n + 1`, step 3 waits: the resume step is 1.
const OLD: &str = "fn f(t: &str) -> usize {\n    let n = t.len();\n    n + 1\n}\n";
/// The editor's save: `t.len()` is gone, the patch still matches.
const NEW: &str = "fn f(t: &str) -> usize {\n    let n = t.chars().count();\n    n + 1\n}\n";
const OTHER: &str = "fn f(t: &str) -> usize {\n    let n = t.bytes().len();\n    n + 1\n}\n";
/// The patch of step 2 no longer matches: it goes stale.
const STALE: &str = "fn f(t: &str) -> usize {\n    let n = t.len();\n    n - 1\n}\n";
const NOT_UTF8: &[u8] = b"fn f() {\n    \xff\xfe\n}\n";

const WAIT: Duration = Duration::from_secs(10);

fn document(looping: bool, typing: bool) -> Value {
    let mut patch = json!({
        "at_step": 2, "action": "code.patch", "op": "replace",
        "find": "n + 1", "content": "    n * 2"
    });
    if typing {
        patch["typing"] = json!("char");
        patch["speed_ms"] = json!(200);
    }
    json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "annotations": [{
            "id": "a1", "step": 1, "type": "highlight",
            "span": {"line": 2, "start": 12, "end": 19}, "label": "length"
        }],
        "step": 0,
        "loop": looping,
        "moves": [
            {"at_step": 1, "action": "cursor", "line": 1},
            patch,
            {"at_step": 3, "action": "wait", "ms": 10},
        ],
    })
}

/// `main.rs` (OLD), the `files` a `code.link` shows, and the sidecar, at
/// step 0, following.
fn followed_doc(name: &str, doc: Value, files: &[(&str, &str)]) -> (Live, PathBuf, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, OLD).unwrap();
    for (file, text) in files {
        std::fs::write(dir.join(file), text).unwrap();
    }
    let tiza = dir.join("main.rs.tiza");
    std::fs::write(&tiza, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(doc, tiza.clone(), dir.join("live.sock"));
    live.command("follow on");
    assert_eq!(message(&live), "following main.rs");
    (live, src, tiza)
}

fn followed(name: &str) -> (Live, PathBuf, PathBuf) {
    followed_doc(name, document(false, false), &[])
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

fn clear(live: &Live) {
    live.with(|app| app.message = None);
}

fn bar(live: &Live) -> String {
    let screen = live.screen(160, 30);
    screen.lines().last().unwrap_or_default().to_string()
}

fn reloads(live: &Live) -> u64 {
    status(live)["reloads"].as_u64().unwrap_or(0)
}

fn goto(live: &Live, step: u32) {
    live.api_ok("step.set", json!({ "step": step }));
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ok() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Saves like an editor: a temp file renamed over the target.
fn save(path: &Path, content: &[u8]) {
    let tmp = path.with_extension("rs.tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

/// Saves `content`, then reads it as the watcher does after a save.
fn save_and_read(live: &Live, src: &Path, content: &[u8]) {
    save(src, content);
    live.with(|app| app.read_save());
}

/// (paused, pending, resume_step) of `follow.status`, and the same in
/// `status.follow`. Both are read in one go: the watcher can read a save
/// between two requests.
fn pause(live: &Live) -> (Value, Value, Value) {
    let (st, brief) = live.with(|app| {
        let status = serde_json::to_value(app.status()).unwrap();
        (app.follow_status(), status["follow"].clone())
    });
    let paused = (
        st["paused"].clone(),
        st["pending"].clone(),
        st["resume_step"].clone(),
    );
    if st["enabled"] == true {
        let from_status = (
            brief["paused"].clone(),
            brief["pending"].clone(),
            brief["resume_step"].clone(),
        );
        assert_eq!(from_status, paused, "status.follow: {brief}");
    } else {
        assert!(brief.is_null(), "status.follow with follow off: {brief}");
    }
    paused
}

fn paused(pending: bool) -> (Value, Value, Value) {
    (json!(true), json!(pending), json!(1))
}

fn live_again() -> (Value, Value, Value) {
    (json!(false), json!(false), Value::Null)
}

/// What the autosave holds as `code`.
fn saved_code(tiza: &Path) -> Value {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(tiza).unwrap()).unwrap();
    v["code"].clone()
}

/// The notice of a pause: an info, not an error, that names the step
/// that loads the change and the file.
fn assert_notice(live: &Live) {
    let (text, error) = live.message().unwrap_or_default();
    assert!(!error, "the notice is not an error: {text}");
    assert!(
        text.contains("step 1") && text.contains("main.rs"),
        "the notice: «{text}»"
    );
}

#[test]
fn a_save_that_waits_has_its_hash_and_the_resume_step_reads_it_again() {
    // Paused, a save is read and waits: `hash` is the one of the save,
    // with `pending`. At the resume step the file is read again to load.
    let (live, src, _tiza) = followed("hash-pause");
    goto(&live, 2);
    save(&src, NEW.as_bytes());
    live.wait_follow_read(&src);
    let st = status(&live);
    assert_eq!(pause(&live), paused(true));
    assert_eq!(
        st["hash"],
        tiza_tui::follow::sha256_hex(NEW.as_bytes()),
        "{st}"
    );
    let reads = st["reads"].as_u64().expect("reads");
    goto(&live, 1);
    assert_eq!(base(&live), lines(NEW));
    assert!(status(&live)["reads"].as_u64().unwrap() > reads);
}

#[test]
fn a_patch_of_a_linked_file_pauses_too() {
    // Step 1 shows other.rs, step 2 patches it: main.rs is not patched,
    // and a save of main.rs still waits.
    let mut doc = document(false, false);
    doc["moves"] = json!([
        {"at_step": 1, "action": "code.link", "file": "other.rs"},
        {"at_step": 2, "action": "code.patch", "op": "replace",
         "find": "fn g() {}", "content": "fn g() { 1; }"},
    ]);
    doc["annotations"] = json!([]);
    let (live, src, _tiza) = followed_doc("linked-patch", doc, &[("other.rs", "fn g() {}\n")]);
    goto(&live, 1);
    assert_eq!(pause(&live), live_again());
    goto(&live, 2);
    assert_eq!(pause(&live), paused(false));
    save_and_read(&live, &src, NEW.as_bytes());
    assert_eq!(pause(&live), paused(true));
    assert_eq!(base(&live), lines(OLD));
    goto(&live, 1);
    assert_eq!(base(&live), lines(NEW));
}

#[test]
fn only_a_patch_pauses() {
    let (live, src, _tiza) = followed("only-patch");
    // Step 1 only moves the cursor: a save loads at once.
    goto(&live, 1);
    assert_eq!(pause(&live), live_again());
    save_and_read(&live, &src, NEW.as_bytes());
    assert_eq!(base(&live), lines(NEW));
    assert_eq!(reloads(&live), 1);
    // Steps 2 and 3: the patch of step 2 is applied.
    for step in [2, 3] {
        goto(&live, step);
        assert_eq!(pause(&live), paused(false), "step {step}");
        let b = bar(&live);
        assert!(
            b.contains("⟳ paused") && !b.contains("paused · changed"),
            "{b}"
        );
        assert!(
            status(&live)["reason"]
                .as_str()
                .is_some_and(|r| r.contains("step 1")),
            "{}",
            status(&live)
        );
    }
    goto(&live, 0);
    assert_eq!(pause(&live), live_again());
    assert!(bar(&live).contains("⟳ following"), "{}", bar(&live));
}

#[test]
fn the_reply_of_follow_on_fits_at_80_columns() {
    // Paused, `:follow on` says where a save loads; at 80 columns the bar
    // shows it whole, the step number with it, and the notice of a save too.
    let (live, src, _tiza) = followed("reply-80");
    goto(&live, 2);
    live.command("follow off");
    live.command("follow on");
    let said = "following main.rs; step 1 loads a save";
    assert_eq!(message(&live), said);
    let row = |live: &Live| {
        live.screen(80, 30)
            .lines()
            .last()
            .unwrap_or_default()
            .to_string()
    };
    let b = row(&live);
    assert!(b.contains("⟳ paused") && b.contains(said), "{b}");
    live.command("follow on");
    assert_eq!(
        message(&live),
        "already following main.rs; step 1 loads a save"
    );
    clear(&live);
    save_and_read(&live, &src, NEW.as_bytes());
    let b = row(&live);
    assert!(
        b.contains("⟳ paused · changed") && b.contains("step 1 loads the change to main.rs"),
        "{b}"
    );
}

#[test]
fn one_notice_per_pause_and_changed_follows_the_file() {
    let (live, src, tiza) = followed("one-notice");
    goto(&live, 2);
    clear(&live);
    // The real watcher: the save waits, the code on screen stays.
    save(&src, NEW.as_bytes());
    wait_for("the change to wait", || pause(&live) == paused(true));
    assert_notice(&live);
    assert!(bar(&live).contains("⟳ paused · changed"), "{}", bar(&live));
    assert_eq!(base(&live), lines(OLD));
    // What is saved agrees with a fresh load: the code on screen, embedded.
    assert!(saved_code(&tiza).is_array(), "{}", saved_code(&tiza));
    // A second save in the same pause: no notice.
    clear(&live);
    save_and_read(&live, &src, OTHER.as_bytes());
    assert_eq!(live.message(), None);
    assert_eq!(pause(&live), paused(true));
    // Saved back as the screen shows it: nothing waits, in silence.
    save_and_read(&live, &src, OLD.as_bytes());
    assert_eq!(live.message(), None);
    assert_eq!(pause(&live), paused(false));
    let b = bar(&live);
    assert!(
        b.contains("⟳ paused") && !b.contains("paused · changed"),
        "{b}"
    );
    // Changed again: still the same pause, still no notice.
    save_and_read(&live, &src, NEW.as_bytes());
    assert_eq!(live.message(), None);
    assert_eq!(pause(&live), paused(true));
    // Forward: it keeps waiting.
    goto(&live, 3);
    assert_eq!(pause(&live), paused(true));
    assert_eq!(base(&live), lines(OLD));
    // Back at the resume step: one normal reload.
    goto(&live, 1);
    assert_eq!(base(&live), lines(NEW));
    assert_eq!(reloads(&live), 1);
    let msg = message(&live);
    assert!(msg.contains("main.rs reloaded (#1)"), "{msg}");
    assert_eq!(pause(&live), live_again());
    assert_eq!(saved_code(&tiza), json!({"file": "main.rs"}));
    // A new pause gives its own notice.
    goto(&live, 2);
    clear(&live);
    save_and_read(&live, &src, OTHER.as_bytes());
    assert_notice(&live);
}

/// At step 2, a change waits and its notice is on the bar.
fn noticed(name: &str) -> (Live, PathBuf, PathBuf) {
    let (live, src, tiza) = followed(name);
    goto(&live, 2);
    clear(&live);
    save_and_read(&live, &src, NEW.as_bytes());
    assert_notice(&live);
    (live, src, tiza)
}

fn notice_on_bar(live: &Live) -> bool {
    bar(live).contains("loads the change")
}

#[test]
fn a_change_dropped_by_a_save_back_takes_its_notice_off_the_bar() {
    let (live, src, _tiza) = noticed("drop-notice");
    save_and_read(&live, &src, OLD.as_bytes());
    assert_eq!(pause(&live), paused(false));
    assert_eq!(live.message(), None);
    assert!(!notice_on_bar(&live), "{}", bar(&live));
}

#[test]
fn follow_off_by_an_agent_takes_the_notice_off_the_bar() {
    // The reply says what was dropped; the bar keeps no message of the
    // agent's, and not the notice either.
    let (live, _src, _tiza) = noticed("agent-off");
    let r = live.api_ok("follow.set", json!({"enabled": false}));
    assert!(
        r["message"]
            .as_str()
            .is_some_and(|m| m.contains("not loaded")),
        "{r}"
    );
    assert_eq!(live.message(), None);
    assert!(!notice_on_bar(&live), "{}", bar(&live));
}

#[test]
fn a_message_that_is_not_the_notice_stays() {
    let (live, src, _tiza) = noticed("other-message");
    live.command("note 1 hello");
    let said = live.message();
    assert!(
        said.as_ref()
            .is_some_and(|(m, _)| !m.contains("loads the change"))
    );
    save_and_read(&live, &src, OLD.as_bytes());
    assert_eq!(pause(&live), paused(false));
    assert_eq!(live.message(), said);
}

#[test]
fn the_reload_at_the_resume_step_takes_the_place_of_the_notice() {
    let (live, _src, _tiza) = noticed("notice-reload");
    goto(&live, 1);
    let msg = message(&live);
    assert!(msg.contains("main.rs reloaded (#1)"), "{msg}");
}

#[test]
fn an_unreadable_save_in_presentation_mode_takes_the_notice_off() {
    // No error in presentation mode, and no change waits any more: the
    // notice given before goes too.
    let (live, src, _tiza) = noticed("present-unreadable");
    live.with(|app| app.set_present(true));
    save_and_read(&live, &src, NOT_UTF8);
    assert_eq!(pause(&live), paused(false));
    assert_eq!(live.message(), None);
}

#[test]
fn a_save_back_redraws_the_bar() {
    // The TUI draws a frame again only when a read says the screen changed:
    // `⟳ paused · changed` must go when the change is dropped.
    let (live, src, _tiza) = followed("saved-back-bar");
    goto(&live, 2);
    save_and_read(&live, &src, NEW.as_bytes());
    let changed = live.with(move |app| {
        save(&src, OLD.as_bytes());
        app.read_save()
    });
    assert!(changed, "the bar must be drawn again");
    assert_eq!(pause(&live), paused(false));
}

#[test]
fn a_save_back_is_a_file_reference_again_at_once() {
    // The change waited, then the file was saved back: the document
    // references the file again at once (a quit or `:follow off` now
    // leaves no embedded code), and at the resume step nothing loads.
    let (live, src, tiza) = followed("saved-back");
    goto(&live, 2);
    save_and_read(&live, &src, NEW.as_bytes());
    assert!(saved_code(&tiza).is_array(), "{}", saved_code(&tiza));
    save_and_read(&live, &src, OLD.as_bytes());
    assert!(!live.with(|app| app.doc.is_embedded()));
    goto(&live, 1);
    assert_eq!(reloads(&live), 0);
    assert_eq!(base(&live), lines(OLD));
    assert_eq!(live.with(|app| app.doc.annotations.len()), 1);
    wait_for("the reference", || saved_code(&tiza).is_object());
    assert_eq!(saved_code(&tiza), json!({"file": "main.rs"}));
}

#[test]
fn a_jump_over_the_resume_step_answers_with_the_new_code() {
    let (live, src, _tiza) = followed("jump");
    goto(&live, 3);
    save_and_read(&live, &src, NEW.as_bytes());
    // Back to step 2: a patch is still applied.
    goto(&live, 2);
    assert_eq!(pause(&live), paused(true));
    assert_eq!(reloads(&live), 0);
    // The jump itself loads it, with no tick of the TUI after it.
    let (step, code) = live.with(|app| {
        app.goto_step("0").unwrap();
        (app.doc.step, app.doc.base().to_vec())
    });
    assert_eq!((step, code), (0, lines(NEW)));
    goto(&live, 2);
    save_and_read(&live, &src, OTHER.as_bytes());
    // `step.goto 0` from step 2: the reply comes after the reload.
    live.api_ok("step.goto", json!({"id": 0}));
    let code = live.api_ok("code.get", json!({}));
    assert!(
        code["text"]
            .as_str()
            .is_some_and(|t| t.contains("t.bytes().len()")),
        "{code}"
    );
    assert_eq!(reloads(&live), 2);
}

#[test]
fn several_paths_at_once_give_one_reload() {
    let (live, src, _tiza) = followed("one-reload");
    goto(&live, 2);
    save_and_read(&live, &src, NEW.as_bytes());
    // A jump to step 0, the ticks of the TUI and the save read again (the
    // watcher saw it twice), all before the next frame.
    let count = live.with(|app| {
        app.goto_step("0").unwrap();
        app.follow_tick();
        app.follow_tick();
        app.read_save();
        app.read_save();
        app.follow_tick();
        app.follow_status()["reloads"].as_u64()
    });
    assert_eq!(count, Some(1));
    assert_eq!(base(&live), lines(NEW));
    // One undo step.
    live.keys(&[KeyCode::Char('u')]);
    assert_eq!(base(&live), lines(OLD));
}

#[test]
fn the_wrap_of_a_loop_loads_and_a_stale_move_is_told_once() {
    let (live, src, _tiza) = followed_doc("loop", document(true, false), &[]);
    goto(&live, 2);
    save_and_read(&live, &src, STALE.as_bytes());
    assert_eq!(pause(&live), paused(true));
    live.with(|app| app.next_step());
    assert_eq!(live.with(|app| app.doc.step), 3);
    assert_eq!(pause(&live), paused(true));
    // The wrap to step 0 loads it: the patch is stale now.
    live.with(|app| app.next_step());
    assert_eq!(live.with(|app| app.doc.step), 0);
    assert_eq!(base(&live), lines(STALE));
    let msg = message(&live);
    assert!(msg.contains("stale"), "{msg}");
    assert_eq!(reloads(&live), 1);
    // Two more laps: no notice about the stale move, no reload.
    for lap in 2..=3 {
        for _ in 0..4 {
            clear(&live);
            live.with(|app| app.next_step());
            let msg = message(&live);
            assert!(!msg.contains("stale"), "lap {lap}: {msg}");
        }
        assert_eq!(live.with(|app| app.doc.step), 0, "lap {lap}");
    }
    assert_eq!(reloads(&live), 1);
}

#[test]
fn follow_off_drops_the_change_and_follow_on_starts_paused() {
    let (live, src, _tiza) = followed("off-on");
    goto(&live, 2);
    save_and_read(&live, &src, NEW.as_bytes());
    live.command("follow off");
    let msg = message(&live);
    assert!(
        msg.starts_with("follow off") && msg.contains("main.rs") && msg.contains("not loaded"),
        "{msg}"
    );
    assert_eq!(status(&live)["enabled"], false);
    assert_eq!(pause(&live), (json!(false), json!(false), Value::Null));
    assert_eq!(base(&live), lines(OLD));
    // On again at step 2: paused, and the file differs from the screen.
    live.command("follow on");
    assert_eq!(pause(&live), paused(true));
    assert!(bar(&live).contains("⟳ paused · changed"), "{}", bar(&live));
    assert_eq!(base(&live), lines(OLD));
    goto(&live, 1);
    assert_eq!(base(&live), lines(NEW));
    assert_eq!(reloads(&live), 1);
}

#[test]
fn follow_on_at_a_step_with_no_patch_loads_the_change() {
    let (live, src, _tiza) = followed("off-step0");
    goto(&live, 2);
    save_and_read(&live, &src, NEW.as_bytes());
    live.command("follow off");
    goto(&live, 0);
    assert_eq!(base(&live), lines(OLD));
    live.command("follow on");
    assert_eq!(base(&live), lines(NEW));
    assert_eq!(pause(&live), live_again());
}

#[test]
fn follow_off_with_nothing_waiting_says_only_follow_off() {
    let (live, _src, _tiza) = followed("off-plain");
    goto(&live, 2);
    live.command("follow off");
    assert_eq!(message(&live), "follow off");
}

#[test]
fn an_unreadable_file_is_told_once_and_is_no_change() {
    let (live, src, _tiza) = followed("utf8-pause");
    goto(&live, 2);
    clear(&live);
    save_and_read(&live, &src, NOT_UTF8);
    let (text, error) = live.message().unwrap_or_default();
    assert!(error && text.contains("not valid UTF-8"), "{text}");
    assert_eq!(pause(&live), paused(false));
    let b = bar(&live);
    assert!(
        b.contains("⟳ paused") && !b.contains("paused · changed"),
        "{b}"
    );
    // Unreadable again: no second error.
    clear(&live);
    save_and_read(&live, &src, b"fn g() {\n    \xfe\n}\n");
    assert_eq!(live.message(), None);
    // Readable and other than the screen: the change waits, with the
    // notice this pause did not give yet.
    save_and_read(&live, &src, NEW.as_bytes());
    assert_eq!(pause(&live), paused(true));
    assert_notice(&live);
    goto(&live, 1);
    assert_eq!(base(&live), lines(NEW));
}

#[test]
fn an_unreadable_file_at_the_resume_step_keeps_the_code_and_follow() {
    let (live, src, _tiza) = followed("utf8-resume");
    goto(&live, 2);
    save_and_read(&live, &src, NEW.as_bytes());
    save_and_read(&live, &src, NOT_UTF8);
    assert_eq!(pause(&live), paused(false));
    clear(&live);
    goto(&live, 1);
    let (text, error) = live.message().unwrap_or_default();
    assert!(error && text.contains("not valid UTF-8"), "{text}");
    assert_eq!(base(&live), lines(OLD));
    assert_eq!(status(&live)["enabled"], true);
    assert_eq!(reloads(&live), 0);
    // Readable again at a step with no patch: it loads.
    save_and_read(&live, &src, NEW.as_bytes());
    assert_eq!(base(&live), lines(NEW));
}

#[test]
fn an_unreadable_file_alone_is_read_again_at_the_resume_step() {
    let (live, src, _tiza) = followed("utf8-alone");
    goto(&live, 2);
    save_and_read(&live, &src, NOT_UTF8);
    clear(&live);
    goto(&live, 1);
    let (text, error) = live.message().unwrap_or_default();
    assert!(error && text.contains("not valid UTF-8"), "{text}");
    assert_eq!(base(&live), lines(OLD));
    assert_eq!(status(&live)["enabled"], true);
}

#[test]
fn an_undo_while_the_file_is_unreadable_keeps_the_read_at_the_resume_step() {
    let (live, src, _tiza) = followed("utf8-undo");
    goto(&live, 2);
    save_and_read(&live, &src, NOT_UTF8);
    live.command("note 1 hello");
    live.keys(&[KeyCode::Char('u')]);
    clear(&live);
    goto(&live, 1);
    let (text, error) = live.message().unwrap_or_default();
    assert!(error && text.contains("not valid UTF-8"), "{text}");
    assert_eq!(status(&live)["enabled"], true);
}

#[test]
fn a_file_removed_and_back_with_other_code_is_one_change() {
    let (live, src, _tiza) = followed("removed");
    goto(&live, 2);
    // A change waits, then the file goes: nothing is left to load.
    save_and_read(&live, &src, OTHER.as_bytes());
    assert_eq!(pause(&live), paused(true));
    std::fs::remove_file(&src).unwrap();
    live.with(|app| app.read_save());
    assert_eq!(status(&live)["missing"], true);
    assert_eq!(pause(&live), paused(false));
    assert!(
        bar(&live).contains("⟳ paused (file removed)"),
        "{}",
        bar(&live)
    );
    clear(&live);
    save_and_read(&live, &src, NEW.as_bytes());
    assert_eq!(status(&live)["missing"], false);
    assert_eq!(pause(&live), paused(true));
    // The same pause, but what the bar said of the file (it was removed)
    // is no longer true: the notice comes again.
    assert_eq!(
        live.message(),
        Some(("step 1 loads the change to main.rs".to_string(), false))
    );
    goto(&live, 1);
    assert_eq!(base(&live), lines(NEW));
    assert_eq!(reloads(&live), 1);
}

#[test]
fn a_file_removed_and_back_as_it_was_is_no_change() {
    let (live, src, _tiza) = followed("removed-same");
    goto(&live, 2);
    std::fs::remove_file(&src).unwrap();
    live.with(|app| app.read_save());
    save_and_read(&live, &src, OLD.as_bytes());
    assert_eq!(pause(&live), paused(false));
    goto(&live, 1);
    assert_eq!(reloads(&live), 0);
}

#[test]
fn presentation_mode_shows_no_notice() {
    let (live, src, _tiza) = followed("present");
    goto(&live, 2);
    live.with(|app| app.set_present(true));
    clear(&live);
    save_and_read(&live, &src, NOT_UTF8);
    assert_eq!(live.message(), None);
    save_and_read(&live, &src, NEW.as_bytes());
    assert_eq!(live.message(), None);
    assert_eq!(pause(&live), paused(true));
    // Out of presentation mode the bar shows it.
    live.with(|app| app.set_present(false));
    assert!(bar(&live).contains("⟳ paused · changed"), "{}", bar(&live));
}

#[test]
fn an_undo_of_a_reload_at_a_paused_step_turns_follow_off() {
    // As before the pause: the code on screen is not the file's any more,
    // so follow goes off, and nothing is paused.
    let (live, src, _tiza) = followed("undo");
    save_and_read(&live, &src, NEW.as_bytes());
    assert_eq!(reloads(&live), 1);
    goto(&live, 2);
    live.keys(&[KeyCode::Char('u')]);
    let msg = message(&live);
    assert!(
        msg.starts_with("follow off: the code on screen is not main.rs"),
        "{msg}"
    );
    assert_eq!(base(&live), lines(OLD));
    assert_eq!(live.with(|app| app.doc.step), 2);
    assert_eq!(status(&live)["enabled"], false);
    assert_eq!(pause(&live), (json!(false), json!(false), Value::Null));
    assert!(!bar(&live).contains("⟳"), "{}", bar(&live));
}

#[test]
fn the_empty_first_write_of_an_editor_is_never_the_change() {
    let (live, src, _tiza) = followed("empty");
    goto(&live, 2);
    clear(&live);
    // An editor that truncates the file, then writes it.
    let src2 = src.clone();
    let (pending, said) = live.with(move |app| {
        std::fs::write(&src2, "").unwrap();
        app.read_save();
        (app.follow_pending(), app.message.clone())
    });
    assert!(!pending);
    assert_eq!(said, None);
    std::fs::write(&src, NEW).unwrap();
    live.with(|app| app.read_save());
    assert_eq!(pause(&live), paused(true));
    goto(&live, 1);
    assert_eq!(base(&live), lines(NEW));
}

#[test]
fn a_save_while_the_command_line_or_a_panel_is_open_waits_for_it() {
    for (open, name) in [
        (KeyCode::Char(':'), "busy-cmd"),
        (KeyCode::Char('?'), "busy-help"),
    ] {
        let (live, src, _tiza) = followed(name);
        // All on the TUI thread: the watcher's own read of this save comes
        // after its debounce, later than the tick after Esc.
        let (under, after) = live.with(move |app| {
            tiza_tui::on_key(app, open, crossterm::event::KeyModifiers::NONE);
            save(&src, NEW.as_bytes());
            app.read_save();
            let under = app.doc.base().to_vec();
            tiza_tui::on_key(app, KeyCode::Esc, crossterm::event::KeyModifiers::NONE);
            app.follow_tick();
            (under, app.doc.base().to_vec())
        });
        assert_eq!(under, lines(OLD), "{name}: loaded under it");
        assert_eq!(after, lines(NEW), "{name}: read when it closed");
    }
}

#[test]
fn a_load_while_a_wait_runs_keeps_the_new_code() {
    // Step 1 has only a `wait`: no patch, so `follow.set` loads the file at
    // once while the wait runs. The end of the wait keeps the new code.
    let mut doc = document(false, false);
    doc["moves"] = json!([
        {"at_step": 1, "action": "wait", "ms": 60000},
        {"at_step": 2, "action": "code.patch", "op": "replace",
         "find": "n + 1", "content": "    n * 2"},
    ]);
    let (live, src, _tiza) = followed_doc("wait-load", doc, &[]);
    live.command("follow off");
    save(&src, NEW.as_bytes());
    let (typing, code) = live.with(|app| {
        app.next_step();
        let typing = app.is_typing();
        app.set_follow(true).unwrap();
        app.finish_typing();
        (typing, app.doc.code.clone())
    });
    assert!(typing, "the wait of step 1 runs");
    assert_eq!(code, lines(NEW));
}

#[test]
fn a_save_while_a_move_types_waits_for_it() {
    let (live, src, _tiza) = followed_doc("typing", document(false, true), &[]);
    goto(&live, 1);
    clear(&live);
    let src2 = src.clone();
    // The step, the save and the checks in one call: no tick comes between
    // them, so the typing cannot end first. Its natural end comes after.
    let (typing, pending) = live.with(move |app| {
        app.next_step();
        save(&src2, NEW.as_bytes());
        app.read_save();
        (app.is_typing(), app.follow_pending())
    });
    assert!(typing, "the patch of step 2 types");
    assert!(!pending, "read while the move types");
    // Once the typing ends the save is read: it waits for step 1.
    wait_for("the typing to end", || !live.with(|app| app.is_typing()));
    wait_for("the change to wait", || pause(&live) == paused(true));
    assert_notice(&live);
    goto(&live, 1);
    assert_eq!(base(&live), lines(NEW));
}
