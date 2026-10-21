//! t-200 — `tiza check` announces a clean width that is not: «no label is
//! clipped at 30 or wider» while 62 clips.
//!
//! The narrowest clean width used to be searched down from 400 in strides
//! of 8, on the assumption that a wider take never clips more than a
//! narrower one. That is false where the side margin first appears (62
//! columns on the t-172 fixtures: the margin is narrower there than the
//! rows under the lines were) and where an `auto` callout moves into the
//! margin and finds no room (106-108 on `examples/tally-tour.tiza`).
//!
//! Acceptance criteria:
//! - AC1 The width `tiza check` announces as the narrowest clean one is
//!   true: at that width and at every width above it up to the cap it
//!   checks, no label is clipped (or the line says which widths in between
//!   clip). No step-of-8 assumption. (Every width from 20 to 400 above the
//!   widest one that clips is laid out at every step; the announced width is
//!   the one above that.)
//! - AC2 Cost: checking every width from 30 to 400 must stay fast; report
//!   the time on the t-138 big file. (The moves are replayed once per step
//!   instead of once per layout, a step that draws no annotation is not laid
//!   out, the widths are shared among the cores, and the search stops at the
//!   widest width that clips; times in the card's note.)
//! - AC3 Same in `check` over the socket/MCP and in `status`. (The socket
//!   and MCP `check` run the same search on the live document; `status`
//!   reports no clean width, only the live layout's warnings.)
//! - AC4 This file, with the t-172 fixtures at 62 and tally-tour at
//!   106–108; CHANGELOG (Fixed).
//!
//! Corner cases: a document that clips at every width keeps saying so; the
//! narrow layout below 62 is unchanged; with a margin cap (`:margin N`) or
//! `--width` only the widths that matter count.

use serde_json::{Value, json};
use tiza_core::layout::{LayoutOptions, MAX_WIDTH, MIN_WIDTH, layout};
use tiza_qa::{Live, McpClient, example, fixture, scratch, tiza, tiza_ok};

/// The fixtures that clipped at 62 while `check` said «30 or wider» (or
/// «33»), with the width it announces now: four of t-172 and two written
/// for this ticket (the second label of L1 arrives with a typing move; the
/// note comes before the highlight, which also clips at 63).
const AT_62: [(&str, usize); 6] = [
    ("t-172/two-labels-hl.tiza", 63),
    ("t-172/two-labels-mark.tiza", 63),
    ("t-172/three-labels.tiza", 63),
    ("t-172/two-labels-hl-mark.tiza", 63),
    ("t-200/typing.tiza", 63),
    ("t-200/order-note-first.tiza", 64),
];

fn path(rel: &str) -> String {
    fixture(rel).to_str().unwrap().to_string()
}

fn open(path: &str) -> tiza_core::Document {
    tiza_core::fs::open(path).unwrap().document
}

/// The widths from [`MIN_WIDTH`] to [`MAX_WIDTH`] at which some step of a
/// take clips a label, found the slow way: every width, every step, the
/// whole document laid out as `tiza render` lays it out.
fn clipping_widths(doc: &tiza_core::Document) -> Vec<usize> {
    let theme = tiza_core::Theme::dark();
    let steps = doc.change_steps(tiza_core::check::last_step(doc));
    let clips = |width: usize| {
        steps.iter().any(|&step| {
            let opts = LayoutOptions {
                width,
                step,
                line_numbers: doc.numbers,
                margin_cap: doc.margin.map(usize::from),
                ..Default::default()
            };
            !layout(doc, &theme, &opts).clipped_labels.is_empty()
        })
    };
    // One width in every `n` per thread: the layouts are independent.
    let n = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut out: Vec<usize> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..n)
            .map(|k| {
                let clips = &clips;
                s.spawn(move || {
                    (MIN_WIDTH + k..=MAX_WIDTH)
                        .step_by(n)
                        .filter(|&w| clips(w))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    out.sort_unstable();
    out
}

/// What the slow way says the narrowest clean width is.
fn true_clean_width(clipping: &[usize]) -> Option<usize> {
    match clipping.last() {
        None => Some(MIN_WIDTH),
        Some(&MAX_WIDTH) => None,
        Some(&w) => Some(w + 1),
    }
}

fn check_json(file: &str, extra: &[&str]) -> Value {
    let mut args = vec!["check", file, "--json"];
    args.extend_from_slice(extra);
    let o = tiza(&args);
    let out = String::from_utf8_lossy(&o.stdout);
    serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"))
}

fn clipped_ids(v: &Value) -> Vec<String> {
    v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "clipped_label")
        .map(|f| f["id"].as_str().unwrap().to_string())
        .collect()
}

/// The last line of a `tiza check` report.
fn last_line(out: &str) -> String {
    out.trim_end().lines().last().unwrap().to_string()
}

#[test]
fn ac1_ac4_the_62_fixtures_announce_a_width_above_62_and_every_width_above_is_clean() {
    for (rel, clean) in AT_62 {
        let p = path(rel);
        let doc = open(&p);
        let clipping = clipping_widths(&doc);
        // The bug: 62 clips (the margin appears there), the narrow widths
        // below it are clean from 30 or 33.
        assert!(clipping.contains(&62), "{rel}: {clipping:?}");
        assert!(!clipping.contains(&61), "{rel}: {clipping:?}");
        assert_eq!(clipping.last(), Some(&(clean - 1)), "{rel}: {clipping:?}");
        // The library, the report and the slow way agree.
        assert_eq!(
            tiza_core::check::min_clean_width(&doc, &tiza_core::Theme::dark()),
            Some(clean),
            "{rel}"
        );
        let out = tiza_ok(&["check", &p]);
        assert_eq!(
            last_line(&out),
            format!("no label is clipped at {clean} or wider (`tiza render --width {clean}`)"),
            "{rel}: {out}"
        );
        // `check --width 62` names what the announcement now accounts for;
        // the announced width itself is clean.
        assert!(
            !clipped_ids(&check_json(&p, &["--width", "62"])).is_empty(),
            "{rel}"
        );
        let at = clean.to_string();
        assert!(
            clipped_ids(&check_json(&p, &["--width", &at])).is_empty(),
            "{rel}"
        );
    }
    // What QA saw: step 2 at 62 columns clips the note of L3.
    let p = path("t-172/two-labels-hl.tiza");
    let at = |w: &str| tiza_ok(&["render", &p, "--width", w, "--step", "2", "--format", "txt"]);
    let t62 = at("62");
    assert!(t62.contains("┆ how chatty") && t62.contains('…'), "{t62}");
    for w in ["63", "100", "200"] {
        let t = at(w);
        assert!(!t.contains('…'), "{w}: {t}");
    }
}

#[test]
fn ac1_ac4_tally_tour_clips_at_106_to_108_and_says_109() {
    let p = example("tally-tour.tiza");
    let p = p.to_str().unwrap();
    // At the default 100 columns nothing is clipped, and the narrower
    // widths that are clean too are below the 106 where the margin appears.
    let out = tiza_ok(&["check", p]);
    assert!(out.contains("0 errors, 0 warnings"), "{out}");
    assert_eq!(
        last_line(&out),
        "no label is clipped at 109 or wider; 91-105 are clean too (notes go under the lines below 106 columns)",
        "{out}"
    );
    // The `auto` callout of step 3 moves into the margin at 106 and has no
    // room for its block until 109.
    for w in ["106", "107", "108"] {
        let v = check_json(p, &["--width", w]);
        assert_eq!(clipped_ids(&v), ["c1"], "{w}: {v}");
        assert_eq!(v["min_clean_width"], 109, "{w}");
    }
    for w in ["105", "109", "130"] {
        assert!(
            clipped_ids(&check_json(p, &["--width", w])).is_empty(),
            "{w}"
        );
    }
    let out = tiza_ok(&["check", p, "--width", "109"]);
    assert_eq!(
        last_line(&out),
        "no label is clipped at 109 or wider (`tiza render --width 109`)",
        "{out}"
    );
}

#[test]
fn ac1_every_example_and_fixture_agrees_with_trying_every_width() {
    let mut files: Vec<String> = AT_62.iter().map(|(r, _)| path(r)).collect();
    for rel in [
        "t-172/consecutive.tiza",
        "t-172/callout-fits.tiza",
        "t-172/callout-short.tiza",
        "t-172/callout-short2.tiza",
        "t-172/geometry.tiza",
        "t-200/capped.tiza",
        "t-200/clips-everywhere.tiza",
    ] {
        files.push(path(rel));
    }
    for name in [
        "tally-tour.tiza",
        "ownership.rs.tiza",
        "stack-and-queue.tiza",
    ] {
        files.push(example(name).to_str().unwrap().to_string());
    }
    for p in files {
        let doc = open(&p);
        let want = true_clean_width(&clipping_widths(&doc));
        let got = tiza_core::check::min_clean_width(&doc, &tiza_core::Theme::dark());
        assert_eq!(got, want, "{p}");
        let v = check_json(&p, &[]);
        assert_eq!(v["min_clean_width"], json!(want), "{p}");
    }
}

#[test]
fn corner_a_document_that_clips_at_every_width_keeps_saying_so() {
    let p = path("t-200/clips-everywhere.tiza");
    assert_eq!(clipping_widths(&open(&p)).len(), MAX_WIDTH - MIN_WIDTH + 1);
    let out = tiza(&["check", &p]);
    let out = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        last_line(&out),
        "no width up to 400 shows every label whole: shorten the labels",
        "{out}"
    );
    assert_eq!(check_json(&p, &[])["min_clean_width"], Value::Null);
}

#[test]
fn corner_the_narrow_layout_below_62_is_unchanged() {
    let p = path("t-172/two-labels-hl.tiza");
    let doc = open(&p);
    // Below the margin the labels go under the lines and fit from 30 on,
    // as before (the code itself needs 47); only the width announced above
    // them moved.
    let clipping = clipping_widths(&doc);
    assert_eq!(clipping, (20..=29).chain([62]).collect::<Vec<_>>());
    assert!(!clipped_ids(&check_json(&p, &["--width", "29"])).is_empty());
    for w in ["30", "47", "61"] {
        let v = check_json(&p, &["--width", w]);
        assert!(clipped_ids(&v).is_empty(), "{w}: {v}");
        assert_eq!(
            v["narrower"],
            json!({"clean": [[47, 61]], "code_fits_from": 47, "margin_from": 62}),
            "{w}"
        );
    }
    let out = tiza_ok(&["check", &p, "--width", "50"]);
    assert_eq!(
        last_line(&out),
        "no label is clipped at 63 or wider; 47-61 are clean too (notes go under the lines below 62 columns)",
        "{out}"
    );
}

#[test]
fn corner_a_margin_cap_and_width_only_count_the_widths_that_matter() {
    // With the margin capped at 16, every width from the one where the cap
    // ends the frame up draws the same take: the announcement is the same
    // as trying every width, and `--width` changes only the findings, never
    // the announced width.
    let p = path("t-200/capped.tiza");
    let doc = open(&p);
    assert_eq!(doc.margin, Some(16));
    let clipping = clipping_widths(&doc);
    let want = true_clean_width(&clipping);
    for w in ["30", "62", "100", "400"] {
        let v = check_json(&p, &["--width", w]);
        assert_eq!(v["min_clean_width"], json!(want), "{w}: {v}");
        let width: usize = w.parse().unwrap();
        assert_eq!(
            !clipped_ids(&v).is_empty(),
            clipping.contains(&width),
            "{w}: {v}"
        );
    }
    // On a document with no cap `--width` does not move the announcement
    // either.
    let p = path("t-172/two-labels-hl.tiza");
    for w in ["40", "62", "100", "300"] {
        assert_eq!(
            check_json(&p, &["--width", w])["min_clean_width"],
            63,
            "{w}"
        );
    }
}

#[test]
fn ac3_the_socket_and_mcp_check_say_the_same() {
    let dir = scratch("t200-live");
    let p = path("t-172/two-labels-hl.tiza");
    let live = Live::open(open(&p), dir.join("doc.tiza"), dir.join("live.sock"));
    let v = live.api_ok("check", json!({}));
    assert_eq!(v["min_clean_width"], 63, "{v}");
    let v = live.api_ok("check", json!({"width": 62}));
    assert_eq!(v["min_clean_width"], 63, "{v}");
    assert!(!clipped_ids(&v).is_empty(), "{v}");
    let mut mcp = McpClient::start(&live.socket);
    let (err, out, _) = mcp.call("check", json!({"width": 62}));
    assert!(!err, "{out}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["min_clean_width"], 63, "{v}");
    assert!(!clipped_ids(&v).is_empty(), "{v}");
    // `status` carries no clean width: its warnings are the live layout's.
    let s = live.api_ok("status", json!({}));
    assert!(s.get("min_clean_width").is_none(), "{s}");
    drop(mcp);
    drop(live);
    let _ = std::fs::remove_dir_all(&dir);
}
