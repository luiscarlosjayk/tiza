//! Opening a talk: a master `.tiza` with `talk` in place of `code`.
//!
//! Acceptance criteria:
//! - AC1 A master that Tiza cannot play does not open: the error names the
//!   key or the scene, and Tiza does not panic. A skipped scene can be
//!   missing.
//! - AC2 A master with a BOM and CRLF line ends opens.
//! - AC3 `--scene` needs a master; `--theme-file` and `--lang` do not work
//!   with a master.
//! - AC4 A command that needs a document (`render`, `check`, `scene put`)
//!   refuses a master and tells what it is.
//!
//! Without a terminal on stdout, `tiza FILE` stops after it opened the
//! file: a talk that opens gives the error about the terminal.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{fixture, scratch, tiza};

const NO_TERMINAL: &str = "the TUI needs a terminal";

/// A copy of the talk of `qa/fixtures/shared/talk/`.
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

fn master(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("talk.tiza")).unwrap()).unwrap()
}

/// Runs `tiza ARGS` and returns (success, stderr).
fn run(args: &[&str]) -> (bool, String) {
    let out = tiza(args);
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!err.contains("panicked"), "tiza {args:?}:\n{err}");
    (out.status.success(), err)
}

/// `tiza MASTER` on the master of `dir` with `change` applied to it.
fn open_changed(name: &str, change: impl FnOnce(&mut Value)) -> String {
    let dir = talk_copy(name);
    let mut raw = master(&dir);
    change(&mut raw);
    std::fs::write(dir.join("talk.tiza"), raw.to_string()).unwrap();
    let (ok, err) = run(&[dir.join("talk.tiza").to_str().unwrap()]);
    assert!(!ok, "{err}");
    err
}

#[test]
fn a_talk_that_tiza_can_play_opens_also_with_a_skipped_scene_that_is_missing() {
    let dir = talk_copy("talk-open-ok");
    assert!(!dir.join("not-shown.tiza").exists());
    let (ok, err) = run(&[dir.join("talk.tiza").to_str().unwrap()]);
    assert!(!ok && err.contains(NO_TERMINAL), "{err}");
}

#[test]
fn a_master_that_tiza_cannot_play_does_not_open_and_the_error_names_why() {
    let cases: Vec<(&str, Box<dyn FnOnce(&mut Value)>, &str)> = vec![
        (
            "both",
            Box::new(|m| m["code"] = json!(["x"])),
            "both `code` and `talk`",
        ),
        (
            "empty",
            Box::new(|m| m["talk"]["scenes"] = json!([])),
            "`scenes` is empty",
        ),
        (
            "all-skipped",
            Box::new(|m| m["talk"]["scenes"] = json!([{"file": "own.tiza", "skip": true}])),
            "every scene has `skip`",
        ),
        (
            "skip-text",
            Box::new(|m| m["talk"]["scenes"][2] = json!({"file": "own.tiza", "skip": "yes"})),
            "talk: scene 3: `skip` must be true or false",
        ),
        (
            "bad-name",
            Box::new(|m| m["talk"]["scenes"][2] = json!({"file": "own.tiza", "name": "my scene"})),
            "talk: scene 3: name",
        ),
        (
            "not-tiza",
            Box::new(|m| m["talk"]["scenes"][2] = json!("src/borrow.rs")),
            "src/borrow.rs is not a .tiza file",
        ),
        (
            "unknown-key",
            Box::new(|m| m["talk"]["order"] = json!("random")),
            "talk: unknown key `order`",
        ),
        (
            "twice",
            Box::new(|m| m["talk"]["scenes"][6] = json!("own.tiza")),
            "a talk shows a file once",
        ),
        (
            "missing",
            Box::new(|m| m["talk"]["scenes"][2] = json!({"file": "gone.tiza", "name": "own"})),
            "scene 3 (own, gone.tiza)",
        ),
        (
            "nested",
            Box::new(|m| m["talk"]["scenes"][2] = json!("talk.tiza")),
            "is a talk",
        ),
    ];
    for (name, change, want) in cases {
        let err = open_changed(&format!("talk-open-{name}"), change);
        assert!(err.contains(want), "{name}: want {want:?} in:\n{err}");
        assert!(!err.contains(NO_TERMINAL), "{name}:\n{err}");
    }
}

#[test]
fn a_scene_that_does_not_load_names_its_scene() {
    let dir = talk_copy("talk-open-broken-scene");
    std::fs::write(dir.join("own.tiza"), "{\"schema\": 1, \"code\": ").unwrap();
    let (ok, err) = run(&[dir.join("talk.tiza").to_str().unwrap()]);
    assert!(!ok && err.contains("scene 3 (own, own.tiza)"), "{err}");
}

#[test]
fn a_master_with_a_bom_and_crlf_opens_and_an_empty_or_binary_one_does_not() {
    let dir = talk_copy("talk-open-bom");
    let text = std::fs::read_to_string(dir.join("talk.tiza")).unwrap();
    let crlf = format!("\u{feff}{}", text.replace('\n', "\r\n"));
    std::fs::write(dir.join("talk.tiza"), crlf).unwrap();
    let (ok, err) = run(&[dir.join("talk.tiza").to_str().unwrap()]);
    assert!(!ok && err.contains(NO_TERMINAL), "{err}");
    for (name, bytes) in [("empty", &b""[..]), ("binary", &b"{\"talk\": \"\xff\xfe\"}"[..])] {
        let file = dir.join(format!("{name}.tiza"));
        std::fs::write(&file, bytes).unwrap();
        let (ok, err) = run(&[file.to_str().unwrap()]);
        assert!(!ok && err.contains(&format!("{name}.tiza")), "{name}:\n{err}");
        assert!(!err.contains(NO_TERMINAL), "{name}:\n{err}");
    }
}

#[test]
fn scene_needs_a_master_and_theme_file_and_lang_do_not_work_with_one() {
    let dir = talk_copy("talk-open-flags");
    let talk = dir.join("talk.tiza");
    let talk = talk.to_str().unwrap();
    let own = dir.join("own.tiza");
    let (ok, err) = run(&[own.to_str().unwrap(), "--scene", "2"]);
    assert!(!ok && err.contains("--scene opens a scene of a talk"), "{err}");
    let theme = dir.join("theme.toml");
    std::fs::write(&theme, "").unwrap();
    let (ok, err) = run(&[talk, "--theme-file", theme.to_str().unwrap()]);
    assert!(!ok && err.contains("--theme-file does not work with a talk"), "{err}");
    let (ok, err) = run(&[talk, "--lang", "py"]);
    assert!(!ok && err.contains("--lang does not work with a talk"), "{err}");
    // A scene that the talk does not have, and one that it skips.
    let (ok, err) = run(&[talk, "--scene", "nowhere"]);
    assert!(!ok && err.contains("nowhere") && !err.contains(NO_TERMINAL), "{err}");
    let (ok, err) = run(&[talk, "--scene", "4"]);
    assert!(!ok && err.contains("skipped"), "{err}");
    // A good scene: the talk opens.
    let (ok, err) = run(&[talk, "--scene", "own", "--step", "1"]);
    assert!(!ok && err.contains(NO_TERMINAL), "{err}");
}

#[test]
fn render_check_and_scene_put_refuse_a_master() {
    let dir = talk_copy("talk-open-commands");
    let talk = dir.join("talk.tiza");
    let talk = talk.to_str().unwrap();
    let sock = dir.join("none.sock");
    let before = std::fs::read(dir.join("talk.tiza")).unwrap();
    for args in [
        vec!["render", talk, "--out", "-"],
        vec!["check", talk],
        vec!["--socket", sock.to_str().unwrap(), "scene", "put", talk],
    ] {
        let (ok, err) = run(&args);
        assert!(!ok && err.contains("this file is a talk"), "{args:?}:\n{err}");
    }
    assert_eq!(std::fs::read(dir.join("talk.tiza")).unwrap(), before);
}
