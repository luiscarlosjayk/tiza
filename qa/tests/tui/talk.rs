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
//! - AC6 On the real binary in a pty: the keys cross scenes; `q`, Ctrl-C
//!   and SIGTERM write `at`; a read-only master plays and the terminal
//!   says that `at` is not saved; a panic while the next scene opens
//!   leaves the scene on screen, and the socket answers.

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
    // The first scene: no scene before to go back to, so `p` goes one
    // step back, and that step waits for the next key. Else it would
    // advance again, and the talk would go on by itself.
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!((scene_file(&live), step(&live)), ("intro.tiza".into(), 1));
    let later = std::time::Instant::now() + std::time::Duration::from_secs(7200);
    assert!(!live.with(move |a| a.auto_advance_tick(later)));
    assert_eq!((scene_file(&live), step(&live)), ("intro.tiza".into(), 1));
    // `n` from the intro's start: its countdown runs again.
    live.command("scene 1");
    assert_eq!((scene_file(&live), step(&live)), ("intro.tiza".into(), 0));
    let later = std::time::Instant::now() + std::time::Duration::from_secs(7200);
    assert!(live.with(move |a| a.auto_advance_tick(later)));
    assert_eq!(step(&live), 1);
}

#[test]
fn p_goes_past_a_step_that_advances_by_itself() {
    let dir = talk_copy("talk-p-auto");
    let path = dir.join("borrow.tiza");
    let mut raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    raw["steps"][0]["auto_advance_ms"] = json!(3600000);
    std::fs::write(&path, raw.to_string()).unwrap();
    let live = open(&dir, Some("borrow"), SessionStart::default());
    live.command("goto 3");
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!(
        (scene_file(&live), step(&live)),
        ("borrow.tiza".into(), 1),
        "step 2 would go on to step 3 by itself"
    );
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
    // A key ends a typing before it does anything else; `step.next` of the
    // socket and `:next` follow the same rule.
    live.api_ok("step.next", json!({}));
    assert_eq!((scene_file(&live), step(&live)), ("own.tiza".into(), 2));
    assert!(
        live.with(|a| a.typing.is_none()),
        "step.next ended the typing"
    );
    live.api_ok("step.next", json!({}));
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
    let dir = talk_copy("talk-autosave-off");
    let live = open(&dir, Some("own"), SessionStart::default());
    live.command("autosave off");
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
    // Autosave stays off for the session: a change in the new scene is not
    // saved either.
    let again = std::fs::read(dir.join("again.tiza")).unwrap();
    live.command("note 1 not saved");
    assert_eq!(std::fs::read(dir.join("again.tiza")).unwrap(), again);
}

#[test]
fn with_no_save_a_scene_with_changes_waits_in_memory() {
    let dir = talk_copy("talk-no-save");
    let start = SessionStart {
        no_save: true,
        ..Default::default()
    };
    let live = open(&dir, Some("own"), start);
    let own = std::fs::read(dir.join("own.tiza")).unwrap();
    live.command("note 1 drawn live");
    live.keys(&[KeyCode::Char('n'), KeyCode::Char('n'), KeyCode::Char('n')]);
    assert_eq!(
        scene_file(&live),
        "loop.tiza",
        "n crosses: --no-save chose not to save"
    );
    assert_eq!(std::fs::read(dir.join("own.tiza")).unwrap(), own);
    let has_note = |live: &Live| {
        live.with(|a| {
            serde_json::to_string(&a.doc.annotations)
                .unwrap()
                .contains("drawn live")
        })
    };
    assert!(!has_note(&live));
    // Back: the scene as it was left, still not saved, and `u` undoes.
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!((scene_file(&live), step(&live)), ("own.tiza".into(), 2));
    assert!(has_note(&live), "the note waits in memory");
    assert!(live.with(|a| a.dirty && a.unsaved_summary().is_some()));
    // The ways out name `:w PATH`: a talk refuses `:saveas`.
    let bar = live.screen(100, 30);
    assert!(bar.contains("unsaved (:w PATH"), "{bar}");
    live.command("q");
    let (msg, _) = live.message().unwrap();
    assert!(
        msg.contains(":w PATH writes a copy") && !msg.contains("saveas"),
        "{msg}"
    );
    assert!(!live.with(|a| a.quit));
    live.command("undo");
    assert!(!has_note(&live), "u takes it back");
    live.command("redo");
    assert!(has_note(&live));
    // The file changes on disk while the scene waits: the disk wins, and
    // the bar says it once.
    live.command("scene loop");
    let changed = String::from_utf8(own.clone())
        .unwrap()
        .replace("\"margin\": 20", "\"margin\": 22");
    std::fs::write(dir.join("own.tiza"), &changed).unwrap();
    live.command("scene own");
    assert_eq!(scene_file(&live), "own.tiza");
    assert!(!has_note(&live), "the file of the disk");
    let (msg, _) = live.message().unwrap();
    assert!(
        msg.contains("not kept") && msg.contains("changed on disk"),
        "{msg}"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("own.tiza")).unwrap(),
        changed
    );
}

#[test]
fn with_no_save_q_waits_while_another_scene_holds_changes_in_memory() {
    let dir = talk_copy("talk-no-save-quit");
    let start = SessionStart {
        no_save: true,
        ..Default::default()
    };
    let live = open(&dir, Some("own"), start);
    live.command("note 1 drawn live");
    live.command("scene loop");
    assert_eq!(scene_file(&live), "loop.tiza");
    assert!(live.with(|a| !a.dirty), "the scene on screen has no change");
    // `q` would lose the note of `own`: it says so, and does not quit.
    live.command("q");
    let (msg, loud) = live.message().unwrap();
    assert!(
        loud && msg.contains("unsaved changes in scene 3 (own)") && msg.contains(":q!"),
        "{msg}"
    );
    assert!(!live.with(|a| a.quit));
    // The local API asks the same: `app.quit` refuses without `force`.
    let reply = live.api("app.quit", json!({}));
    assert_eq!(
        reply.error.as_ref().map(|e| e.code.as_str()),
        Some("unsaved_changes"),
        "{reply:?}"
    );
    // On the terminal at the exit: which changes are not saved.
    let said = live.with(|a| a.parked_summary()).expect("a summary");
    assert!(said.contains("scene 3 (own)"), "{said}");
    // Back in `own`, the note is on screen: nothing waits elsewhere.
    live.command("scene own");
    assert_eq!(live.with(|a| a.parked_changes()), Vec::<String>::new());
    // `:q!` discards.
    live.command("scene loop");
    live.command("q!");
    assert!(live.with(|a| a.quit));
}

#[test]
fn the_socket_does_not_change_the_scene_under_a_command_or_unsaved_changes() {
    let dir = talk_copy("talk-socket-busy");
    let live = open(&dir, Some("own"), SessionStart::default());
    live.command("goto 2");
    // The presenter types a `:` command: the next scene would take the
    // rest of the keys as keys of normal mode.
    live.keys(&[KeyCode::Char(':'), KeyCode::Char('t')]);
    let reply = live.api("step.next", json!({}));
    let error = reply.error.as_ref().expect("an error");
    assert!(error.message.contains("typing a command"), "{reply:?}");
    assert_eq!((scene_file(&live), step(&live)), ("own.tiza".into(), 2));
    // A step inside the scene is not a scene change: it goes on.
    live.keys(&[KeyCode::Esc]);
    live.api_ok("step.prev", json!({}));
    assert_eq!(step(&live), 1);
    live.api_ok("step.next", json!({}));
    live.api_ok("step.next", json!({}));
    assert_eq!((scene_file(&live), step(&live)), ("loop.tiza".into(), 0));
    // Changes that are not saved: the reply says so, as the bar does.
    live.command("autosave off");
    live.command("note 1 drawn live");
    let reply = live.api("step.prev", json!({}));
    assert_eq!(
        reply.error.as_ref().map(|e| e.code.as_str()),
        Some("unsaved_changes"),
        "{reply:?}"
    );
    assert_eq!(scene_file(&live), "loop.tiza");
}

#[test]
fn a_scene_shown_at_another_step_than_its_file_has_nothing_unsaved_after_undo() {
    let dir = talk_copy("talk-saved-step");
    let start = SessionStart {
        no_save: true,
        ..Default::default()
    };
    // The file of borrow is on step 1; the talk shows it at step 0.
    let live = open(&dir, Some("borrow"), start);
    assert_eq!(step(&live), 0);
    live.command("note 1 drawn live");
    live.command("undo");
    assert_eq!(live.with(|a| a.unsaved_summary()), None);
    // Also after a scene change.
    live.command("scene own");
    live.command("scene borrow");
    live.command("note 1 drawn live");
    live.command("undo");
    assert_eq!(live.with(|a| a.unsaved_summary()), None);
}

#[test]
fn with_no_save_then_autosave_on_the_changes_that_waited_are_saved() {
    let dir = talk_copy("talk-autosave-on");
    let start = SessionStart {
        no_save: true,
        ..Default::default()
    };
    let live = open(&dir, Some("own"), start);
    live.command("note 1 drawn live");
    live.command("scene loop");
    live.command("autosave on");
    live.command("scene own");
    let own = std::fs::read_to_string(dir.join("own.tiza")).unwrap();
    assert!(own.contains("drawn live"), "{own}");
    assert!(live.with(|a| !a.dirty));
    // The presenter chose to save: the place in the talk is written too.
    assert_eq!(live.with(|a| a.save_talk_position()), None);
    assert_eq!(
        master_json(&dir)["talk"]["at"],
        json!({"position": 3, "name": "own", "step": 0})
    );
}

#[test]
fn mouse_off_stays_off_in_the_next_scene_over_the_config() {
    let dir = talk_copy("talk-mouse");
    let start = SessionStart {
        config_defaults: tiza_core::config::Defaults {
            mouse: Some(true),
            ..Default::default()
        },
        ..Default::default()
    };
    let live = open(&dir, Some("own"), start);
    assert!(live.with(|a| a.mouse), "the config turns it on");
    live.command("mouse off");
    live.command("scene loop");
    assert!(live.with(|a| !a.mouse));
}

#[test]
fn n_on_the_last_step_of_the_last_scene_does_not_move_also_with_loop() {
    let dir = talk_copy("talk-last-loop");
    let last = dir.join("2024.tiza");
    let looping = std::fs::read_to_string(&last).unwrap().replacen(
        "\"step\": 0,",
        "\"step\": 0, \"loop\": true,",
        1,
    );
    std::fs::write(&last, looping).unwrap();
    // A name of digits only is a number: open the scene by its place.
    let live = open(&dir, Some("7"), SessionStart::default());
    assert!(live.with(|a| a.doc.looping));
    live.command("goto 1");
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!((scene_file(&live), step(&live)), ("2024.tiza".into(), 1));
}

#[test]
fn with_no_save_changes_of_a_scene_whose_file_breaks_as_it_is_left_are_said_lost() {
    let dir = talk_copy("talk-no-save-broken");
    let start = SessionStart {
        no_save: true,
        ..Default::default()
    };
    let file = dir.join("own.tiza");
    let good = std::fs::read(&file).unwrap();
    let live = open(&dir, Some("own"), start);
    live.command("note 1 drawn live");
    std::fs::write(&file, "{ \"schema\": ").unwrap();
    live.command("scene loop");
    assert_eq!(scene_file(&live), "loop.tiza");
    std::fs::write(&file, &good).unwrap();
    live.command("scene own");
    assert_eq!(scene_file(&live), "own.tiza");
    let (msg, _) = live.message().unwrap();
    assert!(
        msg.contains("changes not kept") && msg.contains("changed on disk"),
        "{msg}"
    );
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
    // A master that says version 1, as one written by hand.
    let one = std::fs::read_to_string(&master)
        .unwrap()
        .replace("\"schema\": 2", "\"schema\": 1");
    std::fs::write(&master, one).unwrap();
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
    // Version 2: a Tiza that does not know `talk` says that the file is
    // newer, not that it is a broken document.
    assert_eq!(raw["schema"], 2);
    assert_eq!(raw["talk"]["scenes"][0], "intro.tiza");
    // The layout of Tiza, in an order that reads well: `schema`, `talk`
    // (`title`, `scenes`, `at` with `position`, `name`, `step`), then the
    // keys that this version does not know.
    let text = std::fs::read_to_string(&master).unwrap();
    let mut from = 0;
    for key in [
        "\"schema\"",
        "\"talk\"",
        "\"title\"",
        "\"scenes\"",
        "\"at\"",
        "\"position\"",
        "\"name\"",
        "\"step\"",
        "\"kept\"",
    ] {
        let at = text[from..].find(key);
        assert!(at.is_some(), "no {key} after byte {from} in:\n{text}");
        from += at.unwrap() + key.len();
    }
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
fn in_a_talk_the_messages_of_scene_put_name_w_path_not_saveas() {
    let dir = talk_copy("talk-put-messages");
    let live = open(&dir, Some("borrow"), SessionStart::default());
    let names_w_path = |live: &Live| {
        let (msg, _) = live.message().unwrap();
        assert!(
            msg.contains(":w PATH writes a copy") && !msg.contains("saveas"),
            "{msg}"
        );
        msg
    };
    // The same scene, with code that differs from its file: not saved.
    let mut scene = live.api_ok("scene.get", json!({}));
    scene["document"]["code"][0] = json!("// edited");
    live.api_ok("scene.put", json!({"document": scene["document"]}));
    let msg = names_w_path(&live);
    assert!(msg.starts_with("not saved, and autosave is off"), "{msg}");
    // Another document: autosave stops.
    live.command("autosave on");
    let other = json!({
        "schema": 1,
        "source": {"language": "rs", "path": "elsewhere.rs"},
        "code": ["fn elsewhere() {}"]
    });
    live.api_ok("scene.put", json!({"document": other}));
    let msg = names_w_path(&live);
    assert!(
        msg.contains("another document loaded: autosave off"),
        "{msg}"
    );
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
    let error = reply.error.as_ref().expect("an error");
    assert_eq!(error.code, "unavailable", "{reply:?}");
    assert!(error.message.contains("the scene changed"), "{reply:?}");
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

// ---- The real binary on a pty ----

/// The prelude of the pty scripts of a talk: `api`, `here()` (the
/// position of the scene on screen and its step) and `until(want)`.
const PTY_PRELUDE: &str = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
def api(method, params=None):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params or {})],
                       capture_output=True, text=True, timeout=10)
    return r.returncode, r.stdout + r.stderr
def answer(method):
    rc, out = api(method)
    return json.loads(out) if rc == 0 else {}
def here():
    scene = answer("follow.status").get("test_scene") or {}
    return [scene.get("position"), answer("status").get("step")]
def until(want, what, timeout=10):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if here() == want:
            return True
        s.settle(0.05)
    print("WAIT_TIMEOUT", what, json.dumps(here()), flush=True)
    return False
def tail():
    left = s.out.rfind(b"\x1b[?1049l")
    return plain(s.out[left + 8:]) if left >= 0 else "NOT_LEFT"
"##;

/// Runs `tiza --socket s.sock ARGS` in `dir` on a pty, with
/// `TIZA_TEST_HOOK=hook` when it is not empty, then `steps` (Python, at
/// the top level: `s` is the pty). Prints the lines of the script.
fn pty_talk(dir: &Path, args: &[&str], hook: &str, steps: &str) -> String {
    let body = format!(
        r##"{PTY_PRELUDE}
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
if {hook:?}:
    env["TIZA_TEST_HOOK"] = {hook:?}
s = Pty([binary, "--socket", "s.sock"] + {args}, 30, 100, env=env)
ok = s.wait_listening()
{steps}
"##,
        args = serde_json::to_string(args).unwrap(),
    );
    tiza_qa::pty_script(
        dir,
        "talk.py",
        &body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    )
}

fn master_json(dir: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("talk.tiza")).unwrap()).unwrap()
}

/// `tiza talk.tiza`: the keys of the terminal go from scene to scene, and
/// each way to quit (`q`, Ctrl-C, SIGTERM) writes where the talk was.
#[test]
fn pty_the_keys_cross_scenes_and_each_way_to_quit_writes_where_the_talk_is() {
    for (close, code) in [("q", 0), ("ctrl-c", 0), ("sigterm", 143)] {
        let dir = talk_copy(&format!("talk-pty-{close}"));
        let own = std::fs::read(dir.join("own.tiza")).unwrap();
        let steps = format!(
            r##"
ok = ok and until([1, 0], "the first scene, at step 0")
ok and s.send(b":scene own 2\r")
ok = ok and until([3, 2], ":scene own 2")
ok and s.send(b"n")
ok = ok and until([5, 0], "n: the scene after the skipped one, at step 0")
ok and s.send(b"p")
ok = ok and until([3, 2], "p: the scene before, at its last step")
ok and s.send(b"\x1b[5~")
ok = ok and until([3, 1], "PageUp: the step before")
print("PATH", ok, flush=True)
close = {close:?}
if close == "q":
    s.send(b"q")
elif close == "ctrl-c":
    s.send(b"\x03")
else:
    os.kill(s.p.pid, signal.SIGTERM)
print("EXIT", s.finish(10), flush=True)
print("TAIL", json.dumps(tail()), flush=True)
"##
        );
        let out = pty_talk(&dir, &["talk.tiza"], "", &steps);
        assert!(out.contains("PATH True"), "{close}:\n{out}");
        assert!(out.contains(&format!("EXIT {code}\n")), "{close}:\n{out}");
        let raw = master_json(&dir);
        assert_eq!(
            raw["talk"]["at"],
            json!({"position": 3, "name": "own", "step": 1}),
            "{close}:\n{out}"
        );
        assert_eq!(raw["kept"], "a key that this version does not know");
        assert_eq!(
            raw["talk"]["scenes"][3],
            json!({"file": "not-shown.tiza", "skip": true})
        );
        // Going from step to step does not change the file of a scene.
        assert_eq!(std::fs::read(dir.join("own.tiza")).unwrap(), own, "{close}");
    }
}

/// A read-only master: the talk plays, and on exit the terminal says that
/// the position is not saved. The master stays as it is.
#[test]
fn pty_a_read_only_master_plays_and_says_that_the_position_is_not_saved() {
    use std::os::unix::fs::PermissionsExt;
    let dir = talk_copy("talk-pty-read-only");
    let master = dir.join("talk.tiza");
    let before = std::fs::read(&master).unwrap();
    std::fs::set_permissions(&master, std::fs::Permissions::from_mode(0o444)).unwrap();
    let out = pty_talk(
        &dir,
        &["talk.tiza"],
        "",
        r##"
ok = ok and until([1, 0], "the first scene")
ok and s.send(b":scene own 2\r")
ok = ok and until([3, 2], ":scene own 2")
print("PATH", ok, flush=True)
s.send(b"q")
print("EXIT", s.finish(10), flush=True)
print("TAIL", json.dumps(tail()), flush=True)
"##,
    );
    std::fs::set_permissions(&master, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(out.contains("PATH True"), "{out}");
    assert!(out.contains("EXIT 0\n"), "{out}");
    let tail = text_after(&out, "TAIL");
    assert!(
        tail.contains("the position in the talk is not saved") && tail.contains("read-only"),
        "{out}"
    );
    assert_eq!(std::fs::read(&master).unwrap(), before);
}

/// A panic while the next scene opens (the `talk:panic` hook): the request
/// gets an error, the scene on screen stays whole, the socket answers, and
/// `q` writes the position of the scene on screen.
#[test]
fn pty_a_panic_while_the_next_scene_opens_leaves_the_scene_on_screen() {
    let dir = talk_copy("talk-pty-panic");
    let out = pty_talk(
        &dir,
        &["talk.tiza", "--scene", "own", "--step", "2"],
        "talk:panic",
        r##"
ok = ok and until([3, 2], "--scene own --step 2")
rc, reply = api("step.next")
print("NEXT", json.dumps([rc, reply]), flush=True)
print("HERE", json.dumps(here()), flush=True)
print("CODE", json.dumps(answer("code.get").get("text")), flush=True)
print("PATH", ok, flush=True)
s.send(b"q")
print("EXIT", s.finish(10), flush=True)
"##,
    );
    assert!(out.contains("PATH True"), "{out}");
    let next: serde_json::Value = serde_json::from_str(text_after(&out, "NEXT")).unwrap();
    assert_ne!(next[0], 0, "the request fails: {out}");
    let here: serde_json::Value = serde_json::from_str(text_after(&out, "HERE")).unwrap();
    assert_eq!(here, json!([3, 2]), "{out}");
    let code: serde_json::Value = serde_json::from_str(text_after(&out, "CODE")).unwrap();
    assert!(code.to_string().contains("let d = c;"), "{out}");
    assert!(out.contains("EXIT 0\n"), "{out}");
    assert_eq!(
        master_json(&dir)["talk"]["at"],
        json!({"position": 3, "name": "own", "step": 2}),
        "{out}"
    );
}

/// The text after `KEY ` on its line.
fn text_after<'a>(out: &'a str, key: &str) -> &'a str {
    out.lines()
        .find_map(|l| l.strip_prefix(&format!("{key} ")))
        .unwrap_or_else(|| panic!("no {key} in:\n{out}"))
}

/// `--smoke` draws one frame and exits: the master stays as it is, also
/// when the talk opens at a scene that is not its `at`.
#[test]
fn pty_smoke_leaves_the_master_as_it_is() {
    let dir = talk_copy("talk-pty-smoke");
    let before = std::fs::read(dir.join("talk.tiza")).unwrap();
    let body = r##"
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
s = Pty([binary, "--smoke", "--no-socket", "talk.tiza", "--scene", "own", "--step", "1"], 30, 100, env=env)
print("EXIT", s.finish(10), flush=True)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "smoke.py",
        body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(out.contains("EXIT 0\n"), "{out}");
    assert_eq!(std::fs::read(dir.join("talk.tiza")).unwrap(), before);
}
