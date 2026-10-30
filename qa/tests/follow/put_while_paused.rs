//! A `scene.put` while follow is paused. At a step with a patch, a save of
//! the followed file waits for the resume step, and the screen keeps the
//! code it had (embedded over the file meanwhile). A `scene.get` → `scene.put`
//! of that copy was refused with «the screen shows the file as it is»,
//! which is not true then: the file on disk is newer than the screen.
//!
//! Acceptance criteria:
//! - AC1 While paused, a put whose code is the code on screen is accepted:
//!   the code does not change, the change still waits, and it loads at the
//!   resume step (step 0 or a later one). The rest of the put applies.
//! - AC2 While a change waits, a put with other code (also the code of the
//!   file on disk, or the same code numbered from another line) is refused
//!   with the true reason: the code on screen is not what the file holds
//!   now (a save, or a paste before `:follow on`), and the step where the
//!   file loads. So is a document opened embedded over the file it follows.
//! - AC3 Without a pause the message is the one of before, which is true
//!   there; a pause with a file that cannot be read takes the put of the
//!   code on screen too.
//!
//! The follows have no watcher (`crate::by_hand`): the saves are read
//! with `App::read_save` (what the watcher does after its debounce) on the
//! TUI thread, so that what a save did is known when the call returns.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{Live, scratch};

const OLD: &str = "fn f(t: &str) -> usize {\n    let n = t.len();\n    n + 1\n}\n";
const NEW: &str = "fn f(t: &str) -> usize {\n    let n = t.chars().count();\n    n + 1\n}\n";
const NOT_UTF8: &[u8] = b"fn f() {\n    \xff\xfe\n}\n";

/// The patch of `n + 1` at `patch_step`, a cursor move before it when
/// that step is 2: the resume step is `patch_step - 1`.
fn document(patch_step: u32) -> Value {
    let mut moves = vec![json!({
        "at_step": patch_step, "action": "code.patch", "op": "replace",
        "find": "n + 1", "content": "    n * 2"
    })];
    if patch_step > 1 {
        moves.insert(0, json!({"at_step": 1, "action": "cursor", "line": 1}));
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
        "moves": moves,
    })
}

/// `main.rs` (OLD) and its sidecar, following, at the step of the patch.
fn paused_at(name: &str, patch_step: u32) -> (Live, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, OLD).unwrap();
    let tiza = dir.join("main.rs.tiza");
    std::fs::write(&tiza, document(patch_step).to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(doc, tiza, dir.join("live.sock"));
    crate::by_hand(&live);
    live.command("follow on");
    live.api_ok("step.set", json!({"step": patch_step}));
    (live, src)
}

/// Saves like an editor, then reads it as the watcher does after a save.
fn save_and_read(live: &Live, src: &Path, content: &[u8]) {
    let tmp = src.with_extension("rs.tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, src).unwrap();
    live.with(|app| app.read_save());
}

fn base(live: &Live) -> Vec<String> {
    live.with(|app| app.doc.base().to_vec())
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

/// `paused`, `pending` and `resume_step` of `follow.status`, and `reloads`.
fn pause(live: &Live) -> (Value, Value, Value, u64) {
    let st = live.api_ok("follow.status", json!({}));
    (
        st["paused"].clone(),
        st["pending"].clone(),
        st["resume_step"].clone(),
        st["reloads"].as_u64().unwrap_or(0),
    )
}

fn bar(live: &Live) -> String {
    let screen = live.screen(160, 30);
    screen.lines().last().unwrap_or_default().to_string()
}

/// The document `scene.get` gives, with its code changed by `edit`.
fn got(live: &Live, edit: impl FnOnce(&mut Value)) -> Value {
    let mut doc = live.api_ok("scene.get", json!({}))["document"].clone();
    edit(&mut doc);
    json!({ "document": doc })
}

fn refused(live: &Live, params: Value) -> String {
    let r = live.api("scene.put", params);
    let e = r.error.expect("scene.put was accepted");
    assert_eq!(e.code, "invalid_argument", "{e:?}");
    e.message
}

#[test]
fn an_unchanged_get_and_put_while_a_change_waits_keeps_the_pause() {
    for (patch_step, resume) in [(2, 1), (1, 0)] {
        let (live, src) = paused_at(&format!("put-paused-{patch_step}"), patch_step);
        save_and_read(&live, &src, NEW.as_bytes());
        let before = pause(&live);
        assert_eq!(
            (&before.0, &before.1, &before.2),
            (&json!(true), &json!(true), &json!(resume)),
            "step {patch_step}"
        );
        let revision = live.api_ok("status", json!({}))["revision"].clone();
        live.api_ok("scene.put", got(&live, |_| {}));
        // Nothing changed: the screen, the pause, the bar.
        assert_eq!(base(&live), lines(OLD), "step {patch_step}");
        assert_eq!(pause(&live), before, "step {patch_step}");
        assert_eq!(live.api_ok("status", json!({}))["revision"], revision);
        assert!(bar(&live).contains("paused · changed"), "{}", bar(&live));
        // The change loads at the resume step, once.
        live.api_ok("step.set", json!({"step": resume}));
        assert_eq!(base(&live), lines(NEW), "step {patch_step}");
        assert_eq!(pause(&live).3, before.3 + 1, "step {patch_step}");
    }
}

#[test]
fn a_put_of_the_screen_with_a_new_annotation_while_a_change_waits() {
    let (live, src) = paused_at("put-paused-note", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    let put = got(&live, |doc| {
        doc["annotations"].as_array_mut().unwrap().push(json!({
            "id": "n1", "step": 1, "type": "note", "line": 3, "text": "doubled"
        }));
    });
    live.api_ok("scene.put", put);
    assert!(live.with(|app| app.doc.find_annotation("n1").is_some()));
    assert_eq!(base(&live), lines(OLD));
    assert_eq!(pause(&live).1, json!(true));
    live.api_ok("step.set", json!({"step": 1}));
    assert_eq!(base(&live), lines(NEW));
    assert!(live.with(|app| app.doc.find_annotation("n1").is_some()));
}

/// The refusal of a put with other code while a change waits.
fn said(when: &str) -> String {
    format!(
        "scene.put: follow is paused, and the code on screen is not what main.rs holds now: \
         main.rs loads {when}. Send the code as scene.get gives it; to change the code, \
         edit main.rs"
    )
}

#[test]
fn a_put_with_other_code_while_a_change_waits_says_why() {
    for (patch_step, when) in [(2, "at step 1 or earlier"), (1, "at step 0")] {
        let (live, src) = paused_at(&format!("put-paused-other-{patch_step}"), patch_step);
        save_and_read(&live, &src, NEW.as_bytes());
        let before = pause(&live);
        // Code of the agent's own, the code of the file on disk, and the
        // code on screen numbered from another line.
        let puts = [
            got(&live, |doc| doc["code"] = json!(["fn f() {}"])),
            got(&live, |doc| doc["code"] = json!(lines(NEW))),
            got(&live, |doc| doc["source"]["first_line"] = json!(5)),
        ];
        for put in puts {
            assert_eq!(refused(&live, put.clone()), said(when), "{put}");
        }
        assert_eq!(base(&live), lines(OLD));
        assert_eq!(pause(&live), before);
    }
}

#[test]
fn a_paste_before_follow_on_waits_too_and_is_named_as_such() {
    // A paste turns follow off, then `:follow on` at the step of the
    // patch: the file did not change, but it is not the code on screen.
    let (live, _src) = paused_at("put-paused-paste", 2);
    live.api_ok("step.set", json!({"step": 0}));
    // A paste is at step 0. The patch of step 2 still finds `n + 1` in it.
    live.with(|app| app.paste("fn g(t: &str) -> usize {\n    let n = 3;\n    n + 1\n}\n"));
    live.api_ok("step.set", json!({"step": 2}));
    live.command("follow on");
    assert_eq!(pause(&live).1, json!(true));
    live.api_ok("scene.put", got(&live, |_| {}));
    let put = got(&live, |doc| doc["code"] = json!(["fn f() {}"]));
    assert_eq!(refused(&live, put), said("at step 1 or earlier"));
}

#[test]
fn a_document_opened_over_the_file_it_follows_is_checked_too() {
    // What the autosave writes during a pause with a change: the code on
    // screen embedded, and `follows` naming the file.
    let dir = scratch("put-paused-embedded");
    std::fs::write(dir.join("main.rs"), NEW).unwrap();
    let mut doc = document(2);
    doc["code"] = json!(lines(OLD));
    doc["follows"] = json!({"file": "main.rs"});
    let tiza = dir.join("main.rs.tiza");
    std::fs::write(&tiza, doc.to_string()).unwrap();
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    let live = Live::open(doc, tiza, dir.join("live.sock"));
    crate::by_hand(&live);
    live.api_ok("step.set", json!({"step": 2}));
    live.command("follow on");
    assert_eq!(pause(&live).1, json!(true));
    live.api_ok("scene.put", got(&live, |_| {}));
    let put = got(&live, |doc| doc["code"] = json!(["fn f() {}"]));
    assert_eq!(refused(&live, put), said("at step 1 or earlier"));
    live.api_ok("step.set", json!({"step": 1}));
    assert_eq!(base(&live), lines(NEW));
}

#[test]
fn without_a_change_that_waits_the_messages_are_the_ones_of_before() {
    // Follow on, at step 0: the screen is the file.
    let (live, _src) = paused_at("put-live", 2);
    live.api_ok("step.set", json!({"step": 0}));
    let put = got(&live, |doc| doc["code"] = json!(["fn f() {}"]));
    assert!(refused(&live, put).contains("follow is on (the screen shows the file as it is)"),);
    // Paused with no save: the file is still the code on screen.
    live.api_ok("step.set", json!({"step": 2}));
    assert_eq!(pause(&live).1, json!(false));
    live.api_ok("scene.put", got(&live, |_| {}));
    // The code is still the file's: a reference, not a copy.
    assert!(!live.with(|app| app.doc.is_embedded()));
    let put = got(&live, |doc| doc["code"] = json!(["fn f() {}"]));
    assert!(refused(&live, put).contains("follow is on (the screen shows the file as it is)"),);
}

#[test]
fn a_pause_with_a_file_that_cannot_be_read_takes_the_code_on_screen() {
    let (live, src) = paused_at("put-paused-unreadable", 2);
    save_and_read(&live, &src, NOT_UTF8);
    live.api_ok("scene.put", got(&live, |_| {}));
    assert_eq!(base(&live), lines(OLD));
    // Removed: the same.
    std::fs::remove_file(&src).unwrap();
    live.with(|app| app.read_save());
    live.api_ok("scene.put", got(&live, |_| {}));
    assert_eq!(base(&live), lines(OLD));
}

#[test]
fn a_put_while_a_change_waits_cannot_change_the_file_that_follow_reads() {
    let (live, src) = paused_at("put-paused-follows", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    let before = pause(&live);
    let said = FOLLOWS;
    for other in [json!({"file": "main0.rs"}), json!({"file": "nowhere.rs"})] {
        let put = got(&live, |doc| doc["follows"] = other.clone());
        assert_eq!(refused(&live, put), said, "{other}");
    }
    assert_eq!(pause(&live), before);
    // The same file written another way: fine. (`..` and the folder's
    // name: only the resolved folder tells that it is the same.)
    let folder = src.parent().unwrap().file_name().unwrap().to_str().unwrap();
    for same in [
        json!({"file": "./main.rs"}),
        json!({"file": src.to_str().unwrap()}),
        json!({"file": format!("../{folder}/main.rs")}),
    ] {
        live.api_ok("scene.put", got(&live, |doc| doc["follows"] = same.clone()));
    }
    // Left out, `null`, or as scene.get gives it: fine.
    live.api_ok(
        "scene.put",
        got(&live, |doc| {
            doc.as_object_mut().unwrap().remove("follows");
        }),
    );
    live.api_ok("scene.put", got(&live, |doc| doc["follows"] = Value::Null));
    live.api_ok("scene.put", got(&live, |_| {}));
    // The change of main.rs is what loads.
    live.api_ok("step.set", json!({"step": 1}));
    assert_eq!(base(&live), lines(NEW));
}

/// The refusal of another `follows` while follow reads the file over the
/// code on screen.
const FOLLOWS: &str = "scene.put: follow reads main.rs over the code on screen: a put cannot \
                       change `follows` while follow is on. Send `follows` as scene.get gives \
                       it, or leave it out";

#[test]
fn another_follows_is_refused_also_when_the_file_is_missing_after_the_pause() {
    // A change waits, the file goes away, the presenter goes to the resume
    // step: not paused, but follow still reads main.rs over the code on
    // screen when it comes back.
    let (live, src) = paused_at("put-missing-follows", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    std::fs::remove_file(&src).unwrap();
    live.with(|app| app.read_save());
    live.api_ok("step.set", json!({"step": 1}));
    assert_eq!(pause(&live).0, json!(false));
    let put = got(&live, |doc| doc["follows"] = json!({"file": "main0.rs"}));
    assert_eq!(refused(&live, put), FOLLOWS);
}

#[test]
fn another_document_with_its_own_follows_goes_in_while_a_change_waits() {
    // Another document turns follow off; its `follows` is its own.
    let (live, src) = paused_at("put-paused-other-doc", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    let put = got(&live, |doc| {
        doc["source"]["path"] = json!("other.rs");
        doc["follows"] = json!({"file": "other.rs"});
    });
    live.api_ok("scene.put", put);
    assert_eq!(
        live.api_ok("follow.status", json!({}))["enabled"],
        json!(false)
    );
}

/// The message on the bar.
fn bar_message(live: &Live) -> String {
    live.message().unwrap_or_default().0
}

#[test]
fn a_put_or_a_move_that_moves_the_resume_step_renews_the_notice() {
    // A put that moves the patch to step 1: the change loads at step 0.
    let (live, src) = paused_at("put-paused-notice", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    assert_eq!(bar_message(&live), "step 1 loads the change to main.rs");
    let put = got(&live, |doc| {
        for m in doc["moves"].as_array_mut().unwrap() {
            if m["action"] == "code.patch" {
                m["at_step"] = json!(1);
            }
        }
    });
    live.api_ok("scene.put", put);
    assert_eq!(pause(&live).2, json!(0));
    assert_eq!(bar_message(&live), "step 0 loads the change to main.rs");
    // A move added at step 1 does the same.
    let (live, src) = paused_at("move-paused-notice", 2);
    save_and_read(&live, &src, NEW.as_bytes());
    let added = live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "// new"}),
    );
    assert_eq!(pause(&live).2, json!(0));
    assert_eq!(bar_message(&live), "step 0 loads the change to main.rs");
    // Deleted, the resume step is 1 again, and the notice says so.
    live.api_ok("move.delete", json!({"index": added["index"]}));
    assert_eq!(pause(&live).2, json!(1));
    assert_eq!(bar_message(&live), "step 1 loads the change to main.rs");
    // Another message on the bar stays: the notice is not there to renew.
    live.with(|app| app.message = Some(("something else".into(), false)));
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "1", "content": "// new"}),
    );
    assert_eq!(pause(&live).2, json!(0));
    assert_eq!(bar_message(&live), "something else");
}

#[test]
fn a_put_or_a_move_that_ends_the_pause_loads_the_change_before_its_reply() {
    // The save has one more line, so the reply's `lines` shows the load.
    let longer = format!("{NEW}// end\n");
    let status = |live: &Live| live.api_ok("status", json!({}));
    // A put without the patch, and a put at the resume step.
    for (name, edit) in [
        ("put-paused-nopatch", "nopatch"),
        ("put-paused-step1", "step1"),
    ] {
        let (live, src) = paused_at(name, 2);
        save_and_read(&live, &src, longer.as_bytes());
        let put = got(&live, |doc| {
            if edit == "nopatch" {
                doc["moves"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|m| m["action"] != "code.patch");
            } else {
                doc["step"] = json!(1);
            }
        });
        let reply = live.api_ok("scene.put", put);
        assert_eq!(base(&live), lines(&longer), "{edit}");
        assert_eq!(reply["lines"], json!(5), "{edit}: {reply}");
        assert_eq!(
            reply["revision"],
            status(&live)["revision"],
            "{edit}: {reply}"
        );
    }
    // A scene.patch to the resume step.
    let (live, src) = paused_at("patch-paused-step1", 2);
    save_and_read(&live, &src, longer.as_bytes());
    let reply = live.api_ok("scene.patch", json!({"step": 1}));
    assert_eq!(base(&live), lines(&longer));
    assert_eq!(reply["lines"], json!(5), "{reply}");
    assert_eq!(reply["revision"], status(&live)["revision"], "{reply}");
    // A move deleted: the patch, move #2.
    let (live, src) = paused_at("move-paused-delete", 2);
    save_and_read(&live, &src, longer.as_bytes());
    live.api_ok("step.set", json!({"step": 1}));
    live.api_ok("step.set", json!({"step": 2}));
    assert_eq!(base(&live), lines(&longer));
    save_and_read(&live, &src, NEW.as_bytes());
    let reply = live.api_ok("move.delete", json!({"index": 2}));
    assert_eq!(base(&live), lines(NEW));
    assert_eq!(reply["revision"], status(&live)["revision"], "{reply}");
}
