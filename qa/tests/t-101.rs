//! t-101 — Preflight: `tiza check FILE` validates anchors, steps, moves,
//! views and redacts; `render --all-steps` writes one image per step.
//!
//! Acceptance criteria:
//! - AC1 `check` exits 0 on every bundled example and 1 with a clear list
//!   on a broken file; each finding names the annotation or move id, the
//!   step, the file (for linked views) and the line.
//! - AC2 Errors: an anchor or `find` that matches nothing or more than
//!   once; a move target that does not match exactly once at its step; a
//!   linked file that cannot be read; an unknown or duplicate id; a schema
//!   violation.
//! - AC3 Warnings: a redact that widens or drops through a move or a
//!   reload (the same text the TUI shows); an annotation hidden as
//!   `(unanchored)` at some step; a label clipped with «…»; a step with
//!   nothing new. `--strict` makes warnings fail.
//! - AC4 `--json` prints the findings as JSON; the MCP gets a `check` tool.
//! - AC5 `render --all-steps --out dir/` produces N+1 files (step 0
//!   included), all the same pixel size, and refuses `--out -`.
//! - AC6 (Luis, 2026-10-18) `check` warns for every note or arrow label
//!   clipped with «…» at the default width (100, as `render`) or at
//!   `--width N`, at any step, naming the step, the id and the full label
//!   text, and ends with the narrowest width from which nothing is clipped:
//!   on examples/ownership.rs.tiza at 100, «move: s moves into takes» and
//!   «read: shared borrow», and 108. A label under a redact is reported
//!   without its text.
//! - AC7 Docs: README, agent guide, CHANGELOG; this file.
//!
//!
//! Rework (2026-10-18, QA 2): the MCP `check` runs inside the instance on
//! the real code (it checked the masked copy: false span errors on every
//! document with a redact, and a label under a mask quoted), compared here
//! with `tiza check --json`; a marker that matches more than once is a
//! warning with its lines; a marker under a mask answers like one that
//! does not exist; `--all-steps` removes its own stale `step-NN` files and
//! honours `--skip-zero`; the narrowest-width line says when a narrower
//! width is clean too.
//!
//! Corner cases: zero annotations → one image (step 0); `--all-steps` with
//! `--out -` → error; `check` writes nothing and needs no terminal; a
//! finding never quotes a secret; a missing linked file → one error per
//! view, not one per annotation.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use tiza_core::fs;
use tiza_qa::{Live, McpClient, bin, example, fixture, scratch, tiza};

const SECRET: &str = "abc123secret";

/// `tiza check <args>` with stdin closed (no terminal): (exit code, stdout, stderr).
fn check(args: &[&str]) -> (i32, String, String) {
    let o = Command::new(bin())
        .arg("check")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run tiza check");
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

/// `tiza check --json <file>` parsed.
fn check_json(file: &Path, extra: &[&str]) -> (i32, Value) {
    let mut args = vec![file.to_str().unwrap(), "--json"];
    args.extend_from_slice(extra);
    let (code, out, err) = check(&args);
    let v: Value = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}\n{err}"));
    (code, v)
}

fn findings(v: &Value) -> Vec<Value> {
    v["findings"].as_array().unwrap().clone()
}

fn with_code(v: &[Value], code: &str) -> Vec<Value> {
    v.iter().filter(|f| f["code"] == code).cloned().collect()
}

/// Every 6-character piece of the secret that holds a digit.
#[track_caller]
fn assert_no_secret(what: &str, s: &str) {
    let c: Vec<char> = SECRET.chars().collect();
    for i in 0..=c.len() - 6 {
        let p: String = c[i..i + 6].iter().collect();
        if p.chars().any(|ch| ch.is_ascii_digit()) {
            assert!(!s.contains(&p), "{what} shows «{p}» of the secret:\n{s}");
        }
    }
}

/// A copy of a bundled example (and the file it references) in a scratch
/// directory, edited by `edit`.
fn edited_example(name: &str, file: &str, edit: impl Fn(String) -> String) -> PathBuf {
    let dir = scratch(name);
    std::fs::copy(example("ownership.rs"), dir.join("ownership.rs")).unwrap();
    let text = std::fs::read_to_string(example(file)).unwrap();
    let path = dir.join(file);
    std::fs::write(&path, edit(text)).unwrap();
    path
}

fn listing(dir: &Path) -> Vec<(String, u64)> {
    let mut v: Vec<(String, u64)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                e.metadata().unwrap().len(),
            )
        })
        .collect();
    v.sort();
    v
}

/// Width and height from a PNG header.
fn png_size(path: &Path) -> (u32, u32) {
    let b = std::fs::read(path).unwrap();
    (
        u32::from_be_bytes([b[16], b[17], b[18], b[19]]),
        u32::from_be_bytes([b[20], b[21], b[22], b[23]]),
    )
}

#[test]
fn ac1_step1_every_bundled_example_passes() {
    let mut n = 0;
    for e in std::fs::read_dir(example("")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().and_then(|x| x.to_str()) != Some("tiza") {
            continue;
        }
        n += 1;
        let (code, out, err) = check(&[p.to_str().unwrap()]);
        assert_eq!(code, 0, "{}:\n{out}{err}", p.display());
        assert!(out.contains(": 0 errors,"), "{out}");
        let (code, v) = check_json(&p, &[]);
        assert_eq!(code, 0);
        assert_eq!(v["ok"], true);
        assert_eq!(v["errors"], 0, "{v}");
    }
    assert!(n >= 4, "the bundled examples were found");
}

#[test]
fn ac1_step2_a_wrong_find_is_listed_with_its_move_and_step() {
    let path = edited_example("t101-find", "ownership-moves.rs.tiza", |t| {
        t.replace(
            r#""find": "let n = len(&t);""#,
            r#""find": "let n = nope;""#,
        )
    });
    let (code, out, _) = check(&[path.to_str().unwrap()]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(
            "error: move #3, step 4: find «let n = nope;» matches nothing in the code at this step"
        ),
        "{out}"
    );
    assert!(out.contains("1 error,"), "{out}");
}

#[test]
fn ac2_step4_a_move_target_that_matches_twice() {
    let ok = example("ownership-moves.rs.tiza");
    assert_eq!(check(&[ok.to_str().unwrap()]).0, 0);
    let path = edited_example("t101-twice", "ownership-moves.rs.tiza", |t| {
        t.replace(r#""find": "let n = len(&t);""#, r#""find": "println""#)
    });
    let (code, v) = check_json(&path, &[]);
    assert_eq!(code, 1);
    let f = findings(&v);
    let moves = with_code(&f, "move");
    assert_eq!(moves.len(), 1, "{v}");
    assert_eq!(moves[0]["level"], "error");
    assert_eq!(moves[0]["id"], "move #3");
    assert_eq!(moves[0]["step"], 4);
    assert_eq!(moves[0]["line"], 4);
    let msg = moves[0]["message"].as_str().unwrap();
    assert!(
        msg.contains("matches 3 times (L4, L8, L12)") && msg.contains("exactly once"),
        "{msg}"
    );
}

#[test]
fn ac2_every_error_of_a_broken_file_at_once() {
    let (code, v) = check_json(&fixture("t-101/broken.tiza"), &[]);
    assert_eq!(code, 1);
    let f = findings(&v);
    let get = |code: &str, id: &str| -> Value {
        f.iter()
            .find(|x| x["code"] == code && x["id"] == id)
            .unwrap_or_else(|| panic!("no {code} for {id}: {v}"))
            .clone()
    };
    // An anchor past the end of the code.
    let a1 = get("anchor", "a1");
    assert_eq!(
        (a1["step"].clone(), a1["line"].clone()),
        (json!(1), json!(99))
    );
    // A duplicate id, an unknown one in a spotlight and in a rebind.
    assert_eq!(get("duplicate_id", "a2")["level"], "error");
    assert!(
        get("unknown_id", "s1")["message"]
            .as_str()
            .unwrap()
            .contains("nobody")
    );
    assert!(
        get("unknown_id", "move #3")["message"]
            .as_str()
            .unwrap()
            .contains("`zz`")
    );
    // A move target that matches twice, one that matches nothing: both.
    assert!(
        get("move", "move #1")["message"]
            .as_str()
            .unwrap()
            .contains("matches 2 times")
    );
    assert!(
        get("move", "move #2")["message"]
            .as_str()
            .unwrap()
            .contains("matches nothing")
    );
    // A marker that matches nothing.
    assert!(
        get("marker", "b1")["message"]
            .as_str()
            .unwrap()
            .contains("«no such end»")
    );
    // A linked file that cannot be read: ONE error for the view, naming
    // both moves that link it, and nothing about its annotations (g1, g2)
    // or the move that edits it (#5).
    let links = with_code(&f, "link");
    assert_eq!(links.len(), 1, "{v}");
    assert_eq!(links[0]["file"], "gone.rs");
    assert_eq!(links[0]["id"], "move #4");
    let msg = links[0]["message"].as_str().unwrap();
    assert!(msg.contains("#4 (step 4), #6 (step 5)"), "{msg}");
    for id in ["g1", "g2", "move #5"] {
        assert!(!f.iter().any(|x| x["id"] == id), "{id}: {v}");
    }
    // The broken annotation is not reported again as hidden.
    assert!(with_code(&f, "unanchored").is_empty(), "{v}");
    assert_eq!(v["errors"], 8, "{v}");
    // The text report: one line per finding, then the summary.
    let (_, out, _) = check(&[fixture("t-101/broken.tiza").to_str().unwrap()]);
    assert!(
        out.contains("error: a1, step 1, L99: line 99 does not exist"),
        "{out}"
    );
    assert!(out.contains("8 errors, 0 warnings"), "{out}");
}

#[test]
fn ac2_schema_violations_and_unreadable_code() {
    let dir = scratch("t101-schema");
    let bad = dir.join("bad.tiza");
    std::fs::write(
        &bad,
        "{\n  \"schema\": 1,\n  \"source\": {\"language\": \"rs\"},\n  \"code\": [\"x\"],\n  \"step\": \"two\"\n}\n",
    )
    .unwrap();
    let (code, v) = check_json(&bad, &[]);
    assert_eq!(code, 1);
    let f = findings(&v);
    assert_eq!(f.len(), 1, "{v}");
    assert_eq!(f[0]["code"], "schema");
    assert_eq!(f[0]["line"], 5, "{v}");
    // Not JSON at all.
    let junk = dir.join("junk.tiza");
    std::fs::write(&junk, "{ not json").unwrap();
    let (code, v) = check_json(&junk, &[]);
    assert_eq!(code, 1);
    assert_eq!(findings(&v)[0]["code"], "schema");
    // A code reference to a file that is not there.
    let gone = dir.join("gone.tiza");
    std::fs::write(
        &gone,
        r#"{"schema": 1, "source": {"language": "rs"}, "code": {"file": "nowhere.rs"}}"#,
    )
    .unwrap();
    let (code, v) = check_json(&gone, &[]);
    assert_eq!(code, 1);
    assert_eq!(findings(&v)[0]["code"], "code");
}

/// The t-140 document: a redact on a line a move rewrites, saved.
fn widened_redact(name: &str) -> PathBuf {
    let dir = scratch(name);
    let src = dir.join("config.json");
    std::fs::write(
        &src,
        format!("{{\"token\": \"{SECRET}\", \"user\": \"ana\"}}\n"),
    )
    .unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("config.json.tiza"), dir.join("live.sock"));
    live.command("redact 1@11..23");
    assert!(!live.message().unwrap().1);
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "replace", "position": "1",
               "content": format!("{{\"apiToken\": \"{SECRET}\", \"user\": \"ana\"}}")}),
    );
    live.command("w");
    dir.join("config.json.tiza")
}

#[test]
fn ac3_step5_a_redact_widened_by_a_move_warns_and_strict_fails() {
    let doc = widened_redact("t101-redact");
    let (code, out, err) = check(&[doc.to_str().unwrap()]);
    assert_eq!(code, 0, "warnings do not fail:\n{out}{err}");
    assert!(
        out.contains(
            "warning: a1, step 1: redact a1 could not follow its text through move #1: masking the rest of L1 — fix it before recording"
        ),
        "the TUI's own text:\n{out}"
    );
    assert_no_secret("check", &(out.clone() + &err));
    let (code, out, _) = check(&[doc.to_str().unwrap(), "--strict"]);
    assert_eq!(code, 1, "{out}");
    let (_, v) = check_json(&doc, &[]);
    assert_no_secret("check --json", &v.to_string());
    assert_eq!(with_code(&findings(&v), "redact").len(), 1, "{v}");
    // The source file finds its sidecar.
    let src = doc.parent().unwrap().join("config.json");
    let (code, out, _) = check(&[src.to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(out.contains("could not follow its text"), "{out}");
}

#[test]
fn ac3_a_redact_saved_widened_by_a_reload_warns() {
    let dir = scratch("t101-reload");
    let path = dir.join("doc.tiza");
    std::fs::write(
        &path,
        r#"{"schema": 1, "source": {"language": "ini"}, "code": ["token=••••••••"],
            "annotations": [{"id": "r1", "step": 1, "type": "redact",
                             "span": {"line": 1, "start": 6, "end": 14}, "widened": true}]}"#,
    )
    .unwrap();
    let (code, v) = check_json(&path, &[]);
    assert_eq!(code, 0);
    let r = with_code(&findings(&v), "redact");
    assert_eq!(r.len(), 1, "{v}");
    assert_eq!(r[0]["id"], "r1");
    assert!(r[0]["message"].as_str().unwrap().contains("WIDENED"));
}

#[test]
fn ac3_an_annotation_a_move_hides_warns_with_its_steps() {
    let (code, v) = check_json(&example("ownership-moves.rs.tiza"), &[]);
    assert_eq!(code, 0);
    let hidden = with_code(&findings(&v), "unanchored")
        .into_iter()
        .map(|f| (f["id"].as_str().unwrap().to_string(), f["step"].clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        hidden,
        vec![("a5".to_string(), json!(4)), ("a6".to_string(), json!(4))],
        "{v}"
    );
}

#[test]
fn ac3_a_step_with_nothing_new_warns() {
    let (code, v) = check_json(&fixture("t-101/empty-step.tiza"), &[]);
    assert_eq!(code, 0);
    let empty = with_code(&findings(&v), "empty_step");
    assert_eq!(empty.len(), 1, "{v}");
    assert_eq!(empty[0]["step"], 2);
}

#[test]
fn ac6_step6_clipped_labels_at_the_default_width_and_the_narrowest_clean_one() {
    let file = example("ownership.rs.tiza");
    let file = file.to_str().unwrap();
    // Plain `check`: the default width (100, as `render`).
    let (code, plain, _) = check(&[file]);
    assert_eq!(code, 0, "{plain}");
    let (_, at100, _) = check(&[file, "--width", "100"]);
    assert_eq!(plain, at100);
    assert!(
        plain.contains(
            "warning: a1, step 3, L3: label «move: s moves into takes» is clipped with «…» at width 100"
        ),
        "{plain}"
    );
    assert!(
        plain.contains(
            "warning: a6, step 4, L7: label «read: shared borrow» is clipped with «…» at width 100"
        ),
        "{plain}"
    );
    assert_eq!(plain.matches("is clipped with").count(), 2, "{plain}");
    assert!(
        plain
            .trim_end()
            .ends_with("no label is clipped at 108 or wider (`tiza render --width 108`)"),
        "{plain}"
    );
    // The step a clipped label names is the first one it is clipped at.
    assert!(
        plain.contains("at width 100, first at step 3, then at steps 5-7"),
        "{plain}"
    );
    let (_, v) = check_json(&example("ownership.rs.tiza"), &["--width", "100"]);
    assert_eq!(v["width"], 100);
    assert_eq!(v["min_clean_width"], 108);
    let clipped = with_code(&findings(&v), "clipped_label");
    let texts: Vec<(&str, &str)> = clipped
        .iter()
        .map(|f| (f["id"].as_str().unwrap(), f["text"].as_str().unwrap()))
        .collect();
    assert_eq!(
        texts,
        vec![
            ("a1", "move: s moves into takes"),
            ("a6", "read: shared borrow")
        ]
    );
    // At 108 nothing is clipped, and the renders agree: no «…» at any step.
    let (_, at108, _) = check(&[file, "--width", "108"]);
    assert!(!at108.contains("is clipped with"), "{at108}");
    for s in 0..=7 {
        let step = s.to_string();
        let r = tiza(&[
            "render", file, "--step", &step, "--width", "108", "--out", "-",
        ]);
        assert!(
            !String::from_utf8_lossy(&r.stdout).contains('…'),
            "step {s}"
        );
    }
    let r = tiza(&[
        "render", file, "--step", "7", "--width", "107", "--out", "-",
    ]);
    assert!(String::from_utf8_lossy(&r.stdout).contains('…'));
    // A width out of bounds is an error, as for `render`.
    let (code, _, err) = check(&[file, "--width", "5"]);
    assert_eq!(code, 1);
    assert!(err.contains("width=5"), "{err}");
}

#[test]
fn ac6_a_clipped_label_under_a_redact_is_named_without_its_text() {
    let dir = scratch("t101-secret-label");
    let path = dir.join("doc.tiza");
    let label = "the key the service gave us, never on video";
    std::fs::write(
        &path,
        json!({
            "schema": 1,
            "source": {"language": "rs"},
            "code": [format!("let key = \"{SECRET}\";"), "call(key);"],
            "annotations": [
                {"id": "r1", "step": 1, "type": "redact",
                 "span": {"line": 1, "start": 11, "end": 23}, "label": label},
                {"id": "m1", "step": 1, "type": "highlight",
                 "span": {"line": 1, "start": 11, "end": 23}, "label": label},
                {"id": "n1", "step": 1, "type": "note", "line": 2, "text": label}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let (_, v) = check_json(&path, &["--width", "60"]);
    let clipped = with_code(&findings(&v), "clipped_label");
    let ids: Vec<&str> = clipped.iter().map(|f| f["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"r1") && ids.contains(&"n1"), "{v}");
    for f in &clipped {
        let id = f["id"].as_str().unwrap();
        if id == "n1" {
            assert_eq!(f["text"], label);
        } else {
            assert!(f.get("text").is_none(), "{f}");
            assert!(!f["message"].as_str().unwrap().contains(label), "{f}");
            assert!(f["message"].as_str().unwrap().contains("under a redact"));
        }
    }
    assert_no_secret("check --json", &v.to_string());
    let (_, out, _) = check(&[path.to_str().unwrap(), "--width", "60"]);
    assert_no_secret("check", &out);
}

#[test]
fn ac4_step7_json_holds_the_same_findings_as_the_text() {
    let file = fixture("t-101/broken.tiza");
    let (_, text, _) = check(&[file.to_str().unwrap()]);
    let (code, v) = check_json(&file, &[]);
    assert_eq!(code, 1);
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("error: ") || l.starts_with("warning: "))
        .collect();
    assert_eq!(lines.len(), findings(&v).len(), "{text}\n{v}");
    for (l, f) in lines.iter().zip(findings(&v)) {
        assert!(l.ends_with(f["message"].as_str().unwrap()), "{l} / {f}");
        assert!(l.starts_with(f["level"].as_str().unwrap()));
    }
    assert_eq!(v["errors"], 8);
    assert_eq!(v["ok"], false);
}

#[test]
fn ac4_the_mcp_check_tool_checks_the_live_document() {
    let dir = scratch("t101-mcp");
    let doc = fs::open(example("ownership.rs.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    let mut mcp = McpClient::start(&live.socket);
    assert!(mcp.tool_names().contains(&"check".to_string()));
    let (err, out, _) = mcp.call("check", json!({}));
    assert!(!err, "{out}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["ok"], true);
    assert_eq!(v["width"], 100);
    assert_eq!(v["min_clean_width"], 108);
    assert_eq!(with_code(&findings(&v), "clipped_label").len(), 2, "{v}");
    let (err, out, _) = mcp.call("check", json!({"width": 108}));
    assert!(!err, "{out}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert!(with_code(&findings(&v), "clipped_label").is_empty(), "{v}");
    let (err, out, _) = mcp.call("check", json!({"width": 3}));
    assert!(err && out.contains("width=3"), "{out}");
}

#[test]
fn ac5_step3_render_all_steps_writes_one_png_per_step_of_one_size() {
    let dir = scratch("t101-steps");
    let out = dir.join("steps");
    let file = example("ownership.rs.tiza");
    let r = tiza(&[
        "render",
        file.to_str().unwrap(),
        "--all-steps",
        "--out",
        &format!("{}/", out.display()),
        "--scale",
        "1",
    ]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let names: Vec<String> = listing(&out).into_iter().map(|(n, _)| n).collect();
    let want: Vec<String> = (0..=7).map(|s| format!("step-{s:02}.png")).collect();
    assert_eq!(names, want);
    let sizes: Vec<(u32, u32)> = want.iter().map(|n| png_size(&out.join(n))).collect();
    assert!(sizes.iter().all(|s| *s == sizes[0]), "{sizes:?}");
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(
        err.contains("wrote 8 files")
            && err.contains(&format!("{}×{} px each", sizes[0].0, sizes[0].1)),
        "{err}"
    );
    // Step 7 is the same picture `render --step 7` draws.
    let single = dir.join("seven.png");
    tiza(&[
        "render",
        file.to_str().unwrap(),
        "--step",
        "7",
        "--scale",
        "1",
        "--out",
        single.to_str().unwrap(),
    ]);
    assert_eq!(png_size(&single), sizes[7]);
    // Text formats too (plain text drops trailing blank rows, so only the
    // images are padded to one size).
    let txt = dir.join("txt");
    let r = tiza(&[
        "render",
        file.to_str().unwrap(),
        "--all-steps",
        "--format",
        "txt",
        "--out",
        txt.to_str().unwrap(),
    ]);
    assert!(r.status.success());
    assert_eq!(listing(&txt).len(), 8);
    let seven = std::fs::read_to_string(txt.join("step-07.txt")).unwrap();
    let alone = tiza(&[
        "render",
        file.to_str().unwrap(),
        "--step",
        "7",
        "--out",
        "-",
    ]);
    assert_eq!(seven, String::from_utf8_lossy(&alone.stdout));
}

#[test]
fn corner_all_steps_of_a_document_without_annotations_and_to_stdout() {
    let dir = scratch("t101-zero");
    let r = tiza(&[
        "render",
        fixture("t-101/zero.tiza").to_str().unwrap(),
        "--all-steps",
        "--format",
        "svg",
        "--out",
        dir.join("out").to_str().unwrap(),
    ]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(listing(&dir.join("out")).len(), 1);
    assert!(dir.join("out/step-00.svg").exists());
    // `--out -` (the default) is refused: one file per step needs a directory.
    let file = example("ownership.rs.tiza");
    for args in [
        vec!["render", file.to_str().unwrap(), "--all-steps"],
        vec![
            "render",
            file.to_str().unwrap(),
            "--all-steps",
            "--out",
            "-",
        ],
    ] {
        let r = tiza(&args);
        assert!(!r.status.success());
        assert!(
            String::from_utf8_lossy(&r.stderr).contains("pass a directory"),
            "{}",
            String::from_utf8_lossy(&r.stderr)
        );
        assert!(r.stdout.is_empty());
    }
    // A recording format already holds every step.
    let r = tiza(&[
        "render",
        file.to_str().unwrap(),
        "--all-steps",
        "--format",
        "gif",
        "--out",
        dir.join("g").to_str().unwrap(),
    ]);
    assert!(!r.status.success());
    // `--step` and `--all-steps` do not go together.
    let r = tiza(&[
        "render",
        file.to_str().unwrap(),
        "--all-steps",
        "--step",
        "2",
        "--out",
        dir.join("x").to_str().unwrap(),
    ]);
    assert!(!r.status.success());
}

#[test]
fn corner_check_writes_nothing_and_needs_no_terminal() {
    let path = edited_example("t101-nowrite", "ownership.rs.tiza", |t| t);
    let dir = path.parent().unwrap().to_path_buf();
    let before = listing(&dir);
    let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    let src = dir.join("ownership.rs");
    for args in [
        vec![path.to_str().unwrap()],
        vec![path.to_str().unwrap(), "--json", "--strict"],
        vec![src.to_str().unwrap()],
    ] {
        let (code, out, err) = check(&args);
        assert!(code == 0 || args.contains(&"--strict"), "{out}{err}");
        assert!(!out.contains('\x1b'), "plain text, no terminal codes");
    }
    assert_eq!(listing(&dir), before, "no sidecar, no autosave");
    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), mtime);
    // A source file with no sidecar has nothing to check.
    std::fs::write(dir.join("lone.rs"), "fn main() {}\n").unwrap();
    let (code, _, err) = check(&[dir.join("lone.rs").to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(err.contains("nothing to check"), "{err}");
    assert!(!dir.join("lone.rs.tiza").exists());
    // stdin works too.
    let o = Command::new(bin())
        .args(["check", "-", "--json"])
        .stdin(std::fs::File::open(fixture("t-101/zero.tiza")).unwrap())
        .output()
        .unwrap();
    assert!(o.status.success());
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["max_step"], 0);
}

/// The secret of the rework fixtures (QA 2's `w-*.tiza`).
const W_SECRET: &str = "abc123SECRETxyz789";

/// The report of the MCP `check` of `file` opened in a live instance.
fn mcp_check(name: &str, file: &Path, args: Value) -> Value {
    let dir = scratch(name);
    let doc = fs::open(file.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    let mut mcp = McpClient::start(&live.socket);
    let (err, out, _) = mcp.call("check", args);
    assert!(!err, "{out}");
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"))
}

#[test]
fn rework1_mcp_check_is_the_cli_check_on_documents_with_redacts() {
    for (name, args, cli) in [
        ("w-redact-only", json!({}), vec![]),
        ("w-label-under", json!({}), vec![]),
        ("w-label-under", json!({"width": 60}), vec!["--width", "60"]),
        ("markers", json!({}), vec![]),
    ] {
        let file = fixture(&format!("t-101/{name}.tiza"));
        let (_, want) = check_json(&file, &cli);
        let got = mcp_check(&format!("t101-mcp-{name}"), &file, args);
        assert_eq!(got, want, "{name} {cli:?}: MCP\n{got}\nCLI\n{want}");
        assert_no(W_SECRET, &format!("MCP check of {name}"), &got.to_string());
    }
    // QA 2's case: one redact, nothing else, is clean (it gave
    // `range 6..24 does not fit on line 2 (it has 14 graphemes)`).
    let v = mcp_check(
        "t101-mcp-only",
        &fixture("t-101/w-redact-only.tiza"),
        json!({}),
    );
    assert_eq!(
        (v["ok"].clone(), v["errors"].clone()),
        (json!(true), json!(0)),
        "{v}"
    );
    // A label under a redact is named without its text, as the CLI does.
    let v = mcp_check(
        "t101-mcp-under",
        &fixture("t-101/w-label-under.tiza"),
        json!({"width": 60}),
    );
    let label = "the key the payments service handed us last week";
    assert!(!v.to_string().contains(label), "{v}");
    let under: Vec<Value> = with_code(&findings(&v), "clipped_label")
        .into_iter()
        .filter(|f| f["id"] == "r1" || f["id"] == "h1")
        .collect();
    assert_eq!(under.len(), 2, "{v}");
    for f in under {
        assert!(f.get("text").is_none(), "{f}");
        assert!(
            f["message"].as_str().unwrap().contains("under a redact"),
            "{f}"
        );
    }
}

/// Every 6-character piece of `value` that holds a digit is absent.
#[track_caller]
fn assert_no(value: &str, what: &str, s: &str) {
    let c: Vec<char> = value.chars().collect();
    for i in 0..=c.len() - 6 {
        let p: String = c[i..i + 6].iter().collect();
        if p.chars().any(|ch| ch.is_ascii_digit()) {
            assert!(!s.contains(&p), "{what} shows «{p}» of the secret:\n{s}");
        }
    }
}

#[test]
fn rework1_the_live_check_sees_unsaved_changes_and_linked_files() {
    // The instance checks what it holds, not the file on disk.
    let dir = scratch("t101-mcp-live");
    let doc = fs::open(fixture("t-101/w-redact-only.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.api_ok(
        "annotation.add",
        json!({"id": "n9", "step": 2, "type": "note", "line": 3, "text": "eu only"}),
    );
    let v = live.api_ok("check", json!({}));
    assert_eq!(v["max_step"], 2, "{v}");
    assert_eq!(v["ok"], true, "{v}");
    let r = live.api("check", json!({"width": 3}));
    assert!(r.error.is_some_and(|e| e.message.contains("width=3")));
    // A linked file: the message names it.
    let (_, cli) = check_json(&fixture("t-101/linked.tiza"), &[]);
    let got = mcp_check("t101-mcp-linked", &fixture("t-101/linked.tiza"), json!({}));
    assert_eq!(got, cli);
    let (_, out, _) = check(&[fixture("t-101/linked.tiza").to_str().unwrap()]);
    assert!(
        out.contains(
            "error: q1, step 2, src/queue.rs, L9: line 9 does not exist (src/queue.rs spans 1 to 3)"
        ),
        "{out}"
    );
}

#[test]
fn rework2_a_marker_that_matches_more_than_once_warns_with_its_lines() {
    let (_, v) = check_json(&fixture("t-101/markers.tiza"), &[]);
    let b1: Vec<Value> = findings(&v)
        .into_iter()
        .filter(|f| f["id"] == "b1")
        .collect();
    assert_eq!(b1.len(), 2, "{v}");
    assert!(
        b1.iter()
            .all(|f| f["level"] == "warning" && f["code"] == "marker")
    );
    assert_eq!(
        b1[0]["message"],
        "start marker «open(» matches 2 times (L3, L6): the range takes the first one (L3); make it unique, or an edit above it can move the range"
    );
    assert!(
        b1[1]["message"]
            .as_str()
            .unwrap()
            .starts_with("end marker «}» matches 2 times after the start (L4, L7)"),
        "{v}"
    );
    // Warnings: the document still passes.
    let dir = scratch("t101-twice");
    let path = dir.join("twice.tiza");
    std::fs::write(
        &path,
        json!({"schema": 1, "source": {"language": "rs"},
               "code": ["fn a() {", "    go();", "}", "fn b() {", "    go();", "}"],
               "annotations": [{"id": "b1", "step": 1, "type": "box", "from_line": 2, "to_line": 3,
                                "markers": {"start": "go()", "end": "}"}}]})
        .to_string(),
    )
    .unwrap();
    let (code, out, _) = check(&[path.to_str().unwrap()]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("warning: b1, step 1, L2: start marker «go()» matches 2 times (L2, L5)"),
        "{out}"
    );
    let (code, _, _) = check(&[path.to_str().unwrap(), "--strict"]);
    assert_eq!(code, 1);
}

#[test]
fn rework3_a_marker_under_a_mask_answers_like_one_that_does_not_exist() {
    let (_, v) = check_json(&fixture("t-101/markers.tiza"), &[]);
    let msg = |id: &str| -> String {
        let f: Vec<Value> = findings(&v).into_iter().filter(|f| f["id"] == id).collect();
        assert_eq!(f.len(), 1, "{id}: {v}");
        assert_eq!(f[0]["level"], "error");
        f[0]["message"].as_str().unwrap().to_string()
    };
    // b2's start marker is (part of) the masked value, b3's is nowhere.
    assert_eq!(msg("b2"), msg("b3"));
    // b4's end marker is under the mask, b5's is nowhere.
    assert_eq!(msg("b4"), msg("b5"));
    assert!(
        msg("b2").starts_with("start marker «…» matches nothing"),
        "{v}"
    );
    assert_no(W_SECRET, "check --json", &v.to_string());
    let (_, out, err) = check(&[fixture("t-101/markers.tiza").to_str().unwrap()]);
    assert_no(W_SECRET, "check", &(out + &err));
}

#[test]
fn rework4_all_steps_cleans_its_own_leftovers_and_honours_skip_zero() {
    let dir = scratch("t101-rerun");
    let out = dir.join("steps");
    std::fs::create_dir_all(&out).unwrap();
    // An earlier run of a longer document, an SVG run, and a file of the user's.
    for f in ["step-08.txt", "step-09.txt", "step-03.svg", "notes.txt"] {
        std::fs::write(out.join(f), "old").unwrap();
    }
    let file = example("ownership.rs.tiza");
    let run = |extra: &[&str]| {
        let mut args = vec![
            "render",
            file.to_str().unwrap(),
            "--all-steps",
            "--format",
            "txt",
            "--out",
            out.to_str().unwrap(),
        ];
        args.extend_from_slice(extra);
        let r = tiza(&args);
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        String::from_utf8_lossy(&r.stderr).into_owned()
    };
    let err = run(&[]);
    assert!(
        err.contains("removed 2 files from an earlier run: step-08.txt, step-09.txt"),
        "{err}"
    );
    assert!(
        err.contains("also holds 1 file of another run, left as it is: step-03.svg"),
        "{err}"
    );
    let names: Vec<String> = listing(&out).into_iter().map(|(n, _)| n).collect();
    let mut want: Vec<String> = (0..=7).map(|s| format!("step-{s:02}.txt")).collect();
    want.extend(["notes.txt".to_string(), "step-03.svg".to_string()]);
    want.sort();
    assert_eq!(names, want);
    // `--skip-zero`: from step 1, and step-00 of the run before goes.
    let err = run(&["--skip-zero"]);
    assert!(
        err.contains("wrote 7 files") && err.contains("(steps 1-7)"),
        "{err}"
    );
    assert!(err.contains("step-00.txt"), "{err}");
    assert!(!out.join("step-00.txt").exists());
    assert!(out.join("step-01.txt").exists() && out.join("notes.txt").exists());
    // One file: singular. A document with only step 0 and --skip-zero: refused.
    let zero = fixture("t-101/zero.tiza");
    let r = tiza(&[
        "render",
        zero.to_str().unwrap(),
        "--all-steps",
        "--out",
        dir.join("one").to_str().unwrap(),
    ]);
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(
        err.contains("wrote 1 file to") && err.contains("(step 0, "),
        "{err}"
    );
    assert!(!err.contains("each"), "{err}");
    let r = tiza(&[
        "render",
        zero.to_str().unwrap(),
        "--all-steps",
        "--skip-zero",
        "--out",
        dir.join("none").to_str().unwrap(),
    ]);
    assert!(!r.status.success());
    assert!(String::from_utf8_lossy(&r.stderr).contains("--skip-zero leaves nothing to write"));
    assert!(!dir.join("none").exists());
}

#[test]
fn rework4_check_help_has_no_socket_or_session() {
    let o = tiza(&["check", "--help"]);
    let help = String::from_utf8_lossy(&o.stdout);
    assert!(
        help.contains("--width") && help.contains("--strict"),
        "{help}"
    );
    assert!(
        !help.contains("--socket") && !help.contains("--session"),
        "{help}"
    );
    // Still accepted (and ignored), before or after the command.
    let file = example("ownership.rs.tiza");
    let (code, _, err) = check(&[file.to_str().unwrap(), "--socket", "/nonexistent.sock"]);
    assert_eq!(code, 0, "{err}");
    let o = tiza(&["--session", "x", "check", file.to_str().unwrap()]);
    assert!(o.status.success());
    // Commands that talk to the instance keep them in their help.
    let o = tiza(&["api", "--help"]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("--socket"));
}

#[test]
fn rework5_a_clean_width_below_the_narrowest_one_says_so() {
    let file = example("ownership.rs.tiza");
    let file = file.to_str().unwrap();
    // 85: no label clipped (notes under the lines), below 108.
    let (code, out, _) = check(&[file, "--width", "85"]);
    assert_eq!(code, 0);
    assert!(out.contains("0 errors, 0 warnings"), "{out}");
    assert!(
        out.trim_end().ends_with(
            "no label is clipped at 108 or wider; 79-93 are clean too (notes go under the lines below 94 columns)"
        ),
        "{out}"
    );
    // 66: no label clipped, but the code is cut (QA 2: line 4 below 79).
    let (_, out, _) = check(&[file, "--width", "66"]);
    assert!(
        out.contains(
            "no label is clipped at width 66, but code lines are: the code needs 79 columns"
        ),
        "{out}"
    );
    assert!(out.contains("79-93 are clean too"), "{out}");
    let (_, v) = check_json(&example("ownership.rs.tiza"), &["--width", "85"]);
    assert_eq!(
        v["narrower"],
        json!({"clean": [[79, 93]], "code_fits_from": 79, "margin_from": 94})
    );
    // At 100 labels are clipped and at 108 nothing is: no `narrower`.
    for w in ["100", "108"] {
        let (_, v) = check_json(&example("ownership.rs.tiza"), &["--width", w]);
        assert!(v.get("narrower").is_none(), "{w}: {v}");
    }
}

// Second rework (QA 2 on d2057db, scenarios A-H from Luis): the cleanup
// removed `step-1.png` and `step-3.png` of the user's. It only ever
// removes names this run's scheme writes: `step-` + exactly the run's
// number of digits (the last step's, at least 2) + `.` + the format
// being written, lowercase, a regular file.

/// `tiza render examples/ownership.rs.tiza --all-steps --out <out> <extra>`:
/// (exit code, stderr). The example has steps 0-7.
fn all_steps(out: &Path, extra: &[&str]) -> (i32, String) {
    let file = example("ownership.rs.tiza");
    let mut args = vec![
        "render",
        file.to_str().unwrap(),
        "--all-steps",
        "--out",
        out.to_str().unwrap(),
    ];
    args.extend_from_slice(extra);
    let r = tiza(&args);
    (
        r.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&r.stderr).into_owned(),
    )
}

fn names_in(dir: &Path) -> Vec<String> {
    listing(dir).into_iter().map(|(n, _)| n).collect()
}

#[test]
fn rework6_a_names_tiza_never_writes_survive_byte_for_byte() {
    let out = scratch("t101-r2-a").join("steps");
    std::fs::create_dir_all(&out).unwrap();
    // No two of these differ only in case (a case-insensitive APFS would
    // make them one file), and none is a name this run writes.
    let user = [
        "step-1.png",
        "step-3.png",
        "STEP-09.PNG",
        "step-10.PNG",
        "step-05.png.bak",
        "step-05.png~",
        ".step-05.png",
        "my-step-05.png",
        "step-5a.png",
        "step-.png",
        "step--1.png",
        "step-05.jpeg",
        "step-11.gif",
        "step-12",
        "notes.txt",
    ];
    for (i, f) in user.iter().enumerate() {
        std::fs::write(out.join(f), format!("mine {i}")).unwrap();
    }
    let (code, err) = all_steps(&out, &[]);
    assert_eq!(code, 0, "{err}");
    for (i, f) in user.iter().enumerate() {
        assert_eq!(
            std::fs::read_to_string(out.join(f)).unwrap(),
            format!("mine {i}"),
            "{f}: {err}"
        );
        assert!(!err.contains(f), "{f} is named: {err}");
    }
    assert!(!err.contains("tiza:"), "{err}");
    let names = names_in(&out);
    for s in 0..=7 {
        assert!(names.contains(&format!("step-{s:02}.png")), "{names:?}");
    }
    assert_eq!(names.len(), user.len() + 8, "{names:?}");
}

#[test]
fn rework6_b_f_leftovers_of_its_format_and_width_go_and_are_named() {
    let out = scratch("t101-r2-b").join("steps");
    std::fs::create_dir_all(&out).unwrap();
    // B: an earlier, longer run. F: names of a 100+ / 1000+ step document
    // are another width: they stay, unnamed (QA 2: the safer rule).
    for f in [
        "step-07.png",
        "step-09.png",
        "step-10.png",
        "step-000.png",
        "step-0123.png",
    ] {
        std::fs::write(out.join(f), "old").unwrap();
    }
    let (code, err) = all_steps(&out, &[]);
    assert_eq!(code, 0, "{err}");
    assert!(
        err.contains("removed 2 files from an earlier run: step-09.png, step-10.png"),
        "{err}"
    );
    // step-07.png was overwritten, not removed.
    for f in ["step-07", "step-000", "step-0123", "another run"] {
        assert!(!err.contains(f), "{f}: {err}");
    }
    for f in ["step-000.png", "step-0123.png"] {
        assert_eq!(std::fs::read(out.join(f)).unwrap(), b"old", "{f}");
    }
    assert_ne!(std::fs::read(out.join("step-07.png")).unwrap(), b"old");
    let mut want: Vec<String> = (0..=7).map(|s| format!("step-{s:02}.png")).collect();
    want.extend(["step-000.png".to_string(), "step-0123.png".to_string()]);
    want.sort();
    assert_eq!(names_in(&out), want);
}

#[cfg(unix)]
#[test]
fn rework6_c_a_directory_or_symlink_named_like_a_step_is_left_and_named() {
    let dir = scratch("t101-r2-c");
    let out = dir.join("steps");
    std::fs::create_dir_all(out.join("step-08.png")).unwrap();
    std::fs::write(out.join("step-08.png").join("inside"), "keep").unwrap();
    std::fs::write(dir.join("outside.png"), "target").unwrap();
    std::os::unix::fs::symlink("../outside.png", out.join("step-09.png")).unwrap();
    let (code, err) = all_steps(&out, &[]);
    assert_eq!(code, 0, "{err}");
    assert!(
        err.contains(
            "also holds 2 entries named like a step that are not a regular file, left as they are: step-08.png (a directory), step-09.png (a symlink)"
        ),
        "{err}"
    );
    assert!(!err.contains("removed"), "{err}");
    assert!(out.join("step-08.png").join("inside").exists());
    let link = std::fs::symlink_metadata(out.join("step-09.png")).unwrap();
    assert!(link.file_type().is_symlink());
    assert_eq!(
        std::fs::read_to_string(dir.join("outside.png")).unwrap(),
        "target"
    );
}

#[test]
fn rework6_d_e_another_format_is_left_and_skip_zero_removes_step_00() {
    let out = scratch("t101-r2-d").join("steps");
    std::fs::create_dir_all(&out).unwrap();
    for s in 0..=7 {
        std::fs::write(out.join(format!("step-{s:02}.svg")), "svg").unwrap();
    }
    std::fs::write(out.join("step-00.png"), "old").unwrap();
    let (code, err) = all_steps(&out, &["--skip-zero"]);
    assert_eq!(code, 0, "{err}");
    // E: step-00.png is not part of this run.
    assert!(
        err.contains("removed 1 file from an earlier run: step-00.png"),
        "{err}"
    );
    assert!(!out.join("step-00.png").exists());
    assert!(out.join("step-01.png").exists() && out.join("step-07.png").exists());
    // D: eight SVGs, named (five, then «and 3 more») and left.
    assert!(
        err.contains(
            "also holds 8 files of another run, left as they are: step-00.svg, step-01.svg, step-02.svg, step-03.svg, step-04.svg and 3 more"
        ),
        "{err}"
    );
    for s in 0..=7 {
        assert!(out.join(format!("step-{s:02}.svg")).exists());
    }
}

/// G. A directory that refuses deletes refuses new images too (unlink and
/// create need the same permission), so the run fails on its first write,
/// exactly as it would without the leftover, and the leftover stays. As
/// root the mode does not bind and the run succeeds; then the leftover is
/// removed like any other. (A single file that cannot be removed, e.g.
/// `chflags uchg` on macOS, gives `could not remove … left as it is`.)
#[cfg(unix)]
#[test]
fn rework6_g_a_directory_without_write_permission_changes_no_exit_code() {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("t101-r2-g");
    let (with, without) = (dir.join("with"), dir.join("without"));
    for d in [&with, &without] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(with.join("step-09.png"), "old").unwrap();
    let ro = std::fs::Permissions::from_mode(0o555);
    std::fs::set_permissions(&with, ro.clone()).unwrap();
    std::fs::set_permissions(&without, ro).unwrap();
    let (code_with, err_with) = all_steps(&with, &[]);
    let (code_without, err_without) = all_steps(&without, &[]);
    for d in [&with, &without] {
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert_eq!(code_with, code_without, "{err_with}\n---\n{err_without}");
    assert!(!err_with.contains("panicked"), "{err_with}");
    if code_with == 0 {
        assert!(err_with.contains("removed 1 file from an earlier run: step-09.png"));
    } else {
        assert!(with.join("step-09.png").exists(), "{err_with}");
        assert!(!err_with.contains("removed"), "{err_with}");
    }
}

#[test]
fn rework6_h_thousands_of_unrelated_files_and_a_long_list() {
    let (big, empty) = (scratch("t101-r2-h").join("big"), scratch("t101-r2-h0"));
    std::fs::create_dir_all(&big).unwrap();
    for i in 0..5000 {
        std::fs::write(big.join(format!("frame-{i}.png")), "").unwrap();
    }
    let (code, err) = all_steps(&big, &["--format", "txt"]);
    assert_eq!(code, 0, "{err}");
    let (_, err0) = all_steps(&empty, &["--format", "txt"]);
    assert_eq!(
        err.replace(big.to_str().unwrap(), "DIR"),
        err0.replace(empty.to_str().unwrap(), "DIR")
    );
    // Twenty leftovers: five names and «and 15 more».
    for s in 10..30 {
        std::fs::write(big.join(format!("step-{s}.txt")), "old").unwrap();
    }
    let (_, err) = all_steps(&big, &["--format", "txt"]);
    assert!(
        err.contains(
            "removed 20 files from an earlier run: step-10.txt, step-11.txt, step-12.txt, step-13.txt, step-14.txt and 15 more"
        ),
        "{err}"
    );
    assert_eq!(names_in(&big).len(), 5000 + 8);
}

/// G, the warning: a leftover that cannot be removed (immutable: `chattr
/// +i` on Linux, `chflags uchg` on macOS) is named, the images are
/// written and the exit code stays 0. Where neither flag can be set (no
/// root for `chattr`, a filesystem without the flag) only the run is checked.
#[cfg(unix)]
#[test]
fn rework6_g_a_leftover_that_cannot_be_removed_is_a_warning() {
    let out = scratch("t101-r2-g2").join("steps");
    std::fs::create_dir_all(&out).unwrap();
    let f = out.join("step-09.png");
    std::fs::write(&f, "old").unwrap();
    let flag = |on: bool| {
        let (cmd, arg) = if cfg!(target_os = "macos") {
            ("chflags", if on { "uchg" } else { "nouchg" })
        } else {
            ("chattr", if on { "+i" } else { "-i" })
        };
        Command::new(cmd)
            .arg(arg)
            .arg(&f)
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    let locked = flag(true);
    let (code, err) = all_steps(&out, &[]);
    if locked {
        flag(false);
    }
    assert_eq!(code, 0, "{err}");
    assert!(out.join("step-07.png").exists(), "{err}");
    if locked {
        assert!(f.exists(), "{err}");
        assert!(
            err.contains("could not remove 1 file from an earlier run (")
                && err.contains("), left as it is: step-09.png"),
            "{err}"
        );
        assert!(!err.contains("removed 1 file"), "{err}");
    } else {
        eprintln!("(no immutable flag here: only the run was checked)");
        assert!(err.contains("removed 1 file from an earlier run: step-09.png"));
    }
}
