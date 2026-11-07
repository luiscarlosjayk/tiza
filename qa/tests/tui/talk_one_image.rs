//! In a talk, a scene with nothing on it is one image: one step, and one
//! key press to leave it.
//!
//! Acceptance criteria:
//! - AC1 `n` on step 0 of a scene with no annotations and no moves goes
//!   to the next scene, and `p` from the next scene lands on its step 0.
//! - AC2 The counter of the talk counts that scene once.
//! - AC3 `--scene ID --step 1` and an `at` of the master at step 1 open
//!   that scene at step 0.
//! - AC4 A step that the presenter adds to that scene counts at once: `n`
//!   goes to it.
//! - AC5 Its `auto_advance_ms` goes to the next scene; in the last
//!   scene it stays, and with `loop` it goes back to step 0.
//! - AC6 The same document alone keeps steps 0 and 1.

use std::path::{Path, PathBuf};

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::{Live, fixture, scratch};
use tiza_tui::SessionStart;

/// A copy of `qa/fixtures/shared/one-image-talk/`: 1 a cover with
/// nothing on it, 2 a scene with steps 0 to 2, 3 an end (name `thanks`)
/// with nothing on it.
fn talk_copy(name: &str) -> PathBuf {
    let dir = scratch(name);
    tiza_qa::copy_dir(&fixture("shared/one-image-talk"), &dir);
    dir
}

fn open(dir: &Path, scene: Option<&str>, start: SessionStart) -> Live {
    Live::open_talk(&dir.join("talk.tiza"), scene, start, dir.join("s.sock"))
}

fn present() -> SessionStart {
    SessionStart {
        present: true,
        ..Default::default()
    }
}

/// The scene on screen (its file name) and its step.
fn here(live: &Live) -> (String, u32) {
    live.with(|a| {
        let file = a.save_path.file_name().unwrap().to_string_lossy();
        (file.into_owned(), a.doc.step)
    })
}

fn at(file: &str, step: u32) -> (String, u32) {
    (file.to_string(), step)
}

fn progress(live: &Live) -> (u32, u32) {
    live.with(|a| a.talk_progress()).expect("a talk")
}

#[test]
fn n_and_p_cross_a_scene_of_one_image_with_one_key_press() {
    let dir = talk_copy("one-image-keys");
    let live = open(&dir, None, present());
    // By hand: the cover 1, the move 3 (steps 0 to 2, then out), the end
    // 0 (the last scene, nothing after it).
    assert_eq!(
        (here(&live), progress(&live)),
        (at("cover.txt.tiza", 0), (0, 4))
    );
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(
        (here(&live), progress(&live)),
        (at("move.rs.tiza", 0), (1, 4))
    );
    live.keys(&[KeyCode::Char('n'), KeyCode::Char('n')]);
    assert_eq!(
        (here(&live), progress(&live)),
        (at("move.rs.tiza", 2), (3, 4))
    );
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(
        (here(&live), progress(&live)),
        (at("thanks.txt.tiza", 0), (4, 4))
    );
    // The end of the talk: `n` stays.
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(here(&live), at("thanks.txt.tiza", 0));
    // Back: the move at its last step, then the cover at step 0.
    live.keys(&[KeyCode::Char('p')]);
    assert_eq!(here(&live), at("move.rs.tiza", 2));
    live.keys(&[KeyCode::Char('p'), KeyCode::Char('p'), KeyCode::Char('p')]);
    assert_eq!(
        (here(&live), progress(&live)),
        (at("cover.txt.tiza", 0), (0, 4))
    );
    // The counter on screen says it.
    live.command("scene 2 2");
    let screen = live.screen(80, 24);
    assert!(
        screen.lines().last().unwrap().trim_end().ends_with("3/4"),
        "{screen}"
    );
}

#[test]
fn a_step_1_asked_for_opens_a_scene_of_one_image_at_step_0() {
    let dir = talk_copy("one-image-step");
    let start = SessionStart {
        step: Some(1),
        ..present()
    };
    let live = open(&dir, Some("thanks"), start.clone());
    assert_eq!(here(&live), at("thanks.txt.tiza", 0));
    // The control: a scene with that step opens on it.
    let other = Live::open_talk(&dir.join("talk.tiza"), Some("2"), start, dir.join("o.sock"));
    assert_eq!(here(&other), at("move.rs.tiza", 1));
    // The `at` of the master: a talk left at step 1 of the cover (`:scene
    // 1 1` goes there, to prepare it).
    live.command("scene 1 1");
    assert_eq!(here(&live), at("cover.txt.tiza", 1));
    assert_eq!(live.with(|a| a.save_talk_position()), None);
    let master = dir.join("talk.tiza");
    let raw = std::fs::read_to_string(&master).unwrap();
    assert!(raw.contains("\"step\": 1"), "{raw}");
    let resumed = Live::open_talk(&master, None, present(), dir.join("r.sock"));
    assert_eq!(here(&resumed), at("cover.txt.tiza", 0));
}

#[test]
fn a_step_added_to_a_scene_of_one_image_counts_at_once() {
    let dir = talk_copy("one-image-added");
    let live = open(&dir, None, present());
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "soon", "step": 1}),
    );
    assert_eq!(progress(&live), (0, 5));
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(here(&live), at("cover.txt.tiza", 1));
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(here(&live), at("move.rs.tiza", 0));
}

#[test]
fn the_same_document_alone_keeps_steps_0_and_1() {
    let dir = talk_copy("one-image-alone");
    let path = dir.join("cover.txt.tiza");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open_start(doc, path, present(), dir.join("a.sock"));
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(here(&live), at("cover.txt.tiza", 1));
}

/// Gives the scene `file` of `dir` an `auto_advance_ms` on step 0.
fn timed(dir: &Path, file: &str) {
    let path = dir.join(file);
    let mut raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    raw["steps"] = json!([{"step": 0, "auto_advance_ms": 1000}]);
    std::fs::write(&path, raw.to_string()).unwrap();
}

#[test]
fn the_auto_advance_of_a_scene_of_one_image_goes_to_the_next_scene() {
    let dir = talk_copy("one-image-auto");
    timed(&dir, "cover.txt.tiza");
    timed(&dir, "thanks.txt.tiza");
    let live = open(&dir, None, present());
    let later = || std::time::Instant::now() + std::time::Duration::from_secs(3600);
    assert!(live.with(move |a| a.auto_advance_tick(later())));
    assert_eq!(here(&live), at("move.rs.tiza", 0));
    // The last scene: nothing after it, so it stays on step 0.
    live.command("scene thanks");
    live.with(move |a| a.auto_advance_tick(later()));
    assert_eq!(here(&live), at("thanks.txt.tiza", 0));
    // The control: alone, the auto-advance goes to step 1.
    let path = dir.join("cover.txt.tiza");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let alone = Live::open_start(doc, path, present(), dir.join("a.sock"));
    alone.with(move |a| a.auto_advance_tick(later()));
    assert_eq!(here(&alone), at("cover.txt.tiza", 1));
}

#[test]
fn a_scene_of_one_image_with_loop_loops_on_step_0() {
    let dir = talk_copy("one-image-loop");
    timed(&dir, "cover.txt.tiza");
    let path = dir.join("cover.txt.tiza");
    let mut raw: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    raw["loop"] = json!(true);
    std::fs::write(&path, raw.to_string()).unwrap();
    let live = open(&dir, None, present());
    let later = || std::time::Instant::now() + std::time::Duration::from_secs(3600);
    live.with(move |a| a.auto_advance_tick(later()));
    assert_eq!(here(&live), at("cover.txt.tiza", 0));
    // `n` leaves it, as the last step of a scene with `loop`.
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(here(&live), at("move.rs.tiza", 0));
}
