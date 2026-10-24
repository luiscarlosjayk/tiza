//! t-113 — Right-margin width cap: `:margin N` / document field `margin`.
//!
//! Acceptance criteria:
//! - AC1 `:margin N` caps the right margin at N cells, `:margin auto`
//!   restores the dynamic width, `:margin` reports; saved in the document
//!   (`margin: N`) and honoured by `render` (`--margin-cols N` overrides).
//! - AC2 Notes, labels and legends wrap at the cap (same rules); labels that
//!   do not fit are clipped with `…` as today.
//! - AC3 The cap never pushes labels under the line on a wide terminal;
//!   arrows and boxes do not change.
//! - AC4 `status.margin`, `scene.patch {margin}`, schema; this file; README;
//!   CHANGELOG.

use serde_json::json;
use tiza_core::layout::{LayoutOptions, layout};
use tiza_qa::{Live, fixture, scratch, tiza, tiza_ok};

fn ownership() -> tiza_core::Document {
    tiza_core::fs::open(fixture("t-113/ownership.rs.tiza").to_str().unwrap())
        .unwrap()
        .document
}

fn text(doc: &tiza_core::Document, width: usize, cap: Option<usize>) -> String {
    layout(
        doc,
        &tiza_core::Theme::dark(),
        &LayoutOptions {
            width,
            step: 7,
            margin_cap: cap,
            ..Default::default()
        },
    )
    .canvas
    .to_plain_text()
}

#[test]
fn ac1_ac2_ac3_the_cap_wraps_notes_and_ends_the_frame() {
    let doc = ownership();
    let wide = text(&doc, 160, None);
    let note = "s no longer owns it: using it here does not compile";
    assert!(wide.contains(note), "one row on a wide terminal:\n{wide}");
    let capped = text(&doc, 160, Some(24));
    assert!(!capped.contains(note), "{capped}");
    assert!(capped.contains("┆ s no longer owns it:"), "{capped}");
    assert!(capped.contains("using it here does"), "{capped}");
    // Labels still in the margin (not under the line), clipped with … when
    // longer than the cap; arrows and boxes as before.
    assert!(capped.contains("━▶ move: s moves into …"), "{capped}");
    assert!(
        !capped.contains("· ━▶ move"),
        "pushed under the line:\n{capped}"
    );
    let code_x = |t: &str| {
        t.lines()
            .find(|l| l.contains("fn main()"))
            .unwrap()
            .find("fn")
            .unwrap()
    };
    assert_eq!(code_x(&wide), code_x(&capped));
    assert!(capped.contains("╭─ takes owns s and frees it at the end"));
    // The frame ends at the cap: no row is wider than margin start + cap.
    let widest = capped.lines().map(|l| l.chars().count()).max().unwrap();
    let margin_x = capped
        .lines()
        .find(|l| l.contains("┆ s no longer"))
        .map(|l| l.find("┆").unwrap())
        .unwrap();
    assert!(widest <= margin_x + 24, "{widest} > {margin_x} + 24");
    assert!(wide.lines().map(|l| l.chars().count()).max().unwrap() > widest);
    // A cap wider than the dynamic margin changes nothing.
    assert_eq!(text(&doc, 100, Some(200)), text(&doc, 100, None));
    // Corner: a cap smaller than a mark's glyph + legend wraps the legend,
    // the glyph stays.
    let tiny = text(&doc, 160, Some(12));
    assert!(tiny.contains("✎▶ &mut"), "{tiny}");
}

#[test]
fn ac1_ac4_tui_command_document_field_status_and_patch() {
    let dir = scratch("t113-live");
    let live = Live::open(ownership(), dir.join("doc.tiza"), dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": 7}));
    let note = "s no longer owns it: using it here does not compile";
    assert!(live.screen(160, 45).contains(note));
    live.command("margin");
    assert!(live.message().unwrap().0.starts_with("margin auto"));
    live.command("margin 24");
    let (m, err) = live.message().unwrap();
    assert!(!err && m.starts_with("margin 24"), "{m}");
    let s = live.screen(160, 45);
    assert!(
        !s.contains(note) && s.contains("┆ s no longer owns it:"),
        "{s}"
    );
    live.command("margin");
    assert!(live.message().unwrap().0.starts_with("margin 24"));
    assert_eq!(live.api_ok("status", json!({}))["margin"], 24);
    live.command("w");
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("doc.tiza")).unwrap()).unwrap();
    assert_eq!(saved["margin"], 24);
    // Errors name the range; the document is untouched.
    live.command("margin 5");
    let (m, err) = live.message().unwrap();
    assert!(err && m.contains("between 12 and 200"), "{m}");
    live.command("margin wide");
    let (m, err) = live.message().unwrap();
    assert!(err && m.contains(":margin auto"), "{m}");
    assert_eq!(live.api_ok("status", json!({}))["margin"], 24);
    // Undo, auto, and the agent's patch.
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert!(live.message().unwrap().0.contains("undone: margin"));
    assert!(live.api_ok("status", json!({})).get("margin").is_none());
    live.api_ok("scene.patch", json!({"margin": 30}));
    assert_eq!(live.api_ok("status", json!({}))["margin"], 30);
    let r = live.api("scene.patch", json!({"margin": 3}));
    assert!(r.error.unwrap().message.contains("between 12 and 200"));
    live.api_ok("scene.patch", json!({"margin": null}));
    assert!(live.api_ok("status", json!({})).get("margin").is_none());
    live.command("margin auto");
    assert_eq!(live.message().unwrap().0, "margin already auto");
    // The export follows the field.
    live.command("margin 24");
    let out = dir.join("x.txt");
    live.command(&format!("export {} width=160", out.display()));
    let t = std::fs::read_to_string(&out).unwrap();
    assert!(
        !t.contains(note) && t.contains("┆ s no longer owns it:"),
        "{t}"
    );
}

#[test]
fn ac1_render_honours_the_field_and_margin_cols_overrides_it() {
    let dir = scratch("t113-render");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixture("t-113/ownership.rs.tiza")).unwrap())
            .unwrap();
    doc["margin"] = json!(24);
    doc["code"] = json!({"file": fixture("t-113/ownership.rs").to_str().unwrap()});
    let path = dir.join("capped.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    let p = path.to_str().unwrap();
    let note = "s no longer owns it: using it here does not compile";
    let capped = tiza_ok(&["render", p, "--step", "7", "--width", "160"]);
    assert!(!capped.contains(note), "{capped}");
    let wider = tiza_ok(&[
        "render",
        p,
        "--step",
        "7",
        "--width",
        "160",
        "--margin-cols",
        "80",
    ]);
    assert!(wider.contains(note), "{wider}");
    let bad = tiza(&["render", p, "--margin-cols", "5"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("between 12 and 200"));
    // A PNG is no wider than the content: the capped one is narrower.
    let a = dir.join("a.png");
    let b = dir.join("b.png");
    tiza_ok(&[
        "render",
        p,
        "--step",
        "7",
        "--width",
        "160",
        "--out",
        a.to_str().unwrap(),
    ]);
    tiza_ok(&[
        "render",
        p,
        "--step",
        "7",
        "--width",
        "160",
        "--margin-cols",
        "80",
        "--out",
        b.to_str().unwrap(),
    ]);
    let w = |p: &std::path::Path| {
        let bytes = std::fs::read(p).unwrap();
        u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]])
    };
    assert!(w(&a) < w(&b), "{} vs {}", w(&a), w(&b));
    // Documents without the field: unchanged (t-053 snapshots cover it).
    assert!(ownership().margin.is_none());
}
