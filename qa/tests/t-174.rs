//! t-174 — One output filter for secrets: every text Tiza draws or returns
//! goes through a single function that masks copies of masked values
//! (labels, notes, titles, messages).
//!
//! Acceptance criteria (reoriented by Luis, 2026-10-18: one choke point,
//! not a patch per output):
//! - AC1 One function in tiza-core (`tiza_core::redact::shown`, with
//!   `shown_doc` and the `Echo` filter of t-167 it lives in) applies the
//!   t-157 copy rule for the values under every redact: any copy of a
//!   masked value, whole or a piece of 4+ characters (one a letter or
//!   digit), is masked with its whole token by the same bullets. It is the
//!   only place that decides how document-derived text is shown; t-167's
//!   «do not quote targets» rule lives there too.
//! - AC2 Every output calls it before showing or returning document text:
//!   labels, notes, callouts, box titles, step titles, the document title,
//!   style names, status-bar messages and notices, the supervised list
//!   (t-167), CLI messages and errors. The known formatting sites are
//!   listed in the module docs and grepped here.
//! - AC3 Same result in every reader and export: TUI, `render` (PNG, SVG,
//!   GIF, MP4, txt, md, ansi, cast), `render.text`/`render_text`,
//!   `code.get`, `scene.list`/`list_annotations`, MCP resources
//!   (`tiza://scene`, `tiza://render`, `tiza://render.md`), `status`.
//! - AC4 `scene.get` and the saved `.tiza` keep the author's text as
//!   written; only `scene.get` does (the `tiza://scene` resource is a
//!   reader and is masked). The agent guide says to edit from `scene.get`.
//! - AC5 One alert the first time a text repeats a masked value
//!   (`redact_alerts`): `h1: its label repeats masked text; masked on
//!   screen — fix the label`; `tiza check` warns with the same words; an
//!   agent's `annotation.add`/`update` with such a text is accepted and the
//!   reply says it was masked, never quoting it.
//! - AC6 This file: one test per reader/export, searching the output bytes
//!   (raw pty included) for any piece of 4+ characters of the value.
//!
//! Corner cases: pieces of 3 or of 4+ without a letter/digit are not
//! masked; a common word sharing a piece with the value is masked (fail
//! closed); `redact_from_step` masks from step 0; a clipped label is masked
//! before it is cut; overlapping redacts; the values are computed once per
//! change, not per frame.
//!
//! The repro is QA 1's pruebas/t167/qa1-label/demo.tiza (on Luis's Mac),
//! rebuilt as qa/fixtures/t-174/demo.tiza.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, fixture, scratch, tiza};

const VALUE: &str = "9vK7wPx3Lm";
const MASK: &str = "••••••••";
const LABEL_ALERT: &str = "h1: its label repeats masked text; masked on screen — fix the label";

/// Every piece of 4 characters of the value (the t-157 rule).
fn pieces() -> Vec<String> {
    let c: Vec<char> = VALUE.chars().collect();
    (0..=c.len() - 4)
        .map(|i| c[i..i + 4].iter().collect())
        .collect()
}

#[track_caller]
fn assert_clean(what: &str, s: &str) {
    for p in pieces() {
        assert!(!s.contains(&p), "{what} shows «{p}» of the value:\n{s}");
    }
}

#[track_caller]
fn assert_clean_bytes(what: &str, b: &[u8]) {
    for p in pieces() {
        assert!(
            !b.windows(p.len()).any(|w| w == p.as_bytes()),
            "{what} holds «{p}» of the value"
        );
    }
}

/// The repro: `TOKEN=9vK7wPx3Lm` on L2 under a redact, a highlight label
/// `was 9vK7wPx3Lm` and a note `rotate 9vK7wPx3Lm soon`.
fn demo() -> PathBuf {
    fixture("t-174/demo.tiza")
}

/// Every kind of text a document can hold, each repeating the value (or a
/// piece of it): title, step title and id, style name and legend, labels
/// of a highlight, mark, box and arrow, a note, a callout's title and a
/// long text (cut at 40 characters in `scene.list`).
fn all_texts() -> Value {
    json!({
        "schema": 1,
        "title": format!("Demo of {VALUE}"),
        "source": {"language": "sh", "first_line": 1},
        "code": ["# demo", format!("TOKEN={VALUE}"), "USER=ana", "PORT=8080", "HOST=db"],
        "styles": {
            format!("k{VALUE}"): {"glyph": "!", "legend": format!("legend {VALUE}"), "color": "red"}
        },
        "steps": [{"step": 1, "id": format!("s{VALUE}"), "title": format!("Rotate {VALUE}")}],
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 6, "end": 16}},
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 1, "start": 2, "end": 6}, "label": format!("was {VALUE}")},
            {"id": "n1", "step": 1, "type": "note", "line": 3, "text": format!("rotate {VALUE} soon")},
            {"id": "b1", "step": 1, "type": "box", "from_line": 3, "to_line": 4, "label": format!("old {VALUE}")},
            {"id": "a1", "step": 1, "type": "arrow", "from": {"line": 4, "col": 0}, "to": {"line": 5, "col": 0}, "label": "see 7wPx3L"},
            {"id": "m1", "step": 1, "type": "mark", "kind": format!("k{VALUE}"), "span": {"line": 4, "start": 0, "end": 4}, "label": "mark K7wP"},
            {"id": "c1", "step": 1, "type": "callout", "from_line": 5, "title": format!("about {VALUE}"),
             "text": format!("The long text of this callout runs past forty characters and then names {VALUE} at its end")}
        ]
    })
}

fn write(dir: &Path, name: &str, doc: &Value) -> PathBuf {
    let p = dir.join(format!("{name}.tiza"));
    std::fs::write(&p, serde_json::to_string_pretty(doc).unwrap()).unwrap();
    p
}

fn live_of(doc: &Value, name: &str) -> Live {
    let dir = scratch(name);
    let doc = tiza_core::fs::parse_document(&doc.to_string(), None).unwrap();
    Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"))
}

fn out_of(args: &[&str]) -> (Vec<u8>, String) {
    let out = tiza(args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    (
        out.stdout,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// An SVG without its embedded fonts: base64 font data may hold any four
/// characters by chance.
fn without_font_data(svg: &str) -> String {
    let mut out = String::new();
    let mut rest = svg;
    while let Some(k) = rest.find("base64,") {
        out.push_str(&rest[..k]);
        let after = &rest[k..];
        let end = after.find(['"', ')', '\'']).unwrap_or(after.len());
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

// ---------- AC1: the one function ----------

#[test]
fn ac1_shown_masks_whole_copies_and_pieces_with_their_token() {
    let doc = tiza_core::fs::parse_document(&all_texts().to_string(), None).unwrap();
    let shown = |t: &str| tiza_core::redact::shown(t, &doc);
    assert_eq!(shown(&format!("was {VALUE}")), format!("was {MASK}"));
    assert_eq!(
        shown(&format!("rotate {VALUE} soon")),
        format!("rotate {MASK} soon")
    );
    // A piece of 4+ with its whole token; inside «» the quotes stay.
    assert_eq!(shown("see 7wPx3L now"), format!("see {MASK} now"));
    assert_eq!(shown("old «xK7wPq»"), format!("old «{MASK}»"));
    // A line at a time: a token ending in `:` runs to the end of its line only.
    assert_eq!(
        shown(&format!("key {VALUE}: here\nnext line")),
        format!("key {MASK}\nnext line")
    );
    // Corner: 3 characters, or 4+ with no letter/digit, are not a copy.
    assert_eq!(shown("ask 3Lm and 9vK"), "ask 3Lm and 9vK");
    // Nothing changes on a document without redacts.
    let plain = tiza_core::model::Document::from_code(&format!("x={VALUE}\n"), "sh", None);
    assert_eq!(
        tiza_core::redact::shown(&format!("was {VALUE}"), &plain),
        format!("was {VALUE}")
    );
}

#[test]
fn corner_no_letter_or_digit_piece_and_fail_closed_common_word() {
    // `x----y` under the mask: `----` has no letter or digit.
    let d = tiza_core::fs::parse_document(
        &json!({"schema": 1, "source": {"language": "txt"}, "code": ["pw=x----y passw0rd!"],
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 1, "start": 3, "end": 9}},
            {"id": "r2", "step": 1, "type": "redact", "span": {"line": 1, "start": 10, "end": 19}}
        ]})
        .to_string(),
        None,
    )
    .unwrap();
    assert_eq!(tiza_core::redact::shown("a ---- b", &d), "a ---- b");
    // Fail closed: `password` shares `pass` with `passw0rd!`.
    assert_eq!(
        tiza_core::redact::shown("the password field", &d),
        format!("the {MASK} field")
    );
    let found = tiza_core::redact::masked_texts(&{
        let mut d = d.clone();
        d.title = Some("set the password".into());
        d
    });
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].notice(),
        "document: its title repeats masked text; masked on screen — fix the title"
    );
}

#[test]
fn corner_overlapping_redacts_mask_every_value() {
    // Two redacts on one value, one longer: both values are searched.
    let d = tiza_core::fs::parse_document(
        &json!({"schema": 1, "source": {"language": "txt"}, "code": ["k=abcd1234efgh5678 end"],
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 1, "start": 2, "end": 10}},
            {"id": "r2", "step": 1, "type": "redact", "span": {"line": 1, "start": 6, "end": 18}}
        ]})
        .to_string(),
        None,
    )
    .unwrap();
    for (label, want) in [
        ("a abcd1234 b", format!("a {MASK} b")),
        ("a efgh5678 b", format!("a {MASK} b")),
        ("a 1234efgh b", format!("a {MASK} b")),
        ("x abcd y 5678 z", format!("x {MASK} y {MASK} z")),
    ] {
        assert_eq!(tiza_core::redact::shown(label, &d), want, "{label}");
    }
}

#[test]
fn corner_values_are_computed_once_per_change_not_per_frame() {
    let mut d = tiza_core::fs::parse_document(&all_texts().to_string(), None).unwrap();
    let a = tiza_core::redact::masked_values_cached(&d);
    let b = tiza_core::redact::masked_values_cached(&d);
    assert!(
        std::sync::Arc::ptr_eq(&a, &b),
        "the same values, not recomputed"
    );
    // Labels and steps do not count: only the code, the redacts, the moves.
    d.step = 0;
    d.title = Some("other".into());
    let c = tiza_core::redact::masked_values_cached(&d);
    assert!(std::sync::Arc::ptr_eq(&a, &c));
    d.code[1] = "TOKEN=zzQQ1122yy".into();
    let e = tiza_core::redact::masked_values_cached(&d);
    assert!(!std::sync::Arc::ptr_eq(&a, &e), "a code change recomputes");
    assert_eq!(e.as_slice(), ["zzQQ1122yy"]);
}

// ---------- AC2: the known formatting sites ----------

#[test]
fn ac2_every_known_formatting_site_calls_the_filter() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap();
    let docs = read("crates/tiza-core/src/redact.rs");
    assert!(
        docs.contains("# The output filter (t-167, t-174)"),
        "module docs list the sites"
    );
    for (file, needle) in [
        (
            "crates/tiza-core/src/layout.rs",
            "f.shown_doc(&mut masked_doc)",
        ),
        (
            "crates/tiza-core/src/protocol.rs",
            "echo.shown_shape(&mut shown.shape)",
        ),
        (
            "crates/tiza-core/src/protocol.rs",
            "crate::redact::shown(t, doc)",
        ),
        (
            "crates/tiza-core/src/steps.rs",
            "crate::redact::shown(t, self)",
        ),
        (
            "crates/tiza-core/src/cast.rs",
            "crate::redact::shown(t, doc)",
        ),
        (
            "crates/tiza-core/src/check.rs",
            "crate::redact::masked_texts(&doc)",
        ),
        (
            "crates/tiza-tui/src/app.rs",
            "self.message = Some((self.shown(&msg.into()), false));",
        ),
        (
            "crates/tiza-tui/src/app.rs",
            "self.message = Some((self.shown(&msg.into()), true));",
        ),
        (
            "crates/tiza-tui/src/app.rs",
            "let notice = self.shown(&notice);",
        ),
        (
            "crates/tiza-tui/src/app.rs",
            "let title = self.doc.title.as_deref().map(|t| self.shown(t));",
        ),
        ("crates/tiza-tui/src/app.rs", "shown_doc(&mut document)"),
        (
            "crates/tiza-tui/src/app.rs",
            "proposal_summary(&self.echo(), &req)",
        ),
        (
            "crates/tiza-cli/src/main.rs",
            "let doc = tiza_core::redact::shown_doc(&doc);",
        ),
    ] {
        assert!(
            read(file).contains(needle),
            "{file} no longer calls the filter: {needle}"
        );
    }
    // The MCP reads a reader's copy for the resource and the exports.
    assert_eq!(
        read("crates/tiza-mcp/src/lib.rs")
            .matches("json!({\"as_shown\": true})")
            .count(),
        2
    );
    // `describe_shape` (the `scene.list` text) is only reached through
    // `describe_with`, which masks first.
    let protocol = read("crates/tiza-core/src/protocol.rs");
    assert_eq!(
        protocol.matches("describe_shape(").count(),
        2,
        "one call, one definition"
    );
    // No export reads the raw title.
    for file in ["crates/tiza-tui/src/app.rs", "crates/tiza-mcp/src/lib.rs"] {
        assert!(
            !read(file).contains("to_markdown(self.doc.title"),
            "{file} formats the raw title"
        );
    }
}

// ---------- AC3: every export of `tiza render` ----------

#[test]
fn ac3_render_txt_shows_bullets_where_the_value_was() {
    let f = demo();
    let (txt, err) = out_of(&[
        "render",
        f.to_str().unwrap(),
        "--format",
        "txt",
        "--width",
        "100",
    ]);
    let txt = String::from_utf8(txt).unwrap();
    assert!(txt.contains(&format!("was {MASK}")), "{txt}");
    assert!(txt.contains(&format!("rotate {MASK} soon")), "{txt}");
    assert_clean("render txt", &txt);
    // Said once on stderr, never quoted.
    assert!(err.contains(&format!("tiza: {LABEL_ALERT}")), "{err}");
    assert!(err.contains("n1: its text repeats masked text"), "{err}");
    assert_clean("render stderr", &err);
}

#[test]
fn ac3_render_every_format_holds_no_piece_of_the_value() {
    let dir = scratch("t174-render");
    let doc = write(&dir, "all", &all_texts());
    let d = doc.to_str().unwrap();
    for fmt in ["txt", "ansi", "md", "cast", "tape"] {
        let (out, err) = out_of(&["render", d, "--format", fmt, "--width", "120"]);
        let s = String::from_utf8_lossy(&out).into_owned();
        assert_clean(&format!("render {fmt}"), &s);
        assert_clean(&format!("render {fmt} stderr"), &err);
    }
    let (md, _) = out_of(&["render", d, "--format", "md", "--step", "1"]);
    let md = String::from_utf8(md).unwrap();
    assert!(
        md.contains(&format!("Demo of {MASK}")),
        "the md title: {md}"
    );
    let (cast, _) = out_of(&["render", d, "--format", "cast"]);
    let cast = String::from_utf8(cast).unwrap();
    assert!(
        cast.lines()
            .next()
            .unwrap()
            .contains(&format!("Demo of {MASK}")),
        "{cast}"
    );

    // SVG with and without fonts (the `<title>` and every text).
    let (svg, _) = out_of(&[
        "render",
        d,
        "--format",
        "svg",
        "--step",
        "1",
        "--no-embed-fonts",
    ]);
    let svg = String::from_utf8(svg).unwrap();
    assert!(
        svg.contains(&format!("<title>Demo of {MASK}</title>")),
        "svg title"
    );
    assert_clean("render svg", &svg);
    let (svg, _) = out_of(&["render", d, "--format", "svg", "--step", "1"]);
    assert_clean(
        "render svg (fonts left out)",
        &without_font_data(&String::from_utf8(svg).unwrap()),
    );

    // Binary exports: PNG, GIF, MP4 (when ffmpeg is there), every byte.
    for fmt in ["png", "gif", "mp4"] {
        if fmt == "mp4"
            && std::process::Command::new("ffmpeg")
                .arg("-version")
                .output()
                .map(|o| !o.status.success())
                .unwrap_or(true)
        {
            eprintln!("skip mp4: no ffmpeg on PATH");
            continue;
        }
        let out = dir.join(format!("all.{fmt}"));
        let (_, err) = out_of(&[
            "render",
            d,
            "--out",
            out.to_str().unwrap(),
            "--width",
            "100",
        ]);
        assert_clean(&format!("render {fmt} stderr"), &err);
        assert_clean_bytes(&format!("render {fmt}"), &std::fs::read(&out).unwrap());
    }

    // --all-steps: one file per step.
    let steps = dir.join("steps");
    out_of(&[
        "render",
        d,
        "--all-steps",
        "--format",
        "txt",
        "--out",
        &format!("{}/", steps.display()),
    ]);
    for e in std::fs::read_dir(&steps).unwrap() {
        let p = e.unwrap().path();
        assert_clean(
            &p.display().to_string(),
            &std::fs::read_to_string(&p).unwrap(),
        );
    }
}

#[test]
fn corner_a_clipped_label_is_masked_before_it_is_cut() {
    let dir = scratch("t174-clip");
    let doc = write(&dir, "all", &all_texts());
    for w in ["40", "46", "60", "80"] {
        let (txt, _) = out_of(&[
            "render",
            doc.to_str().unwrap(),
            "--format",
            "txt",
            "--width",
            w,
        ]);
        assert_clean(
            &format!("render at width {w}"),
            &String::from_utf8_lossy(&txt),
        );
    }
    // The check names clipped labels with their full text: masked too.
    let out = tiza(&["check", doc.to_str().unwrap(), "--width", "40", "--json"]);
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(report.contains("clipped_label"), "{report}");
    assert_clean("check --json at 40", &report);
}

#[test]
fn corner_redact_from_step_masks_labels_from_step_zero() {
    let mut doc = all_texts();
    doc["redact_from_step"] = json!(true);
    doc["annotations"][0]["step"] = json!(3);
    doc["annotations"][1]["step"] = json!(1);
    let dir = scratch("t174-from-step");
    let f = write(&dir, "from-step", &doc);
    for step in ["0", "1", "2", "3"] {
        let (txt, _) = out_of(&[
            "render",
            f.to_str().unwrap(),
            "--format",
            "txt",
            "--step",
            step,
        ]);
        let txt = String::from_utf8(txt).unwrap();
        if step == "1" || step == "2" {
            // The code is not masked yet (the author asked for that), but
            // the label already is: a later mask leaves no earlier frame in
            // clear.
            assert!(txt.contains(&format!("was {MASK}")), "step {step}:\n{txt}");
        }
        if step == "3" {
            assert_clean("step 3", &txt);
        }
        for line in txt.lines().filter(|l| !l.contains("TOKEN=")) {
            assert_clean(
                &format!("step {step} (outside the code under the mask)"),
                line,
            );
        }
    }
}

// ---------- AC3/AC4: the live readers ----------

#[test]
fn ac3_live_readers_mask_and_scene_get_keeps_the_author_text() {
    let live = live_of(&all_texts(), "t174-live");
    live.with(|a| a.set_step(1));
    let text = live.api_ok("render.text", json!({"width": 120}));
    let text = text["text"].as_str().unwrap();
    assert!(text.contains(&format!("was {MASK}")), "{text}");
    assert_clean("render.text", text);
    let code = live.api_ok("code.get", json!({}));
    assert_clean("code.get", &code.to_string());
    let list = live.api_ok("scene.list", json!({}));
    assert!(
        list.to_string().contains(&format!("«was {MASK}»")),
        "{list}"
    );
    assert_clean("scene.list", &list.to_string());
    let status = live.api_ok("status", json!({}));
    assert_eq!(status["step_title"], json!(format!("Rotate {MASK}")));
    assert_eq!(
        status["step_label"],
        json!(format!("step 1/1 · Rotate {MASK}"))
    );
    assert_clean("status", &status.to_string());
    let styles = live.api_ok("style.list", json!({}));
    assert_clean("style.list", &styles["document"].to_string());
    for (m, p) in [("step.prev", json!({})), ("step.next", json!({}))] {
        let r = live.api_ok(m, p);
        assert_clean(m, &r.to_string());
    }
    let shown = live.api_ok("scene.get", json!({"as_shown": true}));
    assert_clean("scene.get as_shown", &shown.to_string());
    assert_eq!(shown["document"]["title"], json!(format!("Demo of {MASK}")));
    // AC4: the editing call keeps the author's text; the code stays masked.
    let scene = live.api_ok("scene.get", json!({}));
    assert_eq!(
        scene["document"]["annotations"][1]["label"],
        json!(format!("was {VALUE}"))
    );
    assert_eq!(scene["document"]["code"][1], json!(format!("TOKEN={MASK}")));
    // The saved file keeps it too.
    live.command("w");
    let saved = std::fs::read_to_string(live.with(|a| a.save_path.clone())).unwrap();
    assert!(saved.contains(&format!("was {VALUE}")), "{saved}");
    // The screen: labels, the bar's step title, `:list`, `:export`.
    let screen = live.screen(140, 30);
    assert!(screen.contains(&format!("Rotate {MASK}")), "{screen}");
    assert_clean("the screen", &screen);
    live.keys(&[crossterm::event::KeyCode::Char('L')]);
    let screen = live.screen(140, 30);
    assert!(screen.contains("Annotations"), "{screen}");
    assert_clean("the :list overlay", &screen);
    let dir = scratch("t174-live-export");
    for ext in ["md", "txt", "ansi"] {
        let p = dir.join(format!("x.{ext}"));
        live.command(&format!("export {}", p.display()));
        assert_clean(
            &format!(":export {ext}"),
            &std::fs::read_to_string(&p).unwrap(),
        );
    }
    let msg = live.message();
    assert_clean("the bar message", &format!("{msg:?}"));
}

#[test]
fn ac3_mcp_resources_and_tools_mask_and_add_note_says_masked() {
    let live = live_of(&all_texts(), "t174-mcp");
    live.with(|a| a.set_step(1));
    let mut mcp = McpClient::start(&live.socket);
    for uri in [
        "tiza://scene",
        "tiza://render",
        "tiza://render.md",
        "tiza://code",
    ] {
        let v = mcp.request("resources/read", json!({"uri": uri}));
        assert!(v["error"].is_null(), "{v}");
        let t = v["result"]["contents"][0]["text"].as_str().unwrap();
        assert_clean(uri, t);
    }
    let v = mcp.request("resources/read", json!({"uri": "tiza://scene"}));
    let scene: Value =
        serde_json::from_str(v["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        scene["document"]["annotations"][1]["label"],
        json!(format!("was {MASK}"))
    );
    for (tool, args) in [
        ("render_text", json!({"width": 120})),
        ("list_annotations", json!({})),
        ("get_code", json!({})),
        ("status", json!({})),
        ("check", json!({"width": 40})),
    ] {
        let (err, text, raw) = mcp.call(tool, args);
        assert!(!err, "{tool}: {raw}");
        assert_clean(tool, &raw.to_string());
        assert_clean(tool, &text);
    }
    let dir = scratch("t174-mcp-export");
    for ext in ["svg", "md", "txt", "png"] {
        let p = dir.join(format!("x.{ext}"));
        let (err, text, raw) = mcp.call("export_image", json!({"path": p.to_str().unwrap()}));
        assert!(!err, "{raw}");
        assert_clean("export_image reply", &text);
        let bytes = std::fs::read(&p).unwrap();
        if ext == "svg" {
            let svg = String::from_utf8(bytes).unwrap();
            assert!(
                svg.contains(&format!("<title>Demo of {MASK}</title>")),
                "svg title"
            );
            assert_clean("export_image svg", &without_font_data(&svg));
        } else {
            assert_clean_bytes(&format!("export_image {ext}"), &bytes);
        }
    }
    // An agent's note with the value: accepted, masked, said so, not quoted.
    let (err, text, raw) = mcp.call(
        "add_note",
        json!({"line": 4, "text": format!("old token {VALUE}"), "id": "n9"}),
    );
    assert!(!err, "{raw}");
    assert!(text.contains("Its text repeats masked text"), "{text}");
    assert!(text.contains("\"masked\": true"), "{text}");
    assert_clean("add_note reply", &raw.to_string());
    let (_, frame, _) = mcp.call("render_text", json!({"width": 120}));
    assert!(frame.contains(&format!("old token {MASK}")), "{frame}");
    let (_, list, _) = mcp.call("list_annotations", json!({}));
    assert!(
        list.contains(&format!("n9 [step 1] note L4 «old token {MASK}»")),
        "{list}"
    );
    let (_, status, _) = mcp.call("status", json!({}));
    assert!(
        status.contains("n9: its text repeats masked text; masked on screen — fix the text"),
        "{status}"
    );
    // A label without a copy says nothing.
    let (_, text, _) = mcp.call("add_note", json!({"line": 4, "text": "fine", "id": "n8"}));
    assert!(!text.contains("masked text"), "{text}");
}

// ---------- AC5: the alert and the check warning ----------

#[test]
fn ac5_alert_once_per_text_and_check_warns_with_the_same_words() {
    let doc = tiza_core::fs::open(demo().to_str().unwrap())
        .unwrap()
        .document;
    let dir = scratch("t174-alert");
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    let _ = live.screen(120, 30);
    let alerts = live.with(|a| a.redact_alerts.clone());
    assert_eq!(
        alerts,
        [
            LABEL_ALERT.to_string(),
            "n1: its text repeats masked text; masked on screen — fix the text".to_string()
        ]
    );
    let screen = live.screen(120, 30);
    assert!(screen.contains(LABEL_ALERT), "{screen}");
    // Seen: dismissed, and not said again on the next change.
    live.with(|a| a.dismiss_redact_alerts());
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "plain"}),
    );
    assert!(live.with(|a| a.redact_alerts.clone()).is_empty());
    // An update that repeats it again (a fixed label broken again) is said.
    live.api_ok("annotation.update", json!({"id": "h1", "label": "fixed"}));
    let r = live.api_ok(
        "annotation.update",
        json!({"id": "h1", "label": format!("now x{VALUE}")}),
    );
    assert_eq!(r["masked"], json!(true), "{r}");
    assert_clean("annotation.update reply", &r.to_string());
    assert_eq!(
        live.with(|a| a.redact_alerts.clone()),
        [LABEL_ALERT.to_string()]
    );

    // `tiza check`: one warning per text, the same words, no value.
    let out = tiza(&["check", demo().to_str().unwrap()]);
    let report =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(
        report.contains(
            "warning: h1, step 1: its label repeats masked text; masked on screen — fix the label"
        ),
        "{report}"
    );
    assert!(
        report.contains("warning: n1, step 1: its text repeats masked text"),
        "{report}"
    );
    assert!(report.contains("0 errors, 2 warnings"), "{report}");
    assert_clean("tiza check", &report);
    let out = tiza(&["check", demo().to_str().unwrap(), "--json"]);
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let codes: Vec<&str> = json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["code"].as_str())
        .collect();
    assert_eq!(codes, ["masked_text", "masked_text"]);
}

#[test]
fn ac5_every_kind_of_text_is_said_and_checked() {
    let dir = scratch("t174-check-all");
    let f = write(&dir, "all", &all_texts());
    let out = tiza(&["check", f.to_str().unwrap()]);
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    for want in [
        "document: its title repeats masked text",
        "step 1: its title repeats masked text",
        "step 1: its id repeats masked text",
        "its name repeats masked text",
        "its legend repeats masked text",
        "h1, step 1: its label",
        "n1, step 1: its text",
        "b1, step 1: its label",
        "a1, step 1: its label",
        "m1, step 1: its kind",
        "m1, step 1: its label",
        "c1, step 1: its text",
        "c1, step 1: its title",
    ] {
        assert!(report.contains(want), "{want}:\n{report}");
    }
    assert_clean("tiza check", &report);
}

// ---------- AC3/AC6: the TUI in a pty, raw bytes ----------

#[test]
fn ac6_tui_in_a_pty_raw_bytes_hold_no_piece() {
    let dir = scratch("t174-pty");
    let doc = write(&dir, "all", &all_texts());
    let sock = dir.join("live.sock");
    let out = tiza_qa::pty_script(
        &dir,
        "tui.py",
        r##"
binary, doc, sock = sys.argv[1], sys.argv[2], sys.argv[3]
pieces = sys.argv[4].split(",")
squeezed = lambda out: plain(out).replace(" ", "")
s = Pty([binary, "--socket", sock, "--no-save", doc], 30, 140)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 10, "first frame"):
    if s.wait_for(lambda out: "itslabelrepeatsmaskedtext" in squeezed(out), 10, "the alert"):
        print("ALERT", flush=True)
    for key in [b"n", b"p", b"n", b"L", b"\x1b", b"P", b"p", b"n", b"P"]:
        s.send(key)
        s.settle(0.2)
    if "••••••••" in plain(s.out):
        print("BULLETS", flush=True)
s.kill()
s.finish(5)
raw = s.out
leaks = [p for p in pieces if p.encode() in raw]
print("RAW_LEAKS" if leaks else "RAW_CLEAN", leaks, flush=True)
"##,
        &[
            tiza_qa::bin().to_str().unwrap(),
            doc.to_str().unwrap(),
            sock.to_str().unwrap(),
            &pieces().join(","),
        ],
    );
    for want in ["ALERT", "BULLETS", "RAW_CLEAN"] {
        assert!(out.contains(want), "{want}:\n{out}");
    }
}

// ---------- AC4: the agent guide ----------

#[test]
fn ac4_the_agent_guide_says_to_edit_from_scene_get() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let guide = std::fs::read_to_string(root.join("docs/agent-guide.md")).unwrap();
    assert!(
        guide.contains("edit from `scene.get`, never from `tiza://scene`"),
        "agent guide"
    );
    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    assert!(
        readme.contains("repeats masked text"),
        "README, secrets section"
    );
}
