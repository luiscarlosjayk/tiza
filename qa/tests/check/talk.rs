//! `tiza check` of a talk (a master `.tiza`): the check of each scene,
//! with its own layout, and what is wrong with the master itself.
//!
//! Acceptance criteria:
//! - AC1 Each scene is checked as `tiza check SCENE` checks it (its own
//!   padding, its own folder), and each finding has the file of its scene
//!   in front; a skipped scene is not read.
//! - AC2 Every bad entry of the master is a finding of its own, and the
//!   other scenes are checked all the same; a scene that is a master
//!   gives one finding, never the findings of its own scenes.
//! - AC3 The exit code counts every scene (and the warnings with
//!   `--strict`); `--scene` checks one scene as `tiza check SCENE` does.
//! - AC4 The clean width of the talk is the widest of its scenes, in
//!   columns of the terminal and in columns of code apart; a scene that is
//!   never clean leaves the talk without one, and says which scene.

use std::path::{Path, PathBuf};

use serde_json::Value;
use tiza_qa::{fixture, scratch};

/// Runs `tiza ARGS` in `cwd`: (success, stdout, stderr).
fn run_in(cwd: &Path, args: &[&str]) -> (bool, String, String) {
    let out = tiza_qa::command()
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    let (stdout, stderr) = (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    );
    assert!(!stderr.contains("panicked"), "tiza {args:?}:\n{stderr}");
    (out.status.success(), stdout, stderr)
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("{e}:\n{text}"))
}

/// A copy of `qa/fixtures/check/talk/`, and a folder beside it to run
/// from: the paths of the scenes are relative to the master, not to the
/// folder the command runs in.
fn talk_copy(name: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(name);
    tiza_qa::copy_dir(&fixture("check/talk"), &dir.join("talk"));
    std::fs::create_dir_all(dir.join("elsewhere")).unwrap();
    (dir.join("talk"), dir.join("elsewhere"))
}

#[test]
fn each_scene_is_checked_as_alone_with_its_file_in_front_of_its_findings() {
    let (talk, cwd) = talk_copy("check-talk-scenes");
    let master = talk.join("talk.tiza");
    let m = master.to_str().unwrap();
    // A narrow terminal: the padded scene cuts its code, the plain one
    // clips its note, the broken one has an anchor past its code.
    let (ok, out, _) = run_in(&cwd, &["check", m, "--width", "50", "--json"]);
    let report = json(&out);
    assert!(!ok && report["ok"] == false);
    let scenes = report["scenes"].as_array().unwrap();
    let mut findings = 0;
    for (k, file) in ["scenes/pad.tiza", "scenes/plain.tiza", "scenes/broken.tiza"]
        .iter()
        .enumerate()
    {
        let path = talk.join(file);
        let (_, alone, _) = run_in(
            &cwd,
            &["check", path.to_str().unwrap(), "--width", "50", "--json"],
        );
        let alone = json(&alone);
        assert_eq!(scenes[k]["status"], "checked");
        assert_eq!(scenes[k]["file"], *file);
        assert_eq!(scenes[k]["report"], alone, "{file}: not its check alone");
        let mine: Vec<&Value> = report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["scene_file"] == *file)
            .collect();
        assert_eq!(
            mine.len(),
            alone["findings"].as_array().unwrap().len(),
            "{file}"
        );
        for f in &mine {
            assert_eq!(f["scene"], k + 1);
        }
        findings += mine.len();
    }
    assert!(findings >= 3, "every scene has a finding at 50 columns");
    // The skipped scene, whose file is not there: not read.
    assert_eq!(scenes[3]["status"], "skipped");
    assert!(scenes[3].get("report").is_none());
    assert_eq!(report["findings"].as_array().unwrap().len(), findings);
    // The text: each line of a scene has its file in front, once.
    let (_, text, _) = run_in(&cwd, &["check", m, "--width", "50"]);
    let (_, alone, _) = run_in(
        &cwd,
        &[
            "check",
            talk.join("scenes/plain.tiza").to_str().unwrap(),
            "--width",
            "50",
        ],
    );
    let finding = alone.lines().find(|l| l.starts_with("warning: ")).unwrap();
    assert!(
        text.contains(&format!("scenes/plain.tiza: {finding}\n")),
        "{text}"
    );
    assert!(
        !text.contains("scenes/plain.tiza: scenes/plain.tiza"),
        "{text}"
    );
    assert!(text.contains("scenes/gone.tiza: skipped"), "{text}");
    // Every line names its scene next to the master, as the command line
    // gave it, or the master for the talk as a whole: the lines of a
    // summary after its first one too.
    let scenes_dir = talk.join("scenes");
    for line in text.lines() {
        assert!(
            line.starts_with(scenes_dir.to_str().unwrap()) || line.starts_with(m),
            "{line:?} in\n{text}"
        );
    }
}

#[test]
fn every_bad_entry_is_a_finding_and_the_other_scenes_are_checked() {
    for c in tiza_qa::bad_masters() {
        let dir = scratch(&format!("check-talk-bad-{}", c.name));
        tiza_qa::copy_dir(&fixture("shared/talk"), &dir);
        let path = dir.join("talk.tiza");
        let mut raw = json(&std::fs::read_to_string(&path).unwrap());
        (c.change)(&mut raw);
        std::fs::write(&path, raw.to_string()).unwrap();
        let (ok, out, _) = run_in(&dir, &["check", "talk.tiza", "--json"]);
        let report = json(&out);
        let messages: Vec<&str> = report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["message"].as_str().unwrap())
            .collect();
        assert!(!ok, "{}: exit 0", c.name);
        assert!(
            messages.iter().any(|m| m.contains(c.want)),
            "{}: want {:?} in {messages:?}",
            c.name,
            c.want
        );
        assert_eq!(report["errors"], 1, "{}: {messages:?}", c.name);
        let checked = report["scenes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["status"] == "checked")
            .count();
        // A scene that was not checked: no clean width for the talk.
        assert!(
            report["clean_terminal_width"].is_null() && report["clean_code_width"].is_null(),
            "{}: {report}",
            c.name
        );
        if c.stops {
            assert_eq!(checked, 0, "{}", c.name);
        } else {
            // The 7 entries less the skipped one and the bad one.
            assert_eq!(checked, 5, "{}: {}", c.name, report["scenes"]);
        }
    }
}

#[test]
fn a_scene_that_is_a_master_gives_one_finding_and_not_the_findings_of_its_scenes() {
    let (talk, cwd) = talk_copy("check-talk-nested");
    // A master with a broken scene of its own, as a scene of the talk; the
    // talk itself as a scene too; a missing scene; a skip that is a text.
    std::fs::write(
        talk.join("inner.tiza"),
        r#"{"schema": 2, "talk": {"scenes": ["scenes/broken.tiza"]}}"#,
    )
    .unwrap();
    std::fs::write(
        talk.join("bad.tiza"),
        r#"{"schema": 2, "talk": {"scenes": [
            "inner.tiza", "bad.tiza", "scenes/nope.tiza",
            {"file": "scenes/plain.tiza", "skip": "yes"}, "scenes/pad.tiza"]}}"#,
    )
    .unwrap();
    let m = talk.join("bad.tiza");
    let (ok, out, _) = run_in(&cwd, &["check", m.to_str().unwrap(), "--json"]);
    let report = json(&out);
    assert!(!ok);
    let status: Vec<&str> = report["scenes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["status"].as_str().unwrap())
        .collect();
    assert_eq!(status, ["talk", "talk", "missing", "invalid", "checked"]);
    let findings = report["findings"].as_array().unwrap();
    assert_eq!(report["errors"], 4, "{findings:?}");
    for (k, want) in [
        (
            1,
            "scene 1 (inner, inner.tiza) is a talk: a talk plays documents, not other talks",
        ),
        (2, "scene 2 (bad, bad.tiza) is a talk"),
        (3, "scene 3 (nope, scenes/nope.tiza)"),
        (4, "talk: scene 4: `skip` must be true or false"),
    ] {
        let of: Vec<&Value> = findings.iter().filter(|f| f["scene"] == k).collect();
        assert_eq!(of.len(), 1, "scene {k}: {of:?}");
        let message = of[0]["message"].as_str().unwrap();
        assert!(
            message.contains(want) && !message.contains("open it with"),
            "{message}"
        );
    }
    // The broken scene of the inner master is not checked.
    assert!(!out.contains("line 7 does not exist"), "{out}");
    // The position of a scene after a bad entry stays its own.
    assert_eq!(report["scenes"][4]["position"], 5);
}

#[test]
fn the_exit_code_counts_every_scene_and_scene_checks_one() {
    let (talk, cwd) = talk_copy("check-talk-exit");
    let widths = talk.join("widths.tiza");
    let w = widths.to_str().unwrap();
    // A warning only in the last scene: 0, and 1 with --strict.
    let (ok, _, _) = run_in(&cwd, &["check", w, "--width", "60"]);
    assert!(ok);
    let (ok, out, _) = run_in(&cwd, &["check", w, "--width", "60", "--strict"]);
    assert!(!ok, "{out}");
    // `--scene` (a position or a name, after a skipped scene too): the
    // report of `tiza check SCENE`, word for word.
    let m = talk.join("talk.tiza");
    let m = m.to_str().unwrap();
    for (spec, file) in [("2", "scenes/plain.tiza"), ("broken", "scenes/broken.tiza")] {
        let (ok, one, _) = run_in(&cwd, &["check", m, "--scene", spec, "--width", "50"]);
        let path = talk.join(file);
        let (ok_alone, alone, _) =
            run_in(&cwd, &["check", path.to_str().unwrap(), "--width", "50"]);
        assert_eq!(ok, ok_alone, "--scene {spec}");
        assert_eq!(one, alone, "--scene {spec}");
        let (_, before, _) = run_in(&cwd, &["--scene", spec, "check", m, "--width", "50"]);
        assert_eq!(before, alone, "tiza --scene {spec} check");
    }
    // A skipped scene: how to check it anyway.
    let (ok, _, err) = run_in(&cwd, &["check", m, "--scene", "4"]);
    assert!(
        !ok && err.contains("is skipped") && err.contains("tiza check"),
        "{err}"
    );
    // `--scene` with a document, and a master on stdin.
    let (ok, _, err) = run_in(
        &cwd,
        &[
            "check",
            talk.join("scenes/pad.tiza").to_str().unwrap(),
            "--scene",
            "1",
        ],
    );
    assert!(
        !ok && err.contains("--scene opens a scene of a talk"),
        "{err}"
    );
    let r = tiza_qa::command()
        .current_dir(&cwd)
        .args(["check", "-"])
        .stdin(std::fs::File::open(&widths).unwrap())
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(
        !r.status.success() && err.contains("give the path of the master"),
        "{err}"
    );
}

#[test]
fn the_clean_width_of_a_talk_is_the_widest_of_its_scenes() {
    let (talk, cwd) = talk_copy("check-talk-widths");
    let alone = |file: &str| {
        let path = talk.join(file);
        json(&run_in(&cwd, &["check", path.to_str().unwrap(), "--json"]).1)
    };
    let (pad, plain) = (alone("scenes/pad.tiza"), alone("scenes/plain.tiza"));
    let terminal = |r: &Value| r["terminal"]["min_clean_width"].as_u64().unwrap();
    let code = |r: &Value| r["min_clean_width"].as_u64().unwrap();
    // The padded scene needs the wider terminal, the plain one the more
    // columns of code: the two widths of the talk come from two scenes.
    assert!(terminal(&pad) > terminal(&plain) && code(&pad) < code(&plain));
    let widths = talk.join("widths.tiza");
    let (_, out, _) = run_in(&cwd, &["check", widths.to_str().unwrap(), "--json"]);
    let report = json(&out);
    assert_eq!(report["clean_terminal_width"], terminal(&pad));
    assert_eq!(report["clean_code_width"], code(&plain));
    let (_, text, _) = run_in(&cwd, &["check", widths.to_str().unwrap()]);
    assert!(
        text.contains(&format!(
            "every scene shows its labels whole at {} or wider ({} columns of code",
            terminal(&pad),
            code(&plain)
        )),
        "{text}"
    );
    // A scene that clips a label at every width: no width for the talk.
    let never = talk.join("never.tiza");
    let (_, out, _) = run_in(&cwd, &["check", never.to_str().unwrap(), "--json"]);
    let report = json(&out);
    assert!(report["clean_terminal_width"].is_null() && report["clean_code_width"].is_null());
    let (_, text, _) = run_in(&cwd, &["check", never.to_str().unwrap()]);
    assert!(
        text.contains("scenes/never.tiza clips a label at every width")
            && !text.contains("every scene shows"),
        "{text}"
    );
}

#[test]
fn a_control_character_in_the_name_of_a_scene_file_never_reaches_the_terminal() {
    let (talk, cwd) = talk_copy("check-talk-control");
    let name = "e\u{1b}[31mvil.tiza";
    std::fs::copy(talk.join("scenes/plain.tiza"), talk.join(name)).unwrap();
    std::fs::write(
        talk.join("esc.tiza"),
        serde_json::json!({"schema": 2, "talk": {"scenes": [name]}}).to_string(),
    )
    .unwrap();
    let m = talk.join("esc.tiza");
    let (_, text, err) = run_in(&cwd, &["check", m.to_str().unwrap(), "--width", "50"]);
    assert!(text.contains("vil.tiza: warning"), "{text}");
    assert!(
        !text.contains('\u{1b}') && !err.contains('\u{1b}'),
        "{text:?}"
    );
}
