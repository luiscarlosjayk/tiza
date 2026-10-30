//! Two labels on the same line: draw them under that line (as
//! narrow layouts do) instead of letting the second sit next to the line
//! below.
//!
//! Acceptance criteria (Luis, 2026-10-18: option b):
//! - AC1 In the side-margin layout (wide terminals), a line that carries two
//!   or more labels at the current step (highlight/mark labels, notes, an
//!   arrow's label at its head, a margin callout with no room for its block,
//!   any mix) draws all of them under that line,
//!   with the under-line rows the narrow layout already uses (`· the key`,
//!   `┆ DB connection string`), in the narrow layout's order. Every other
//!   line keeps its labels in the margin.
//! - AC2 Every label of such a line is drawn (or, if one truly has no room,
//!   `tiza check` names it).
//! - AC3 Documents with no multi-label line render byte-identical to before;
//!   README images and qa/herdr snapshots change only if a shipped example
//!   has such a line.
//! - AC4 `tiza check` and its narrowest-clean-width line take the new rows
//!   into account.
//! - AC5 Same layout in the TUI and every export (PNG, SVG, GIF, MP4, txt,
//!   md, ansi).
//! - AC6 Before/after real `tiza render` captures in the card note.
//! - AC7 This file; CHANGELOG.

use crossterm::event::KeyCode;
use tiza_core::layout::{LayoutOptions, Row, layout};
use tiza_qa::{Live, example, fixture, scratch, tiza_ok};

fn path(name: &str) -> String {
    fixture(&format!("render/two_labels_on_a_line/{name}"))
        .to_str()
        .unwrap()
        .to_string()
}

fn render(name: &str, width: usize, extra: &[&str]) -> String {
    let w = width.to_string();
    let p = path(name);
    let mut args = vec!["render", p.as_str(), "--width", &w, "--format", "txt"];
    args.extend_from_slice(extra);
    tiza_ok(&args)
}

fn open(name: &str) -> tiza_core::Document {
    tiza_core::fs::open(&path(name)).unwrap().document
}

fn lay(
    doc: &tiza_core::Document,
    width: usize,
    step: u32,
    typing: bool,
) -> tiza_core::layout::Layout {
    layout(
        doc,
        &tiza_core::Theme::dark(),
        &LayoutOptions {
            width,
            step,
            typing,
            ..Default::default()
        },
    )
}

/// The rows of a render, trailing blanks trimmed.
fn rows(text: &str) -> Vec<String> {
    text.lines().map(|l| l.trim_end().to_string()).collect()
}

/// What sits right of the code on the row of line `n` (`" 2 "` prefix).
fn margin_of(text: &str, n: u32, code: &str) -> String {
    let row = rows(text)
        .into_iter()
        .find(|r| r.starts_with(&format!("{n:>2} ")) && r.contains(code))
        .unwrap_or_else(|| panic!("no row for L{n}\n{text}"));
    row.split_once(code).unwrap().1.trim().to_string()
}

const HL_100: &str = " 1   DB_URL=postgres://demo:demo@localhost/app
     · the key
     ┆ DB connection string
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info                             ┆ how chatty the logs are
 4   PORT=8080
";

#[test]
fn highlight_and_note_go_under_l1_at_100_columns() {
    let t = render("two-labels-hl.tiza", 100, &[]);
    assert_eq!(t, HL_100, "{t}");
    assert_eq!(margin_of(&t, 2, "API_TOKEN=demo-token-123"), "");
    // A line with one label keeps it in the margin.
    assert_eq!(
        margin_of(&t, 3, "LOG_LEVEL=info"),
        "┆ how chatty the logs are"
    );
}

#[test]
fn mark_and_note_and_highlight_and_mark() {
    let t = render("two-labels-mark.tiza", 100, &[]);
    assert_eq!(
        t,
        " 1   DB_URL=postgres://demo:demo@localhost/app
     · ! warning: the key
     ┆ DB connection string
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info                             ┆ how chatty the logs are
 4   PORT=8080
",
        "{t}"
    );
    let t = render("two-labels-hl-mark.tiza", 100, &[]);
    assert_eq!(
        t,
        " 1   DB_URL=postgres://demo:demo@localhost/app
     · the key
     · ! warning: read at startup
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info                             ┆ how chatty the logs are
 4   PORT=8080
",
        "{t}"
    );
}

#[test]
fn sixty_columns_are_the_narrow_layout_as_before() {
    // The 60-column renders, as `origin/main` (c8a271e) drew them.
    let cases = [
        (
            "two-labels-hl.tiza",
            " 1   DB_URL=postgres://demo:demo@localhost/app
     · the key
     ┆ DB connection string
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info
     ┆ how chatty the logs are
 4   PORT=8080
",
        ),
        (
            "two-labels-mark.tiza",
            " 1   DB_URL=postgres://demo:demo@localhost/app
     · ! warning: the key
     ┆ DB connection string
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info
     ┆ how chatty the logs are
 4   PORT=8080
",
        ),
        (
            "two-labels-hl-mark.tiza",
            " 1   DB_URL=postgres://demo:demo@localhost/app
     · the key
     · ! warning: read at startup
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info
     ┆ how chatty the logs are
 4   PORT=8080
",
        ),
    ];
    for (name, want) in cases {
        let t = render(name, 60, &[]);
        assert_eq!(t, want, "{name}\n{t}");
        // The rows under L1 are the same ones the 100-column render draws.
        let wide = render(name, 100, &[]);
        assert_eq!(rows(&t)[..4], rows(&wide)[..4], "{name}");
    }
}

#[test]
fn examples_with_one_label_per_line_render_as_before() {
    // `qa/fixtures/render/two_labels_on_a_line/expected/<example>-<width>.txt`: `render
    // --all-steps --format txt` of the example by the binary of c8a271e
    // (before two labels on a line), the steps one after another behind `== step-NN.txt`.
    // Later layout decisions change them on purpose, in the commit of each
    // decision: a dimmed mark keeps its label in the margin.
    // ownership.rs at 108 is the README's width (scripts/readme-images.sh).
    // ownership-moves (L7) and stack-impl (L20) change on purpose at steps
    // 4-5 and are not here.
    for (name, width) in [
        ("ownership.rs", 108),
        ("ownership.rs", 130),
        ("stack-and-queue", 100),
        ("stack-and-queue", 130),
        ("tally-tour", 100),
        ("tally-tour", 130),
    ] {
        let dir = scratch(&format!("ac3-{name}-{width}"));
        let out = format!("{}/", dir.display());
        // The ownership example as it was then: its own copy, since the
        // example changed later (shorter labels and code).
        let file = if name == "ownership.rs" {
            fixture("render/two_labels_on_a_line/ownership.rs.tiza")
        } else {
            example(&format!("{name}.tiza"))
        };
        let w = width.to_string();
        tiza_ok(&[
            "render",
            file.to_str().unwrap(),
            "--all-steps",
            "--width",
            &w,
            "--format",
            "txt",
            "--out",
            &out,
        ]);
        let mut steps: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        steps.sort();
        let mut got = String::new();
        for s in &steps {
            got.push_str(&format!("== {s}\n"));
            got.push_str(&std::fs::read_to_string(dir.join(s)).unwrap());
        }
        let want = std::fs::read_to_string(fixture(&format!(
            "render/two_labels_on_a_line/expected/{name}-{width}.txt"
        )))
        .unwrap();
        assert!(
            got == want,
            "{name} at {width} differs from before two labels on a line:\n{got}"
        );
    }
}

fn live(name: &str, test: &str) -> Live {
    let dir = scratch(test);
    Live::open(open(name), dir.join("doc.tiza"), dir.join("live.sock"))
}

/// The screen rows from L1's row on, trimmed, without the status bar.
fn from_l1(screen: &str) -> Vec<String> {
    let all = rows(screen);
    let i = all
        .iter()
        .position(|r| r.contains("DB_URL="))
        .unwrap_or_else(|| panic!("no L1 on screen\n{screen}"));
    all[i..i + 6].to_vec()
}

#[test]
fn tui_switches_l1_to_under_rows_when_its_second_label_arrives() {
    let live = live("two-labels-hl.tiza", "tui");
    live.command("step 0");
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(live.with(|a| a.doc.step), 1);
    let s1 = from_l1(&live.screen(100, 30));
    assert!(
        s1[0].ends_with("DB_URL=postgres://demo:demo@localhost/app  the key"),
        "step 1: the one label is in the margin\n{s1:#?}"
    );
    assert!(s1[1].contains("API_TOKEN="), "{s1:#?}");
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(live.with(|a| a.doc.step), 2);
    let s2 = from_l1(&live.screen(100, 30));
    assert!(
        s2[0].ends_with("DB_URL=postgres://demo:demo@localhost/app"),
        "{s2:#?}"
    );
    assert!(s2[1].ends_with("· the key"), "{s2:#?}");
    assert!(s2[2].ends_with("┆ DB connection string"), "{s2:#?}");
    assert!(s2[3].ends_with("API_TOKEN=demo-token-123"), "{s2:#?}");
    // The TUI draws the same rows as `tiza render --step 2` (AC5).
    let r = render("two-labels-hl.tiza", 100, &["--step", "2"]);
    for (screen, txt) in s2.iter().zip(rows(&r)) {
        assert!(screen.ends_with(txt.trim_start()), "«{screen}» vs «{txt}»");
    }
}

#[test]
fn second_label_at_a_later_step_moves_the_code_down_like_narrow() {
    // Step 1: one label on L1, in the margin; step 2: the note arrives and
    // L1 switches to under rows, so L2 moves two rows down (the label of
    // step 1 and the note) — at 60 columns the label of step 1 was under
    // L1 already and the note moves L2 one row down.
    let s1 = render("two-labels-hl.tiza", 100, &["--step", "1"]);
    assert_eq!(
        s1,
        " 1   DB_URL=postgres://demo:demo@localhost/app  the key
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info                             ┆ how chatty the logs are
 4   PORT=8080
"
    );
    let s2 = render("two-labels-hl.tiza", 100, &["--step", "2"]);
    assert_eq!(s2, HL_100);
    let doc = open("two-labels-hl.tiza");
    for width in [60usize, 100] {
        let l1 = lay(&doc, width, 1, false).row_of_line[&2];
        let l2 = lay(&doc, width, 2, false).row_of_line[&2];
        assert_eq!(l2, l1 + if width == 60 { 1 } else { 2 }, "width {width}");
    }
    // While step 2's typing runs its annotations wait: L1 still has
    // one label and stays in the margin until the typing ends.
    let typing = lay(&doc, 100, 2, true);
    assert_eq!(typing.row_of_line[&2], 1);
    // `--all-steps` pads every step to the tallest one, and a GIF takes them.
    let dir = scratch("all-steps");
    let out = format!("{}/", dir.display());
    let p = path("two-labels-hl.tiza");
    tiza_ok(&[
        "render",
        &p,
        "--all-steps",
        "--width",
        "100",
        "--format",
        "txt",
        "--out",
        &out,
    ]);
    let step2 = std::fs::read_to_string(dir.join("step-02.txt")).unwrap();
    assert_eq!(step2, HL_100);
    let gif = dir.join("take.gif");
    tiza_ok(&[
        "render",
        &p,
        "--width",
        "100",
        "--out",
        gif.to_str().unwrap(),
    ]);
    assert!(std::fs::metadata(&gif).unwrap().len() > 0);
}

#[test]
fn three_labels_on_one_line_all_under_it_in_order() {
    let t = render("three-labels.tiza", 100, &[]);
    assert_eq!(
        t,
        " 1   DB_URL=postgres://demo:demo@localhost/app
     · the key
     · ! warning: read at startup
     ┆ DB connection string
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info                             ┆ how chatty the logs are
 4   PORT=8080
",
        "{t}"
    );
    // Same order as the narrow layout.
    assert_eq!(
        rows(&render("three-labels.tiza", 60, &[]))[..5],
        rows(&t)[..5]
    );
}

#[test]
fn two_consecutive_lines_each_under_its_own_line() {
    let t = render("consecutive.tiza", 100, &[]);
    assert_eq!(
        t,
        " 1   DB_URL=postgres://demo:demo@localhost/app
     · the key
     ┆ DB connection string
 2   API_TOKEN=demo-token-123
     · ! warning: demo token
     ┆ sent as a bearer header
 3   LOG_LEVEL=info
 4   PORT=8080                                  ┆ one label: stays in the margin
",
        "{t}"
    );
}

#[test]
fn box_arrow_and_spotlight_follow_the_extra_rows() {
    let t = render("geometry.tiza", 100, &[]);
    assert_eq!(
        t,
        "       ╭─ secrets ──────────────────────────────────╮
 1 ╭─╭─┼▶ DB_URL=postgres://demo:demo@localhost/app │
   │ │ │  ─▶ read first                             │
   │ │ │  · the key                                 │
   │ │ │  ┆ DB connection string                    │
 2 │ │ │  API_TOKEN=demo-token-123                  │
   │ │ ╰────────────────────────────────────────────╯
 3 │ ╰─── LOG_LEVEL=info
 4 ╰────▶ PORT=8080                                   same service
",
        "{t}"
    );
    // The spotlight on L1 (step 2) keeps the same rows: the code of the
    // other lines is dimmed, the labels under L1 keep their colour.
    let doc = open("geometry.tiza");
    let (s1, s2) = (lay(&doc, 100, 1, false), lay(&doc, 100, 2, false));
    assert_eq!(s1.rows, s2.rows);
    assert_eq!(s1.canvas.to_plain_text(), s2.canvas.to_plain_text());
}

#[test]
fn presentation_mode_uses_the_same_rule() {
    let live = live("two-labels-hl.tiza", "present");
    live.with(|a| {
        a.set_present(true);
        a.set_step(2);
    });
    let s = live.screen(100, 30);
    let r = from_l1(&s);
    assert!(r[1].ends_with("· the key"), "{s}");
    assert!(r[2].ends_with("┆ DB connection string"), "{s}");
    assert!(r[3].ends_with("API_TOKEN=demo-token-123"), "{s}");
}

#[test]
fn check_counts_the_new_rows() {
    // Before two labels on a line, consecutive.tiza lost «sent as a bearer header» at 100
    // columns and no width showed every label; now nothing is clipped from
    // 81 columns on (below that the last line's margin note is clipped).
    let p = path("consecutive.tiza");
    let out = tiza_ok(&["check", &p, "--width", "100"]);
    assert!(out.contains("0 errors, 0 warnings"), "{out}");
    assert!(out.contains("no label is clipped at 81 or wider"), "{out}");
    // two-labels-hl is clean under the lines from 30 columns, but at 62,
    // where the margin first appears, step 2 clips the note of L3: the
    // narrowest width from which every wider one is clean is 63.
    let p = path("two-labels-hl.tiza");
    let out = tiza_ok(&["check", &p, "--width", "100"]);
    assert!(out.contains("0 errors, 0 warnings"), "{out}");
    assert!(out.contains("no label is clipped at 63 or wider"), "{out}");
    // The library agrees, step by step.
    let doc = open("consecutive.tiza");
    assert!(lay(&doc, 100, 1, false).clipped_labels.is_empty());
    assert_eq!(
        tiza_core::check::min_clean_width(&doc, &tiza_core::Theme::dark()),
        Some(81)
    );
}

#[test]
fn every_export_draws_the_same_rows() {
    let p = path("two-labels-hl.tiza");
    let txt = render("two-labels-hl.tiza", 100, &[]);
    // ansi without its escapes is the txt.
    let ansi = tiza_ok(&["render", &p, "--width", "100", "--format", "ansi"]);
    let mut plain = String::new();
    let mut esc = false;
    for c in ansi.chars() {
        match (esc, c) {
            (false, '\x1b') => esc = true,
            (true, 'm') => esc = false,
            (true, _) => {}
            (false, c) => plain.push(c),
        }
    }
    assert_eq!(rows(&plain), rows(&txt));
    // md wraps the same text.
    let md = tiza_ok(&["render", &p, "--width", "100", "--format", "md"]);
    assert!(md.contains(txt.trim_end()), "{md}");
    // SVG: the two labels are on their own rows, between L1 and L2, at
    // the code's x + 2.
    let svg = tiza_ok(&[
        "render",
        &p,
        "--width",
        "100",
        "--format",
        "svg",
        "--no-embed-fonts",
    ]);
    let at = |needle: &str| -> (f32, f32) {
        let i = svg.find(&format!(">{needle}")).expect(needle);
        let tag = &svg[svg[..i].rfind("<text").unwrap()..i];
        let num = |attr: &str| -> f32 {
            let s = &tag[tag.find(attr).unwrap() + attr.len()..];
            s[..s.find('"').unwrap()].parse().unwrap()
        };
        (num(" x=\""), num(" y=\""))
    };
    let (l1, key, note, l2) = (
        at("DB_URL"),
        at("the key"),
        at("DB connection string"),
        at("API_TOKEN"),
    );
    assert!(l1.1 < key.1 && key.1 < note.1 && note.1 < l2.1, "{svg}");
    assert_eq!(key.0, note.0);
    assert!(key.0 > l1.0);
    // PNG: the same canvas as the SVG (scale 2).
    let dir = scratch("png");
    let png = dir.join("hl.png");
    tiza_ok(&[
        "render",
        &p,
        "--width",
        "100",
        "--out",
        png.to_str().unwrap(),
    ]);
    let bytes = std::fs::read(&png).unwrap();
    let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    let svg_h: f32 = {
        let s = &svg[svg.find(" height=\"").unwrap() + 9..];
        s[..s.find('"').unwrap()].parse().unwrap()
    };
    assert_eq!(h as f32, svg_h * 2.0);
}

#[test]
fn a_margin_callout_with_no_room_counts_as_a_label_of_its_line() {
    // QA's callout-short: the callout has no room for its block on L3 and
    // falls back; with the note it goes under L3 (it sat beside L4 before),
    // as a whole block since the margin callouts (it was one row cut with `…`).
    let t = render("callout-short.tiza", 100, &["--step", "1"]);
    assert_eq!(
        t,
        " 1   DB_URL=postgres://demo:demo@localhost/app
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info
     ┆ how chatty the logs are
     ╭──────────────────────╮
     │ set to debug locally │
     ╰──────────────────────╯
 4   PORT=8080
",
        "{t}"
    );
    // callout-short2: L4 has a label of its own, and the callout was not
    // drawn at all; `check` said no width showed every label.
    let t = render("callout-short2.tiza", 100, &["--step", "1"]);
    assert_eq!(
        t,
        " 1   DB_URL=postgres://demo:demo@localhost/app
 2   API_TOKEN=demo-token-123
 3   LOG_LEVEL=info
     ┆ how chatty the logs are
     ╭──────────────────────╮
     │ set to debug locally │
     ╰──────────────────────╯
 4   PORT=8080                                  ┆ the port
",
        "{t}"
    );
    let p = path("callout-short2.tiza");
    let out = tiza_ok(&["check", &p, "--width", "100"]);
    assert!(out.contains("0 errors, 0 warnings"), "{out}");
    assert!(out.contains("no label is clipped at 30 or wider"), "{out}");
    // A callout whose block fits stays a block in the margin, beside the
    // two labels under L1.
    let t = render("callout-fits.tiza", 100, &[]);
    assert!(
        t.starts_with(
            " 1   DB_URL=postgres://demo:demo@localhost/app  ╭─────────╮
     · the key                                  │ short   │
     ┆ DB connection string                     │ callout │
"
        ),
        "{t}"
    );
}

#[test]
fn labels_under_a_line_keep_the_fill_of_their_box() {
    // stack-impl step 5: the rows under L20 are inside the filled `impl`
    // box; every cell of them between the rails has the fill the code rows
    // have (it was the base background behind the text), at 100 and 60.
    let doc = tiza_core::fs::open(example("stack-impl.tiza").to_str().unwrap())
        .unwrap()
        .document;
    for width in [100usize, 60] {
        let l = lay(&doc, width, 5, false);
        let y20 = l.row_of_line[&20];
        let y21 = l.row_of_line[&21];
        assert!(y21 > y20 + 1, "rows under L20 at {width}");
        let fill = l.canvas.get(l.code_x, y21).unwrap().style.bg;
        for y in y20 + 1..y21 {
            assert!(matches!(l.rows[y], Row::Under(_)));
            for x in l.code_x..l.code_x + 20 {
                let c = l.canvas.get(x, y).unwrap();
                assert_eq!(
                    c.style.bg,
                    fill,
                    "width {width}, cell {x},{y} «{}»\n{}",
                    c.sym.as_str(),
                    l.canvas.to_plain_text()
                );
            }
        }
    }
}
