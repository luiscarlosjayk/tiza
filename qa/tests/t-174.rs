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
//! - AC4 `scene.get` keeps the author's text as written; only `scene.get`
//!   does (the `tiza://scene` resource is a reader and is masked). The
//!   agent guide says to edit from `scene.get`. Review round 1 (fail
//!   closed, for Luis to confirm): a `.tiza` whose code is embedded is
//!   saved with its code masked (t-117), so its texts that share text with
//!   a masked value are saved masked too — reopened, it no longer knows the
//!   value. A file reference keeps the author's texts. Across `scene.get` →
//!   `scene.put` the instance keeps the values it knew, in memory only.
//! - AC5 One alert the first time a text shares text with a masked value
//!   (`redact_alerts`): `h1: its label shares text with a masked value;
//!   masked on screen — reword the label` (Luis's wording, the field named
//!   as it is: label, text, title, kind, legend, glyph, name, id); `tiza
//!   check` warns with the same words; an agent's `annotation.add`/`update`
//!   or `style.set` with such a text is accepted and the reply says
//!   `"masked": true` with the same sentence, never quoting it. An agent's
//!   new id that shares text with a masked value is refused
//!   (`invalid_argument`, not quoted); one already in the file is masked
//!   wherever ids are shown, and said.
//! - AC10 (Luis, 2026-10-18, absorbs t-177): every write of a `.tiza` with
//!   embedded code (autosave, `:w`, `:w PATH`, after `scene.put`/`patch`,
//!   `:embed`) saves masked every copy of a value — move content, labels,
//!   texts, titles, step titles, legends, glyphs, style names, ids — and
//!   the reopened file shows exactly what the instance showed (`render
//!   --all-steps --format txt` compared). Never silent: `a3: its label
//!   shares text with a masked value; saved masked — reword it`, and
//!   `:embed` says `embedded: masked 1 copy of a redacted value in move
//!   #N`. A file-reference `.tiza` keeps the author's texts.
//! - Review round 1: a move that writes a copy of the value says
//!   `redact r1: masked a copy of its value on L7 (move #1)`, not «could
//!   not follow … L1-7»; glyphs, Tab completion, `steps` warnings,
//!   `style.set` and `move.list`/`:list` targets go through the filter.
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
/// The alert on a document whose code is embedded (it is saved masked,
/// AC10); a file-reference document says «masked on screen — reword the
/// label» instead.
const LABEL_ALERT: &str = "h1: its label shares text with a masked value; saved masked — reword it";
/// Before Tiza wrote it (rework point 3): a reader (`tiza check`, `tiza
/// render`) of the fixture, whose embedded code holds the value in clear.
const READER_ALERT: &str = "h1: its label shares text with a masked value; demo.tiza still holds it in clear — open and save it in Tiza, or reword it";
/// The same words from the instance that has it open and has not written
/// it (its `check` is the CLI's, t-101).
const LIVE_ALERT: &str = READER_ALERT;

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
    let found = tiza_core::redact::masked_texts(
        &{
            let mut d = d.clone();
            d.title = Some("set the password".into());
            d
        },
        &tiza_core::redact::Kept::Saved,
    );
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].notice(),
        "document: its title shares text with a masked value; saved masked — reword it"
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
            "f.shown_texts(&mut masked_doc)",
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
            "crate::redact::masked_texts(&doc, &kept)",
        ),
        (
            "crates/tiza-tui/src/app.rs",
            "self.message = Some((self.shown_message(&msg.into()), false));",
        ),
        (
            "crates/tiza-tui/src/app.rs",
            "self.message = Some((self.shown_message(&msg.into()), true));",
        ),
        (
            "crates/tiza-tui/src/app.rs",
            "let notice = self.shown_message(&notice);",
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
        ("crates/tiza-core/src/fs.rs", ".shown_doc(&mut doc);"),
        (
            "crates/tiza-core/src/fs.rs",
            "crate::redact::shown_message(",
        ),
        (
            "crates/tiza-core/src/model.rs",
            "crate::redact::shown_message(n, self)",
        ),
        ("crates/tiza-tui/src/app.rs", "self.refuse_masked_id(id)?;"),
        ("crates/tiza-tui/src/app.rs", "!filter.repeats(c)"),
        (
            "crates/tiza-tui/src/app.rs",
            "tiza_core::redact::carry_values(&self.doc, &mut doc);",
        ),
        ("crates/tiza-tui/src/ui.rs", "describe_move_with("),
        // A file name is the document's text (rework point 4).
        ("crates/tiza-tui/src/ui.rs", "let file = app.shown(&file);"),
        ("crates/tiza-tui/src/app.rs", ".map(|f| self.shown(&f))"),
        ("crates/tiza-tui/src/app.rs", ".map(|l| self.shown(&l))"),
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
    // Said once on stderr, never quoted; nobody saved it: never «saved
    // masked» (rework point 3).
    assert!(err.contains(&format!("tiza: {READER_ALERT}")), "{err}");
    assert!(
        err.contains(
            "n1: its text shares text with a masked value; demo.tiza still holds it in clear"
        ),
        "{err}"
    );
    assert!(!err.contains("saved masked"), "{err}");
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
    // The saved file: its code is embedded, so it is saved masked (t-117)
    // and so are the texts that share text with the value (review round
    // 1: reopened, the file no longer knows the value).
    live.command("w");
    let saved = std::fs::read_to_string(live.with(|a| a.save_path.clone())).unwrap();
    assert!(saved.contains(&format!("was {MASK}")), "{saved}");
    assert_clean("the saved .tiza", &saved);
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
    assert!(
        text.contains("n9: its text shares text with a masked value; saved masked — reword it."),
        "{text}"
    );
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
        status.contains("n9: its text shares text with a masked value; saved masked — reword it"),
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
    // Saved nowhere yet (no file at its save path): masked on screen.
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    let _ = live.screen(120, 30);
    let alerts = live.with(|a| a.redact_alerts.clone());
    let on_screen = [
        "h1: its label shares text with a masked value; masked on screen — reword the label"
            .to_string(),
        "n1: its text shares text with a masked value; masked on screen — reword the text"
            .to_string(),
    ];
    assert_eq!(alerts, on_screen);
    let screen = live.screen(120, 30);
    assert!(screen.contains(&on_screen[0]), "{screen}");
    // Seen: dismissed. The next change autosaves it: written now, each text
    // is said once more, «saved masked» (AC10: never silent).
    live.with(|a| a.dismiss_redact_alerts());
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "plain"}),
    );
    let _ = live.screen(120, 30);
    assert_eq!(
        live.with(|a| a.redact_alerts.clone()),
        [
            LABEL_ALERT.to_string(),
            "n1: its text shares text with a masked value; saved masked — reword it".to_string()
        ]
    );
    // And not again on the next change.
    live.with(|a| a.dismiss_redact_alerts());
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "plain too"}),
    );
    let _ = live.screen(120, 30);
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
            "warning: h1, step 1: its label shares text with a masked value; demo.tiza still holds it in clear — open and save it in Tiza, or reword it"
        ),
        "{report}"
    );
    assert!(
        report.contains("warning: n1, step 1: its text shares text with a masked value"),
        "{report}"
    );
    // Its embedded code holds the value in clear (written by hand).
    assert!(
        report.contains(
            "warning: r1: the embedded code holds the value under this redact in clear; demo.tiza still holds it — open and save it in Tiza"
        ),
        "{report}"
    );
    assert!(!report.contains("saved masked"), "{report}");
    assert!(report.contains("0 errors, 3 warnings"), "{report}");
    assert_clean("tiza check", &report);
    let out = tiza(&["check", demo().to_str().unwrap(), "--json"]);
    let json: Value = serde_json::from_slice(&out.stdout).unwrap();
    let codes: Vec<&str> = json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| f["code"].as_str())
        .collect();
    assert_eq!(codes, ["masked_text", "masked_text", "clear_value"]);
}

#[test]
fn ac5_every_kind_of_text_is_said_and_checked() {
    let dir = scratch("t174-check-all");
    let f = write(&dir, "all", &all_texts());
    let out = tiza(&["check", f.to_str().unwrap()]);
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    for want in [
        "document: its title shares text with a masked value",
        "step 1: its title shares text with a masked value",
        "step 1: its id shares text with a masked value",
        "its name shares text with a masked value",
        "its legend shares text with a masked value",
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
    if s.wait_for(lambda out: "itslabelsharestextwithamaskedvalue" in squeezed(out), 10, "the alert"):
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
        readme.contains("shares text with a masked value")
            && readme.contains("random-looking fake values"),
        "README, secrets section"
    );
}

// ---------- review round 1 ----------

/// A document whose code is a file next to it: saved, it keeps the
/// author's texts (the value is read from the file again).
fn file_backed(dir: &Path) -> PathBuf {
    std::fs::write(
        dir.join("demo.sh"),
        format!("# demo\nTOKEN={VALUE}\nUSER=ana\n"),
    )
    .unwrap();
    let mut d: Value = serde_json::from_str(&std::fs::read_to_string(demo()).unwrap()).unwrap();
    d["code"] = json!({"file": "demo.sh"});
    write(dir, "demo.sh", &d)
}

#[test]
fn b1_saving_an_embedded_document_masks_its_texts_and_reopening_shows_no_value() {
    let dir = scratch("t174-b1-save");
    let path = dir.join("demo.tiza");
    std::fs::copy(demo(), &path).unwrap();
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    // Any change autosaves.
    live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 1, "text": "plain"}),
    );
    drop(live);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert_clean("the autosaved .tiza", &saved);
    assert!(saved.contains(&format!("was {MASK}")), "{saved}");
    let (txt, err) = out_of(&["render", path.to_str().unwrap(), "--format", "txt"]);
    assert_clean(
        "render of the reopened file",
        &String::from_utf8_lossy(&txt),
    );
    assert_clean("its stderr", &err);
    let (svg, _) = out_of(&[
        "render",
        path.to_str().unwrap(),
        "--format",
        "svg",
        "--no-embed-fonts",
    ]);
    assert_clean("svg of the reopened file", &String::from_utf8_lossy(&svg));

    // A file reference keeps the author's texts: the value is in the file.
    let fb = file_backed(&dir);
    let doc = tiza_core::fs::open(fb.to_str().unwrap()).unwrap().document;
    let save = dir.join("fb.tiza");
    tiza_core::fs::save_document(&doc, &save).unwrap();
    let saved = std::fs::read_to_string(&save).unwrap();
    assert!(saved.contains(&format!("was {VALUE}")), "{saved}");
}

#[test]
fn b1_scene_get_then_put_keeps_the_values_in_memory() {
    // A value of 8 characters or fewer: the masked copy shows no tail.
    let short = "Ab3x9Kq";
    let doc = json!({
        "schema": 1, "source": {"language": "sh"},
        "code": ["# demo", format!("PIN={short}")],
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 4, "end": 11}},
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 1, "start": 2, "end": 6}, "label": format!("was {short}")}
        ]
    });
    let live = live_of(&doc, "t174-b1-put");
    live.with(|a| a.set_step(1));
    let scene = live.api_ok("scene.get", json!({}));
    assert_eq!(scene["document"]["code"][1], json!(format!("PIN={MASK}")));
    live.api_ok("scene.put", json!({"document": scene["document"]}));
    let text = live.api_ok("render.text", json!({"width": 100}));
    let text = text["text"].as_str().unwrap();
    assert!(text.contains(&format!("was {MASK}")), "{text}");
    assert!(!text.contains(short) && !text.contains("3x9K"), "{text}");
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(!list.contains("3x9K"), "{list}");
    let check = live.api_ok("check", json!({})).to_string();
    assert!(
        check.contains("masked_text") && !check.contains("3x9K"),
        "{check}"
    );
    // Never written: the values live in memory only.
    let saved = std::fs::read_to_string(live.with(|a| a.save_path.clone())).unwrap();
    assert!(!saved.contains("3x9K"), "{saved}");
    // Debug of the document names how many, never which.
    let known = format!("{:?}", live.with(|a| a.doc.known_values.clone()));
    assert_eq!(known, "KnownValues(1 values)");
}

fn glyphs_and_ids() -> Value {
    json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": ["# demo", format!("TOKEN={VALUE}"), "USER=ana", "PORT=1"],
        "styles": {"secretish": {"glyph": "K7wP", "legend": "plain legend", "color": "red"}},
        "steps": [{"step": 1, "title": "one", "x9vK7w": true}],
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 6, "end": 16}},
            {"id": "m1", "step": 1, "type": "mark", "kind": "secretish", "span": {"line": 3, "start": 0, "end": 4}},
            {"id": "m2", "step": 1, "type": "mark", "kind": "read", "span": {"line": 4, "start": 0, "end": 4}, "style": {"glyph": "wPx3"}},
            {"id": format!("n{VALUE}"), "step": 1, "type": "note", "line": 3, "text": "plain"},
            {"id": "s1", "step": 1, "type": "spotlight", "ids": [format!("n{VALUE}")]},
            {"id": "out7wPx3", "step": 1, "type": "note", "line": 99, "text": "outside"}
        ]
    })
}

#[test]
fn b2_glyphs_go_through_the_filter() {
    let dir = scratch("t174-b2");
    let f = write(&dir, "glyphs", &glyphs_and_ids());
    let (txt, err) = out_of(&[
        "render",
        f.to_str().unwrap(),
        "--format",
        "txt",
        "--step",
        "1",
    ]);
    let txt = String::from_utf8_lossy(&txt).into_owned();
    assert_clean("render with glyphs", &txt);
    assert_clean("render stderr", &err);
    assert!(
        err.contains("its glyph shares text with a masked value"),
        "{err}"
    );
    let report =
        String::from_utf8_lossy(&tiza(&["check", f.to_str().unwrap()]).stdout).into_owned();
    assert!(
        report.contains("style secretish: its glyph shares text"),
        "{report}"
    );
    assert!(
        report.contains("m2, step 1: its glyph shares text"),
        "{report}"
    );
    assert_clean("check", &report);
    let live = live_of(&glyphs_and_ids(), "t174-b2-live");
    live.with(|a| a.set_step(1));
    for (m, p) in [
        ("render.text", json!({"width": 100})),
        ("style.list", json!({})),
        ("scene.get", json!({"as_shown": true})),
    ] {
        assert_clean(m, &live.api_ok(m, p).to_string());
    }
    assert_clean("the screen", &live.screen(120, 30));
}

#[test]
fn a2_ids_are_refused_from_agents_and_masked_from_the_file() {
    let live = live_of(&glyphs_and_ids(), "t174-a2");
    live.with(|a| a.set_step(1));
    // An agent's new id: refused, not quoted.
    for (m, p) in [
        (
            "annotation.add",
            json!({"type": "note", "line": 1, "text": "x", "id": format!("a{VALUE}")}),
        ),
        (
            "annotation.add",
            json!({"type": "note", "line": 1, "text": "x", "id": "z7wPx3"}),
        ),
        (
            "scene.patch",
            json!({"annotations": [{"id": "q9vK7", "type": "note", "step": 1, "line": 1, "text": "x"}]}),
        ),
    ] {
        let r = live.api(m, p);
        let e = r.error.unwrap_or_else(|| panic!("{m} accepted the id"));
        assert_eq!(e.code, "invalid_argument");
        assert!(
            e.message
                .contains("this id shares text with a masked value"),
            "{}",
            e.message
        );
        assert_clean(m, &e.message);
    }
    // Ids from the file: masked wherever ids are shown, and said.
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(list.contains(&format!("{MASK} [step 1] note L3")), "{list}");
    assert!(list.contains(&format!("spotlight [{MASK}]")), "{list}");
    assert_clean("scene.list", &list);
    assert_clean(
        "scene.get as_shown",
        &live
            .api_ok("scene.get", json!({"as_shown": true}))
            .to_string(),
    );
    let status = live.api_ok("status", json!({})).to_string();
    assert!(
        status.contains("its id shares text with a masked value"),
        "{status}"
    );
    assert_clean("status", &status);
    assert_clean("the screen", &live.screen(120, 30));
    live.keys(&[crossterm::event::KeyCode::Char('L')]);
    assert_clean("the :list overlay", &live.screen(120, 30));
    let mut mcp = McpClient::start(&live.socket);
    let v = mcp.request("resources/read", json!({"uri": "tiza://scene"}));
    assert_clean("tiza://scene", &v.to_string());
    let (_, text, raw) = mcp.call("list_annotations", json!({}));
    assert_clean("list_annotations", &text);
    assert_clean("list_annotations raw", &raw.to_string());
    // The CLI: the outside notice («not drawn: ID»), render and check.
    let dir = scratch("t174-a2-cli");
    let f = write(&dir, "ids", &glyphs_and_ids());
    let (_, err) = out_of(&["render", f.to_str().unwrap(), "--format", "txt"]);
    assert!(err.contains("not drawn"), "{err}");
    assert_clean("render stderr", &err);
    let report = String::from_utf8_lossy(&tiza(&["check", f.to_str().unwrap(), "--json"]).stdout)
        .into_owned();
    assert!(report.contains("masked_text"), "{report}");
    assert_clean("check --json", &report);
}

#[test]
fn a2_ids_in_the_list_overlay_in_a_pty() {
    let dir = scratch("t174-a2-pty");
    let doc = write(&dir, "ids", &glyphs_and_ids());
    let sock = dir.join("live.sock");
    let out = tiza_qa::pty_script(
        &dir,
        "list.py",
        r##"
binary, doc, sock = sys.argv[1], sys.argv[2], sys.argv[3]
pieces = sys.argv[4].split(",")
squeezed = lambda out: plain(out).replace(" ", "")
s = Pty([binary, "--socket", sock, "--no-save", doc], 30, 140)
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists(sock), 10, "first frame"):
    s.send(b"n")
    s.settle(0.3)
    mark = len(s.out)
    s.send(b"L")
    if s.wait_for(lambda out: "Annotations" in squeezed(out[mark:]), 10, "the list"):
        print("LISTED", flush=True)
    s.send(b"\x1b")
    s.settle(0.2)
    # Tab completion on `:style ` and `:del `: never types a masked name.
    for cmd in [b":style ", b":del "]:
        s.send(cmd)
        s.settle(0.2)
        s.send(b"\t")
        s.settle(0.3)
        s.send(b"\x1b")
        s.settle(0.2)
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
    for want in ["LISTED", "RAW_CLEAN"] {
        assert!(out.contains(want), "{want}:\n{out}");
    }
}

#[test]
fn b4_tab_completion_never_offers_a_masked_name() {
    let mut doc = glyphs_and_ids();
    doc["styles"][format!("k{VALUE}")] = json!({"glyph": "!", "legend": "x"});
    let live = live_of(&doc, "t174-b4");
    let (line, cands) = live.with(|a| a.complete("style "));
    assert_clean("completion", &format!("{line} {cands:?}"));
    assert!(
        line.contains("secretish") || cands.iter().any(|c| c == "secretish"),
        "{line} {cands:?}"
    );
    let (line, cands) = live.with(|a| a.complete("del "));
    assert_clean("id completion", &format!("{line} {cands:?}"));
}

#[test]
fn b5_unknown_keys_of_steps_are_shown_text() {
    let live = live_of(&glyphs_and_ids(), "t174-b5");
    let status = live.api_ok("status", json!({}));
    let warnings = status["warnings"].to_string();
    assert!(warnings.contains("unknown key"), "{warnings}");
    assert_clean("status.warnings", &warnings);
    let dir = scratch("t174-b5-cli");
    let f = write(&dir, "steps", &glyphs_and_ids());
    let (_, err) = out_of(&["render", f.to_str().unwrap(), "--format", "txt"]);
    assert!(err.contains("unknown key"), "{err}");
    assert_clean("render stderr", &err);
}

#[test]
fn b6_style_set_and_define_style_reply_masked() {
    let live = live_of(&all_texts(), "t174-b6");
    let r = live.api_ok(
        "style.set",
        json!({"name": "warnish", "glyph": "!", "legend": format!("old {VALUE}")}),
    );
    assert_eq!(r["masked"], json!(true), "{r}");
    assert!(
        r["masked_notice"]
            .as_str()
            .unwrap()
            .contains("its legend shares text with a masked value; saved masked — reword it"),
        "{r}"
    );
    assert_clean("style.set reply", &r.to_string());
    let mut mcp = McpClient::start(&live.socket);
    let (err, text, raw) = mcp.call(
        "define_style",
        json!({"name": "other", "glyph": "?", "legend": format!("x {VALUE}")}),
    );
    assert!(!err, "{raw}");
    assert!(
        text.contains("its legend shares text with a masked value"),
        "{text}"
    );
    assert_clean("define_style", &raw.to_string());
}

#[test]
fn b7_move_targets_are_elided_in_move_list_and_the_list_overlay() {
    let mut doc = all_texts();
    doc["moves"] = json!([{"at_step": 1, "action": "code.patch", "op": "replace", "find": "USER=ana", "content": "USER=bob"}]);
    let live = live_of(&doc, "t174-b7");
    let list = live.api_ok("move.list", json!({}));
    let summary = list["moves"][0]["summary"].as_str().unwrap().to_string();
    assert_eq!(summary, "#1 [step 1] code.patch replace «…» 1 line");
    live.keys(&[crossterm::event::KeyCode::Char('L')]);
    let screen = live.screen(140, 40);
    assert!(screen.contains("code.patch replace «…»"), "{screen}");
    // Without redacts the target is quoted as before.
    let mut plain = doc.clone();
    plain["annotations"] = json!([]);
    let live = live_of(&plain, "t174-b7-plain");
    let list = live.api_ok("move.list", json!({}));
    assert_eq!(
        list["moves"][0]["summary"],
        json!("#1 [step 1] code.patch replace «USER=ana» 1 line")
    );
}

#[test]
fn a5_a_copy_written_by_a_move_is_said_line_by_line() {
    let f = fixture("t-174/conmove.tiza");
    let (txt, err) = out_of(&["render", f.to_str().unwrap(), "--step", "1", "--out", "-"]);
    assert!(
        err.contains("tiza: redact r1: masked a copy of its value on L7 (move #1)"),
        "{err}"
    );
    assert!(
        !err.contains("could not follow"),
        "the redact did follow its text: {err}"
    );
    assert_clean("stderr", &err);
    assert_clean("render", &String::from_utf8_lossy(&txt));
}

#[test]
fn a3_a_message_keeps_its_own_words_and_masks_document_text() {
    // A value that holds an ordinary word (`value`): Tiza's own words in a
    // notice stay; an id or a whole copy goes.
    let d = tiza_core::fs::parse_document(
        &json!({"schema": 1, "source": {"language": "txt"}, "code": ["k=LONGERvalue77"],
                "annotations": [{"id": "r1", "step": 1, "type": "redact", "span": {"line": 1, "start": 2, "end": 15}}]})
        .to_string(),
        None,
    )
    .unwrap();
    let m = tiza_core::redact::shown_message;
    assert_eq!(
        m("redact r1: masked a copy of its value on L7 (move #1)", &d),
        "redact r1: masked a copy of its value on L7 (move #1)"
    );
    assert_eq!(
        m("a1: copy LONGERvalue77 here", &d),
        format!("a1: copy {MASK} here")
    );
    assert_eq!(m("idERvalu7 is gone", &d), format!("{MASK} is gone"));
    // Document text stays fully fail closed: the word goes.
    assert_eq!(
        tiza_core::redact::shown("the value here", &d),
        format!("the {MASK} here")
    );
}

// ---------- AC10 (t-177): every write of embedded code, round trips ----------

/// Every step as text, file by file, from `tiza render --all-steps`.
fn all_steps(path: &Path, dir: &Path, name: &str) -> Vec<String> {
    let out = dir.join(name);
    let _ = std::fs::remove_dir_all(&out);
    out_of(&[
        "render",
        path.to_str().unwrap(),
        "--all-steps",
        "--format",
        "txt",
        "--width",
        "100",
        "--out",
        &format!("{}/", out.display()),
    ]);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect()
}

/// Every reader and export of AC3 on a reopened file, searched for any
/// piece of `value`.
fn reopened_is_clean(path: &Path, value: &str, name: &str) {
    let c: Vec<char> = value.chars().collect();
    let pieces: Vec<String> = c.windows(4).map(|w| w.iter().collect()).collect();
    let clean = |what: &str, s: &str| {
        for p in &pieces {
            assert!(!s.contains(p.as_str()), "{what} shows «{p}»:\n{s}");
        }
    };
    let saved = std::fs::read_to_string(path).unwrap();
    clean("the saved .tiza", &saved);
    let f = path.to_str().unwrap();
    for fmt in ["txt", "ansi", "md", "cast", "tape"] {
        let (out, err) = out_of(&["render", f, "--format", fmt]);
        clean(&format!("render {fmt}"), &String::from_utf8_lossy(&out));
        clean(&format!("render {fmt} stderr"), &err);
    }
    let (svg, _) = out_of(&["render", f, "--format", "svg", "--no-embed-fonts"]);
    clean("render svg", &String::from_utf8_lossy(&svg));
    let report = String::from_utf8_lossy(&tiza(&["check", f, "--json"]).stdout).into_owned();
    clean("check", &report);
    let doc = tiza_core::fs::open(f).unwrap().document;
    let dir = scratch(name);
    let live = Live::open(doc, dir.join("re.tiza"), dir.join("live.sock"));
    let max = live.with(|a| a.doc.max_step());
    for step in 0..=max {
        live.with(move |a| a.set_step(step));
        for (m, p) in [
            ("render.text", json!({"width": 100})),
            ("code.get", json!({})),
            ("scene.list", json!({})),
            ("status", json!({})),
            ("scene.get", json!({})),
            ("scene.get", json!({"as_shown": true})),
        ] {
            clean(
                &format!("{m} at step {step}"),
                &live.api_ok(m, p).to_string(),
            );
        }
        clean(&format!("the screen at step {step}"), &live.screen(120, 30));
    }
    let mut mcp = McpClient::start(&live.socket);
    for uri in [
        "tiza://scene",
        "tiza://render",
        "tiza://render.md",
        "tiza://code",
    ] {
        clean(
            uri,
            &mcp.request("resources/read", json!({"uri": uri}))
                .to_string(),
        );
    }
}

#[test]
fn ac10_autosave_of_the_card_fixture_round_trips_identical_and_clean() {
    let dir = scratch("t174-ac10-autosave");
    let path = dir.join("demo.tiza");
    std::fs::copy(demo(), &path).unwrap();
    let before = all_steps(&path, &dir, "before");
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    // Not written yet: the file still holds the value in clear.
    let _ = live.screen(120, 30);
    let alerts = live.with(|a| a.redact_alerts.clone());
    assert!(alerts.contains(&LIVE_ALERT.to_string()), "{alerts:?}");
    assert!(
        !alerts.iter().any(|a| a.contains("saved masked")),
        "{alerts:?}"
    );
    // An autosave: a change that leaves every frame as it was. Never
    // silent: once written, the alert says it was saved masked.
    live.api_ok("style.set", json!({"name": "unused", "glyph": "*"}));
    let _ = live.screen(120, 30);
    assert!(
        live.with(|a| a.redact_alerts.clone())
            .contains(&LABEL_ALERT.to_string()),
        "the saved-masked alert"
    );
    drop(live);
    assert_eq!(
        all_steps(&path, &dir, "after"),
        before,
        "reopened = shown before"
    );
    reopened_is_clean(&path, VALUE, "t174-ac10-autosave-re");
}

/// t-177's repro: a file-reference document whose move writes
/// `BACKUP=<value>` at step 4, with a label repeating the value.
fn embed_probe(dir: &Path) -> PathBuf {
    std::fs::write(
        dir.join("main.env"),
        format!("# main\nTOKEN={VALUE}\nHOST=db\nPORT=8080\n"),
    )
    .unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": {"file": "main.env"},
        "annotations": [
            {"id": "r1", "step": 0, "type": "redact", "span": {"line": 2, "start": 6, "end": 16}},
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 1, "start": 2, "end": 6}, "label": format!("was {VALUE}")},
            {"id": "n2", "step": 2, "type": "note", "line": 3, "text": "the host"},
            {"id": "n3", "step": 3, "type": "note", "line": 4, "text": "the port"}
        ],
        "moves": [
            {"at_step": 4, "action": "code.patch", "op": "insert", "after": "PORT=8080", "content": format!("BACKUP={VALUE}")}
        ]
    });
    write(dir, "doc", &doc)
}

#[test]
fn ac10_embed_round_trips_identical_and_clean_t177() {
    let dir = scratch("t174-ac10-embed");
    let path = embed_probe(&dir);
    let before = all_steps(&path, &dir, "before");
    assert!(
        before[4].contains(&format!("BACKUP={MASK}")),
        "live masks the copy:\n{}",
        before[4]
    );
    // A file reference keeps the author's text as written.
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains(&format!("BACKUP={VALUE}")));
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    let _ = live.screen(120, 30);
    // On screen only, while the code is a file reference.
    assert!(
        live.with(|a| a.redact_alerts.clone()).contains(
            &"h1: its label shares text with a masked value; masked on screen — reword the label"
                .to_string()
        ),
        "{:?}",
        live.with(|a| a.redact_alerts.clone())
    );
    live.command("embed");
    let msg = live.message().unwrap().0;
    assert!(
        msg.contains("embedded: masked 1 copy of a redacted value in move #1"),
        "{msg}"
    );
    assert_clean("the :embed message", &msg);
    let alerts = live.with(|a| a.redact_alerts.clone());
    assert!(alerts.contains(&LABEL_ALERT.to_string()), "{alerts:?}");
    drop(live);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains(&format!("BACKUP={MASK}")), "{saved}");
    assert_eq!(
        all_steps(&path, &dir, "after"),
        before,
        "reopened = shown before"
    );
    reopened_is_clean(&path, VALUE, "t174-ac10-embed-re");
    // Re-embedding is idempotent.
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, path.clone(), dir.join("live2.sock"));
    live.command("embed");
    drop(live);
    assert_eq!(all_steps(&path, &dir, "again"), before);
}

#[test]
fn ac10_live_get_put_then_save_round_trips_for_a_short_value() {
    let short = "Ab3x9Kq";
    let dir = scratch("t174-ac10-put");
    let path = write(
        &dir,
        "short",
        &json!({
            "schema": 1, "source": {"language": "sh"},
            "code": ["# demo", format!("PIN={short}"), "USER=ana"],
            "annotations": [
                {"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 4, "end": 11}},
                {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 1, "start": 2, "end": 6}, "label": format!("was {short}")},
                {"id": "n1", "step": 1, "type": "note", "line": 3, "text": format!("pin {short} rotated")}
            ],
            "moves": [{"at_step": 2, "action": "code.patch", "op": "insert", "after": "USER=ana", "content": format!("OLD={short}")}]
        }),
    );
    let before = all_steps(&path, &dir, "before");
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    let scene = live.api_ok("scene.get", json!({}));
    live.api_ok("scene.put", json!({"document": scene["document"]}));
    // Saved after the put (autosave), and once more by hand.
    live.command("w");
    drop(live);
    assert_eq!(
        all_steps(&path, &dir, "after"),
        before,
        "reopened = shown before"
    );
    reopened_is_clean(&path, short, "t174-ac10-put-re");
}

// ---------- review round 2 ----------

/// A value whose pieces are ordinary-looking words (`ffee`, `beef`).
const WORDY: &str = "9f2c0ffeebeef417";

#[track_caller]
fn assert_no_wordy(what: &str, s: &str) {
    let c: Vec<char> = WORDY.chars().collect();
    for w in c.windows(4) {
        let p: String = w.iter().collect();
        assert!(!s.contains(&p), "{what} shows «{p}» of the value:\n{s}");
    }
}

fn wordy_doc() -> Value {
    json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": ["# demo", format!("KEY={WORDY}"), "USER=ana", "PORT=1"],
        "steps": [
            {"step": 1, "id": "s-ffee", "title": "one"},
            {"step": 2, "id": "t-beef", "title": "two"}
        ],
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 4, "end": 20}},
            {"id": "n1", "step": 1, "type": "note", "line": 3, "text": "plain"},
            {"id": "n2", "step": 2, "type": "note", "line": 4, "text": "plain too"},
            {"id": "beef", "step": 2, "type": "note", "line": 99, "text": "outside"},
            {"id": "sp", "step": 1, "type": "spotlight", "ids": ["ffee-beef"]}
        ]
    })
}

/// Opens `doc` in a live instance saving to `dir/name.tiza`, autosaves
/// once with a change that leaves every frame as it was, and closes it.
fn autosave(dir: &Path, name: &str, doc: &Value) -> PathBuf {
    let path = write(dir, name, doc);
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join(format!("{name}.sock")));
    live.api_ok("style.set", json!({"name": "unused", "glyph": "*"}));
    drop(live);
    path
}

#[test]
fn r2_1_masked_step_ids_stay_unique_and_goto_still_works() {
    let dir = scratch("t174-r2-1");
    let path = autosave(&dir, "steps", &wordy_doc());
    let saved = std::fs::read_to_string(&path).unwrap();
    assert_no_wordy("the saved .tiza", &saved);
    // It opens again, and its steps are reachable by their masked ids.
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap_or_else(|e| panic!("the saved file does not open: {e}"))
        .document;
    let live = Live::open(doc, dir.join("re.tiza"), dir.join("re.sock"));
    let ids: Vec<String> = live.with(|a| a.doc.steps.iter().filter_map(|n| n.id.clone()).collect());
    // The whole token goes (`s-ffee` → `••••••••`), so they are numbered.
    assert_eq!(ids, [MASK.to_string(), format!("{MASK}-2")], "{ids:?}");
    live.api_ok("step.goto", json!({"id": format!("{MASK}-2")}));
    assert_eq!(live.api_ok("status", json!({}))["step"], json!(2));
    live.api_ok("step.goto", json!({"id": MASK}));
    live.api_ok("step.next", json!({}));
    assert_eq!(live.api_ok("status", json!({}))["step"], json!(2));
    // Two ids masked to the same text are kept apart.
    let mut doc = wordy_doc();
    doc["steps"] = json!([{"step": 1, "id": "ffee"}, {"step": 2, "id": "beef"}]);
    let path = autosave(&dir, "same", &doc);
    let reopened = tiza_core::fs::open(path.to_str().unwrap());
    assert!(reopened.is_ok(), "{:?}", reopened.err());
    let ids: Vec<String> = reopened
        .unwrap()
        .document
        .steps
        .iter()
        .filter_map(|n| n.id.clone())
        .collect();
    assert_eq!(ids, [MASK.to_string(), format!("{MASK}-2")]);
}

#[test]
fn r2_2_document_words_in_messages_are_masked() {
    let dir = scratch("t174-r2-2");
    let f = write(&dir, "wordy", &wordy_doc());
    let (_, err) = out_of(&["render", f.to_str().unwrap(), "--format", "txt"]);
    assert!(err.contains("not drawn"), "{err}");
    assert_no_wordy("render stderr", &err);
    for args in [
        vec!["check", f.to_str().unwrap()],
        vec!["check", f.to_str().unwrap(), "--json"],
    ] {
        let out = tiza(&args);
        let report = String::from_utf8_lossy(&out.stdout).into_owned()
            + &String::from_utf8_lossy(&out.stderr);
        assert!(
            report.contains("does not exist") || report.contains("unknown"),
            "{report}"
        );
        assert_no_wordy("tiza check", &report);
    }
    // The message's own words stay.
    assert!(
        err.contains("annotations outside the code (lines 1-4) and not drawn"),
        "{err}"
    );
}

#[test]
fn r2_3_a_reference_to_a_missing_id_is_masked_and_saved_masked() {
    let dir = scratch("t174-r2-3");
    let live = live_of(&wordy_doc(), "t174-r2-3-live");
    live.with(|a| a.set_step(1));
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(list.contains(&format!("spotlight [{MASK}]")), "{list}");
    assert_no_wordy("scene.list", &list);
    let sel = live.with(|a| {
        a.selected = Some("sp".into());
        a.selected_text()
    });
    assert_no_wordy("the selection line", &format!("{sel:?}"));
    assert_no_wordy(
        "as_shown",
        &live
            .api_ok("scene.get", json!({"as_shown": true}))
            .to_string(),
    );
    let path = autosave(&dir, "missing", &wordy_doc());
    assert_no_wordy("the saved .tiza", &std::fs::read_to_string(&path).unwrap());
}

/// A document with moves whose redacted line has a highlight after the
/// value, for a value shorter and one longer than the mask.
fn shifted_doc(value: &str) -> Value {
    let line = format!("TOKEN={value} # tail");
    let tail = line.chars().count() as u32 - 4;
    json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": ["# demo", line, "X=1"],
        "annotations": [
            {"id": "r1", "step": 0, "type": "redact", "span": {"line": 2, "start": 6, "end": 6 + value.chars().count() as u32}},
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 2, "start": tail, "end": tail + 4}, "label": "the tail"},
            {"id": "m1", "step": 2, "type": "mark", "kind": "warn", "span": {"line": 2, "start": tail, "end": tail + 4}}
        ],
        "moves": [{"at_step": 2, "action": "code.patch", "op": "replace", "find": "X=1", "content": "X=2"}]
    })
}

fn all_steps_as(path: &Path, dir: &Path, name: &str, fmt: &str) -> Vec<String> {
    let out = dir.join(name);
    let _ = std::fs::remove_dir_all(&out);
    let mut args = vec![
        "render".to_string(),
        path.to_str().unwrap().to_string(),
        "--all-steps".into(),
        "--format".into(),
        fmt.into(),
        "--width".into(),
        "100".into(),
        "--out".into(),
        format!("{}/", out.display()),
    ];
    if fmt == "svg" {
        args.push("--no-embed-fonts".into());
    }
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    out_of(&a);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect()
}

#[test]
fn r2_4_annotations_after_the_value_keep_their_place_with_moves() {
    for value in ["Ab3x9", "Zq8wR4tY7uV2"] {
        let dir = scratch(&format!("t174-r2-4-{}", value.len()));
        let path = write(&dir, "shift", &shifted_doc(value));
        let before: Vec<Vec<String>> = ["ansi", "svg"]
            .iter()
            .map(|f| all_steps_as(&path, &dir, &format!("b-{f}"), f))
            .collect();
        autosave(&dir, "shift", &shifted_doc(value));
        let after: Vec<Vec<String>> = ["ansi", "svg"]
            .iter()
            .map(|f| all_steps_as(&path, &dir, &format!("a-{f}"), f))
            .collect();
        assert_eq!(
            after,
            before,
            "value of {}: save and reopen moved something",
            value.len()
        );
        // The same through scene.get → scene.put → save.
        let p2 = write(&dir, "put", &shifted_doc(value));
        let d = tiza_core::fs::open(p2.to_str().unwrap()).unwrap().document;
        let live = Live::open(d, p2.clone(), dir.join("put.sock"));
        let scene = live.api_ok("scene.get", json!({}));
        live.api_ok("scene.put", json!({"document": scene["document"]}));
        live.command("w");
        drop(live);
        let after_put: Vec<Vec<String>> = ["ansi", "svg"]
            .iter()
            .map(|f| all_steps_as(&p2, &dir, &format!("p-{f}"), f))
            .collect();
        assert_eq!(
            after_put,
            before,
            "value of {}: get → put moved something",
            value.len()
        );
    }
}

/// A document whose code is embedded, with a linked file frozen in `views`
/// (in clear, as a hand-written file may hold it) and a redact on it.
fn views_doc(shown: bool) -> Value {
    let mut moves = vec![
        json!({"at_step": 2, "action": "code.patch", "op": "replace", "find": "USER=ana", "content": "USER=bob"}),
    ];
    if shown {
        moves.push(json!({"at_step": 3, "action": "code.link", "file": "lib.env"}));
    }
    json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": ["# main", "USER=ana"],
        "views": {"lib.env": ["# lib", format!("LIBKEY={VALUE}"), "MODE=x"]},
        "annotations": [
            {"id": "r2", "step": 0, "type": "redact", "file": "lib.env", "span": {"line": 2, "start": 7, "end": 17}},
            {"id": "n9", "step": 1, "type": "note", "file": "lib.env", "line": 3, "text": format!("was {VALUE}")}
        ],
        "moves": moves
    })
}

#[test]
fn r2_5_views_are_saved_masked() {
    let dir = scratch("t174-r2-5");
    let path = autosave(&dir, "views", &views_doc(true));
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains(&format!("LIBKEY={MASK}")), "{saved}");
    assert_clean("the saved .tiza", &saved);
}

#[test]
fn r2_6_a_note_of_a_linked_file_no_step_shows_is_saved_masked() {
    let dir = scratch("t174-r2-6");
    let path = autosave(&dir, "unshown", &views_doc(false));
    assert_clean("the saved .tiza", &std::fs::read_to_string(&path).unwrap());
}

#[test]
fn r2_7_embed_with_a_linked_file_reopens_clean_and_checks() {
    let dir = scratch("t174-r2-7");
    std::fs::write(dir.join("main.env"), "# main\nUSER=ana\n").unwrap();
    std::fs::write(
        dir.join("lib.env"),
        format!("# lib\nLIBKEY={VALUE}\nMODE=x\n"),
    )
    .unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": {"file": "main.env"},
        "annotations": [
            {"id": "r2", "step": 0, "type": "redact", "file": "lib.env", "span": {"line": 2, "start": 7, "end": 17}},
            {"id": "n1", "step": 1, "type": "note", "line": 2, "text": "the user"}
        ],
        "moves": [{"at_step": 2, "action": "code.link", "file": "lib.env"}]
    });
    let path = write(&dir, "doc", &doc);
    let before = all_steps(&path, &dir, "before");
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    live.command("embed");
    drop(live);
    let out = tiza(&["check", path.to_str().unwrap()]);
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{report}");
    assert!(!report.contains("does not fit"), "{report}");
    let (_, err) = out_of(&["render", path.to_str().unwrap(), "--format", "txt"]);
    assert!(!err.contains("not drawn"), "{err}");
    assert_eq!(
        all_steps(&path, &dir, "after"),
        before,
        "reopened = shown before"
    );
    assert_clean("the saved .tiza", &std::fs::read_to_string(&path).unwrap());
}

#[test]
fn r2_8_a_copy_alone_is_no_widened() {
    let f = fixture("t-174/conmove.tiza");
    let doc = tiza_core::fs::open(f.to_str().unwrap()).unwrap().document;
    let dir = scratch("t174-r2-8");
    let live = Live::open(doc, dir.join("c.tiza"), dir.join("c.sock"));
    live.with(|a| a.set_step(1));
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(list.contains("r1 [step 0] redact L1"), "{list}");
    assert!(!list.contains("WIDENED"), "only a copy: {list}");
    live.keys(&[crossterm::event::KeyCode::Char('L')]);
    assert!(!live.screen(120, 30).contains("WIDENED"));
}

#[test]
fn r2_9_one_alert_per_masked_id_and_each_copy_with_its_move() {
    // Two ids masked to the same text: two alerts.
    let mut doc = glyphs_and_ids();
    doc["annotations"].as_array_mut().unwrap().push(
        json!({"id": format!("x{VALUE}"), "step": 1, "type": "note", "line": 4, "text": "plain"}),
    );
    let live = live_of(&doc, "t174-r2-9");
    let _ = live.screen(120, 30);
    let alerts = live.with(|a| a.redact_alerts.clone());
    let ids: Vec<&String> = alerts
        .iter()
        .filter(|a| a.contains("its id shares text"))
        .collect();
    assert_eq!(ids.len(), 3, "{alerts:?}");
    assert!(
        alerts
            .iter()
            .any(|a| a.starts_with(&format!("{MASK}-2: its id"))),
        "{alerts:?}"
    );
    // Copies written by two moves: each named with its own lines.
    let two = json!({
        "schema": 1, "source": {"language": "sh"},
        "code": ["TOKEN=9vK7wPx3Lm", "A=1", "B=2", "C=3"],
        "annotations": [{"id": "r1", "step": 0, "type": "redact", "span": {"line": 1, "start": 6, "end": 16}}],
        "moves": [
            {"at_step": 1, "action": "code.patch", "op": "insert", "after": "A=1", "content": "# old 9vK7wPx3"},
            {"at_step": 2, "action": "code.patch", "op": "insert", "after": "C=3", "content": "# was K7wPx3Lm"}
        ]
    });
    let dir = scratch("t174-r2-9-moves");
    let f = write(&dir, "two", &two);
    let (_, err) = out_of(&[
        "render",
        f.to_str().unwrap(),
        "--all-steps",
        "--format",
        "txt",
        "--out",
        &format!("{}/", dir.join("o").display()),
    ]);
    assert!(
        err.contains("redact r1: masked a copy of its value on L3 (move #1)"),
        "{err}"
    );
    assert!(
        err.contains("redact r1: masked a copy of its value on L6 (move #2)"),
        "{err}"
    );
    assert_clean("stderr", &err);
}

// ---------- review round 3 ----------

#[test]
fn r3_1_a_linked_file_read_from_disk_keeps_its_real_columns() {
    let long = "Zq8wR4tY7uV2mK6YqA8m";
    let dir = scratch("t174-r3-1");
    std::fs::write(
        dir.join("lib.env"),
        format!("# lib\nLIBKEY={long}\nMODE=x\n"),
    )
    .unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": ["# main", "USER=ana"],
        "annotations": [
            {"id": "r2", "step": 0, "type": "redact", "file": "lib.env", "span": {"line": 2, "start": 7, "end": 27}},
            {"id": "n1", "step": 1, "type": "note", "line": 2, "text": "the user"}
        ],
        "moves": [{"at_step": 2, "action": "code.link", "file": "lib.env"}]
    });
    let path = write(&dir, "linked", &doc);
    let before = all_steps(&path, &dir, "before");
    autosave(&dir, "linked", &doc);
    let after = all_steps(&path, &dir, "after");
    assert_eq!(after, before, "reopened = shown before");
    let c: Vec<char> = long.chars().collect();
    for frame in &after {
        for w in c.windows(4) {
            let p: String = w.iter().collect();
            assert!(!frame.contains(&p), "«{p}» shows:\n{frame}");
        }
    }
}

#[test]
fn r3_2_messages_about_a_deleted_annotation_mask_its_id() {
    let live = live_of(&wordy_doc(), "t174-r3-2");
    live.command("del beef");
    let msg = format!("{:?}", live.message());
    assert!(msg.contains("deleted"), "{msg}");
    assert_no_wordy("the delete message", &msg);
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_no_wordy("the undo message", &format!("{:?}", live.message()));
    live.keys(&[crossterm::event::KeyCode::Char('U')]);
    let msg = format!("{:?}", live.message());
    assert!(msg.contains("redone"), "{msg}");
    assert_no_wordy("the redo message", &msg);
    assert_no_wordy("the screen", &live.screen(120, 30));
}

/// Moves that insert a line above the redacted one before the step of the
/// annotations after the value.
fn shifted_by_a_move(value: &str) -> Value {
    let line = format!("TOKEN={value} # tail");
    let tail = line.chars().count() as u32 - 4;
    json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": ["# demo", line, "X=1"],
        "annotations": [
            {"id": "r1", "step": 0, "type": "redact", "span": {"line": 2, "start": 6, "end": 6 + value.chars().count() as u32}},
            {"id": "h1", "step": 2, "type": "highlight", "span": {"line": 3, "start": tail, "end": tail + 4}, "label": "the tail"},
            {"id": "m1", "step": 3, "type": "mark", "kind": "warn", "span": {"line": 3, "start": tail, "end": tail + 4}},
            {"id": "a1", "step": 3, "type": "arrow", "from": {"line": 3, "col": tail}, "to": {"line": 4, "col": 0}}
        ],
        "moves": [
            {"at_step": 1, "action": "code.patch", "op": "insert", "before": "# demo", "content": "# PROD"},
            {"at_step": 3, "action": "code.patch", "op": "replace", "find": "X=1", "content": "X=2"}
        ]
    })
}

#[test]
fn r3_3_annotations_on_lines_moves_shifted_keep_their_place() {
    for value in ["Ab3x9", "Zq8wR4tY7uV2"] {
        let dir = scratch(&format!("t174-r3-3-{}", value.len()));
        let path = write(&dir, "shift", &shifted_by_a_move(value));
        let before: Vec<Vec<String>> = ["ansi", "svg"]
            .iter()
            .map(|f| all_steps_as(&path, &dir, &format!("b-{f}"), f))
            .collect();
        autosave(&dir, "shift", &shifted_by_a_move(value));
        let after: Vec<Vec<String>> = ["ansi", "svg"]
            .iter()
            .map(|f| all_steps_as(&path, &dir, &format!("a-{f}"), f))
            .collect();
        assert_eq!(
            after,
            before,
            "value of {}: save and reopen moved something",
            value.len()
        );
        let p2 = write(&dir, "put", &shifted_by_a_move(value));
        let d = tiza_core::fs::open(p2.to_str().unwrap()).unwrap().document;
        let live = Live::open(d, p2.clone(), dir.join("put.sock"));
        let scene = live.api_ok("scene.get", json!({}));
        live.api_ok("scene.put", json!({"document": scene["document"]}));
        live.command("w");
        drop(live);
        let after_put: Vec<Vec<String>> = ["ansi", "svg"]
            .iter()
            .map(|f| all_steps_as(&p2, &dir, &format!("p-{f}"), f))
            .collect();
        assert_eq!(
            after_put,
            before,
            "value of {}: get → put moved something",
            value.len()
        );
    }
}

#[test]
fn r3_4_masked_style_names_stay_unique() {
    let doc = json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": ["# demo", format!("KEY={WORDY}"), "A=1", "B=2"],
        "styles": {
            "beef": {"glyph": "★", "legend": "first"},
            "coffee": {"glyph": "◆", "legend": "second"}
        },
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 4, "end": 20}},
            {"id": "m1", "step": 1, "type": "mark", "kind": "beef", "span": {"line": 3, "start": 0, "end": 3}},
            {"id": "m2", "step": 1, "type": "mark", "kind": "coffee", "span": {"line": 4, "start": 0, "end": 3}}
        ]
    });
    let live = live_of(&doc, "t174-r3-4");
    live.with(|a| a.set_step(1));
    let text = live.api_ok("render.text", json!({"width": 100}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        text.contains("★ first") && text.contains("◆ second"),
        "{text}"
    );
    assert_no_wordy("render.text", &text);
    let shown = live.api_ok("scene.get", json!({"as_shown": true}));
    assert_eq!(
        shown["document"]["styles"].as_object().unwrap().len(),
        2,
        "{shown}"
    );
    let r = live.api_ok(
        "style.set",
        json!({"name": "coffee", "glyph": "◆", "legend": "second"}),
    );
    let other = live.api_ok(
        "style.set",
        json!({"name": "beef", "glyph": "★", "legend": "first"}),
    );
    assert_ne!(r["name"], other["name"], "{r} {other}");
    assert_ne!(r["masked_notice"], other["masked_notice"], "{r} {other}");
    assert_no_wordy("style.set", &format!("{r} {other}"));
    let dir = scratch("t174-r3-4-save");
    let path = autosave(&dir, "styles", &doc);
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    // Both kept (plus the `unused` one the autosave added).
    let styles = saved["styles"].as_object().unwrap();
    assert!(
        styles.contains_key(MASK) && styles.contains_key(&format!("{MASK}-2")),
        "{saved}"
    );
    let (txt, _) = out_of(&[
        "render",
        path.to_str().unwrap(),
        "--format",
        "txt",
        "--step",
        "1",
    ]);
    let txt = String::from_utf8_lossy(&txt).into_owned();
    assert!(txt.contains("★ first") && txt.contains("◆ second"), "{txt}");
}

// ---------- review round 4 ----------

fn rewritten_line() -> Value {
    json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": ["# a", "# b", format!("TOKEN={VALUE} # tail")],
        "annotations": [
            {"id": "r1", "step": 0, "type": "redact", "span": {"line": 3, "start": 6, "end": 16}},
            {"id": "h1", "step": 2, "type": "highlight", "span": {"line": 3, "start": 19, "end": 23}, "label": "the tail"},
            {"id": "a1", "step": 2, "type": "arrow", "from": {"line": 3, "col": 20}, "to": {"line": 1, "col": 0}}
        ],
        "moves": [{"at_step": 1, "action": "code.patch", "op": "replace", "position": "3", "content": "API=newvalue1234 # tail2"}]
    })
}

#[test]
fn r4_1_annotations_past_a_mask_that_could_not_follow_are_clipped_and_said() {
    let dir = scratch("t174-r4-1");
    let path = write(&dir, "rewrite", &rewritten_line());
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    live.with(|a| a.set_step(0));
    live.keys(&[crossterm::event::KeyCode::Char('n')]);
    let alerts = live.with(|a| a.redact_alerts.clone()).join("\n");
    assert!(
        alerts.contains("could not follow its text through move #1")
            && alerts.contains(
                "fix it before recording; annotations past the mask are clipped when saved"
            ),
        "{alerts}"
    );
    assert_clean("the alert", &alerts);
    live.api_ok("style.set", json!({"name": "unused", "glyph": "*"}));
    drop(live);
    let out = tiza(&["check", path.to_str().unwrap()]);
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "the reopened file checks clean:\n{report}"
    );
    assert!(!report.contains("does not fit"), "{report}");
    assert_clean("the saved .tiza", &std::fs::read_to_string(&path).unwrap());
    let (_, err) = out_of(&[
        "render",
        path.to_str().unwrap(),
        "--all-steps",
        "--format",
        "ansi",
        "--out",
        &format!("{}/", dir.join("o").display()),
    ]);
    assert!(!err.contains("not drawn"), "{err}");
}

#[test]
fn r4_2_an_unknown_kind_never_lands_on_a_masked_style() {
    let doc = json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": ["# demo", format!("KEY={WORDY}"), "A=1", "B=2"],
        "styles": {"beef": {"glyph": "★", "legend": "first"}},
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 4, "end": 20}},
            {"id": "m1", "step": 1, "type": "mark", "kind": "beef", "span": {"line": 3, "start": 0, "end": 3}},
            {"id": "m2", "step": 1, "type": "mark", "kind": "ffee-x", "span": {"line": 4, "start": 0, "end": 3}}
        ]
    });
    let live = live_of(&doc, "t174-r4-2");
    live.with(|a| a.set_step(1));
    let text = live.api_ok("render.text", json!({"width": 100}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(text.matches("★ first").count(), 1, "{text}");
    assert_no_wordy("render.text", &text);
    let shown = live.api_ok("scene.get", json!({"as_shown": true}));
    let kinds: Vec<String> = shown["document"]["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["kind"].as_str().map(String::from))
        .collect();
    assert_eq!(kinds.len(), 2);
    assert_ne!(kinds[0], kinds[1], "{kinds:?}");
    let dir = scratch("t174-r4-2-save");
    let path = autosave(&dir, "kinds", &doc);
    let (txt, _) = out_of(&[
        "render",
        path.to_str().unwrap(),
        "--format",
        "txt",
        "--step",
        "1",
    ]);
    assert_eq!(String::from_utf8_lossy(&txt).matches("★ first").count(), 1);
}

// ---------- rework 1 (QA 1 + QA 2, 2026-10-18 20:20; Luis's decisions A and B) ----------

/// QA's pruebas/t174/p3b/getput_ref.py, rebuilt: a `.tiza` whose code is
/// `main.env` next to it, with a redact, the value in its title, a label
/// and a note, a label after the value on its line and a move.
fn file_ref(dir: &Path, moves: bool) -> PathBuf {
    std::fs::write(
        dir.join("main.env"),
        format!("# main\nTOKEN={VALUE} # tail\nUSER=ana\nPORT=8080\n"),
    )
    .unwrap();
    let mut doc = json!({
        "schema": 1,
        "title": format!("Doc {VALUE}"),
        "source": {"language": "sh"},
        "code": {"file": "main.env"},
        "annotations": [
            {"id": "r1", "step": 0, "type": "redact", "span": {"line": 2, "start": 6, "end": 16}},
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 2, "start": 19, "end": 23}, "label": format!("was {VALUE}")},
            {"id": "n1", "step": 1, "type": "note", "line": 3, "text": format!("rotate {VALUE} soon")}
        ]
    });
    if moves {
        doc["moves"] = json!([
            {"at_step": 2, "action": "code.patch", "op": "insert", "after": "PORT=8080", "content": "HOST=db"}
        ]);
    }
    write(dir, "doc", &doc)
}

#[test]
fn rw1_get_put_of_a_file_reference_keeps_the_reference_and_the_author_text() {
    for moves in [false, true] {
        let dir = scratch(&format!("t174-rw1-ref-{moves}"));
        let path = file_ref(&dir, moves);
        let before = all_steps(&path, &dir, "before");
        let written = std::fs::read_to_string(&path).unwrap();
        let doc = tiza_core::fs::open(path.to_str().unwrap())
            .unwrap()
            .document;
        let live = Live::open(doc, path.clone(), dir.join("live.sock"));
        let scene = live.api_ok("scene.get", json!({}));
        live.api_ok("scene.put", json!({"document": scene["document"]}));
        // Autosaved at once: still the reference, with the author's texts.
        live.command("w");
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["code"], json!({"file": "main.env"}), "{saved:#}");
        let original: Value = serde_json::from_str(&written).unwrap();
        assert_eq!(saved["title"], original["title"], "{saved:#}");
        assert_eq!(saved["annotations"], original["annotations"], "{saved:#}");
        assert!(saved.get("views").is_none(), "{saved:#}");
        // The live instance still shows what it showed.
        let text = live.api_ok("render.text", json!({"width": 100}))["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert_clean("render.text after the put", &text);
        // An edit made by the agent on the copy it got is kept, on the
        // real columns: a label after the mask moved one cell right.
        let mut scene = live.api_ok("scene.get", json!({}));
        let anns = scene["document"]["annotations"].as_array_mut().unwrap();
        let h1 = anns.iter_mut().find(|a| a["id"] == "h1").unwrap();
        h1["label"] = json!("the tail");
        let masked_start = h1["span"]["start"].as_u64().unwrap();
        h1["span"]["start"] = json!(masked_start + 1);
        live.api_ok("scene.put", json!({"document": scene["document"]}));
        live.command("w");
        drop(live);
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["code"], json!({"file": "main.env"}), "{saved:#}");
        let h1 = saved["annotations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == "h1")
            .unwrap()
            .clone();
        assert_eq!(h1["label"], json!("the tail"));
        assert_eq!(
            h1["span"],
            json!({"line": 2, "start": 20, "end": 23}),
            "{h1}"
        );
        assert_eq!(saved["title"], original["title"]);
        // Reopened from the file it still follows: the same frames as before
        // (but the edited label).
        let after = all_steps(&path, &dir, "after");
        assert_eq!(after.len(), before.len());
        assert_eq!(after[0], before[0]);
    }
}

#[test]
fn rw1_get_put_keeps_a_linked_file_read_from_disk_on_its_file() {
    let dir = scratch("t174-rw1-link");
    std::fs::write(dir.join("main.env"), "# main\nUSER=ana\n").unwrap();
    std::fs::write(
        dir.join("lib.env"),
        format!("# lib\nLIBKEY={VALUE} # x\nMODE=x\n"),
    )
    .unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": {"file": "main.env"},
        "annotations": [
            {"id": "r2", "step": 0, "type": "redact", "file": "lib.env", "span": {"line": 2, "start": 7, "end": 17}},
            {"id": "h2", "step": 2, "type": "highlight", "file": "lib.env", "span": {"line": 2, "start": 20, "end": 21}, "label": "the x"},
            {"id": "n2", "step": 2, "type": "note", "file": "lib.env", "line": 3, "text": format!("was {VALUE}")}
        ],
        "moves": [{"at_step": 1, "action": "code.link", "file": "lib.env"}]
    });
    let path = write(&dir, "linked", &doc);
    let before = all_steps(&path, &dir, "before");
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    let scene = live.api_ok("scene.get", json!({}));
    assert!(scene["document"]["views"]["lib.env"].is_array());
    live.api_ok("scene.put", json!({"document": scene["document"]}));
    live.command("w");
    drop(live);
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["code"], json!({"file": "main.env"}), "{saved:#}");
    assert!(
        saved.get("views").is_none(),
        "frozen for the wire only:\n{saved:#}"
    );
    assert_eq!(saved["annotations"], doc["annotations"], "{saved:#}");
    assert_eq!(all_steps(&path, &dir, "after"), before);
}

// Rework point 7 (an adversarial review): with moves, the cache key of the
// masked values did not hold what they were read from at the current step
// (the code in memory, part of a move being typed).
#[test]
fn rw7_values_read_while_a_move_is_typed_never_go_stale() {
    let doc = json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": ["# demo", "USER=ana"],
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 3, "start": 6, "end": 16}},
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 1, "start": 2, "end": 6}, "label": format!("was {VALUE}")}
        ],
        "moves": [
            {"at_step": 1, "action": "code.patch", "op": "insert", "after": "USER=ana",
             "content": format!("TOKEN={VALUE}"), "typing": "char", "speed_ms": 600000}
        ]
    });
    let live = live_of(&doc, "t174-rw7");
    live.with(|a| a.set_step(0));
    assert_clean("step 0", &live.screen(120, 20));
    // `n`: the move starts being typed (it stays at its first character).
    live.keys(&[crossterm::event::KeyCode::Char('n')]);
    assert!(live.with(|a| a.is_typing()));
    // An agent changes the redact while it is typed: the values are read
    // again, on the code typed so far.
    live.api_ok(
        "annotation.update",
        json!({"id": "r1", "label": "the token"}),
    );
    assert_clean("a typing frame", &live.screen(120, 20));
    live.with(|a| a.finish_typing());
    let screen = live.screen(120, 20);
    assert_clean("the screen once typed", &screen);
    assert!(screen.contains(&format!("was {MASK}")), "{screen}");
    for (m, p) in [
        ("render.text", json!({"width": 100})),
        ("scene.list", json!({})),
        ("scene.get", json!({"as_shown": true})),
        ("check", json!({})),
    ] {
        assert_clean(m, &live.api_ok(m, p).to_string());
    }
    live.command("w");
    let saved = std::fs::read_to_string(live.with(|a| a.save_path.clone())).unwrap();
    assert_clean("the saved .tiza", &saved);
}

// Rework point 3: «saved masked» only after a real write.
#[test]
fn rw3_read_only_paths_say_the_file_still_holds_it_in_clear() {
    let dir = scratch("t174-rw3");
    let path = dir.join("demo.tiza");
    std::fs::copy(demo(), &path).unwrap();
    let f = path.to_str().unwrap();
    // `tiza render` and `tiza check` read it; nobody wrote it.
    let (_, err) = out_of(&["render", f, "--format", "txt"]);
    assert!(err.contains(READER_ALERT), "{err}");
    assert!(!err.contains("saved masked"), "{err}");
    let report = String::from_utf8_lossy(&tiza(&["check", f]).stdout).into_owned();
    assert!(
        report.contains(READER_ALERT.split_once(": ").unwrap().1),
        "{report}"
    );
    assert!(
        report
            .contains("warning: r1: the embedded code holds the value under this redact in clear"),
        "{report}"
    );
    assert!(!report.contains("saved masked"), "{report}");
    assert_clean("check", &report);
    // A file nobody saved: the reads changed nothing on disk.
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        std::fs::read_to_string(demo()).unwrap()
    );
    // Opened: the same until the instance writes it, then «saved masked».
    let doc = tiza_core::fs::open(f).unwrap().document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    let _ = live.screen(120, 30);
    let check = live.api_ok("check", json!({})).to_string();
    assert!(
        check.contains("demo.tiza still holds it in clear — open and save it in Tiza"),
        "{check}"
    );
    assert!(
        check.contains("clear_value") && !check.contains("saved masked"),
        "{check}"
    );
    live.with(|a| a.autosave = false);
    let r = live.api_ok(
        "annotation.add",
        json!({"type": "note", "line": 3, "text": format!("old {VALUE}"), "id": "n5"}),
    );
    assert!(
        r["masked_notice"]
            .as_str()
            .unwrap()
            .contains("still holds it in clear"),
        "{r}"
    );
    live.command("w");
    let _ = live.screen(120, 30);
    let alerts = live.with(|a| a.redact_alerts.clone());
    assert!(alerts.contains(&LABEL_ALERT.to_string()), "{alerts:?}");
    let check = live.api_ok("check", json!({})).to_string();
    assert!(
        check.contains("saved masked") && !check.contains("clear_value"),
        "{check}"
    );
    drop(live);
    // A file reference keeps the author's text: masked on screen, and its
    // code is no embedded copy to warn about.
    let fb = file_backed(&dir);
    let report =
        String::from_utf8_lossy(&tiza(&["check", fb.to_str().unwrap()]).stdout).into_owned();
    assert!(
        report.contains("masked on screen — reword the label"),
        "{report}"
    );
    assert!(
        !report.contains("clear_value") && !report.contains("still holds"),
        "{report}"
    );
    // A move written by hand whose content holds the value in clear.
    let mut d = all_texts();
    d["moves"] = json!([{"at_step": 2, "action": "code.patch", "op": "insert", "after": "HOST=db", "content": format!("OLD={VALUE}")}]);
    let p = write(&dir, "moved", &d);
    let report =
        String::from_utf8_lossy(&tiza(&["check", p.to_str().unwrap()]).stdout).into_owned();
    assert!(
        report.contains("warning: move #1: its content holds a masked value in clear; moved.tiza still holds it — open and save it in Tiza"),
        "{report}"
    );
    assert_clean("check of the moves", &report);
    // A file reference written by the instance, then `:embed` without a
    // save: the file is still the reference, which holds the label in clear.
    let probe_dir = scratch("t174-rw3-embed");
    let probe = embed_probe(&probe_dir);
    let doc = tiza_core::fs::open(probe.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, probe.clone(), probe_dir.join("live.sock"));
    live.api_ok("style.set", json!({"name": "unused", "glyph": "*"}));
    live.with(|a| a.autosave = false);
    live.command("embed");
    let _ = live.screen(120, 30);
    let alerts = live.with(|a| a.redact_alerts.clone()).join("\n");
    assert!(
        alerts.contains("doc.tiza still holds it in clear"),
        "{alerts}"
    );
    assert!(!alerts.contains("saved masked"), "{alerts}");
    live.command("w");
    let _ = live.screen(120, 30);
    assert!(
        live.with(|a| a.redact_alerts.clone())
            .contains(&LABEL_ALERT.to_string()),
        "saved masked once written"
    );
}

// Rework point 4: the name of the code file is the document's text.
#[test]
fn rw4_the_code_file_name_goes_through_the_filter() {
    let dir = scratch("t174-rw4");
    let name = "c0ffeebeef.env";
    std::fs::write(dir.join(name), format!("# env\nKEY={WORDY}\nUSER=ana\n")).unwrap();
    // As the main file, and as a linked file a `code.link` shows.
    let main = write(
        &dir,
        "main",
        &json!({
            "schema": 1, "source": {"language": "sh"},
            "code": {"file": name},
            "annotations": [{"id": "r1", "step": 0, "type": "redact", "span": {"line": 2, "start": 4, "end": 20}}]
        }),
    );
    let linked = write(
        &dir,
        "linked",
        &json!({
            "schema": 1, "source": {"language": "sh"},
            "code": ["# main", "USER=bob"],
            "annotations": [{"id": "r1", "step": 0, "type": "redact", "file": name, "span": {"line": 2, "start": 4, "end": 20}}],
            "moves": [{"at_step": 1, "action": "code.link", "file": name}]
        }),
    );
    for (path, step) in [(main, 0), (linked, 1)] {
        let doc = tiza_core::fs::open(path.to_str().unwrap())
            .unwrap()
            .document;
        let live = Live::open(doc, path.clone(), dir.join("live.sock"));
        live.with(move |a| a.set_step(step));
        let status = live.api_ok("status", json!({}));
        assert_no_wordy("status", &status.to_string());
        assert!(
            status["file"].is_string() || status["view"].is_string(),
            "{status}"
        );
        assert_no_wordy("code.get", &live.api_ok("code.get", json!({})).to_string());
        // `move.list` hands each move as written (the editing structure,
        // like `scene.get`); its summary and the view are shown text.
        let list = live.api_ok("move.list", json!({}));
        assert_no_wordy("move.list view", &list["view"].to_string());
        for m in list["moves"].as_array().unwrap() {
            assert_no_wordy("move.list summary", m["summary"].as_str().unwrap());
        }
        let screen = live.screen(120, 20);
        assert_no_wordy("the bar", &screen);
        if step == 1 {
            assert!(screen.contains(&format!("↪ {MASK}")), "{screen}");
        }
        let mut mcp = McpClient::start(&live.socket);
        for tool in ["get_code", "status", "list_annotations"] {
            let (err, text, _) = mcp.call(tool, json!({}));
            assert!(!err, "{tool}: {text}");
            assert_no_wordy(tool, &text);
        }
        assert_no_wordy(
            "tiza://render.md",
            &mcp.request("resources/read", json!({"uri": "tiza://render.md"}))
                .to_string(),
        );
        // The real path stays inside: the file is still read and followed.
        assert!(live.with(|a| a.doc.code.len()) >= 2);
    }
}

// Rework point 5 (Luis's decision A): copies are found without case.
#[test]
fn rw5_copies_are_masked_without_case_in_the_filter_and_when_saved() {
    let value = "hunter2";
    let doc = json!({
        "schema": 1,
        "title": "Rotate HUNTER2",
        "source": {"language": "sh"},
        "code": ["# demo", format!("PASS={value}"), "USER=ana", "PORT=1"],
        "styles": {"Hunter2x": {"glyph": "HUNT", "legend": "legend HUNTER2"}},
        "annotations": [
            {"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 5, "end": 12}},
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 1, "start": 2, "end": 6}, "label": "was HUNTER2"},
            {"id": "n1", "step": 1, "type": "note", "line": 3, "text": "was Hunter2"},
            {"id": "n2", "step": 1, "type": "note", "line": 4, "text": format!("was {value}")},
            {"id": "m1", "step": 1, "type": "mark", "kind": "Hunter2x", "span": {"line": 4, "start": 0, "end": 4}}
        ],
        "moves": [{"at_step": 2, "action": "code.patch", "op": "insert", "after": "PORT=1", "content": "OLD=HUNTER2"}]
    });
    let clean = |what: &str, s: &str| {
        let low = s.to_lowercase();
        for w in ["hunt", "unte", "nter", "ter2"] {
            assert!(!low.contains(w), "{what} shows «{w}» in some case:\n{s}");
        }
    };
    let d = tiza_core::fs::parse_document(&doc.to_string(), None).unwrap();
    for form in ["HUNTER2", "Hunter2", "hunter2"] {
        assert_eq!(
            tiza_core::redact::shown(&format!("was {form}"), &d),
            format!("was {MASK}")
        );
    }
    let dir = scratch("t174-rw5");
    let path = write(&dir, "case", &doc);
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    for step in [1, 2] {
        live.with(move |a| a.set_step(step));
        clean("the screen", &live.screen(120, 20));
        for (m, p) in [
            ("render.text", json!({"width": 100})),
            ("scene.list", json!({})),
            ("style.list", json!({})),
            ("scene.get", json!({"as_shown": true})),
            ("check", json!({})),
        ] {
            let v = live.api_ok(m, p).to_string();
            clean(m, &v.replace("PASS", "").replace("\"code\"", ""));
        }
    }
    // Ids in any case are refused; style names, glyphs and legends masked.
    for id in ["HUNTER2-x", "xHunter2", "hunter2y"] {
        let r = live.api(
            "annotation.add",
            json!({"type": "note", "line": 1, "text": "x", "id": id}),
        );
        assert!(r.error.is_some(), "{id} accepted");
    }
    let r = live.api_ok(
        "style.set",
        json!({"name": "HUNTER2z", "glyph": "Hunter2", "legend": "the HUNTER2"}),
    );
    assert_eq!(r["masked"], json!(true), "{r}");
    clean("style.set", &r.to_string());
    live.command("w");
    drop(live);
    let saved = std::fs::read_to_string(&path).unwrap();
    clean("the saved .tiza", &saved.replace("PASS", ""));
    assert!(saved.contains(&format!("OLD={MASK}")), "{saved}");
    // The review of t-167: the lowercase id of an uppercase value.
    let d = tiza_core::fs::parse_document(
        &json!({
            "schema": 1, "source": {"language": "sh"},
            "code": ["# x", "K=123SECRETXYZ"],
            "annotations": [{"id": "r1", "step": 1, "type": "redact", "span": {"line": 2, "start": 2, "end": 14}}]
        })
        .to_string(),
        None,
    )
    .unwrap();
    let dir = scratch("t174-rw5-ids");
    let live = Live::open(d, dir.join("ids.tiza"), dir.join("live.sock"));
    for id in ["123secretxyz", "123SECRETXYZ"] {
        let r = live.api(
            "annotation.add",
            json!({"type": "note", "line": 1, "text": "x", "id": id}),
        );
        assert!(r.error.is_some(), "{id} accepted");
    }
}

// Rework point 6 (Luis's decision B): a text Tiza saved masked is said.
#[test]
fn rw6_a_text_saved_masked_is_said_on_opening_and_by_check() {
    let dir = scratch("t174-rw6");
    let path = dir.join("demo.tiza");
    std::fs::copy(demo(), &path).unwrap();
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    live.command("w");
    drop(live);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains(&format!("rotate {MASK} soon")), "{saved}");
    let report =
        String::from_utf8_lossy(&tiza(&["check", path.to_str().unwrap()]).stdout).into_owned();
    assert!(
        report.contains("warning: h1, step 1: its label holds masked text — reword it"),
        "{report}"
    );
    assert!(
        report.contains("warning: n1, step 1: its text holds masked text — reword it"),
        "{report}"
    );
    assert_eq!(report.matches("holds masked text").count(), 2, "{report}");
    assert!(
        !report.contains("clear_value") && !report.contains("in clear"),
        "{report}"
    );
    // A render does not repeat it.
    let (_, err) = out_of(&["render", path.to_str().unwrap(), "--format", "txt"]);
    assert!(!err.contains("holds masked text"), "{err}");
    // On opening: one alert per field.
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, path.clone(), dir.join("live2.sock"));
    let _ = live.screen(120, 30);
    assert_eq!(
        live.with(|a| a.redact_alerts.clone()),
        [
            "h1: its label holds masked text — reword it".to_string(),
            "n1: its text holds masked text — reword it".to_string()
        ]
    );
    // Reworded: said no more.
    live.api_ok(
        "annotation.update",
        json!({"id": "h1", "label": "the old token"}),
    );
    let check = live.api_ok("check", json!({})).to_string();
    assert_eq!(check.matches("holds masked text").count(), 1, "{check}");
}

// ---------- review of the rework (2ac8a53) ----------

/// A 40-character value with a highlight after it on its line.
const LONG40: &str = "sk_live_4eC39HqLyjWDarjtT1zdp7dcAbC9xYz1";

fn long_doc() -> Value {
    json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": ["# keys", format!("STRIPE={LONG40} # tail"), "USER=ana"],
        "annotations": [
            {"id": "r1", "step": 0, "type": "redact", "span": {"line": 2, "start": 7, "end": 47}},
            {"id": "h1", "step": 1, "type": "highlight", "span": {"line": 2, "start": 50, "end": 54}, "label": "the tail"}
        ]
    })
}

#[track_caller]
fn assert_no_long(what: &str, s: &str) {
    let c: Vec<char> = LONG40.chars().collect();
    for w in c.windows(4) {
        let p: String = w.iter().collect();
        if p.chars().any(|c| c.is_ascii_digit()) {
            assert!(!s.contains(&p), "{what} shows «{p}» of the value:\n{s}");
        }
    }
}

// Review point 1: `scene.get` → `scene.patch` put the copy's columns on the
// real code, narrowing the redact (`STRIPE=••••••••4eC39H…`).
#[test]
fn rv1_get_then_patch_keeps_the_real_columns_and_never_narrows_a_redact() {
    let dir = scratch("t174-rv1");
    let path = write(&dir, "long", &long_doc());
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    live.with(|a| a.set_step(1));
    let scene = live.api_ok("scene.get", json!({}));
    let anns = scene["document"]["annotations"].clone();
    live.api_ok("scene.patch", json!({"annotations": anns}));
    for (m, p) in [
        ("code.get", json!({})),
        ("render.text", json!({"width": 100})),
        ("scene.get", json!({})),
    ] {
        assert_no_long(m, &live.api_ok(m, p).to_string());
    }
    assert_no_long("the screen", &live.screen(120, 20));
    let real = live.with(|a| a.doc.find_annotation("h1").unwrap().shape.clone());
    assert_eq!(
        tiza_core::redact::columns(&real),
        [(2, 50), (2, 54)],
        "h1 keeps its real columns"
    );
    // A redact sent back narrower than its value: refused, not quoted.
    let mut narrow = scene["document"]["annotations"].clone();
    narrow[0]["span"]["end"] = json!(10);
    let r = live.api("scene.patch", json!({"annotations": narrow}));
    let e = r.error.expect("a narrower redact is refused");
    assert_eq!(e.code, "invalid_argument");
    assert!(
        e.message.contains("does not come back over its value"),
        "{}",
        e.message
    );
    assert_no_long(
        "code.get after the refusal",
        &live.api_ok("code.get", json!({})).to_string(),
    );
    live.command("w");
    drop(live);
    assert_no_long("the saved .tiza", &std::fs::read_to_string(&path).unwrap());
}

fn long_ref(dir: &Path) -> PathBuf {
    std::fs::write(
        dir.join("main.env"),
        format!("# keys\nSTRIPE={LONG40} # tail\nUSER=ana\nPORT=1\n"),
    )
    .unwrap();
    let mut d = long_doc();
    d["code"] = json!({"file": "main.env"});
    d["moves"] = json!([
        {"at_step": 2, "action": "code.patch", "op": "insert", "after": "PORT=1", "content": format!("BACKUP={LONG40}")},
        {"at_step": 3, "action": "code.patch", "op": "insert", "after": "USER=ana", "content": "HOST=db"}
    ]);
    write(dir, "ref", &d)
}

// Review point 2: a put whose redacts do not come back over the value
// (left out, narrower) is refused: the real code would show under them.
#[test]
fn rv2_a_put_without_its_redact_or_narrower_is_refused() {
    let dir = scratch("t174-rv2");
    let path = long_ref(&dir);
    let written = std::fs::read_to_string(&path).unwrap();
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    let scene = live.api_ok("scene.get", json!({}));
    for case in ["left out", "narrower"] {
        let mut doc = scene["document"].clone();
        let anns = doc["annotations"].as_array_mut().unwrap();
        if case == "left out" {
            anns.retain(|a| a["id"] != "r1");
        } else {
            anns[0]["span"]["end"] = json!(8);
        }
        let r = live.api("scene.put", json!({"document": doc}));
        let e = r.error.unwrap_or_else(|| panic!("{case}: accepted"));
        assert!(
            e.message
                .contains("redact r1 does not come back over its value"),
            "{}",
            e.message
        );
        for (m, p) in [
            ("code.get", json!({})),
            ("render.text", json!({"width": 100})),
        ] {
            assert_no_long(&format!("{m} ({case})"), &live.api_ok(m, p).to_string());
        }
        assert_no_long("the screen", &live.screen(120, 20));
    }
    drop(live);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        written,
        "nothing written"
    );
}

// Review point 3: moves are matched by content, not by their place.
#[test]
fn rv3_moves_keep_their_real_content_wherever_they_go() {
    let dir = scratch("t174-rv3");
    let path = long_ref(&dir);
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    let scene = live.api_ok("scene.get", json!({}));
    let mut doc = scene["document"].clone();
    let moves = doc["moves"].as_array_mut().unwrap();
    assert!(moves[0]["content"].as_str().unwrap().contains(MASK));
    // Reordered, the secret's move one step later, a new move in front.
    moves.reverse();
    moves[1]["at_step"] = json!(4);
    moves.insert(0, json!({"at_step": 5, "action": "code.patch", "op": "insert", "after": "# keys", "content": "# new"}));
    live.api_ok("scene.put", json!({"document": doc.clone()}));
    live.command("w");
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["code"], json!({"file": "main.env"}));
    let contents: Vec<String> = saved["moves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["content"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        contents,
        ["# new", "HOST=db", format!("BACKUP={LONG40}").as_str()],
        "{saved:#}"
    );
    assert_eq!(saved["moves"][2]["at_step"], json!(4));
    // The masked content twice: one of them would lose the real one.
    let scene = live.api_ok("scene.get", json!({}));
    let mut doc = scene["document"].clone();
    let m = doc["moves"][2].clone();
    doc["moves"].as_array_mut().unwrap().push(m);
    let r = live.api("scene.put", json!({"document": doc}));
    let e = r.error.expect("an unmatched masked move is refused");
    assert!(
        e.message
            .contains("move #4 carries content masked for the wire"),
        "{}",
        e.message
    );
    drop(live);
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains(&format!("BACKUP={LONG40}"))
    );
}

// Review point 4: a put whose code is not the file's (edited, or older
// than a reload) is refused instead of turning the reference embedded.
#[test]
fn rv4_a_put_with_other_code_keeps_the_reference() {
    let dir = scratch("t174-rv4");
    let path = file_ref(&dir, false);
    let written = std::fs::read_to_string(&path).unwrap();
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    let mut scene = live.api_ok("scene.get", json!({}));
    scene["document"]["code"][2] = json!("USER=bob");
    let r = live.api("scene.put", json!({"document": scene["document"]}));
    let e = r.error.expect("a put with other code is refused");
    assert_eq!(e.code, "invalid_argument");
    assert!(
        e.message.contains("this document follows main.env"),
        "{}",
        e.message
    );
    assert_clean("the refusal", &e.message);
    assert!(live.with(|a| !a.doc.is_embedded()));
    // `tiza scene put FILE` of the same `.tiza` sends the masked copy: it
    // goes back on the file.
    let out = tiza(&[
        "--socket",
        live.socket.to_str().unwrap(),
        "scene",
        "put",
        path.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(live.with(|a| !a.doc.is_embedded()));
    drop(live);
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["code"], json!({"file": "main.env"}));
    assert_eq!(
        saved["annotations"],
        serde_json::from_str::<Value>(&written).unwrap()["annotations"]
    );
}

// Review point 5: a linked file frozen in `views` comes back as it was.
#[test]
fn rv5_frozen_views_come_back_unchanged() {
    let dir = scratch("t174-rv5");
    std::fs::write(dir.join("main.env"), "# main\nUSER=ana\n").unwrap();
    let doc = json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": {"file": "main.env"},
        "views": {"lib.env": ["# lib", format!("LIBKEY={VALUE} # x"), "MODE=x"]},
        "annotations": [
            {"id": "r2", "step": 0, "type": "redact", "file": "lib.env", "span": {"line": 2, "start": 7, "end": 17}},
            {"id": "n2", "step": 2, "type": "note", "file": "lib.env", "line": 3, "text": "the mode"}
        ],
        "moves": [{"at_step": 1, "action": "code.link", "file": "lib.env"}]
    });
    let path = write(&dir, "frozen", &doc);
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    live.command("w");
    let after_w = std::fs::read_to_string(&path).unwrap();
    let scene = live.api_ok("scene.get", json!({}));
    live.api_ok("scene.put", json!({"document": scene["document"]}));
    live.command("w");
    drop(live);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), after_w);
}

// Review point 6: the remaining file names go through the filter.
#[test]
fn rv6_follow_move_list_reader_copy_and_check_mask_file_names() {
    let dir = scratch("t174-rv6");
    std::fs::write(
        dir.join("beefcake.env"),
        format!("# env\nKEY={WORDY}\nUSER=ana\n"),
    )
    .unwrap();
    std::fs::write(dir.join("c0ffeebeef.env"), "# lib\nMODE=x\n").unwrap();
    let doc = json!({
        "schema": 1, "source": {"language": "sh"},
        "code": {"file": "beefcake.env"},
        "annotations": [
            {"id": "r1", "step": 0, "type": "redact", "span": {"line": 2, "start": 4, "end": 20}},
            {"id": "n1", "step": 2, "type": "note", "file": "c0ffeebeef.env", "line": 2, "text": "the mode"}
        ],
        "moves": [{"at_step": 2, "action": "code.link", "file": "c0ffeebeef.env"}]
    });
    let path = write(&dir, &format!("x{WORDY}"), &doc);
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    // (a) follow.
    let r = live.api_ok("follow.set", json!({"enabled": true}));
    assert_no_wordy("follow.set", &r.to_string());
    assert_no_wordy(
        "follow.status",
        &live.api_ok("follow.status", json!({})).to_string(),
    );
    live.api_ok("follow.set", json!({"enabled": false}));
    // (b) move.list, list_moves and `:list`.
    let list = live.api_ok("move.list", json!({}));
    assert_no_wordy("move.list", &list.to_string());
    let mut mcp = McpClient::start(&live.socket);
    let (_, text, _) = mcp.call("list_moves", json!({}));
    assert_no_wordy("list_moves", &text);
    live.keys(&[crossterm::event::KeyCode::Char('L')]);
    assert_no_wordy(":list", &live.screen(140, 40));
    // (c) the reader's copy.
    let shown = live.api_ok("scene.get", json!({"as_shown": true}));
    assert_no_wordy("scene.get as_shown", &shown.to_string());
    assert_no_wordy(
        "tiza://scene",
        &mcp.request("resources/read", json!({"uri": "tiza://scene"}))
            .to_string(),
    );
    drop(mcp);
    drop(live);
    // (d) `tiza check`'s last line.
    let out =
        String::from_utf8_lossy(&tiza(&["check", path.to_str().unwrap()]).stdout).into_owned();
    assert!(out.contains(": 0 errors"), "{out}");
    assert_no_wordy("tiza check", &out);
}

// Review point 7: the filter prepares its values once; a frame with many
// labels and redacts costs about what it costs without them.
#[test]
fn rv7_the_filter_costs_little_per_frame() {
    let mut code = Vec::new();
    let mut anns = Vec::new();
    for i in 0..400 {
        if i < 20 {
            code.push(format!("KEY_{i}=Zq8wR{i:04}tY7uV2mK6YqA8m"));
            anns.push(json!({"id": format!("r{i}"), "step": 1, "type": "redact", "span": {"line": i + 1, "start": 6 + i.to_string().len(), "end": 6 + i.to_string().len() + 22}}));
        } else {
            code.push(format!("let value_{i} = compute_queue_item({i});"));
        }
    }
    for k in 0..200 {
        anns.push(json!({"id": format!("n{k}"), "step": 1, "type": "note", "line": 21 + k, "text": format!("note {k}: the consumer reads the next item here")}));
    }
    let with = tiza_core::fs::parse_document(
        &json!({"schema": 1, "source": {"language": "rs"}, "code": code, "annotations": anns})
            .to_string(),
        None,
    )
    .unwrap();
    // One redact against twenty: the cost per text must not grow with them.
    let mut without = with.clone();
    without
        .annotations
        .retain(|a| !a.id.starts_with('r') || a.id == "r0");
    let time = |d: &tiza_core::model::Document| {
        let mut v: Vec<f64> = (0..15)
            .map(|_| {
                let t = std::time::Instant::now();
                let f = tiza_core::redact::Echo::quoting(d);
                let mut c = d.clone();
                f.shown_doc(&mut c);
                t.elapsed().as_secs_f64()
            })
            .collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[7]
    };
    let _ = time(&with);
    let (a, b) = (time(&with), time(&without));
    eprintln!("the filter: {a:.5}s with 20 redacts, {b:.5}s with one");
    assert!(
        a < b * 4.0,
        "the filter: {a:.5}s with 20 redacts, {b:.5}s with one"
    );
}

// Review point 8: one alert per text, never «saved masked» of something not
// written, and frozen linked files of a file reference are checked.
#[test]
fn rv8_alerts_say_one_thing_and_a_failed_write_is_no_save() {
    let dir = scratch("t174-rv8");
    let path = dir.join("demo.tiza");
    std::fs::copy(demo(), &path).unwrap();
    let doc = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    let _ = live.screen(120, 30);
    live.api_ok("style.set", json!({"name": "unused", "glyph": "*"}));
    let _ = live.screen(120, 30);
    let h1: Vec<String> = live
        .with(|a| a.redact_alerts.clone())
        .into_iter()
        .filter(|a| a.starts_with("h1:"))
        .collect();
    assert_eq!(h1, [LABEL_ALERT.to_string()]);
    // A write that fails: no longer «saved masked».
    live.with(|a| a.dismiss_redact_alerts());
    let mut perm = std::fs::metadata(&path).unwrap().permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(&path, perm.clone()).unwrap();
    live.api_ok(
        "annotation.update",
        json!({"id": "h1", "label": format!("now {VALUE}")}),
    );
    let _ = live.screen(120, 30);
    let alerts = live.with(|a| a.redact_alerts.clone()).join("\n");
    assert!(
        alerts.contains("h1: its label shares text with a masked value; masked on screen"),
        "{alerts}"
    );
    assert!(!alerts.contains("saved masked"), "{alerts}");
    #[allow(clippy::permissions_set_readonly_false)]
    perm.set_readonly(false);
    std::fs::set_permissions(&path, perm).unwrap();
    drop(live);
    // A file reference with a linked file frozen in clear.
    std::fs::write(dir.join("main.env"), "# main\nUSER=ana\n").unwrap();
    let p = write(
        &dir,
        "frozen",
        &json!({
            "schema": 1, "source": {"language": "sh"},
            "code": {"file": "main.env"},
            "views": {"lib.env": ["# lib", format!("LIBKEY={VALUE}")]},
            "annotations": [{"id": "r2", "step": 0, "type": "redact", "file": "lib.env", "span": {"line": 2, "start": 7, "end": 17}}],
            "moves": [{"at_step": 1, "action": "code.link", "file": "lib.env"}]
        }),
    );
    let out = String::from_utf8_lossy(&tiza(&["check", p.to_str().unwrap()]).stdout).into_owned();
    assert!(
        out.contains("warning: r2: the linked file frozen in `views` holds the value under this redact in clear; frozen.tiza still holds it — drop it from `views` (Tiza then reads the file), or :embed in Tiza (which embeds the main code too)"),
        "{out}"
    );
    assert_clean("check", &out);
}

// ---------- second check of the review (dbd7192) ----------

/// A value shorter than the mask (`ab1Xz`, 5 < 8) and one longer.
fn short_long(value: &str) -> Value {
    json!({
        "schema": 1,
        "source": {"language": "sh"},
        "code": [format!("KEY={value} tail end"), "USER=ana"],
        "annotations": [
            {"id": "r1", "step": 0, "type": "redact", "span": {"line": 1, "start": 4, "end": 4 + value.chars().count()}}
        ]
    })
}

// Second check, point 1: an agent's column inside or past a mask never
// overflows (it took the instance down) and never lands inside the value
// uncovered.
#[test]
fn rc1_columns_before_inside_and_after_a_mask_are_mapped_safely() {
    for value in ["ab1Xz", "Zq8wR4tY7uV2mK6YqA8m"] {
        let n = value.chars().count() as u32;
        let masked_len = 4 + 8 + " tail end".len() as u32;
        for (i, (s, e)) in [
            (0, 2),
            (5, 6),
            (9, 10),
            (10, 11),
            (11, 12),
            (13, 17),
            (20, 21),
            (40, 41),
        ]
        .into_iter()
        .enumerate()
        {
            for via in ["patch", "put", "arrow"] {
                // A fresh instance each time: a put of the copy into an
                // embedded document leaves its code masked in memory.
                let live = live_of(&short_long(value), &format!("t174-rc1-{n}-{i}-{via}"));
                live.with(|a| a.set_step(1));
                let id = format!("z{i}{via}");
                let ann = if via == "arrow" {
                    json!({"id": id, "step": 1, "type": "arrow", "from": {"line": 1, "col": s}, "to": {"line": 2, "col": 0}})
                } else {
                    json!({"id": id, "step": 1, "type": "highlight", "span": {"line": 1, "start": s, "end": e}})
                };
                let r = if via == "put" {
                    let mut doc = live.api_ok("scene.get", json!({}))["document"].clone();
                    doc["annotations"].as_array_mut().unwrap().push(ann);
                    live.api("scene.put", json!({"document": doc}))
                } else {
                    live.api("scene.patch", json!({"annotations": [ann]}))
                };
                // Accepted or refused cleanly (past the line), never a crash.
                if let Some(err) = &r.error {
                    assert!(e > masked_len || s > masked_len, "{via} {s}..{e}: {err:?}");
                    continue;
                }
                let shape = live.with({
                    let id = id.clone();
                    move |a| a.doc.find_annotation(&id).map(|x| x.shape.clone())
                });
                let cols = tiza_core::redact::columns(&shape.unwrap());
                // On the instance's code (a put into an embedded document
                // leaves it masked, and its columns are the copy's).
                let len = live.with(|a| a.doc.code[0].chars().count()) as u32;
                assert!(
                    cols.iter().all(|(_, c)| *c <= len),
                    "{via} {s}..{e}: {cols:?}"
                );
                // A column inside the masked area stays inside the value or
                // on its border: covered by the mask.
                if via == "patch" && (5..12).contains(&s) {
                    assert!(cols[0].1 >= 4 && cols[0].1 <= 4 + n, "{via} {s}: {cols:?}");
                }
                let text = live
                    .api_ok("render.text", json!({"width": 100}))
                    .to_string();
                let c: Vec<char> = value.chars().collect();
                for w in c.windows(4) {
                    let p: String = w.iter().collect();
                    assert!(!text.contains(&p), "{via} {s}..{e} shows «{p}»");
                }
            }
        }
    }
}

// Second check, point 2: a put never tells whether a guess under a mask
// was right: a wrong guess and the real value get the same answer.
#[test]
fn rc2_a_put_with_the_real_value_answers_like_a_wrong_guess() {
    let dir = scratch("t174-rc2");
    let path = file_ref(&dir, false);
    let written = std::fs::read_to_string(&path).unwrap();
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    let scene = live.api_ok("scene.get", json!({}));
    let before = live.api_ok("scene.get", json!({}));
    let mut answers = Vec::new();
    for guess in ["wrongguess", &VALUE.to_lowercase(), VALUE] {
        let mut doc = scene["document"].clone();
        doc["code"][1] = json!(format!("TOKEN={guess} # tail"));
        let r = live.api("scene.put", json!({"document": doc}));
        let e = r.error.expect("refused");
        answers.push((e.code, e.message));
        assert_eq!(
            live.api_ok("scene.get", json!({})),
            before,
            "{guess}: no effect"
        );
    }
    assert!(answers.windows(2).all(|w| w[0] == w[1]), "{answers:?}");
    drop(live);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), written);
}

// Second check, point 3: a redact keeps its step 0 through scene.patch,
// annotation.update and a reload of a followed file, moves or not.
#[test]
fn rc3_a_redact_keeps_its_step_zero() {
    for (moves, reference) in [(false, false), (false, true), (true, false), (true, true)] {
        let dir = scratch(&format!("t174-rc3-{moves}-{reference}"));
        std::fs::write(
            dir.join("main.env"),
            format!("# main\nTOKEN={VALUE}\nUSER=ana\n"),
        )
        .unwrap();
        let mut doc = json!({
            "schema": 1, "source": {"language": "sh"},
            "redact_from_step": true,
            "code": ["# main", format!("TOKEN={VALUE}"), "USER=ana"],
            "annotations": [
                {"id": "r1", "step": 0, "type": "redact", "span": {"line": 2, "start": 6, "end": 16}},
                {"id": "n1", "step": 1, "type": "note", "line": 3, "text": "the user"}
            ]
        });
        if reference {
            doc["code"] = json!({"file": "main.env"});
        }
        if moves {
            doc["moves"] = json!([{"at_step": 2, "action": "code.patch", "op": "insert", "after": "USER=ana", "content": "HOST=db"}]);
        }
        let path = write(&dir, "doc", &doc);
        let d = tiza_core::fs::open(path.to_str().unwrap())
            .unwrap()
            .document;
        let live = Live::open(d, path.clone(), dir.join("live.sock"));
        let step_of = |live: &Live| live.with(|a| a.doc.find_annotation("r1").unwrap().step);
        let scene = live.api_ok("scene.get", json!({}));
        live.api_ok(
            "scene.patch",
            json!({"annotations": scene["document"]["annotations"]}),
        );
        assert_eq!(step_of(&live), 0, "patch ({moves}, {reference})");
        live.api_ok(
            "annotation.update",
            json!({"id": "r1", "step": 0, "label": "k"}),
        );
        assert_eq!(step_of(&live), 0, "update ({moves}, {reference})");
        if reference {
            live.api_ok("follow.set", json!({"enabled": true}));
            std::fs::write(
                dir.join("main.env"),
                format!("# main\nTOKEN={VALUE}\nUSER=bob\n"),
            )
            .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while live.with(|a| a.doc.base().get(2).map(String::as_str) != Some("USER=bob")) {
                live.with(|a| {
                    a.follow_tick();
                });
                assert!(
                    std::time::Instant::now() < deadline,
                    "the reload never came"
                );
            }
            assert_eq!(step_of(&live), 0, "follow ({moves})");
            live.api_ok("follow.set", json!({"enabled": false}));
        }
        live.with(|a| a.set_step(0));
        assert_clean(
            "code.get at step 0",
            &live.api_ok("code.get", json!({})).to_string(),
        );
        live.command("w");
        drop(live);
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            saved["annotations"][0]["step"],
            json!(0),
            "saved ({moves}, {reference}): {saved:#}"
        );
    }
}

// Second check, point 4: an annotation's linked file is shown text in
// `scene.list`, `list_annotations` and `:list`.
#[test]
fn rc4_an_annotations_file_is_masked_in_the_lists() {
    let dir = scratch("t174-rc4");
    std::fs::write(dir.join("beefcake.env"), "# lib\nMODE=x\n").unwrap();
    let doc = json!({
        "schema": 1, "source": {"language": "sh"},
        "code": ["# main", format!("KEY={WORDY}")],
        "annotations": [
            {"id": "r1", "step": 0, "type": "redact", "span": {"line": 2, "start": 4, "end": 20}},
            {"id": "n2", "step": 2, "type": "note", "file": "beefcake.env", "line": 2, "text": "the mode"}
        ],
        "moves": [{"at_step": 2, "action": "code.link", "file": "beefcake.env"}]
    });
    let path = write(&dir, "doc", &doc);
    let d = tiza_core::fs::open(path.to_str().unwrap())
        .unwrap()
        .document;
    let live = Live::open(d, path.clone(), dir.join("live.sock"));
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert!(list.contains(&format!("({MASK})")), "{list}");
    assert_no_wordy("scene.list", &list);
    let mut mcp = McpClient::start(&live.socket);
    let (_, text, _) = mcp.call("list_annotations", json!({}));
    assert_no_wordy("list_annotations", &text);
    live.keys(&[crossterm::event::KeyCode::Char('L')]);
    assert_no_wordy(":list", &live.screen(140, 40));
}

// Third check: with `redact_from_step`, a new redact made while step 0 is
// shown masks step 0 (it was born at step 1, the value in clear at 0).
#[test]
fn rc5_a_new_redact_masks_the_step_it_is_made_at_zero_included() {
    let doc = json!({
        "schema": 1, "source": {"language": "sh"},
        "redact_from_step": true,
        "code": ["# x", format!("TOKEN={VALUE}"), "USER=ana"],
        "annotations": [{"id": "n1", "step": 2, "type": "note", "line": 3, "text": "the user"}]
    });
    for via in ["add", "add step 0", ":redact"] {
        let live = live_of(&doc, &format!("t174-rc5-{}", via.replace([' ', ':'], "-")));
        let first = if via == "add step 0" { 1 } else { 0 };
        live.with(move |a| a.set_step(first));
        match via {
            "add" => {
                let r = live.api_ok(
                    "annotation.add",
                    json!({"type": "redact", "at": {"line": 2, "find": "TOKEN="}}),
                );
                assert!(r.get("hidden").is_none(), "{r}");
            }
            "add step 0" => {
                live.api_ok(
                    "annotation.add",
                    json!({"type": "redact", "step": 0, "at": {"line": 2, "find": "TOKEN="}}),
                );
            }
            _ => live.command("redact 2:TOKEN="),
        }
        assert_eq!(
            live.with(|a| a
                .doc
                .annotations
                .iter()
                .find(|x| matches!(x.shape, tiza_core::model::Shape::Redact { .. }))
                .map(|x| x.step)),
            Some(0),
            "{via}"
        );
        live.with(|a| a.set_step(0));
        assert_clean(
            &format!("code.get at step 0 ({via})"),
            &live.api_ok("code.get", json!({})).to_string(),
        );
        assert_clean(
            &format!("the screen at step 0 ({via})"),
            &live.screen(120, 20),
        );
        live.command("w");
        let saved = std::fs::read_to_string(live.with(|a| a.save_path.clone())).unwrap();
        assert_clean(&format!("the saved .tiza ({via})"), &saved);
    }
    // A redact that does not mask the step on screen says so in the TUI too.
    let live = live_of(&doc, "t174-rc5-later");
    live.with(|a| a.set_step(0));
    live.api_ok(
        "annotation.add",
        json!({"type": "redact", "step": 2, "at": {"line": 2, "find": "TOKEN="}}),
    );
    let alerts = live.with(|a| a.redact_alerts.clone()).join("\n");
    assert!(
        alerts.contains("does not mask step 0: it masks from step 2"),
        "{alerts}"
    );
}
