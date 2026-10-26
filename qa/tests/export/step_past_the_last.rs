//! `tiza render --step N` with N past the last step said «showing step L»
//! but laid out step N: a spotlight of step L was off there, so the take
//! was not the last step's. `:export step=N` did the same and named step N.
//! A `--step` of 4294967295 was taken as no `--step` at all, with no
//! warning.
//!
//! Acceptance criteria:
//! - AC1 `--step N` past the last step renders exactly what `--step L`
//!   renders (byte-identical in txt, ansi, md, svg, png and webp), as the message
//!   says; the same for `:export step=N` and MCP `export_image`.
//! - AC2 This file; CHANGELOG (Fixed).
//!
//! Corner cases: `--all-steps` (never past the last); a huge `--step`
//! (99999, 4294967295); the live instance; `:export` with no `step=` on the
//! new empty step after the last one (the step on screen, no warning); a
//! recording (`.cast`, `.gif`…) shows every step and gives no warning.

use serde_json::json;
use tiza_qa::{Live, McpClient, fixture, scratch};

const LAST: u32 = 2;
const PAST: [u32; 3] = [3, 99999, u32::MAX];

fn doc_path() -> String {
    fixture("export/step_past_the_last/spotlight.tiza")
        .to_str()
        .unwrap()
        .to_string()
}

/// `tiza render` of the fixture at `step`, in `format`, to a file: its
/// bytes and what it said on stderr.
fn rendered(dir: &std::path::Path, step: u32, format: &str) -> (Vec<u8>, String) {
    let out = dir.join(format!("s{step}.{format}"));
    let r = tiza_qa::tiza(&[
        "render",
        &doc_path(),
        "--step",
        &step.to_string(),
        "--width",
        "80",
        "--format",
        format,
        "--out",
        out.to_str().unwrap(),
    ]);
    assert!(r.status.success(), "{step} {format}: {r:?}");
    (
        std::fs::read(&out).unwrap(),
        String::from_utf8_lossy(&r.stderr).into_owned(),
    )
}

#[test]
fn render_past_the_last_step_is_the_last_step() {
    let dir = scratch("render");
    for format in ["txt", "ansi", "md", "svg", "png", "webp"] {
        let (last, said) = rendered(&dir, LAST, format);
        assert!(!said.contains("does not exist"), "{format}: {said}");
        for step in PAST {
            let (bytes, said) = rendered(&dir, step, format);
            assert!(bytes == last, "{format}: step {step} is not step {LAST}");
            assert!(
                said.contains(&format!(
                    "step {step} does not exist (the document has steps 0 to {LAST}); showing step {LAST}"
                )),
                "{format} {step}: {said}"
            );
        }
    }
    // The fixture tells the two takes apart: without `--step` a file shows
    // every step's annotations with the spotlight of step 2 off.
    let all = tiza_qa::tiza(&[
        "render",
        &doc_path(),
        "--width",
        "80",
        "--format",
        "ansi",
        "--out",
        "-",
    ]);
    let (last, _) = rendered(&dir, LAST, "ansi");
    assert_ne!(all.stdout, last, "the spotlight changes nothing here");
}

#[test]
fn render_of_the_live_instance_past_the_last_step() {
    let dir = scratch("live");
    let doc = tiza_core::fs::load_document(std::path::Path::new(&doc_path())).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    let sock = live.socket.to_str().unwrap().to_string();
    let at = |step: u32| {
        tiza_qa::tiza(&[
            "--socket",
            &sock,
            "render",
            "--step",
            &step.to_string(),
            "--width",
            "80",
            "--format",
            "ansi",
            "--out",
            "-",
        ])
    };
    let last = at(LAST);
    for step in [3, u32::MAX] {
        let r = at(step);
        assert!(r.stdout == last.stdout, "step {step}");
        assert!(
            String::from_utf8_lossy(&r.stderr).contains("showing step 2"),
            "{r:?}"
        );
    }
}

#[test]
fn export_without_a_step_is_the_step_on_screen() {
    // `:step 3` opens a new empty step after the last one: `:export` with
    // no `step=` writes what the screen shows, with no warning.
    let dir = scratch("new-step");
    let doc = tiza_core::fs::load_document(std::path::Path::new(&doc_path())).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.command("step 3");
    assert_eq!(live.with(|a| a.doc.step), 3);
    live.command(&format!("export {}", dir.join("new.txt").display()));
    let m = live.message().unwrap_or_default().0;
    assert!(m.starts_with("exported step 3 to"), "{m}");
}

#[test]
fn a_recording_takes_no_step_and_gives_no_warning() {
    let dir = scratch("cast");
    let r = tiza_qa::tiza(&[
        "render",
        &doc_path(),
        "--step",
        "99",
        "--out",
        dir.join("all.cast").to_str().unwrap(),
    ]);
    assert!(r.status.success(), "{r:?}");
    let said = String::from_utf8_lossy(&r.stderr);
    assert!(!said.contains("does not exist"), "{said}");
}

#[test]
fn export_past_the_last_step_is_the_last_step() {
    let dir = scratch("export");
    let doc = tiza_core::fs::load_document(std::path::Path::new(&doc_path())).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.with(|a| a.size = (82, 30));
    for ext in ["txt", "ansi", "md"] {
        let last = dir.join(format!("last.{ext}"));
        live.command(&format!("export {} step={LAST}", last.display()));
        let m = live.message().unwrap_or_default().0;
        assert!(m.starts_with(&format!("exported step {LAST} to")), "{m}");
        for step in [3, 99999] {
            let past = dir.join(format!("past{step}.{ext}"));
            live.command(&format!("export {} step={step}", past.display()));
            let m = live.message().unwrap_or_default().0;
            assert!(
                m.contains(&format!(
                    "step {step} does not exist (the document has steps 0 to {LAST}); exported step {LAST}"
                )),
                "{ext} {step}: {m}"
            );
            assert!(
                std::fs::read(&past).unwrap() == std::fs::read(&last).unwrap(),
                "{ext}: step {step} is not step {LAST}"
            );
        }
    }
}

#[test]
fn mcp_export_image_past_the_last_step_is_the_last_step() {
    let dir = scratch("mcp");
    let doc = tiza_core::fs::load_document(std::path::Path::new(&doc_path())).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    let mut mcp = McpClient::start(&live.socket);
    // A .webp with `step` is a still of that step, as a .png.
    for ext in ["txt", "svg", "webp"] {
        let last = dir.join(format!("last.{ext}"));
        let (err, _, v) = mcp.call(
            "export_image",
            json!({"path": last.to_str().unwrap(), "step": LAST, "width": 80}),
        );
        assert!(!err, "{v}");
        let past = dir.join(format!("past.{ext}"));
        let (err, text, v) = mcp.call(
            "export_image",
            json!({"path": past.to_str().unwrap(), "step": 7, "width": 80}),
        );
        assert!(!err, "{v}");
        assert!(text.contains("exported step 2"), "{text}");
        assert!(
            std::fs::read(&past).unwrap() == std::fs::read(&last).unwrap(),
            "{ext}"
        );
    }
}

#[test]
fn all_steps_never_goes_past_the_last() {
    let dir = scratch("all");
    let out = dir.join("steps");
    let r = tiza_qa::tiza(&[
        "render",
        &doc_path(),
        "--all-steps",
        "--format",
        "txt",
        "--width",
        "80",
        "--out",
        &format!("{}/", out.display()),
    ]);
    assert!(r.status.success(), "{r:?}");
    let mut names: Vec<String> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["step-00.txt", "step-01.txt", "step-02.txt"]);
    let (last, _) = rendered(&dir, LAST, "txt");
    assert_eq!(std::fs::read(out.join("step-02.txt")).unwrap(), last);
}
