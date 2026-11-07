//! The socket methods of a talk: `status.talk`, the `scene` of a step
//! reply, `talk.get`, `talk.goto`, `talk.put` and `talk.insert`.
//!
//! Acceptance criteria:
//! - AC1 `status` has `talk` when a master is open (the master, the scene
//!   on screen, its place among the scenes that show), and no `talk` for a
//!   document alone. A step reply in a talk names its scene.
//! - AC2 `talk.goto` goes to a scene by position or name, at a step;
//!   `talk.put` replaces the list (order, `skip`, names) and writes only
//!   that in the master; `talk.insert` adds an entry and can write its new
//!   file; `scene.put` of the scene on screen keeps autosave on.
//! - AC3 Each of them checks everything before it writes: a bad request
//!   leaves the master and the files as they were.
//! - AC4 In supervised mode they wait for the presenter, and a change that
//!   the list moved under is a `conflict`.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};
use tiza_core::protocol::Response;
use tiza_qa::{Live, fixture, scratch};
use tiza_tui::SessionStart;

/// A copy of `qa/fixtures/shared/talk/` in a folder of its own. The
/// master lists: 1 intro, 2 borrow, 3 own (name `own`), 4 not-shown
/// (skip), 5 loop, 6 again (name `borrow`), 7 2024.
fn talk_copy(name: &str) -> PathBuf {
    let dir = scratch(name);
    copy_dir(&fixture("shared/talk"), &dir);
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let target = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).unwrap();
        }
    }
}

/// The talk of `dir`, opened at the scene `own` (position 3).
fn open(dir: &Path, start: SessionStart) -> Live {
    Live::open_talk(
        &dir.join("talk.tiza"),
        Some("own"),
        start,
        dir.join("s.sock"),
    )
}

fn no_save() -> SessionStart {
    SessionStart {
        no_save: true,
        ..Default::default()
    }
}

fn master(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("talk.tiza")).unwrap()).unwrap()
}

fn bytes(dir: &Path) -> Vec<u8> {
    std::fs::read(dir.join("talk.tiza")).unwrap()
}

/// The code and the message of an error reply.
fn err(r: &Response) -> (String, String) {
    let e = r
        .error
        .as_ref()
        .unwrap_or_else(|| panic!("no error: {r:?}"));
    (e.code.clone(), e.message.clone())
}

/// The position of the scene on screen, from `status`.
fn position(live: &Live) -> u64 {
    live.api_ok("status", json!({}))["talk"]["position"]
        .as_u64()
        .unwrap()
}

/// The file name of the scene on screen.
fn scene_file(live: &Live) -> String {
    live.with(|a| {
        a.save_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    })
}

/// The `entry` values of `talk.get`: a list that `talk.put` takes.
fn entries(live: &Live) -> Vec<Value> {
    live.api_ok("talk.get", json!({}))["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["entry"].clone())
        .collect()
}

/// An annotation on the scene on screen: a change of the document.
fn edit(live: &Live) {
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "x"}),
    );
}

// ---------------------------------------------------------------- AC1

#[test]
fn status_says_where_the_talk_is_and_a_document_alone_has_no_talk() {
    let dir = talk_copy("tapi-status");
    let live = Live::open_talk(
        &dir.join("talk.tiza"),
        Some("loop"),
        SessionStart::default(),
        dir.join("s.sock"),
    );
    let st = live.api_ok("status", json!({}));
    let t = &st["talk"];
    assert_eq!(t["position"], 5, "{st}");
    assert_eq!(t["name"], "loop", "{st}");
    // Entry 4 is skipped: `loop` is the 4th of 6 scenes that show.
    assert_eq!(t["shown"], 4, "{st}");
    assert_eq!(t["shown_count"], 6, "{st}");
    assert_eq!(t["title"], "A short talk", "{st}");
    let m = t["master"].as_str().unwrap();
    assert!(
        Path::new(m).is_absolute() && m.ends_with("talk.tiza"),
        "{st}"
    );
    assert!(t.get("scenes").is_none(), "the list is in talk.get: {st}");

    let alone = Live::from_code("fn main() {}\n", "rs", "tapi-alone");
    let st = alone.api_ok("status", json!({}));
    assert!(st.get("talk").is_none(), "{st}");
    let r = alone.api_ok("step.next", json!({}));
    assert!(r.get("scene").is_none(), "{r}");
}

#[test]
fn a_step_reply_names_its_scene_and_the_next_scene_after_the_last_step() {
    let dir = talk_copy("tapi-step");
    let live = open(&dir, SessionStart::default());
    let r = live.api_ok("step.set", json!({"step": 1}));
    assert_eq!(r["scene"], json!({"position": 3, "name": "own"}), "{r}");
    live.api_ok("step.set", json!({"step": 2}));
    // The last step of `own`: past it comes `loop` (entry 4 is skipped).
    let r = live.api_ok("step.next", json!({}));
    assert_eq!(r["scene"], json!({"position": 5, "name": "loop"}), "{r}");
    assert_eq!(r["step"], 0, "{r}");
    assert_eq!(scene_file(&live), "loop.tiza");
    let r = live.api_ok("step.prev", json!({}));
    assert_eq!(r["scene"]["position"], 3, "{r}");
}

#[test]
fn step_next_on_the_last_step_of_the_last_scene_does_not_move_also_with_loop() {
    let dir = talk_copy("tapi-last-loop");
    let last = dir.join("2024.tiza");
    let looping = std::fs::read_to_string(&last).unwrap().replacen(
        "\"step\": 0,",
        "\"step\": 0, \"loop\": true,",
        1,
    );
    std::fs::write(&last, looping).unwrap();
    let live = open(&dir, SessionStart::default());
    live.api_ok("talk.goto", json!({"scene": 7}));
    assert!(live.with(|a| a.doc.looping));
    let end = live.with(|a| a.doc.max_step());
    live.api_ok("step.set", json!({"step": end}));
    // As the key `n`: the last scene does not go back to step 0.
    let r = live.api_ok("step.next", json!({}));
    assert_eq!(r["scene"]["position"], 7, "{r}");
    assert_eq!(r["step"], end, "{r}");
}

#[test]
fn a_scene_of_one_image_has_one_step_over_the_socket() {
    // `qa/fixtures/shared/one-image-talk/`: a cover and an end with
    // nothing on them, and a scene with steps 0 to 2 between them.
    let dir = scratch("tapi-one-image");
    copy_dir(&fixture("shared/one-image-talk"), &dir);
    let live = Live::open_talk(
        &dir.join("talk.tiza"),
        None,
        SessionStart::default(),
        dir.join("s.sock"),
    );
    let steps: Vec<Value> = live.api_ok("talk.get", json!({}))["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["steps"].clone())
        .collect();
    assert_eq!(steps, [json!(0), json!(2), json!(0)]);
    // As the key `n`: one `step.next` leaves the cover.
    let r = live.api_ok("step.next", json!({}));
    let place = |r: &Value| (r["scene"]["position"].clone(), r["step"].clone());
    assert_eq!(place(&r), (json!(2), json!(0)), "{r}");
}

// ---------------------------------------------------------------- AC2

#[test]
fn goto_takes_a_position_a_name_and_a_step() {
    let dir = talk_copy("tapi-goto");
    let live = open(&dir, SessionStart::default());
    let r = live.api_ok("talk.goto", json!({"scene": 2}));
    assert_eq!(
        (r["scene"]["position"].clone(), r["step"].clone()),
        (json!(2), json!(0)),
        "{r}"
    );
    // Two scenes are named `borrow`: the name goes to the first.
    live.api_ok("talk.goto", json!({"scene": 6}));
    let r = live.api_ok("talk.goto", json!({"scene": "borrow", "step": "shared"}));
    assert_eq!(r["scene"]["position"], 2, "{r}");
    assert_eq!(r["id"], "shared", "{r}");
    let r = live.api_ok("talk.goto", json!({"scene": "6", "step": 1}));
    assert_eq!(
        (r["scene"]["position"].clone(), r["step"].clone()),
        (json!(6), json!(1)),
        "{r}"
    );
    assert_eq!(scene_file(&live), "again.tiza");
}

#[test]
fn goto_errors_name_the_scenes_and_change_nothing() {
    let dir = talk_copy("tapi-goto-err");
    let live = open(&dir, SessionStart::default());
    for (scene, code, says) in [
        (json!(4), "not_found", "skipped"),
        (json!(99), "not_found", "1 to 7"),
        (json!("nope"), "not_found", "own"),
        (json!(-1), "invalid_argument", "position"),
        (json!(null), "invalid_argument", "position"),
    ] {
        let r = live.api("talk.goto", json!({"scene": scene}));
        let (c, m) = err(&r);
        assert_eq!(c, code, "{scene}: {m}");
        assert!(m.contains(says), "{scene}: {m}");
        assert_eq!(position(&live), 3, "{scene}");
    }
    let (c, m) = err(&live.api("talk.goto", json!({"scene": 2, "step": "nope"})));
    assert_eq!(c, "not_found", "{m}");
    assert_eq!(position(&live), 3);
}

/// To the scene on screen is a goto of the step: it does not open the
/// scene again, so changes that are not saved (autosave off) do not stop
/// it, as they stop a change of scene.
#[test]
fn goto_the_scene_on_screen_is_a_goto_of_its_step() {
    let dir = talk_copy("tapi-goto-here");
    let live = open(&dir, SessionStart::default());
    live.command("autosave off");
    let before = live.with(|a| a.doc.annotations.len());
    edit(&live);
    let r = live.api_ok("talk.goto", json!({"scene": "own", "step": 2}));
    assert_eq!(
        (r["scene"]["position"].clone(), r["step"].clone()),
        (json!(3), json!(2)),
        "{r}"
    );
    assert_eq!(
        live.with(|a| a.doc.annotations.len()),
        before + 1,
        "the edit is still there"
    );
}

#[test]
fn unsaved_changes_stop_goto_and_step_next_until_force() {
    let dir = talk_copy("tapi-unsaved");
    let live = open(&dir, SessionStart::default());
    live.command("autosave off");
    edit(&live);
    let (c, m) = err(&live.api("talk.goto", json!({"scene": 2})));
    assert_eq!(c, "unsaved_changes", "{m}");
    assert!(m.contains("force: true"), "{m}");
    assert_eq!(position(&live), 3);
    live.api_ok("step.set", json!({"step": 2}));
    let (c, m) = err(&live.api("step.next", json!({})));
    assert_eq!(c, "unsaved_changes", "{m}");
    assert!(m.contains("talk.goto with force: true"), "{m}");
    assert_eq!(position(&live), 3);
    let r = live.api_ok("talk.goto", json!({"scene": 2, "force": true}));
    assert_eq!(r["scene"]["position"], 2, "{r}");
}

#[test]
fn put_reorders_writes_only_the_list_and_keeps_the_scene_on_screen() {
    let dir = talk_copy("tapi-put");
    let live = open(&dir, SessionStart::default());
    let before = live.api_ok("status", json!({}))["revision"]
        .as_u64()
        .unwrap();
    let notes = live.with(|a| a.doc.annotations.len());
    edit(&live);
    let mut list = entries(&live);
    let own = list.remove(2);
    list.insert(0, own);
    let r = live.api_ok("talk.put", json!({"scenes": list}));
    assert_eq!(r["position"], 1, "own is first now: {r}");
    assert!(r["revision"].as_u64().unwrap() > before, "{r}");
    assert_eq!(scene_file(&live), "own.tiza");
    let raw = master(&dir);
    assert_eq!(raw["talk"]["scenes"], json!(list), "{raw}");
    assert_eq!(raw["kept"], "a key that this version does not know");
    assert_eq!(raw["talk"]["title"], "A short talk");
    // The undo of the scene on screen is still there.
    live.command("undo");
    assert_eq!(
        live.with(|a| a.doc.annotations.len()),
        notes,
        "undo undid the edit"
    );
    // And the next scene is the one of the new list.
    live.api_ok("step.set", json!({"step": 2}));
    let r = live.api_ok("step.next", json!({}));
    assert_eq!(r["scene"], json!({"position": 2, "name": "intro"}), "{r}");
}

/// What `talk.get` gives goes back to `talk.put` as it is.
#[test]
fn the_entries_of_get_go_back_to_put_and_leave_the_master_the_same() {
    let dir = talk_copy("tapi-round");
    let live = open(&dir, SessionStart::default());
    // The first put writes the master in the layout of Tiza.
    live.api_ok("talk.put", json!({"scenes": entries(&live)}));
    let once = bytes(&dir);
    live.api_ok("talk.put", json!({"scenes": entries(&live)}));
    assert_eq!(bytes(&dir), once);
    let raw = master(&dir);
    assert_eq!(
        raw["talk"]["scenes"][3],
        json!({"file": "not-shown.tiza", "skip": true})
    );
    assert_eq!(
        raw["talk"]["scenes"][2],
        json!({"file": "own.tiza", "name": "own"})
    );
}

#[test]
fn a_bad_list_is_refused_and_the_master_keeps_its_bytes() {
    let dir = talk_copy("tapi-put-bad");
    let live = open(&dir, SessionStart::default());
    let before = bytes(&dir);
    let base = entries(&live);
    let with = |e: Value| {
        let mut l = base.clone();
        l.push(e);
        json!({"scenes": l})
    };
    for (params, code, says) in [
        (with(json!("missing.tiza")), "io", "missing.tiza"),
        (
            with(json!("./borrow.tiza")),
            "invalid_argument",
            "same file as scene 2",
        ),
        (
            with(json!({"file": "src/../borrow.tiza"})),
            "invalid_argument",
            "same file",
        ),
        (
            with(json!({"file": "2024.tiza", "name": "12"})),
            "invalid_argument",
            "12",
        ),
        (with(json!("notes.md")), "invalid_argument", "not a .tiza"),
        (
            with(json!({"file": "x.tiza", "colour": 1})),
            "invalid_argument",
            "unknown key",
        ),
        (json!({"scenes": []}), "invalid_argument", "empty"),
        (
            json!({"scenes": "intro.tiza"}),
            "invalid_argument",
            "invalid parameters",
        ),
    ] {
        let (c, m) = err(&live.api("talk.put", params.clone()));
        assert_eq!(c, code, "{params}: {m}");
        assert!(m.contains(says), "{params}: {m}");
        assert_eq!(bytes(&dir), before, "{params}");
        assert_eq!(position(&live), 3, "{params}");
    }
}

#[test]
fn the_scene_on_screen_can_be_skipped_but_not_removed() {
    let dir = talk_copy("tapi-put-here");
    let live = open(&dir, SessionStart::default());
    let mut list = entries(&live);
    let removed: Vec<Value> = list.iter().filter(|e| **e != list[2]).cloned().collect();
    let (c, m) = err(&live.api("talk.put", json!({"scenes": removed})));
    assert_eq!(c, "invalid_argument", "{m}");
    assert!(
        m.contains("scene 3 (own)") && m.contains("go to another scene"),
        "{m}"
    );
    list[2] = json!({"file": "own.tiza", "name": "own", "skip": true});
    let r = live.api_ok("talk.put", json!({"scenes": list}));
    assert_eq!(r["position"], 3, "{r}");
    assert_eq!(r["shown"], Value::Null, "{r}");
    assert_eq!(r["shown_count"], 5, "{r}");
    // Its own position still goes to it, and its end goes on.
    live.api_ok("talk.goto", json!({"scene": 3, "step": 2}));
    let r = live.api_ok("step.next", json!({}));
    assert_eq!(r["scene"]["position"], 5, "{r}");
    let (c, _) = err(&live.api("talk.goto", json!({"scene": 3})));
    assert_eq!(c, "not_found", "a skipped scene that is not on screen");
}

#[test]
fn a_master_edited_on_disk_is_a_conflict_until_force() {
    let dir = talk_copy("tapi-conflict");
    let live = open(&dir, SessionStart::default());
    let list = entries(&live);
    let mut raw = master(&dir);
    raw["talk"]["scenes"].as_array_mut().unwrap().pop();
    let edited = serde_json::to_string_pretty(&raw).unwrap();
    std::fs::write(dir.join("talk.tiza"), &edited).unwrap();
    let mut new = list.clone();
    new.swap(0, 1);
    let (c, m) = err(&live.api("talk.put", json!({"scenes": new})));
    assert_eq!(c, "conflict", "{m}");
    assert!(m.contains("changed while the talk ran"), "{m}");
    assert_eq!(
        std::fs::read_to_string(dir.join("talk.tiza")).unwrap(),
        edited
    );
    assert_eq!(entries(&live), list, "memory keeps the old list");
    live.api_ok("talk.put", json!({"scenes": new, "force": true}));
    let raw = master(&dir);
    assert_eq!(raw["talk"]["scenes"], json!(new));
    assert_eq!(raw["kept"], "a key that this version does not know");
    // The list in memory and on disk is the same again: no conflict.
    live.api_ok("talk.put", json!({"scenes": list}));
}

#[test]
fn insert_writes_a_new_scene_and_refuses_a_file_that_exists() {
    let dir = talk_copy("tapi-insert");
    let live = open(&dir, SessionStart::default());
    let scene = tiza_core::fs::load_document(&dir.join("2024.tiza")).unwrap();
    let doc = serde_json::to_value(&scene).unwrap();
    let r = live.api_ok(
        "talk.insert",
        json!({"file": "src/new.tiza", "at": 4, "scene": doc, "name": "fresh"}),
    );
    assert_eq!(r["created"], "src/new.tiza", "{r}");
    assert_eq!(r["position"], 3, "{r}");
    assert!(tiza_core::fs::load_document(&dir.join("src/new.tiza")).is_ok());
    let raw = master(&dir);
    assert_eq!(
        raw["talk"]["scenes"][3],
        json!({"file": "src/new.tiza", "name": "fresh"}),
        "{raw}"
    );
    let r = live.api_ok("talk.goto", json!({"scene": "fresh"}));
    assert_eq!(r["scene"]["position"], 4, "{r}");

    let before = bytes(&dir);
    for (file, says) in [
        ("src/new.tiza", "already exists"),
        ("intro.tiza", "already exists"),
        ("nofolder/x.tiza", "does not exist"),
        ("../x.tiza", "`..`"),
        ("/tmp/x.tiza", "absolute"),
    ] {
        let (c, m) = err(&live.api("talk.insert", json!({"file": file, "at": 1, "scene": doc})));
        assert_eq!(c, "invalid_argument", "{file}: {m}");
        assert!(m.contains(says), "{file}: {m}");
        assert_eq!(bytes(&dir), before, "{file}");
    }
    assert!(!dir.join("nofolder").exists());
    let (c, m) = err(&live.api(
        "talk.insert",
        json!({"file": "y.tiza", "at": 99, "scene": doc}),
    ));
    assert_eq!(c, "invalid_argument", "{m}");
    assert!(m.contains("from 1 to 9"), "{m}");
    assert!(!dir.join("y.tiza").exists());
    let (c, m) = err(&live.api(
        "talk.insert",
        json!({"file": "z.tiza", "at": 1, "scene": master(&dir)}),
    ));
    assert_eq!(c, "invalid_argument", "a master as a scene: {m}");
    assert!(!dir.join("z.tiza").exists());
}

/// The master changed on disk: the insert writes no scene file, so no
/// file stays behind to make the next try fail.
#[test]
fn an_insert_onto_a_changed_master_leaves_no_new_file() {
    let dir = talk_copy("tapi-insert-conflict");
    let live = open(&dir, SessionStart::default());
    let mut raw = master(&dir);
    raw["talk"]["title"] = json!("Another title");
    raw["talk"]["scenes"].as_array_mut().unwrap().pop();
    std::fs::write(dir.join("talk.tiza"), serde_json::to_string(&raw).unwrap()).unwrap();
    let doc = serde_json::to_value(tiza_core::fs::load_document(&dir.join("2024.tiza")).unwrap())
        .unwrap();
    let (c, m) = err(&live.api(
        "talk.insert",
        json!({"file": "n.tiza", "at": 1, "scene": doc}),
    ));
    assert_eq!(c, "conflict", "{m}");
    assert!(!dir.join("n.tiza").exists());
    let r = live.api_ok(
        "talk.insert",
        json!({"file": "n.tiza", "at": 1, "scene": doc, "force": true}),
    );
    assert_eq!(r["created"], "n.tiza", "{r}");
    assert_eq!(master(&dir)["talk"]["title"], "Another title");
}

#[test]
fn insert_of_a_file_that_does_not_open_changes_nothing() {
    let dir = talk_copy("tapi-insert-ref");
    let live = open(&dir, SessionStart::default());
    std::fs::write(dir.join("broken.tiza"), "{ not json").unwrap();
    let before = bytes(&dir);
    let (c, m) = err(&live.api("talk.insert", json!({"file": "broken.tiza", "at": 1})));
    assert_eq!(c, "invalid_json", "{m}");
    assert!(m.contains("broken.tiza"), "{m}");
    assert_eq!(bytes(&dir), before);
    let (c, m) = err(&live.api("talk.insert", json!({"file": "src/../2024.tiza", "at": 1})));
    assert_eq!(c, "invalid_argument", "the same file twice: {m}");
    assert!(m.contains("same file"), "{m}");
    assert_eq!(bytes(&dir), before);
    // A file that opens goes in, and the master names it.
    std::fs::copy(dir.join("2024.tiza"), dir.join("copy.tiza")).unwrap();
    let r = live.api_ok("talk.insert", json!({"file": "copy.tiza", "at": 1}));
    assert!(r.get("created").is_none(), "{r}");
    assert_eq!(master(&dir)["talk"]["scenes"][0], "copy.tiza");
    assert_eq!(r["position"], 4, "own moved one place: {r}");
}

#[test]
fn with_no_save_the_list_is_not_changed_and_no_file_is_written() {
    let dir = talk_copy("tapi-nosave");
    let live = open(&dir, no_save());
    let before = bytes(&dir);
    let doc = serde_json::to_value(tiza_core::fs::load_document(&dir.join("2024.tiza")).unwrap())
        .unwrap();
    for (method, params) in [
        ("talk.put", json!({"scenes": entries(&live)})),
        (
            "talk.insert",
            json!({"file": "n.tiza", "at": 1, "scene": doc}),
        ),
    ] {
        let (c, m) = err(&live.api(method, params));
        assert_eq!(c, "invalid_argument", "{method}: {m}");
        assert!(m.contains("--no-save"), "{method}: {m}");
    }
    assert_eq!(bytes(&dir), before);
    assert!(!dir.join("n.tiza").exists());
    // `talk.goto` still works.
    live.api_ok("talk.goto", json!({"scene": 2}));
}

#[test]
fn with_no_talk_each_method_is_unavailable() {
    let live = Live::from_code("fn main() {}\n", "rs", "tapi-none");
    for (method, params) in [
        ("talk.get", json!({})),
        ("talk.goto", json!({"scene": 1})),
        ("talk.put", json!({"scenes": ["a.tiza"]})),
        ("talk.insert", json!({"file": "a.tiza", "at": 1})),
    ] {
        let (c, m) = err(&live.api(method, params));
        assert_eq!(c, "unavailable", "{method}: {m}");
        assert!(m.contains("no talk is open"), "{method}: {m}");
    }
}

#[test]
fn a_put_of_the_same_document_keeps_autosave_and_the_talk_goes_on() {
    let dir = talk_copy("tapi-scene-put");
    let live = open(&dir, SessionStart::default());
    let mut doc = live.api_ok("scene.get", json!({}))["document"].clone();
    doc["annotations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "put1", "type": "note", "line": 1, "text": "put", "step": 1}));
    live.api_ok("scene.put", json!({"document": doc}));
    assert_eq!(live.api_ok("status", json!({}))["autosave"], true);
    let own = std::fs::read_to_string(dir.join("own.tiza")).unwrap();
    assert!(own.contains("\"put1\""), "the scene file has the put");
    live.api_ok("step.set", json!({"step": 2}));
    let r = live.api_ok("step.next", json!({}));
    assert_eq!(r["scene"]["position"], 5, "{r}");
}

// ---------------------------------------------------------------- AC4

/// A request sent on its own thread: its answer, when it comes.
fn send(live: &Live, method: &str, params: Value) -> mpsc::Receiver<Response> {
    let (tx, rx) = mpsc::channel();
    let socket = live.socket.clone();
    let method = method.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(tiza_tui::ipc::call(&socket, &method, params).expect("socket call"));
    });
    rx
}

fn wait_pending(live: &Live, n: usize) {
    let end = std::time::Instant::now() + Duration::from_secs(10);
    while live.with(|a| a.proposals.len()) < n {
        assert!(std::time::Instant::now() < end, "no {n} pending changes");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn in_supervised_mode_goto_waits_and_get_does_not() {
    let dir = talk_copy("tapi-sup");
    let live = open(&dir, SessionStart::default());
    live.api_ok("supervise.set", json!({"enabled": true}));
    live.api_ok("talk.get", json!({}));
    let rx = send(&live, "talk.goto", json!({"scene": 2}));
    wait_pending(&live, 1);
    let st = live.api_ok("supervise.status", json!({}));
    assert_eq!(
        st["pending"][0]["summary"], "go to scene 2 (borrow)",
        "{st}"
    );
    let n = live.with(|a| {
        let id = a.proposals[0].id;
        a.decide_proposals(&[id], false)
    });
    assert_eq!(n, 1);
    let (c, _) = err(&rx.recv_timeout(Duration::from_secs(10)).unwrap());
    assert_eq!(c, "rejected");
    assert_eq!(position(&live), 3);

    let rx = send(
        &live,
        "talk.put",
        json!({"scenes": entries(&live).into_iter().rev().collect::<Vec<_>>()}),
    );
    wait_pending(&live, 1);
    let st = live.api_ok("supervise.status", json!({}));
    let line = st["pending"][0]["summary"].as_str().unwrap().to_string();
    assert!(
        line.starts_with("change the list of scenes: ") && line.contains("moved"),
        "{line}"
    );
    live.with(|a| a.accept_proposal());
    assert!(
        rx.recv_timeout(Duration::from_secs(10))
            .unwrap()
            .error
            .is_none()
    );
}

/// A `talk.goto` that waited while a `talk.put` moved its scene: the
/// presenter accepts both, and the goto is a `conflict`, not a change to
/// the scene that is now at that place.
#[test]
fn a_waiting_goto_whose_scene_moved_is_a_conflict() {
    let dir = talk_copy("tapi-sup-moved");
    let live = open(&dir, SessionStart::default());
    live.api_ok("supervise.set", json!({"enabled": true}));
    let goto = send(&live, "talk.goto", json!({"scene": 2}));
    wait_pending(&live, 1);
    let mut list = entries(&live);
    list.swap(0, 1);
    let put = send(&live, "talk.put", json!({"scenes": list}));
    wait_pending(&live, 2);
    live.with(|a| a.accept_proposal_at(1));
    assert!(
        put.recv_timeout(Duration::from_secs(10))
            .unwrap()
            .error
            .is_none()
    );
    live.with(|a| a.accept_proposal());
    let (c, m) = err(&goto.recv_timeout(Duration::from_secs(10)).unwrap());
    assert_eq!(c, "conflict", "{m}");
    assert_eq!(position(&live), 3);
}

#[test]
fn in_supervised_mode_with_no_talk_a_talk_method_is_refused_at_once() {
    let live = Live::from_code("fn main() {}\n", "rs", "tapi-sup-none");
    live.api_ok("supervise.set", json!({"enabled": true}));
    let (c, _) = err(&live.api("talk.goto", json!({"scene": 1})));
    assert_eq!(c, "unavailable");
    assert_eq!(live.with(|a| a.proposals.len()), 0);
}
