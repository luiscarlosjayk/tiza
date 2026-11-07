//! A talk: a master `.tiza` that plays several scenes in a row.
//!
//! Acceptance criteria:
//! - AC1 `n` past the end of a scene opens the next scene that shows, at
//!   step 0; `p` before its start opens the scene before, at its last step.
//! - AC2 `:scene X [STEP]` opens a scene by its position or its name.
//! - AC3 A scene in a talk looks as it does when it opens alone, at every
//!   step, with the same options and config.
//! - AC4 A crossing loses no change: changes that could not be saved stop
//!   it until `:w` or `:next!`, and the file of a scene keeps its own step.
//! - AC5 The talk opens again where it was left (`at` in the master).

use std::path::{Path, PathBuf};

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::{Live, fixture, scratch};
use tiza_tui::SessionStart;

/// A copy of the talk of `qa/fixtures/shared/talk/` in a folder of its
/// own: a test may write in it.
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

fn open(dir: &Path, scene: Option<&str>, start: SessionStart) -> Live {
    Live::open_talk(&dir.join("talk.tiza"), scene, start, dir.join("s.sock"))
}

/// The scene on screen: its file name.
fn scene_file(live: &Live) -> String {
    live.with(|a| {
        a.save_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    })
}

fn step(live: &Live) -> u32 {
    live.with(|a| a.doc.step)
}

/// The scene at `path`, opened alone at `step` with `start`, as `tiza
/// SCENE --step N` opens it: what the talk must show.
fn alone(path: &Path, step: u32, start: &SessionStart, sock: PathBuf) -> Live {
    let doc = tiza_core::fs::load_document(path).unwrap();
    let start = SessionStart {
        step: Some(step),
        ..start.clone()
    };
    Live::open_start(doc, path.to_path_buf(), start, sock)
}

/// A config that changes the look: no line numbers, other padding, the
/// stable layout. A scene must keep it after a crossing.
fn hostile_start(present: bool) -> SessionStart {
    SessionStart {
        present,
        config_defaults: tiza_core::config::Defaults {
            pad: Some((1, 5)),
            numbers: Some(false),
            layout: Some(tiza_core::config::LayoutMode::Stable),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn each_scene_of_a_talk_looks_as_it_does_alone_at_every_step() {
    for present in [true, false] {
        let dir = talk_copy(if present { "talk-walk-p" } else { "talk-walk" });
        let start = hostile_start(present);
        let live = open(&dir, None, start.clone());
        let order = ["intro", "borrow", "own", "loop", "again", "2024"];
        let mut seen = Vec::new();
        for (i, name) in order.iter().enumerate() {
            let file = format!("{name}.tiza");
            assert_eq!(
                scene_file(&live),
                file,
                "n goes to the next scene that shows"
            );
            assert_eq!(step(&live), 0, "{file}: a scene opens at step 0");
            let max = live.with(|a| a.doc.max_step());
            for s in 0..=max {
                let sock = dir.join(format!("a{i}{s}.sock"));
                let solo = alone(&dir.join(&file), s, &start, sock);
                for (cols, rows) in [(80, 24), (120, 30)] {
                    assert_eq!(
                        live.snapshot(cols, rows),
                        solo.snapshot(cols, rows),
                        "{file} at step {s}, {cols}x{rows}, present {present}"
                    );
                }
                seen.push(live.snapshot(80, 24));
                live.keys(&[KeyCode::Char('n')]);
            }
        }
        assert_eq!(
            scene_file(&live),
            "2024.tiza",
            "n at the end of the talk stays"
        );
        // The control: the scenes do not look the same, so the comparison
        // above sees a scene that leaks into the next one.
        let distinct: std::collections::HashSet<_> = seen.iter().collect();
        assert!(distinct.len() > order.len(), "{} screens", distinct.len());
    }
}

#[test]
fn a_scene_opened_with_other_options_looks_different() {
    // The control of the comparison: the padding of the config shows.
    let dir = talk_copy("talk-control");
    let plain = alone(
        &dir.join("own.tiza"),
        1,
        &SessionStart::default(),
        dir.join("a.sock"),
    );
    let padded = alone(
        &dir.join("own.tiza"),
        1,
        &hostile_start(false),
        dir.join("b.sock"),
    );
    assert_ne!(plain.snapshot(80, 24), padded.snapshot(80, 24));
}

#[test]
fn p_before_the_start_of_a_scene_opens_the_scene_before_at_its_last_step() {
    let dir = talk_copy("talk-p");
    let start = SessionStart {
        present: true,
        ..Default::default()
    };
    let live = open(&dir, Some("own"), start.clone());
    assert_eq!((scene_file(&live), step(&live)), ("own.tiza".into(), 0));
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!((scene_file(&live), step(&live)), ("borrow.tiza".into(), 3));
    let solo = alone(&dir.join("borrow.tiza"), 3, &start, dir.join("a.sock"));
    assert_eq!(live.snapshot(100, 24), solo.snapshot(100, 24));
    // Back in the scene: one step at a time, then the intro.
    live.keys(&[KeyCode::Char('p'), KeyCode::Char('p'), KeyCode::Char('p')]);
    assert_eq!((scene_file(&live), step(&live)), ("borrow.tiza".into(), 0));
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!((scene_file(&live), step(&live)), ("intro.tiza".into(), 2));
}

#[test]
fn an_intro_that_advances_by_itself_waits_when_p_comes_back_to_it() {
    let dir = talk_copy("talk-intro");
    let live = open(&dir, Some("borrow"), SessionStart::default());
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!((scene_file(&live), step(&live)), ("intro.tiza".into(), 2));
    // Two hours later the intro is still on its last step: the countdown
    // waits for the next key, so it does not push the presenter forward.
    let later = std::time::Instant::now() + std::time::Duration::from_secs(7200);
    assert!(!live.with(move |a| a.auto_advance_tick(later)));
    assert_eq!((scene_file(&live), step(&live)), ("intro.tiza".into(), 2));
    // Each step of the intro advances by itself: `p` goes past all of them
    // and stays at the start of the talk.
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!((scene_file(&live), step(&live)), ("intro.tiza".into(), 1));
    // `n` from the intro's start: its countdown runs again.
    live.command("scene 1");
    assert_eq!((scene_file(&live), step(&live)), ("intro.tiza".into(), 0));
    let later = std::time::Instant::now() + std::time::Duration::from_secs(7200);
    assert!(live.with(move |a| a.auto_advance_tick(later)));
    assert_eq!(step(&live), 1);
}

#[test]
fn the_last_step_of_an_intro_advances_to_the_next_scene() {
    let dir = talk_copy("talk-auto");
    let live = open(&dir, None, SessionStart::default());
    for s in 1..=3 {
        let later = std::time::Instant::now() + std::time::Duration::from_secs(7200);
        assert!(live.with(move |a| a.auto_advance_tick(later)), "tick {s}");
    }
    assert_eq!((scene_file(&live), step(&live)), ("borrow.tiza".into(), 0));
}

#[test]
fn a_scene_that_loops_goes_back_to_step_0_by_itself_and_on_with_n() {
    let dir = talk_copy("talk-loop");
    let live = open(&dir, Some("loop"), SessionStart::default());
    live.keys(&[KeyCode::Char('n'), KeyCode::Char('n')]);
    assert_eq!((scene_file(&live), step(&live)), ("loop.tiza".into(), 2));
    let later = std::time::Instant::now() + std::time::Duration::from_secs(7200);
    assert!(live.with(move |a| a.auto_advance_tick(later)));
    assert_eq!(
        (scene_file(&live), step(&live)),
        ("loop.tiza".into(), 0),
        "the auto-advance of the last step loops"
    );
    live.keys(&[KeyCode::Char('n'), KeyCode::Char('n'), KeyCode::Char('n')]);
    assert_eq!(
        (scene_file(&live), step(&live)),
        ("again.tiza".into(), 0),
        "n at the last step goes to the next scene"
    );
}

#[test]
fn n_during_the_typing_of_the_last_step_ends_the_typing_first() {
    let dir = talk_copy("talk-typing");
    // A typed move at the last step of `own`, one hour for each character.
    let path = dir.join("own.tiza");
    let mut raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    raw["moves"] = json!([{"at_step": 2, "action": "code.patch", "op": "insert",
        "after": "let d", "content": "let e = d;", "typing": "char", "speed_ms": 3600000}]);
    std::fs::write(&path, raw.to_string()).unwrap();
    let live = open(&dir, Some("own"), SessionStart::default());
    live.keys(&[KeyCode::Char('n'), KeyCode::Char('n')]);
    assert!(live.with(|a| a.typing.is_some()), "step 2 types");
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!((scene_file(&live), step(&live)), ("own.tiza".into(), 2));
    assert!(live.with(|a| a.typing.is_none()), "n ended the typing");
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(scene_file(&live), "loop.tiza");
}

#[test]
fn a_change_saves_the_scene_with_its_own_step() {
    let dir = talk_copy("talk-disk-step");
    let live = open(&dir, Some("borrow"), SessionStart::default());
    live.keys(&[KeyCode::Char('n'), KeyCode::Char('n'), KeyCode::Char('n')]);
    assert_eq!(step(&live), 3);
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "added in the talk"}),
    );
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("borrow.tiza")).unwrap()).unwrap();
    assert_eq!(saved["step"], 1, "the step of the file stays");
    assert!(saved.to_string().contains("added in the talk"));
    assert_eq!(saved["theme"], "dark", "the scene keeps its own theme");
    assert!(
        live.with(|a| a.unsaved_summary()).is_none(),
        "the change is on disk"
    );
}

#[test]
fn changes_that_are_not_saved_stop_a_crossing_until_w_or_next_bang() {
    let dir = talk_copy("talk-no-save");
    let start = SessionStart {
        no_save: true,
        ..Default::default()
    };
    let live = open(&dir, Some("own"), start);
    let own = std::fs::read(dir.join("own.tiza")).unwrap();
    live.command("note 1 kept");
    live.keys(&[KeyCode::Char('n'), KeyCode::Char('n'), KeyCode::Char('n')]);
    assert_eq!(scene_file(&live), "own.tiza", "n does not cross");
    let (msg, loud) = live.message().unwrap();
    assert!(loud, "{msg}");
    assert!(
        msg.contains("changes not saved") && msg.contains(":w") && msg.contains(":next!"),
        "{msg}"
    );
    live.command("w");
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(scene_file(&live), "loop.tiza", "after :w, n crosses");
    assert!(
        std::fs::read_to_string(dir.join("own.tiza"))
            .unwrap()
            .contains("kept")
    );
    // Another change, and `:next!`: it goes on and the file stays.
    let looped = std::fs::read(dir.join("loop.tiza")).unwrap();
    live.command("note 1 lost");
    live.command("next!");
    live.command("next!");
    live.command("next!");
    assert_eq!(scene_file(&live), "again.tiza");
    assert_eq!(std::fs::read(dir.join("loop.tiza")).unwrap(), looped);
    assert_ne!(std::fs::read(dir.join("own.tiza")).unwrap(), own);
    // `--no-save` stays for the session: a change in the new scene is not
    // saved either.
    let again = std::fs::read(dir.join("again.tiza")).unwrap();
    live.command("note 1 not saved");
    assert_eq!(std::fs::read(dir.join("again.tiza")).unwrap(), again);
}

#[test]
fn a_put_of_another_document_holds_autosave_for_its_scene_only() {
    let dir = talk_copy("talk-put");
    let live = open(&dir, Some("own"), SessionStart::default());
    // Another document: its code is of another file.
    let mut other = tiza_core::fs::load_document(&dir.join("2024.tiza")).unwrap();
    other.source.path = Some("other.rs".into());
    live.api_ok("scene.put", json!({"document": other}));
    assert!(!live.with(|a| a.autosave));
    live.command("w");
    live.command("scene loop");
    assert_eq!(scene_file(&live), "loop.tiza");
    assert!(live.with(|a| a.autosave), "the next scene has autosave on");
    live.command("note 1 saved in loop");
    assert!(
        std::fs::read_to_string(dir.join("loop.tiza"))
            .unwrap()
            .contains("saved in loop")
    );
    live.command("next");
    live.command("next");
    live.command("next");
    assert_eq!(scene_file(&live), "again.tiza", "n crosses again");
}

#[test]
fn coming_back_to_a_scene_brings_back_its_undo_while_its_files_are_the_same() {
    let dir = talk_copy("talk-undo");
    let live = open(&dir, Some("borrow"), SessionStart::default());
    live.command("step 2");
    live.command("note 1 undo me");
    live.command("goto 3");
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(scene_file(&live), "own.tiza");
    // A new scene has nothing to undo, and its file stays.
    let own = std::fs::read(dir.join("own.tiza")).unwrap();
    live.command("undo");
    assert_eq!(std::fs::read(dir.join("own.tiza")).unwrap(), own);
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!((scene_file(&live), step(&live)), ("borrow.tiza".into(), 3));
    live.command("undo");
    assert!(
        !std::fs::read_to_string(dir.join("borrow.tiza"))
            .unwrap()
            .contains("undo me"),
        "u takes the note back"
    );
    // Again, and the code file changes on disk while the talk is in
    // another scene: the history is of other code, and it is gone.
    live.command("note 1 undo me");
    live.command("next");
    assert_eq!(scene_file(&live), "own.tiza");
    std::fs::write(
        dir.join("src/borrow.rs"),
        "fn main() {\n    let s = 1;\n    let r = &s;\n    println!(\"{r}\");\n}\n",
    )
    .unwrap();
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!(scene_file(&live), "borrow.tiza");
    assert!(live.with(|a| a.doc.code[1].contains("let s = 1;")));
    live.command("undo");
    assert!(
        std::fs::read_to_string(dir.join("borrow.tiza"))
            .unwrap()
            .contains("undo me"),
        "nothing to undo"
    );
}

#[test]
fn scene_opens_a_scene_by_its_position_or_its_name() {
    let dir = talk_copy("talk-names");
    let live = open(&dir, None, SessionStart::default());
    // A name that two scenes have: the first one.
    live.command("scene borrow");
    assert_eq!(scene_file(&live), "borrow.tiza");
    // By position, the second one (the skipped entry counts).
    live.command("scene 6");
    assert_eq!(scene_file(&live), "again.tiza");
    live.command("scene 07");
    assert_eq!(scene_file(&live), "2024.tiza");
    // A named step of the scene.
    live.command("scene 2 shared");
    assert_eq!((scene_file(&live), step(&live)), ("borrow.tiza".into(), 2));
    // The scene on screen: a goto.
    live.command("scene borrow 3");
    assert_eq!((scene_file(&live), step(&live)), ("borrow.tiza".into(), 3));
    for (spec, says) in [
        ("4", "is skipped"),
        ("0", "no scene 0"),
        ("9", "no scene 9"),
        ("nothing", "no scene is named"),
    ] {
        live.command(&format!("scene {spec}"));
        let (msg, loud) = live.message().unwrap();
        assert!(loud && msg.contains(says), "{spec}: {msg}");
        assert_eq!(scene_file(&live), "borrow.tiza", "{spec}");
    }
}

#[test]
fn the_talk_writes_where_it_is_and_opens_there_again() {
    let dir = talk_copy("talk-at");
    let master = dir.join("talk.tiza");
    let live = open(&dir, None, SessionStart::default());
    live.command("scene 6 2");
    assert_eq!(live.with(|a| a.save_talk_position()), None);
    drop(live);
    let raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&master).unwrap()).unwrap();
    assert_eq!(
        raw["talk"]["at"],
        json!({"position": 6, "name": "borrow", "step": 2})
    );
    assert_eq!(
        raw["kept"], "a key that this version does not know",
        "only `at` changes"
    );
    assert_eq!(raw["talk"]["scenes"][0], "intro.tiza");
    let live = open(&dir, None, SessionStart::default());
    assert_eq!((scene_file(&live), step(&live)), ("again.tiza".into(), 2));
    // The countdown waits at a resumed step.
    assert!(live.with(|a| a.step_ready_at.is_none()));
    drop(live);
    // With `--no-save` the master stays as it is.
    let before = std::fs::read(&master).unwrap();
    let live = open(
        &dir,
        Some("1"),
        SessionStart {
            no_save: true,
            ..Default::default()
        },
    );
    assert_eq!(live.with(|a| a.save_talk_position()), None);
    assert_eq!(std::fs::read(&master).unwrap(), before);
}

#[test]
fn a_master_edited_while_the_talk_runs_is_not_overwritten() {
    let dir = talk_copy("talk-edited");
    let master = dir.join("talk.tiza");
    let live = open(&dir, None, SessionStart::default());
    live.command("scene own");
    let edited = std::fs::read_to_string(&master)
        .unwrap()
        .replace("\"2024.tiza\"", "\"2024.tiza\", \"own.tiza\"");
    std::fs::write(&master, &edited).unwrap();
    let said = live.with(|a| a.save_talk_position()).expect("it says why");
    assert!(
        said.contains("not saved") && said.contains("changed"),
        "{said}"
    );
    assert_eq!(std::fs::read_to_string(&master).unwrap(), edited);
}

#[test]
fn saveas_does_not_work_in_a_talk() {
    let dir = talk_copy("talk-saveas");
    let live = open(&dir, Some("own"), SessionStart::default());
    live.command(&format!("saveas {}", dir.join("elsewhere.tiza").display()));
    let (msg, loud) = live.message().unwrap();
    assert!(loud && msg.contains("talk"), "{msg}");
    assert!(!dir.join("elsewhere.tiza").exists());
    assert_eq!(scene_file(&live), "own.tiza");
}

#[test]
fn a_crossing_answers_what_waits_and_the_revision_grows() {
    let dir = talk_copy("talk-session");
    let live = open(&dir, Some("own"), SessionStart::default());
    live.command("supervise on");
    let before = live.api_ok("status", json!({}))["revision"]
        .as_u64()
        .unwrap();
    // A change that waits for the review, from another thread.
    let waiting = tiza_qa::ipc::send_async(
        &live.socket,
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "proposed"}),
    );
    tiza_qa::ipc::wait_pending(&live, 1);
    live.command("scene loop");
    let reply = waiting.join().unwrap();
    assert_eq!(
        reply.error.as_ref().map(|e| e.code.as_str()),
        Some("unavailable"),
        "{reply:?}"
    );
    assert!(
        !std::fs::read_to_string(dir.join("loop.tiza"))
            .unwrap()
            .contains("proposed")
    );
    let after = live.api_ok("status", json!({}))["revision"]
        .as_u64()
        .unwrap();
    assert!(after > before, "{before} then {after}");
    assert!(live.with(|a| a.supervised), "supervised mode stays on");
}
