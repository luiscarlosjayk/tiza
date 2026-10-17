//! t-111 — Stable geometry across steps in presentation mode and exports.
//!
//! Acceptance criteria:
//! - AC1 In presentation mode and in every export that spans steps (`render
//!   --step N`, `.cast`, `.tape`) the arrow lanes, the box band, the code
//!   column and the margin are sized once for the maximum over every step,
//!   so the code's x and the margin width never change between steps.
//! - AC2 Authoring mode keeps the dynamic layout; `:layout stable|dynamic`
//!   forces the take's geometry while rehearsing and the status line shows
//!   `⊞ stable`.
//! - AC3 A label that fits at step 1 still fits at step 7: the margin never
//!   shrinks mid-take.
//! - AC5 For the example, code x and margin are equal at every step in
//!   presentation/export and differ in normal mode.

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_core::layout::{LayoutOptions, StableGeometry, layout, stable_geometry};
use tiza_qa::{Live, example, tiza_ok};

fn ownership() -> tiza_core::Document {
    tiza_core::fs::open(example("ownership.rs.tiza").to_str().unwrap())
        .unwrap()
        .document
}

/// x of the first code cell and width of the right margin at `step`.
fn geometry(doc: &tiza_core::Document, step: u32, stable: bool) -> (usize, usize) {
    let l = layout(
        doc,
        &tiza_core::Theme::dark(),
        &LayoutOptions {
            width: 100,
            step,
            stable,
            ..Default::default()
        },
    );
    // The margin starts after the widest code line (+ the box rail).
    let code_w = doc
        .code
        .iter()
        .map(|t| t.chars().count())
        .max()
        .unwrap_or(0);
    (l.code_x, 100usize.saturating_sub(l.code_x + code_w + 2))
}

#[test]
fn ac1_ac5_stable_layout_puts_the_code_at_one_x_for_every_step() {
    let doc = ownership();
    let last = doc.max_step();
    assert!(last >= 7, "the example has {last} steps");
    let stable: Vec<_> = (1..=last).map(|s| geometry(&doc, s, true)).collect();
    let dynamic: Vec<_> = (1..=last).map(|s| geometry(&doc, s, false)).collect();
    assert!(
        stable.iter().all(|g| *g == stable[0]),
        "stable geometry changes between steps: {stable:?}"
    );
    assert!(
        dynamic.iter().any(|g| *g != dynamic[0]),
        "the dynamic layout should still grow with the steps: {dynamic:?}"
    );
    // The take's geometry is the maximum over the steps, never less.
    assert_eq!(stable[0].0, dynamic.iter().map(|g| g.0).max().unwrap());
    // And it is what `stable_geometry` reports, so a recorder can check its
    // frames against one struct.
    let g: StableGeometry = stable_geometry(&doc, &LayoutOptions::default());
    assert!(g.lane_count >= 2, "{g:?}");
    assert_eq!(stable[0].0, g.num_w + g.lane_count * 2 + g.box_depth + 2);
}

#[test]
fn ac1_each_arrow_keeps_its_lane_across_the_take() {
    let doc = ownership();
    let th = tiza_core::Theme::dark();
    // The first arrow of the example is drawn at step 1; at step 7 three more
    // are on screen. In a take its rail stays in the same column.
    let rail_x = |step: u32| {
        let l = layout(
            &doc,
            &th,
            &LayoutOptions {
                width: 100,
                step,
                ..Default::default()
            },
        );
        let y = l.row_of_line[&3];
        (0..l.code_x)
            .find(|x| {
                l.canvas
                    .get(*x, y)
                    .is_some_and(|c| c.sym == "╭" || c.sym == "│")
            })
            .unwrap_or_else(|| panic!("no rail at step {step}:\n{}", l.canvas.to_plain_text()))
    };
    let at_1 = rail_x(1);
    for step in 2..=7 {
        assert_eq!(rail_x(step), at_1, "the first arrow moved at step {step}");
    }
}

#[test]
fn ac1_render_per_step_and_the_cast_share_the_geometry() {
    let path = example("ownership.rs.tiza");
    let path = path.to_str().unwrap();
    let code_col = |text: &str| {
        text.lines()
            .find(|l| l.contains("fn main()"))
            .map(|l| l.find("fn main()").unwrap())
            .expect("fn main() in the frame")
    };
    let cols: Vec<usize> = [1u32, 3, 5, 7]
        .iter()
        .map(|s| {
            code_col(&tiza_ok(&[
                "render",
                path,
                "--step",
                &s.to_string(),
                "--width",
                "100",
            ]))
        })
        .collect();
    assert!(
        cols.iter().all(|c| *c == cols[0]),
        "render --step: {cols:?}"
    );
    // The cast's frames: every step at the same column (frames are ANSI; the
    // text between escapes keeps the column count once they are stripped).
    let dir = tiza_qa::scratch("t111-cast");
    let out = dir.join("take.cast");
    tiza_ok(&[
        "render",
        path,
        "--format",
        "cast",
        "--width",
        "100",
        "--out",
        out.to_str().unwrap(),
    ]);
    let cast = std::fs::read_to_string(&out).unwrap();
    let strip = |s: &str| {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    };
    let mut frame_cols = Vec::new();
    for line in cast.lines().skip(1) {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        if v[1] != "o" {
            continue;
        }
        let text = strip(v[2].as_str().unwrap());
        if let Some(l) = text.lines().find(|l| l.contains("fn main()")) {
            frame_cols.push(l.find("fn main()").unwrap());
        }
    }
    assert!(frame_cols.len() >= 7, "frames: {frame_cols:?}");
    assert!(
        frame_cols.iter().all(|c| *c == frame_cols[0]),
        "the code slides in the cast: {frame_cols:?}"
    );
    assert_eq!(frame_cols[0], cols[0]);
}

#[test]
fn ac3_a_label_that_fits_at_step_1_still_fits_at_step_7() {
    let doc = ownership();
    let th = tiza_core::Theme::dark();
    let text_at = |step: u32, width: usize| {
        layout(
            &doc,
            &th,
            &LayoutOptions {
                width,
                step,
                ..Default::default()
            },
        )
        .canvas
        .to_plain_text()
    };
    // Wide enough for the longest label at the widest step.
    let s1 = text_at(1, 120);
    let s7 = text_at(7, 120);
    assert!(s1.contains("move: s moves into takes"), "{s1}");
    assert!(s7.contains("move: s moves into takes"), "{s7}");
    // Narrower: the label starts in the same column at both steps (the
    // margin does not shrink mid-take); whether it wraps into the row below
    // or is cut with `…` depends only on what else sits in the margin.
    let s1 = text_at(1, 100);
    let s7 = text_at(7, 100);
    let label_x = |t: &str| t.lines().find_map(|l| l.find("▶ move: s")).unwrap();
    assert_eq!(label_x(&s1), label_x(&s7));
}

#[test]
fn ac2_authoring_is_dynamic_unless_layout_stable_and_presentation_is_always_stable() {
    let dir = tiza_qa::scratch("t111-live");
    let live = Live::open(ownership(), dir.join("doc.tiza"), dir.join("live.sock"));
    let code_col = |live: &Live| {
        let s = live.screen(100, 30);
        s.lines()
            .find(|l| l.contains("fn main()"))
            .map(|l| l.find("fn main()").unwrap())
            .expect("fn main() on screen")
    };
    live.api_ok("step.set", json!({"step": 1}));
    let dyn_1 = code_col(&live);
    live.api_ok("step.set", json!({"step": 7}));
    let dyn_7 = code_col(&live);
    assert!(dyn_1 < dyn_7, "authoring should grow: {dyn_1} vs {dyn_7}");
    assert!(!live.screen(100, 30).contains("⊞ stable"));

    live.command("layout");
    assert_eq!(live.message().unwrap().0, "layout dynamic");
    live.command("layout stable");
    assert!(live.message().unwrap().0.starts_with("layout stable"));
    assert!(live.screen(100, 30).contains("⊞ stable"));
    live.api_ok("step.set", json!({"step": 1}));
    assert_eq!(
        code_col(&live),
        dyn_7,
        "stable rehearsal uses the take's geometry"
    );
    live.command("layout bogus");
    let (m, err) = live.message().unwrap();
    assert!(err && m.contains(":layout stable"), "{m}");
    live.command("layout dynamic");
    assert_eq!(code_col(&live), dyn_1);

    // Presentation mode: stable whatever the toggle says, no badge (no UI).
    live.keys(&[KeyCode::Char('P')]);
    assert_eq!(code_col(&live), dyn_7);
    live.keys(&[KeyCode::Char('n')]);
    live.keys(&[KeyCode::Char('n')]);
    assert_eq!(code_col(&live), dyn_7, "the code slid during the take");
    live.command("layout");
    assert!(
        live.message()
            .unwrap()
            .0
            .contains("presentation mode is always stable")
    );
}
