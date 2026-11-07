//! What the presenter changed in the session goes on to the next scene of
//! a talk.
//!
//! Acceptance criteria:
//! - AC1 Follow is on in a scene with a file as `--follow`, `:follow` and
//!   `follow.set` asked for. When follow goes off by itself (a paste and
//!   its undo), the next scene with a file follows its file again, also
//!   after a scene with no file.
//! - AC2 `--padding` applies to each scene, also after `:pad` in a scene
//!   before, and when the presenter comes back to that scene.
//! - AC3 The history of `:` commands goes on from scene to scene.
//! - AC4 With `--no-save`, `:q!` in a scene that came back from memory
//!   names its changes, as before the crossing.

use std::path::{Path, PathBuf};

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_qa::{Live, fixture, scratch};
use tiza_tui::SessionStart;

/// A copy of the talk of `qa/fixtures/shared/talk/` in a folder of its
/// own. The master lists: 1 intro, 2 borrow (follows `src/borrow.rs`), 3
/// own (name `own`), 4 not-shown (skip), 5 loop, 6 again (name `borrow`),
/// 7 2024.
fn talk_copy(name: &str) -> PathBuf {
    let dir = scratch(name);
    tiza_qa::copy_dir(&fixture("shared/talk"), &dir);
    dir
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

/// A talk whose last scene (7) follows the same file as scene 2: two
/// scenes with a file, and scenes without one between them.
fn talk_with_two_followed_scenes(name: &str) -> PathBuf {
    let dir = talk_copy(name);
    std::fs::copy(dir.join("borrow.tiza"), dir.join("2024.tiza")).unwrap();
    dir
}

fn following(live: &Live) -> bool {
    live.with(|a| a.follow.is_some())
}

#[test]
fn follow_that_goes_off_by_itself_comes_back_in_the_next_scene_with_a_file() {
    let dir = talk_with_two_followed_scenes("talk-follow-paste");
    let start = SessionStart {
        follow: true,
        ..Default::default()
    };
    let live = open(&dir, Some("2"), start);
    assert!(following(&live), "--follow follows the file of scene 2");
    // A paste puts other code on screen and the undo puts the file back:
    // follow is off, the scene has its file, and the presenter did not ask
    // for follow off.
    live.with(|a| a.replace_code("fn other() {}\n").unwrap());
    live.command("undo");
    assert!(!following(&live), "a paste and its undo leave follow off");
    assert!(live.with(|a| a.source_file().is_some()), "the file is back");
    // A scene with no file between them does not change it.
    live.command("scene 3");
    assert!(!following(&live), "scene 3 has no file to follow");
    live.command("scene 7");
    assert_eq!(scene_file(&live), "2024.tiza");
    assert!(following(&live), "the next scene with a file follows it");
}

#[test]
fn follow_off_asked_for_stays_off_in_the_next_scenes() {
    let dir = talk_with_two_followed_scenes("talk-follow-off");
    let start = SessionStart {
        follow: true,
        ..Default::default()
    };
    let live = open(&dir, Some("2"), start);
    assert!(following(&live));
    live.command("follow off");
    live.command("scene 3");
    live.command("scene 7");
    assert!(!following(&live), ":follow off holds for the talk");
    live.command("follow on");
    live.command("scene 3");
    live.command("scene 2");
    assert!(following(&live), ":follow on holds for the talk");
}

#[test]
fn follow_set_off_over_the_socket_stays_off_in_the_next_scenes() {
    let dir = talk_with_two_followed_scenes("talk-follow-set");
    let start = SessionStart {
        follow: true,
        ..Default::default()
    };
    let live = open(&dir, Some("2"), start);
    live.api_ok("follow.set", json!({"enabled": false}));
    live.command("scene 7");
    assert!(
        !following(&live),
        "follow.set holds for the talk as :follow does"
    );
}

#[test]
fn padding_of_the_command_line_stays_on_every_scene_after_pad() {
    let dir = talk_copy("talk-pad");
    let start = SessionStart {
        padding: Some((2, 4)),
        ..Default::default()
    };
    let live = open(&dir, Some("own"), start);
    assert_eq!(live.with(|a| a.effective_padding()), (2, 4));
    live.command("pad 1");
    assert_eq!(live.with(|a| a.effective_padding()), (1, 2));
    live.command("scene loop");
    assert_eq!(
        live.with(|a| a.effective_padding()),
        (2, 4),
        "the next scene opens with --padding"
    );
    // Back to the scene: it opens as its file opens with --padding, and
    // its file keeps what :pad wrote.
    live.command("scene own");
    assert_eq!(live.with(|a| a.effective_padding()), (2, 4));
    assert_eq!(json_of(&dir.join("own.tiza"))["padding"], json!([1, 2]));
}

fn json_of(path: &Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn the_history_of_colon_commands_goes_on_in_the_next_scene() {
    let dir = talk_copy("talk-history");
    let live = open(&dir, Some("own"), SessionStart::default());
    live.command("goto 1");
    live.command("scene loop");
    assert_eq!(scene_file(&live), "loop.tiza");
    live.keys(&[KeyCode::Char(':'), KeyCode::Up]);
    assert_eq!(
        live.with(|a| a.mode.clone()),
        tiza_tui::app::Mode::Command("scene loop".into())
    );
    live.keys(&[KeyCode::Up]);
    assert_eq!(
        live.with(|a| a.mode.clone()),
        tiza_tui::app::Mode::Command("goto 1".into())
    );
}

#[test]
fn with_no_save_q_bang_names_the_changes_of_a_scene_that_came_back() {
    let dir = talk_copy("talk-no-save-summary");
    let start = SessionStart {
        no_save: true,
        ..Default::default()
    };
    let live = open(&dir, Some("own"), start);
    live.command("note 1 drawn live");
    let before = live.with(|a| a.unsaved_summary()).unwrap();
    live.command("scene loop");
    live.command("scene own");
    let after = live.with(|a| a.unsaved_summary()).unwrap();
    assert!(!after.contains("since the last save"), "{after}");
    assert_eq!(after, before, "the same changes as before the crossing");
}
