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
//!   the time on the t-138 big file. (The search starts at the width from
//!   which the take no longer changes, `check::saturation_width`, a bound
//!   from the layout's rules checked on every step against the take at 400;
//!   the moves are replayed once per step instead of once per layout, a step
//!   that draws no annotation is not laid out, the widths are shared among
//!   the cores, and the search stops at the widest width that clips; times
//!   in the card's note.)
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

/// The layout `tiza render --step STEP --width WIDTH` draws of the whole
/// document.
fn lay(doc: &tiza_core::Document, width: usize, step: u32) -> tiza_core::layout::Layout {
    let opts = LayoutOptions {
        width,
        step,
        line_numbers: doc.numbers,
        margin_cap: doc.margin.map(usize::from),
        ..Default::default()
    };
    layout(doc, &tiza_core::Theme::dark(), &opts)
}

/// Some step of the take, every one from 0 to the last (not only those
/// where something changes), clips a label at `width`.
fn clips(doc: &tiza_core::Document, width: usize) -> bool {
    (0..=tiza_core::check::last_step(doc))
        .any(|step| !lay(doc, width, step).clipped_labels.is_empty())
}

/// `f` on every width from [`MIN_WIDTH`] to [`MAX_WIDTH`], one width in
/// every `n` per thread (the layouts are independent), widest first; a
/// thread stops at a width `stop` says is no longer needed.
fn per_width(f: impl Fn(usize) -> bool + Sync, stop: impl Fn(usize) -> bool + Sync) -> Vec<usize> {
    let n = std::thread::available_parallelism().map_or(1, |n| n.get());
    let mut out: Vec<usize> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..n)
            .map(|k| {
                let (f, stop) = (&f, &stop);
                s.spawn(move || {
                    let mut mine = Vec::new();
                    for w in (MIN_WIDTH..=MAX_WIDTH - k).rev().step_by(n) {
                        if stop(w) {
                            break;
                        }
                        if f(w) {
                            mine.push(w);
                        }
                    }
                    mine
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

/// The widths from [`MIN_WIDTH`] to [`MAX_WIDTH`] at which some step of a
/// take clips a label, found the slow way: every width, every step, the
/// whole document laid out as `tiza render` lays it out.
fn clipping_widths(doc: &tiza_core::Document) -> Vec<usize> {
    per_width(|w| clips(doc, w), |_| false)
}

/// The widest width that clips, the slow way: down from [`MAX_WIDTH`],
/// every width and every step until one clips.
fn widest_clipping(doc: &tiza_core::Document) -> Option<usize> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let found = AtomicUsize::new(0);
    per_width(
        |w| {
            let c = clips(doc, w);
            if c {
                found.fetch_max(w, Ordering::Relaxed);
            }
            c
        },
        |w| w <= found.load(Ordering::Relaxed),
    )
    .last()
    .copied()
}

/// Two layouts draw the same take: the same rows, labels and text, the
/// wider one only blank on the right.
fn same_take(a: &tiza_core::layout::Layout, b: &tiza_core::layout::Layout) -> Result<(), String> {
    let text = |l: &tiza_core::layout::Layout| {
        l.canvas
            .to_plain_text()
            .lines()
            .map(|r| r.trim_end().to_string())
            .collect::<Vec<_>>()
    };
    if a.rows != b.rows {
        return Err("rows differ".into());
    }
    if a.clipped_labels != b.clipped_labels {
        return Err(format!(
            "clipped {:?} / {:?}",
            a.clipped_labels, b.clipped_labels
        ));
    }
    if (a.labels_in_margin, a.code_needs, a.code_x) != (b.labels_in_margin, b.code_needs, b.code_x)
    {
        return Err("margin, code or code column differ".into());
    }
    let (ta, tb) = (text(a), text(b));
    if ta != tb {
        return Err(format!(
            "text differs:\n{}\n---\n{}",
            ta.join("\n"),
            tb.join("\n")
        ));
    }
    Ok(())
}

/// The take at the saturation width is the one at [`MAX_WIDTH`] at every
/// step from 0 to the last.
///
/// Also that no step got there by the safety net: every step is searched
/// from one of the two bounds (or the bound is the widest width itself),
/// never from [`MAX_WIDTH`] because the take at both bounds differed.
fn saturates(doc: &tiza_core::Document) -> Result<usize, String> {
    use tiza_core::check::Bound;
    let theme = tiza_core::Theme::dark();
    let net: Vec<u32> = tiza_core::check::saturation_bounds(doc, &theme)
        .into_iter()
        .filter(|(_, b)| *b == Bound::Fallback)
        .map(|(s, _)| s)
        .collect();
    if !net.is_empty() {
        return Err(format!(
            "steps {net:?} are searched from {MAX_WIDTH}: the bound is wrong"
        ));
    }
    let sat = tiza_core::check::saturation_width(doc, &theme);
    for step in 0..=tiza_core::check::last_step(doc) {
        same_take(&lay(doc, sat, step), &lay(doc, MAX_WIDTH, step))
            .map_err(|e| format!("step {step}, {sat} vs {MAX_WIDTH}: {e}"))?;
    }
    Ok(sat)
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
fn ac1_ac4_tally_tour_says_55_and_its_callout_is_never_cut_at_106_to_108() {
    let p = example("tally-tour.tiza");
    let p = p.to_str().unwrap();
    // Until t-201 the `auto` callout of step 3 moved into the margin at 106
    // and, with no room for its block until 109, was a label cut with …
    // there (`check` said «109 or wider; 91-105 are clean too»). With no
    // room for its block in the margin it is now a block under its lines,
    // so nothing is clipped from 55 columns on.
    let out = tiza_ok(&["check", p]);
    assert!(out.contains("0 errors, 0 warnings"), "{out}");
    assert_eq!(
        last_line(&out),
        "no label is clipped at 55 or wider (`tiza render --width 55`)",
        "{out}"
    );
    for w in ["55", "105", "106", "107", "108", "109", "130"] {
        let v = check_json(p, &["--width", w]);
        assert!(clipped_ids(&v).is_empty(), "{w}: {v}");
        assert_eq!(v["min_clean_width"], 55, "{w}");
    }
    let t = tiza_ok(&[
        "render", p, "--step", "3", "--width", "107", "--format", "txt", "--out", "-",
    ]);
    assert!(
        t.contains("╭─ bytes come free") && !t.contains("┆ text.len()"),
        "{t}"
    );
}

/// Every `.tiza` under `examples/` and `qa/fixtures/` that opens.
fn corpus() -> Vec<String> {
    fn walk(dir: &std::path::Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "tiza") {
                out.push(p.to_str().unwrap().to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(&fixture(""), &mut out);
    walk(&example(""), &mut out);
    out.sort();
    out
}

#[test]
fn ac1_ac2_every_example_and_fixture_saturates_at_the_bound_and_agrees_with_every_width() {
    let mut opened = Vec::new();
    for p in corpus() {
        let Ok(o) = tiza_core::fs::open(&p) else {
            continue;
        };
        opened.push((p, o.document));
    }
    let names: Vec<&str> = opened.iter().map(|(p, _)| p.as_str()).collect();
    for want in [
        "ownership-moves.rs.tiza",
        "stack-impl.tiza",
        "tally-tour.tiza",
        "t-200/capped.tiza",
    ] {
        assert!(
            names.iter().any(|p| p.ends_with(want)),
            "{want} not in {names:?}"
        );
    }
    assert!(opened.len() >= 35, "{names:?}");
    for (p, doc) in &opened {
        // The bound holds: the take does not change above it.
        let sat = saturates(doc).unwrap_or_else(|e| panic!("{p}: {e}"));
        eprintln!("{p}: bound {sat}");
        // The announced width is the one trying every width says.
        let want =
            widest_clipping(doc).map_or(Some(MIN_WIDTH), |w| (w < MAX_WIDTH).then_some(w + 1));
        let got = tiza_core::check::min_clean_width(doc, &tiza_core::Theme::dark());
        assert_eq!(got, want, "{p} (bound {sat})");
        let v = check_json(p, &[]);
        if v["max_step"].is_number() {
            assert_eq!(v["min_clean_width"], json!(want), "{p}");
        }
    }
}

/// A small deterministic generator (xorshift): the same documents on every
/// run.
struct Gen(u64);

impl Gen {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    /// Words of a label: plain ones, and some with a zero-width space, a
    /// ZWJ sequence, a combining mark or a no-break space, which take more
    /// (or other) cells once drawn than their display width says.
    fn words(&mut self, max: u64) -> String {
        const W: [&str; 12] = [
            "the",
            "borrow",
            "checker",
            "moves",
            "a",
            "value",
            "here",
            "once",
            "zero\u{200b}width",
            "👩\u{200d}💻",
            "cafe\u{301}",
            "no\u{a0}break",
        ];
        let n = 1 + self.below(max);
        (0..n)
            .map(|_| W[self.below(W.len() as u64) as usize])
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A line of random code: plain cells, tabs, wide characters (CJK, emoji).
fn random_line(g: &mut Gen, i: u32) -> String {
    let mut out = format!("let v{i} = ");
    for _ in 0..g.below(40) {
        out.push_str(["x", "x", "x", "\t", "変", "🦀", "y "][g.below(7) as usize]);
    }
    out.push(';');
    out
}

/// A random annotation on `code` (lines from 1), shown at `step`.
fn random_annotation(g: &mut Gen, id: String, step: u32, code: &[String]) -> Value {
    let lines = code.len() as u32;
    let len = |l: u32| code[(l - 1) as usize].chars().count() as u64;
    let line = |g: &mut Gen| 1 + g.below(lines as u64) as u32;
    let l = line(g);
    let long = if g.below(3) == 0 { 20 } else { 5 };
    match g.below(7) {
        0 => json!({"id": id, "step": step, "type": "note", "line": l, "text": g.words(long)}),
        1 => {
            let end = 1 + g.below(len(l) - 1);
            json!({"id": id, "step": step, "type": "highlight", "span": {"line": l, "start": 0, "end": end}, "label": g.words(long)})
        }
        2 => {
            let end = 1 + g.below(len(l) - 1);
            json!({"id": id, "step": step, "type": "mark", "kind": "warn", "span": {"line": l, "start": 0, "end": end}, "label": g.words(long)})
        }
        3 => {
            let to = line(g);
            json!({"id": id, "step": step, "type": "arrow", "from": {"line": l, "col": 1}, "to": {"line": to, "col": 2}, "label": g.words(4)})
        }
        4 => {
            let to = (l + g.below(3) as u32).min(lines);
            json!({"id": id, "step": step, "type": "box", "from_line": l, "to_line": to, "label": g.words(3)})
        }
        _ => {
            let to = (l + g.below(2) as u32).min(lines);
            let mut c = json!({"id": id, "step": step, "type": "callout", "from_line": l, "to_line": to, "text": g.words(25)});
            if g.below(2) == 0 {
                c["width"] = json!(8 + g.below(50));
            }
            if g.below(2) == 0 {
                c["title"] = json!(g.words(6));
            }
            c["placement"] = json!(["auto", "margin", "below"][g.below(3) as usize]);
            c
        }
    }
}

/// A random document: code of random widths with tabs and wide
/// characters, labels long and short, notes, marks, callouts with and
/// without `width` (and with titles), arrows across lines, nested boxes,
/// sometimes a margin cap, sometimes a `code.link` to a second file with
/// annotations of its own. Returns the document and that file (`name`,
/// lines), to write next to it.
fn random_doc(g: &mut Gen, name: &str) -> (Value, Option<Vec<String>>) {
    let lines = 4 + g.below(10) as u32;
    let code: Vec<String> = (0..lines).map(|i| random_line(g, i)).collect();
    let steps = 1 + g.below(4) as u32;
    let mut anns = Vec::new();
    for i in 0..(2 + g.below(9)) {
        let step = 1 + g.below(steps as u64) as u32;
        anns.push(random_annotation(g, format!("a{i}"), step, &code));
    }
    let mut doc = json!({
        "schema": 1,
        "title": "random",
        "source": {"language": "rs", "first_line": 1},
        "code": code,
        "annotations": anns,
    });
    if g.below(4) == 0 {
        doc["margin"] = json!(12 + g.below(40));
    }
    if g.below(5) == 0 {
        doc["numbers"] = json!(false);
    }
    let mut linked = None;
    if g.below(4) == 0 {
        // From a step on, the take shows the second file.
        let other: Vec<String> = (0..4 + g.below(8) as u32)
            .map(|i| random_line(g, i))
            .collect();
        let at = 1 + g.below(steps as u64) as u32;
        doc["moves"] = json!([{"at_step": at, "action": "code.link", "file": name}]);
        for i in 0..(1 + g.below(4)) {
            let step = at + g.below(2) as u32;
            let mut a = random_annotation(g, format!("l{i}"), step, &other);
            a["file"] = json!(name);
            doc["annotations"].as_array_mut().unwrap().push(a);
        }
        linked = Some(other);
    }
    (doc, linked)
}

#[test]
fn ac2_random_documents_saturate_at_the_bound() {
    let dir = scratch("t200-random");
    let mut g = Gen(0x7a11_2000_5eed_0001);
    let (mut checked, mut below, mut linked, mut brute) = (0, 0, 0, 0);
    let start = std::time::Instant::now();
    for i in 0..300 {
        let file = dir.join(format!("r{i}.tiza"));
        let other = format!("o{i}.rs");
        let (doc, code) = random_doc(&mut g, &other);
        if let Some(code) = &code {
            std::fs::write(dir.join(&other), code.join("\n") + "\n").unwrap();
        }
        std::fs::write(&file, doc.to_string()).unwrap();
        // A random anchor can be refused by the loader: skip that one.
        let Ok(o) = tiza_core::fs::open(file.to_str().unwrap()) else {
            continue;
        };
        let doc = o.document;
        let show = |e: String| {
            panic!(
                "{}: {e}\n{}",
                file.display(),
                std::fs::read_to_string(&file).unwrap()
            )
        };
        match saturates(&doc) {
            Ok(sat) => below += usize::from(sat < MAX_WIDTH),
            Err(e) => show(e),
        }
        // One in six: the announced width against every width and every
        // step, the slow way.
        if i % 6 == 0 {
            let want =
                widest_clipping(&doc).map_or(Some(MIN_WIDTH), |w| (w < MAX_WIDTH).then_some(w + 1));
            let got = tiza_core::check::min_clean_width(&doc, &tiza_core::Theme::dark());
            if got != want {
                show(format!("announces {got:?}, every width says {want:?}"));
            }
            brute += 1;
        }
        linked += usize::from(code.is_some());
        checked += 1;
    }
    assert!(checked >= 200, "only {checked} documents opened");
    assert!(linked >= 30, "only {linked} documents with a code.link");
    // The bound is below the widest width for most of them, or the test
    // would prove nothing.
    assert!(
        below * 2 >= checked,
        "{below} of {checked} below {MAX_WIDTH}"
    );
    eprintln!(
        "{checked} random documents ({below} bounded below {MAX_WIDTH}, {linked} with a code.link, {brute} against every width) in {:?}",
        start.elapsed()
    );
    let _ = std::fs::remove_dir_all(&dir);
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
