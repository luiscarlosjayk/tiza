//! t-166 — Exporting the live instance (`tiza render` with no FILE or with
//! `--socket`, and the MCP `export_image`) draws the files a `code.link`
//! shows, never panics, and every export call answers.
//!
//! Acceptance criteria:
//! - AC1 `tiza render` with no FILE loads the linked files like opening
//!   the `.tiza` does, so every step draws the file the live TUI shows:
//!   `--step N --format md` of every step (step 0 included, with no column
//!   shift), `--all-steps` and the GIF are the same as `tiza render FILE`.
//!   The instance sends them already loaded and masked (`scene.get
//!   {"as_shown": true}` embeds them under `views`, t-174): nothing is read
//!   from the client's directory.
//! - AC2 Same for the MCP `export_image` (png compared with the file's
//!   render, md, gif).
//! - AC3 A missing or unreadable linked file is a clear error naming it
//!   (through the redact filter), never a panic; the layout never indexes
//!   out of bounds on a view with no loaded lines.
//! - AC4 (QA 2) Every export call answers, with a result or an error: a
//!   panic while an MCP call runs is an error reply, never a call left
//!   waiting (the unit test `answer_always_*` in tiza-mcp drives a panic).
//! - AC5 This file; CHANGELOG.
//! - Review round 1 (adversarial review of 2f653ce), one test each (`r1`…`r8`):
//!   R1 steps capped at 999 everywhere a step enters a document, and the
//!   take's loops visit only the steps where something changes; R2
//!   `export_image` off the async threads with a deadline (120 s): `status`
//!   answers meanwhile, past it an error and nothing written; R3 holds
//!   capped at one hour, `--interval`/`interval` checked, saturating
//!   arithmetic in anim/gif/cast; R4 `first_line` capped, `last_line`
//!   saturates, a panicking request answers `internal` and the TUI keeps
//!   running with the document as before; R5 `check` at the cap; R6 a step
//!   past the last one is said; R7 the live `.tape` opens the instance's
//!   `.tiza`; R8 a real MCP panic (test hook) answers an error.
//!
//! Corner cases: a linked file deleted while the TUI is open (the instance
//! keeps the code it shows, so the live export draws it and a render of the
//! `.tiza` names the missing file); a live document with no `.tiza` on disk
//! and a `move.add code.link`; a redact inside a linked file stays masked
//! in the live export.
//!
//! The repros are QA's pruebas/t046/livebug.py, mcpbug.py, pruebas/t166/
//! qa_t166.py and pruebas/t174/p3/panic_probe.py (on Luis's Mac), rebuilt
//! here on scratch copies of examples/tally-tour.tiza and
//! examples/stack-and-queue.tiza.

use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, bin, example, scratch};

/// Every MCP export must answer well before this (debug builds included).
const ANSWER: Duration = Duration::from_secs(120);

/// Runs `tiza <args>` from `cwd`, so a live render cannot lean on files
/// next to the test.
fn tiza_in(cwd: &Path, args: &[&str]) -> Output {
    std::process::Command::new(bin())
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("run tiza")
}

fn ok(cwd: &Path, args: &[&str]) -> Vec<u8> {
    let out = tiza_in(cwd, args);
    assert!(
        out.status.success(),
        "tiza {args:?} failed with {:?}:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

/// Scratch copy of a demo: the `.tiza` and the files it links.
fn demo(name: &str, which: &str) -> PathBuf {
    let dir = scratch(name);
    match which {
        "tally" => {
            std::fs::create_dir_all(dir.join("tally")).unwrap();
            copy_dir(&example("tally/src"), &dir.join("tally/src"));
            std::fs::copy(example("tally-tour.tiza"), dir.join("tally-tour.tiza")).unwrap();
            dir.join("tally-tour.tiza")
        }
        _ => {
            copy_dir(&example("src"), &dir.join("src"));
            std::fs::copy(
                example("stack-and-queue.tiza"),
                dir.join("stack-and-queue.tiza"),
            )
            .unwrap();
            dir.join("stack-and-queue.tiza")
        }
    }
}

fn open_live(path: &Path) -> Live {
    let doc = tiza_core::fs::load_document(path).unwrap();
    let sock = path.parent().unwrap().join("live.sock");
    Live::open(doc, path.to_path_buf(), sock)
}

/// An empty directory to run live renders from.
fn elsewhere(name: &str) -> PathBuf {
    scratch(&format!("{name}-cwd"))
}

fn sock(live: &Live) -> &str {
    live.socket.to_str().unwrap()
}

fn render_file(path: &Path, step: u32, format: &str) -> String {
    let s = step.to_string();
    String::from_utf8(ok(
        path.parent().unwrap(),
        &[
            "render",
            path.to_str().unwrap(),
            "--step",
            &s,
            "--format",
            format,
            "--width",
            "100",
            "--out",
            "-",
        ],
    ))
    .unwrap()
}

fn render_live(live: &Live, cwd: &Path, step: u32, format: &str) -> String {
    let s = step.to_string();
    String::from_utf8(ok(
        cwd,
        &[
            "render",
            "--socket",
            sock(live),
            "--step",
            &s,
            "--format",
            format,
            "--width",
            "100",
            "--out",
            "-",
        ],
    ))
    .unwrap()
}

fn max_step(path: &Path) -> u32 {
    tiza_core::fs::load_document(path).unwrap().max_step()
}

fn first_line_of(file: &Path, contains: &str) -> String {
    std::fs::read_to_string(file)
        .unwrap()
        .lines()
        .find(|l| l.contains(contains))
        .unwrap()
        .trim()
        .to_string()
}

fn every_step_matches(which: &str) {
    let path = demo(&format!("steps-{which}"), which);
    let live = open_live(&path);
    let cwd = elsewhere(&format!("steps-{which}"));
    for step in 0..=max_step(&path) {
        for format in ["md", "txt"] {
            let want = render_file(&path, step, format);
            let got = render_live(&live, &cwd, step, format);
            assert_eq!(
                got, want,
                "{which}: live --step {step} --format {format} differs from the file's render"
            );
        }
    }
}

#[test]
fn ac1_tally_every_step_live_is_the_files_render_step_0_included() {
    every_step_matches("tally");
}

#[test]
fn ac1_stack_and_queue_every_step_live_is_the_files_render() {
    // QA 1 (20:20): `--step 1` exited 101 here.
    every_step_matches("stack");
}

#[test]
fn ac1_step_2_of_tally_shows_args_rs_as_on_screen() {
    let path = demo("args", "tally");
    let live = open_live(&path);
    let cwd = elsewhere("args");
    let md = render_live(&live, &cwd, 2, "md");
    let args = path.parent().unwrap().join("tally/src/args.rs");
    let main = path.parent().unwrap().join("tally/src/main.rs");
    let args_line = first_line_of(&args, "pub fn parse(");
    let main_line = first_line_of(&main, "fn main(");
    assert!(md.contains(&args_line), "step 2 must show args.rs:\n{md}");
    assert!(
        !md.contains(&main_line),
        "step 2 must not show main.rs:\n{md}"
    );
    // What the TUI shows at step 2 is args.rs too.
    live.api_ok("step.set", json!({"step": 2}));
    let text = live.api_ok("render.text", json!({}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        text.contains("pub fn parse(") && !text.contains("fn main("),
        "{text}"
    );
}

fn all_steps_match(which: &str) {
    let path = demo(&format!("all-{which}"), which);
    let live = open_live(&path);
    let cwd = elsewhere(&format!("all-{which}"));
    let out = path.parent().unwrap();
    let from_file = out.join("from-file");
    let from_live = out.join("from-live");
    ok(
        out,
        &[
            "render",
            path.to_str().unwrap(),
            "--all-steps",
            "--format",
            "md",
            "--width",
            "100",
            "--out",
            from_file.to_str().unwrap(),
        ],
    );
    ok(
        &cwd,
        &[
            "render",
            "--socket",
            sock(&live),
            "--all-steps",
            "--format",
            "md",
            "--width",
            "100",
            "--out",
            from_live.to_str().unwrap(),
        ],
    );
    let mut names: Vec<String> = std::fs::read_dir(&from_file)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names.len() as u32, max_step(&path) + 1, "{names:?}");
    for n in names {
        assert_eq!(
            std::fs::read_to_string(from_live.join(&n)).unwrap(),
            std::fs::read_to_string(from_file.join(&n)).unwrap(),
            "{which}: --all-steps {n} differs"
        );
    }
}

#[test]
fn ac1_all_steps_live_is_the_files_render() {
    all_steps_match("tally");
    all_steps_match("stack");
}

fn gif_matches(which: &str) {
    let path = demo(&format!("gif-{which}"), which);
    let live = open_live(&path);
    let cwd = elsewhere(&format!("gif-{which}"));
    let dir = path.parent().unwrap();
    let file_gif = dir.join("file.gif");
    let live_gif = dir.join("live.gif");
    ok(
        dir,
        &[
            "render",
            path.to_str().unwrap(),
            "--width",
            "100",
            "--scale",
            "1",
            "--out",
            file_gif.to_str().unwrap(),
        ],
    );
    ok(
        &cwd,
        &[
            "render",
            "--socket",
            sock(&live),
            "--width",
            "100",
            "--scale",
            "1",
            "--out",
            live_gif.to_str().unwrap(),
        ],
    );
    assert!(
        std::fs::read(&live_gif).unwrap() == std::fs::read(&file_gif).unwrap(),
        "{which}: the live GIF differs from the file's"
    );
}

#[test]
fn ac1_live_gif_does_not_panic_and_is_the_files_gif() {
    gif_matches("tally");
    gif_matches("stack");
}

/// `export_image` through the MCP server, with a deadline: it must answer.
fn export(mcp: &mut McpClient, args: Value) -> (bool, String) {
    let v = mcp
        .request_within(
            "tools/call",
            json!({"name": "export_image", "arguments": args}),
            ANSWER,
        )
        .unwrap_or_else(|| panic!("export_image {args} did not answer within {ANSWER:?}"));
    let is_error = v["result"]["isError"] == true || !v["error"].is_null();
    let text = v["result"]["content"][0]["text"]
        .as_str()
        .map(str::to_string)
        .or_else(|| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_default();
    (is_error, text)
}

#[test]
fn ac2_mcp_export_image_png_md_gif_answer_and_draw_the_linked_file() {
    let path = demo("mcp", "tally");
    let live = open_live(&path);
    let dir = path.parent().unwrap().to_path_buf();
    let mut mcp = McpClient::start(&live.socket);

    // PNG at step 2: pixel for pixel the file's render.
    let png = dir.join("mcp.png");
    let (err, text) = export(
        &mut mcp,
        json!({"path": png.to_str().unwrap(), "step": 2, "width": 100}),
    );
    assert!(!err, "{text}");
    let file_png = dir.join("file.png");
    ok(
        &dir,
        &[
            "render",
            path.to_str().unwrap(),
            "--step",
            "2",
            "--width",
            "100",
            "--out",
            file_png.to_str().unwrap(),
        ],
    );
    assert!(
        std::fs::read(&png).unwrap() == std::fs::read(&file_png).unwrap(),
        "the MCP PNG of step 2 differs from the file's render"
    );

    // md of every step: the file's render.
    for step in 0..=max_step(&path) {
        let md = dir.join(format!("mcp-{step}.md"));
        let (err, text) = export(
            &mut mcp,
            json!({"path": md.to_str().unwrap(), "step": step, "width": 100}),
        );
        assert!(!err, "step {step}: {text}");
        assert_eq!(
            std::fs::read_to_string(&md).unwrap(),
            render_file(&path, step, "md"),
            "MCP md of step {step}"
        );
    }

    // GIF: QA 2's call that never answered.
    let gif = dir.join("mcp.gif");
    let (err, text) = export(
        &mut mcp,
        json!({"path": gif.to_str().unwrap(), "width": 100, "scale": 1}),
    );
    assert!(!err, "{text}");
    let reply: Value = serde_json::from_str(&text).unwrap();
    assert!(reply["frames"].as_u64().unwrap() > 1, "{reply}");
    let bytes = std::fs::read(&gif).unwrap();
    assert!(bytes.starts_with(b"GIF89a"), "not a GIF");
    // The server still answers afterwards.
    let (err, text) = export(
        &mut mcp,
        json!({"path": dir.join("after.md").to_str().unwrap(), "step": 2}),
    );
    assert!(!err, "{text}");
}

#[test]
fn ac2_mcp_export_of_stack_and_queue_answers_at_every_step() {
    let path = demo("mcp-stack", "stack");
    let live = open_live(&path);
    let dir = path.parent().unwrap().to_path_buf();
    let mut mcp = McpClient::start(&live.socket);
    for step in 0..=max_step(&path) {
        let md = dir.join(format!("s{step}.md"));
        let (err, text) = export(
            &mut mcp,
            json!({"path": md.to_str().unwrap(), "step": step, "width": 100}),
        );
        assert!(!err, "step {step}: {text}");
        assert_eq!(
            std::fs::read_to_string(&md).unwrap(),
            render_file(&path, step, "md"),
            "step {step}"
        );
    }
    let (err, text) = export(
        &mut mcp,
        json!({"path": dir.join("s.gif").to_str().unwrap(), "width": 100, "scale": 1}),
    );
    assert!(!err, "{text}");
}

#[test]
fn corner_a_linked_file_deleted_with_the_tui_open() {
    let path = demo("deleted", "tally");
    let live = open_live(&path);
    let cwd = elsewhere("deleted");
    let before = render_file(&path, 3, "md");
    let count = path.parent().unwrap().join("tally/src/count.rs");
    std::fs::remove_file(&count).unwrap();

    // The instance still shows count.rs (linked files are not followed):
    // the live export draws what is on screen, with no panic.
    assert_eq!(render_live(&live, &cwd, 3, "md"), before);
    let gif = cwd.join("live.gif");
    ok(
        &cwd,
        &[
            "render",
            "--socket",
            sock(&live),
            "--width",
            "100",
            "--scale",
            "1",
            "--out",
            gif.to_str().unwrap(),
        ],
    );
    let mut mcp = McpClient::start(&live.socket);
    let md = cwd.join("mcp.md");
    let (err, text) = export(
        &mut mcp,
        json!({"path": md.to_str().unwrap(), "step": 3, "width": 100}),
    );
    assert!(!err, "{text}");
    assert_eq!(std::fs::read_to_string(&md).unwrap(), before);

    // A render of the `.tiza` now names the missing file: an error, exit 1,
    // never a panic (101).
    let out = tiza_in(
        &cwd,
        &[
            "render",
            path.to_str().unwrap(),
            "--step",
            "3",
            "--out",
            "-",
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("tally/src/count.rs") && err.contains("could not read"),
        "{err}"
    );
    assert!(!err.contains("panicked"), "{err}");
}

const VALUE: &str = "9vK7wPx3Lm";

fn pieces_absent(what: &str, s: &str) {
    let chars: Vec<char> = VALUE.chars().collect();
    for w in chars.windows(4) {
        let piece: String = w.iter().collect();
        assert!(!s.contains(&piece), "{what} holds «{piece}»:\n{s}");
    }
}

/// A main file with a masked token and a linked file named after it (a
/// name repeating a masked value), with a redact of its own.
fn secret_demo(name: &str) -> PathBuf {
    secret_demo_linking(name, &format!("vault-{VALUE}.sh"))
}

fn secret_demo_linking(name: &str, linked: &str) -> PathBuf {
    let dir = scratch(name);
    std::fs::write(
        dir.join("main.sh"),
        format!("# deploy\nTOKEN={VALUE}\nrun\n"),
    )
    .unwrap();
    std::fs::write(
        dir.join(linked),
        format!("# vault\nexport KEY={VALUE}\necho ok\n"),
    )
    .unwrap();
    let doc = json!({
        "schema": 1,
        "title": "Deploy",
        "source": {"language": "sh", "first_line": 1},
        "code": {"file": "main.sh"},
        "step": 0,
        "moves": [{"at_step": 2, "action": "code.link", "file": linked}],
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 6, "end": 16}},
            {"id": "r2", "step": 2, "type": "redact", "file": linked, "span": {"line": 2, "start": 11, "end": 21}},
            {"id": "n1", "step": 2, "type": "note", "file": linked, "line": 3, "text": "prints ok"}
        ]
    });
    let path = dir.join("deploy.tiza");
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    path
}

#[test]
fn corner_a_redact_in_a_linked_file_stays_masked_in_the_live_export() {
    let path = secret_demo("secret");
    let live = open_live(&path);
    let cwd = elsewhere("secret");
    for step in 0..=max_step(&path) {
        let got = render_live(&live, &cwd, step, "md");
        assert_eq!(got, render_file(&path, step, "md"), "step {step}");
        pieces_absent(&format!("live md of step {step}"), &got);
    }
    let md = render_live(&live, &cwd, 2, "md");
    assert!(
        md.contains("export KEY=") && md.contains("prints ok"),
        "{md}"
    );
    let mut mcp = McpClient::start(&live.socket);
    let out = cwd.join("s.md");
    let (err, text) = export(
        &mut mcp,
        json!({"path": out.to_str().unwrap(), "step": 2, "width": 100}),
    );
    assert!(!err, "{text}");
    let got = std::fs::read_to_string(&out).unwrap();
    assert_eq!(got, md);
    pieces_absent("MCP md", &got);
}

#[test]
fn ac3_a_missing_linked_file_is_named_through_the_redact_filter() {
    let render_missing = |path: &Path, linked: &str| {
        std::fs::remove_file(path.parent().unwrap().join(linked)).unwrap();
        let out = tiza_in(
            path.parent().unwrap(),
            &["render", path.to_str().unwrap(), "--out", "-"],
        );
        assert_eq!(out.status.code(), Some(1), "an error, never a panic");
        String::from_utf8_lossy(&out.stderr).into_owned()
    };
    // A name that repeats nothing masked is named (its path).
    let path = secret_demo_linking("missing-plain", "notes.sh");
    let err = render_missing(&path, "notes.sh");
    assert!(
        err.contains("move #1, step 2: code.link")
            && err.contains("could not read")
            && err.contains("/notes.sh"),
        "{err}"
    );
    pieces_absent("the error", &err);
    // A name repeating a masked value is not: the move and step say which.
    let linked = format!("vault-{VALUE}.sh");
    let path = secret_demo("missing-secret");
    let err = render_missing(&path, &linked);
    assert!(
        err.contains("move #1, step 2: code.link") && err.contains("could not read"),
        "{err}"
    );
    pieces_absent("the error", &err);
}

#[test]
fn ac3_a_scene_without_a_linked_files_code_is_an_error_naming_it() {
    // The client never guesses a linked file from its own disk: a scene
    // whose `views` lack one (an older instance) says which.
    let path = demo("noview", "tally");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let mut copy: tiza_core::Document =
        serde_json::from_value(serde_json::to_value(doc.embedded_copy()).unwrap()).unwrap();
    assert!(
        copy.views.remove("tally/src/args.rs").is_some(),
        "{:?}",
        copy.views.keys()
    );
    let e = tiza_core::fs::load_scene(&mut copy)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("tally/src/args.rs") && e.contains("move #1") && e.contains("did not send"),
        "{e}"
    );
    // With every view, it loads like the file.
    let mut full: tiza_core::Document =
        serde_json::from_value(serde_json::to_value(doc.embedded_copy()).unwrap()).unwrap();
    tiza_core::fs::load_scene(&mut full).unwrap();
    assert_eq!(full.view_code.len(), doc.view_code.len());
}

#[test]
fn ac3_layout_never_indexes_out_of_bounds_on_a_view_with_no_loaded_lines() {
    // What the live render did before t-166: the wire copy with its code
    // resolved and no view loaded, laid out at every step and as a GIF.
    for which in ["tally", "stack"] {
        let path = demo(&format!("oob-{which}"), which);
        let doc = tiza_core::fs::load_document(&path).unwrap();
        let mut d: tiza_core::Document =
            serde_json::from_value(serde_json::to_value(doc.embedded_copy()).unwrap()).unwrap();
        tiza_core::fs::resolve_code(&mut d, None).unwrap();
        let theme = tiza_core::Theme::dark();
        for step in 0..=d.max_step() {
            for stable in [false, true] {
                let _ = tiza_core::layout::layout(
                    &d,
                    &theme,
                    &tiza_core::layout::LayoutOptions {
                        width: 100,
                        step,
                        focus: None,
                        pointer: None,
                        line_numbers: true,
                        tab_width: 4,
                        typing: false,
                        redact: true,
                        stable,
                        margin_cap: None,
                    },
                );
            }
        }
        let _ = tiza_core::anim::frames(
            &d,
            &theme,
            &tiza_core::anim::AnimOptions {
                interval: 2.0,
                width: 100,
                line_numbers: true,
                from_zero: true,
            },
        );
    }
}

#[test]
fn corner_a_live_document_with_no_tiza_and_a_move_add_code_link() {
    let dir = scratch("unsaved");
    copy_dir(&example("tally/src"), &dir.join("src"));
    let main = dir.join("src/main.rs");
    let opened = tiza_core::fs::open(main.to_str().unwrap()).unwrap();
    assert!(!opened.existing && !opened.save_path.exists());
    let live = Live::open(opened.document, opened.save_path, dir.join("live.sock"));
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.link", "file": "args.rs"}),
    );
    let cwd = elsewhere("unsaved");
    let md = render_live(&live, &cwd, 1, "md");
    let args_line = first_line_of(&dir.join("src/args.rs"), "pub fn parse(");
    assert!(md.contains(&args_line), "step 1 must show args.rs:\n{md}");
    // The same as rendering the instance's own copy (code and views
    // embedded) as a file, at every step.
    let scene = live.api_ok("scene.get", json!({"as_shown": true}));
    let copy = cwd.join("copy.tiza");
    std::fs::write(&copy, serde_json::to_string(&scene["document"]).unwrap()).unwrap();
    for step in 0..=1 {
        assert_eq!(
            render_live(&live, &cwd, step, "md"),
            render_file(&copy, step, "md"),
            "step {step}"
        );
    }
    let mut mcp = McpClient::start(&live.socket);
    let out = cwd.join("m.md");
    let (err, text) = export(
        &mut mcp,
        json!({"path": out.to_str().unwrap(), "step": 1, "width": 100}),
    );
    assert!(!err, "{text}");
    assert_eq!(std::fs::read_to_string(&out).unwrap(), md);
    let (err, text) = export(
        &mut mcp,
        json!({"path": cwd.join("u.gif").to_str().unwrap(), "width": 100, "scale": 1}),
    );
    assert!(!err, "{text}");
}

#[test]
fn ac5_changelog_names_the_card() {
    let log = std::fs::read_to_string(example("../CHANGELOG.md")).unwrap();
    let fixed = &log[log.find("### Fixed").unwrap()..];
    assert!(
        fixed.contains("(t-166") && fixed.contains("load_scene"),
        "CHANGELOG.md has no Fixed entry for t-166"
    );
}

/// The scene the instance sends has its moves' content masked (t-140):
/// readied for the export, it draws what the file's render draws, and the
/// value never comes back.
#[test]
fn corner_masked_move_content_in_the_live_export() {
    let dir = scratch("moves-masked");
    std::fs::write(
        dir.join("main.sh"),
        format!("# deploy\nTOKEN={VALUE}\nrun\n"),
    )
    .unwrap();
    let doc = json!({
        "schema": 1,
        "title": "Deploy",
        "source": {"language": "sh", "first_line": 1},
        "code": {"file": "main.sh"},
        "step": 0,
        "moves": [
            {"at_step": 2, "action": "code.patch", "op": "replace", "find": "run", "content": format!("run --key {VALUE}")},
            {"at_step": 3, "action": "select", "from": {"find": "TOKEN"}, "to": {"find": "run"}}
        ],
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 6, "end": 16}}
        ]
    });
    let path = dir.join("deploy.tiza");
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    let live = open_live(&path);
    let cwd = elsewhere("moves-masked");
    for step in 0..=3 {
        let l = render_live(&live, &cwd, step, "md");
        assert_eq!(l, render_file(&path, step, "md"), "step {step}");
        pieces_absent(&format!("live md of step {step}"), &l);
    }
}

// ---------- Review round 1 (adversarial review of 2f653ce) ----------

/// A small document with a masked token, as a value.
fn small_doc() -> Value {
    json!({
        "schema": 1,
        "title": "Deploy",
        "source": {"language": "sh", "first_line": 1},
        "code": ["# deploy", format!("TOKEN={VALUE}"), "run", "done"],
        "step": 0,
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 6, "end": 16}},
            {"id": "n1", "step": 1, "type": "note", "line": 3, "text": "runs it"}
        ]
    })
}

fn live_doc(name: &str, doc: &Value) -> (PathBuf, Live) {
    let dir = scratch(name);
    let path = dir.join("doc.tiza");
    std::fs::write(&path, serde_json::to_string_pretty(doc).unwrap()).unwrap();
    let live = open_live(&path);
    (path, live)
}

fn api_err(live: &Live, method: &str, params: Value) -> (String, String) {
    let r = live.api(method, params);
    let e = r
        .error
        .unwrap_or_else(|| panic!("{method} was accepted: {:?}", r.result));
    (e.code, e.message)
}

/// R1: a step past the cap is refused everywhere a step enters a document,
/// with a clear error that quotes nothing under a mask.
#[test]
fn r1_steps_past_the_cap_are_refused_everywhere() {
    let (path, live) = live_doc("r1", &small_doc());
    let huge = 4_294_967_295u32;
    for (method, params) in [
        (
            "move.add",
            json!({"at_step": huge, "action": "wait", "ms": 10}),
        ),
        (
            "move.add",
            json!({"at_step": 1000, "action": "cursor", "line": 1}),
        ),
        (
            "annotation.add",
            json!({"type": "note", "line": 3, "text": "x", "step": 1000}),
        ),
        (
            "scene.patch",
            json!({"annotations": [{"id": "n2", "type": "note", "line": 3, "text": "x", "step": huge}]}),
        ),
        (
            "scene.patch",
            json!({"steps": [{"step": 5000, "title": "far"}]}),
        ),
    ] {
        let (code, msg) = api_err(&live, method, params.clone());
        assert_eq!(code, "invalid_argument", "{method} {params}: {msg}");
        assert!(
            msg.contains("past the last step a document can have (999)"),
            "{msg}"
        );
        pieces_absent(method, &msg);
    }
    // Updating an annotation's step past the cap.
    let (code, msg) = api_err(
        &live,
        "annotation.update",
        json!({"id": "n1", "step": huge}),
    );
    assert_eq!(code, "invalid_argument", "{msg}");
    // A put of a document with such a step.
    let mut doc = small_doc();
    doc["moves"] = json!([{"at_step": huge, "action": "wait", "ms": 5}]);
    let (code, msg) = api_err(&live, "scene.put", json!({"document": doc}));
    assert_eq!(code, "invalid_argument", "{msg}");
    pieces_absent("scene.put", &msg);
    // Nothing of it reached the document.
    let status = live.api_ok("status", json!({}));
    assert_eq!(status["max_step"], 1, "{status}");
    // A .tiza on disk with such a step: `render` and `check` say so (exit 1).
    let far = path.parent().unwrap().join("far.tiza");
    std::fs::write(&far, serde_json::to_string(&doc).unwrap()).unwrap();
    for args in [
        vec!["render", far.to_str().unwrap(), "--out", "-"],
        vec!["check", far.to_str().unwrap()],
    ] {
        let out = tiza_in(path.parent().unwrap(), &args);
        let text = String::from_utf8_lossy(&out.stderr).into_owned()
            + &String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {text}");
        assert!(text.contains("past the last step"), "{args:?}: {text}");
        pieces_absent("the load error", &text);
    }
}

/// R1/R5: at the cap, a live export and `check` cost what a two-step take
/// costs: only the steps where something changes are laid out. Compared
/// by the work done, not by time: `check` names every empty step once and
/// answers, and the live export of step 0 is the file's.
#[test]
fn r1_r5_a_take_at_the_cap_exports_and_checks_like_a_short_one() {
    let mut doc = small_doc();
    doc["annotations"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "n9", "step": 999, "type": "note", "line": 4, "text": "the end"}));
    doc["moves"] = json!([{"at_step": 500, "action": "code.patch", "op": "replace", "find": "done", "content": "done!"}]);
    let (path, live) = live_doc("r5", &doc);
    let cwd = elsewhere("r5");
    for step in [0, 1, 499, 500, 998, 999] {
        assert_eq!(
            render_live(&live, &cwd, step, "md"),
            render_file(&path, step, "md"),
            "step {step}"
        );
    }
    // The geometry, the masked values and `check` visit the change steps:
    // 0, 1, 2, 500, 999 (and 1000, past the last).
    let d = tiza_core::fs::load_document(&path).unwrap();
    assert_eq!(d.change_steps(d.max_step()), vec![0, 1, 2, 500, 999]);
    let started = std::time::Instant::now();
    let report = live.api_ok("check", json!({}));
    let took = started.elapsed();
    let empty = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "empty_step")
        .count();
    assert_eq!(empty, 996, "{report}");
    // Generous: a frozen TUI took ~2.3 s per thousand steps before.
    assert!(took < Duration::from_secs(20), "check took {took:?}");
}

fn many_steps(n: u32) -> Value {
    let mut doc = small_doc();
    let anns = doc["annotations"].as_array_mut().unwrap();
    for s in 2..=n {
        anns.push(
            json!({"id": format!("h{s}"), "step": s, "type": "highlight",
            "span": {"line": 3, "start": 0, "end": 3}, "label": format!("step {s}")}),
        );
    }
    doc
}

/// R2: export_image runs off the server's async threads, with a deadline:
/// while exports render, `status` and `tools/list` answer; an export past
/// its deadline answers an error and writes nothing.
#[test]
fn r2_a_slow_export_never_holds_up_the_server_and_answers_at_its_deadline() {
    let (path, live) = live_doc("r2", &many_steps(200));
    let dir = path.parent().unwrap().to_path_buf();
    let mut mcp = McpClient::start_with_env(&live.socket, &[("TIZA_MCP_EXPORT_TIMEOUT_MS", "400")]);
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    let gifs: Vec<PathBuf> = (0..cores)
        .map(|i| dir.join(format!("slow-{i}.gif")))
        .collect();
    let ids: Vec<u64> = gifs
        .iter()
        .map(|g| {
            mcp.send(
                "tools/call",
                json!({"name": "export_image", "arguments": {"path": g.to_str().unwrap(), "scale": 2}}),
            )
        })
        .collect();
    for method in ["tools/list", "tools/call"] {
        let params = if method == "tools/list" {
            json!({})
        } else {
            json!({"name": "status", "arguments": {}})
        };
        let id = mcp.send(method, params);
        let v = mcp
            .reply_within(id, Duration::from_secs(5))
            .unwrap_or_else(|| panic!("{method} waited behind the exports"));
        assert!(
            v["error"].is_null() && v["result"]["isError"] != true,
            "{v}"
        );
    }
    for (id, gif) in ids.into_iter().zip(&gifs) {
        let v = mcp
            .reply_within(id, Duration::from_secs(30))
            .expect("the export answers at its deadline");
        let msg = v["error"]["message"].as_str().unwrap_or_default();
        assert!(
            msg.contains("did not finish within 0.4 s") && msg.contains("nothing was written"),
            "{v}"
        );
        assert!(
            !gif.exists(),
            "{} was written after the deadline",
            gif.display()
        );
    }
    // Without the short deadline, the same export answers with the file.
    let mut mcp = McpClient::start(&live.socket);
    let small = dir.join("ok.md");
    let (err, text) = export(
        &mut mcp,
        json!({"path": small.to_str().unwrap(), "step": 7}),
    );
    assert!(!err, "{text}");
}

/// R3: holds and intervals are bounded in the CLI, the MCP and the
/// document; the arithmetic that adds them up saturates.
#[test]
fn r3_holds_and_intervals_are_bounded() {
    let (path, live) = live_doc("r3", &small_doc());
    let dir = path.parent().unwrap().to_path_buf();
    // move.add: a wait past an hour, a typing speed past an hour.
    for params in [
        json!({"at_step": 1, "action": "wait", "ms": u64::MAX}),
        json!({"at_step": 1, "action": "wait", "ms": 3_600_001}),
        json!({"at_step": 1, "action": "code.patch", "op": "replace", "find": "run", "content": "run", "typing": "char", "speed_ms": u64::MAX}),
    ] {
        let (code, msg) = api_err(&live, "move.add", params.clone());
        assert_eq!(code, "invalid_argument", "{params}: {msg}");
        assert!(msg.contains("longest hold"), "{msg}");
    }
    // A step's auto_advance_ms.
    let (_, msg) = api_err(
        &live,
        "scene.patch",
        json!({"steps": [{"step": 1, "auto_advance_ms": u64::MAX}]}),
    );
    assert!(msg.contains("longest hold"), "{msg}");
    // In a .tiza on disk.
    let mut doc = small_doc();
    doc["moves"] = json!([{"at_step": 1, "action": "wait", "ms": u64::MAX}]);
    let far = dir.join("wait.tiza");
    std::fs::write(&far, serde_json::to_string(&doc).unwrap()).unwrap();
    let out = tiza_in(
        &dir,
        &[
            "render",
            far.to_str().unwrap(),
            "--out",
            dir.join("w.tape").to_str().unwrap(),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The CLI's --interval.
    let file = dir.join("doc.tiza");
    for bad in ["1e7", "1e308", "NaN", "-1", "inf"] {
        let out = tiza_in(
            &dir,
            &[
                "render",
                file.to_str().unwrap(),
                &format!("--interval={bad}"),
                "--out",
                dir.join("i.gif").to_str().unwrap(),
            ],
        );
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "--interval {bad}: {err}");
        assert!(err.contains("interval="), "{err}");
    }
    // The MCP's interval: refused at once, never a call that hangs.
    let mut mcp = McpClient::start(&live.socket);
    for bad in [1e7, 1e308, -1.0] {
        let v = mcp
            .request_within(
                "tools/call",
                json!({"name": "export_image", "arguments": {"path": dir.join("m.gif").to_str().unwrap(), "interval": bad}}),
                Duration::from_secs(10),
            )
            .unwrap_or_else(|| panic!("interval {bad} did not answer"));
        assert!(
            v["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("interval="),
            "{v}"
        );
    }
    // The longest hold still records (a GIF and a tape).
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "wait", "ms": 3_600_000}),
    );
    let gif = dir.join("hour.gif");
    ok(
        &dir,
        &[
            "render",
            "--socket",
            sock(&live),
            "--interval",
            "3600",
            "--scale",
            "1",
            "--out",
            gif.to_str().unwrap(),
        ],
    );
    let (err, text) = export(
        &mut mcp,
        json!({"path": dir.join("hour2.gif").to_str().unwrap(), "interval": 3600, "scale": 1}),
    );
    assert!(!err, "{text}");
}

/// R3: the encoders never overflow or loop on a hold, even from a document
/// built in memory past the checks.
#[test]
fn r3_the_arithmetic_of_holds_saturates() {
    let mut doc = tiza_core::Document::from_code("a\nb\n", "sh", None);
    doc.moves = vec![
        serde_json::from_value(json!({"at_step": 1, "action": "wait", "ms": u64::MAX})).unwrap(),
        serde_json::from_value(json!({"at_step": 1, "action": "wait", "ms": u64::MAX})).unwrap(),
    ];
    assert_eq!(doc.wait_ms_at(1), u64::MAX);
    assert_eq!(doc.typing_ms_at(1), u64::MAX);
    let theme = tiza_core::Theme::dark();
    let opts = tiza_core::cast::CastOptions {
        interval: 1e308,
        ..Default::default()
    };
    let _ = tiza_core::cast::to_vhs_tape(&doc, &theme, "x.tiza", "x.gif", &opts);
    let _ = tiza_core::cast::to_asciicast(&doc, &theme, &opts);
    let frames = tiza_core::anim::frames(
        &doc,
        &theme,
        &tiza_core::anim::AnimOptions {
            interval: 1e308,
            width: 40,
            line_numbers: true,
            from_zero: true,
        },
    );
    assert!(
        frames
            .iter()
            .all(|f| f.delay_ms <= 2 * tiza_core::model::MAX_HOLD_MS)
    );
    let canvases: Vec<(tiza_core::canvas::Canvas, u64)> =
        frames.into_iter().map(|f| (f.canvas, u64::MAX)).collect();
    let (_, report) = tiza_export::to_gif(
        &canvases,
        &theme,
        &tiza_export::GifOptions {
            svg: Default::default(),
            png: tiza_export::PngOptions {
                scale: 1.0,
                system_fonts: false,
            },
            repeat: None,
        },
    )
    .unwrap();
    assert!(report.duration_ms <= canvases.len() as u64 * 2 * tiza_core::model::MAX_HOLD_MS);
}

/// R4: a first line near `u32::MAX` is refused (it overflowed and closed
/// the TUI), and `last_line` saturates.
#[test]
fn r4_a_first_line_near_u32_max_is_refused() {
    let (_, live) = live_doc("r4", &small_doc());
    let mut doc = small_doc();
    doc["source"]["first_line"] = json!(4_294_967_294u32);
    let (code, msg) = api_err(&live, "scene.put", json!({"document": doc}));
    assert_eq!(code, "invalid_argument", "{msg}");
    assert!(msg.contains("first_line"), "{msg}");
    // The instance is still there and unchanged.
    let status = live.api_ok("status", json!({}));
    assert_eq!(status["first_line"], 1, "{status}");
    let mut d = tiza_core::Document::from_code("a\nb\nc\n", "sh", None);
    d.source.first_line = u32::MAX - 1;
    assert_eq!(d.last_line(), u32::MAX);
}

/// R4: a panic while the TUI answers a request answers an `internal` error;
/// the TUI keeps running (its screen is not restored), the document is as
/// it was before the request and so is the file it autosaves.
#[test]
fn r4_a_panic_in_a_request_never_closes_the_tui() {
    let dir = scratch("r4-pty");
    let doc = dir.join("doc.tiza");
    std::fs::write(&doc, serde_json::to_string_pretty(&small_doc()).unwrap()).unwrap();
    let sock = dir.join("live.sock");
    let out = tiza_qa::pty_script(
        &dir,
        "panic.py",
        r##"
import json
binary, doc, sock = sys.argv[1], sys.argv[2], sys.argv[3]
env = dict(os.environ, TIZA_PANIC_FOR_TESTS="request:annotation.add")
def api(*args):
    r = subprocess.run([binary, "api", *args, "--socket", sock], capture_output=True, text=True, timeout=20)
    return r.returncode, r.stdout + r.stderr
s = Pty([binary, "--socket", sock, doc], 30, 120, env=env)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 10, "first frame"):
    before = len(s.out)
    code, text = api("annotation.add", '{"type": "note", "line": 4, "text": "added"}')
    print("ADD", code, text.replace("\n", " "), flush=True)
    code, text = api("status")
    print("STATUS", code, json.loads(text)["annotations"] if code == 0 else text, flush=True)
    print("RESTORED" if b"\x1b[?1049l" in s.out[before:] else "SCREEN_KEPT", flush=True)
    print("ALIVE" if s.p.poll() is None else "EXITED", flush=True)
s.kill()
s.finish(5)
"##,
        &[
            tiza_qa::bin().to_str().unwrap(),
            doc.to_str().unwrap(),
            sock.to_str().unwrap(),
        ],
    );
    let add = out
        .lines()
        .find(|l| l.starts_with("ADD"))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(
        add.contains("internal") && add.contains("a bug in Tiza"),
        "{out}"
    );
    assert!(
        !add.contains("TIZA_PANIC_FOR_TESTS"),
        "the panic's text leaked: {out}"
    );
    assert!(
        out.contains("STATUS 0 2"),
        "the annotation must not stay:\n{out}"
    );
    assert!(
        out.contains("SCREEN_KEPT") && out.contains("ALIVE"),
        "{out}"
    );
    let saved = std::fs::read_to_string(&doc).unwrap();
    assert!(!saved.contains("added"), "{saved}");
}

/// R6: a step past the last one exports the last and says so.
#[test]
fn r6_export_image_past_the_last_step_says_so() {
    let (path, live) = live_doc("r6", &small_doc());
    let dir = path.parent().unwrap().to_path_buf();
    let mut mcp = McpClient::start(&live.socket);
    for asked in [99u32, 4_294_967_295] {
        let md = dir.join(format!("p{asked}.md"));
        let (err, text) = export(
            &mut mcp,
            json!({"path": md.to_str().unwrap(), "step": asked, "width": 100}),
        );
        assert!(!err, "{text}");
        let reply: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(reply["step"], 1, "{reply}");
        assert_eq!(
            reply["warning"],
            format!("step {asked} does not exist (the document has steps 0 to 1); exported step 1"),
            "{reply}"
        );
        assert_eq!(
            std::fs::read_to_string(&md).unwrap(),
            render_file(&path, 1, "md")
        );
    }
    let (_, text) = export(
        &mut mcp,
        json!({"path": dir.join("in.md").to_str().unwrap(), "step": 1}),
    );
    assert!(!text.contains("warning"), "{text}");
}

/// R7: the `.tape` of a live instance opens the instance's own `.tiza`.
#[test]
fn r7_a_live_tape_opens_the_instances_document() {
    let (path, live) = live_doc("r7", &small_doc());
    let cwd = elsewhere("r7");
    let tape = cwd.join("demo.tape");
    ok(
        &cwd,
        &[
            "render",
            "--socket",
            sock(&live),
            "--out",
            tape.to_str().unwrap(),
        ],
    );
    let text = std::fs::read_to_string(&tape).unwrap();
    let abs = std::path::absolute(&path).unwrap();
    assert!(text.contains(abs.to_str().unwrap()), "{text}");
    assert!(!text.contains("scene.tiza"), "{text}");
    // An instance with no `.tiza` on disk yet: a clear error, no tape.
    let dir = scratch("r7-unsaved");
    std::fs::write(dir.join("main.sh"), "echo hi\n").unwrap();
    let opened = tiza_core::fs::open(dir.join("main.sh").to_str().unwrap()).unwrap();
    let unsaved = Live::open(opened.document, opened.save_path, dir.join("u.sock"));
    let out = tiza_in(
        &cwd,
        &[
            "render",
            "--socket",
            sock(&unsaved),
            "--out",
            cwd.join("u.tape").to_str().unwrap(),
        ],
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(err.contains("no .tiza on disk yet"), "{err}");
}

/// R8: a real panic inside an MCP call (a test hook) answers an error that
/// never quotes the panic, and the server keeps answering.
#[test]
fn r8_a_panic_in_an_mcp_call_answers_an_error() {
    let (path, live) = live_doc("r8", &small_doc());
    let dir = path.parent().unwrap().to_path_buf();
    for (place, tool, args) in [
        ("tool:status", "status", json!({})),
        (
            "export:render",
            "export_image",
            json!({"path": dir.join("x.md").to_str().unwrap()}),
        ),
    ] {
        let mut mcp =
            McpClient::start_with_env(&live.socket, &[("TIZA_MCP_PANIC_FOR_TESTS", place)]);
        let v = mcp
            .request_within(
                "tools/call",
                json!({"name": tool, "arguments": args}),
                Duration::from_secs(30),
            )
            .unwrap_or_else(|| panic!("{place}: the call never answered"));
        let msg = v["error"]["message"].as_str().unwrap_or_default();
        assert!(
            msg.contains(&format!("tool {tool} call failed with a bug in Tiza")),
            "{v}"
        );
        assert!(!msg.contains("TIZA_MCP_PANIC_FOR_TESTS"), "{v}");
        let v = mcp.request(
            "tools/call",
            json!({"name": "list_annotations", "arguments": {}}),
        );
        assert!(v["error"].is_null(), "the server must keep answering: {v}");
    }
}
