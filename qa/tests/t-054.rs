//! t-054 — Render budget: a frame in under 16 ms with 2,000 lines and 200
//! annotations (release build).
//!
//! Acceptance criteria:
//! - AC1 With 2,000 lines of Rust and 200 annotations of every kind, the
//!   median frame (layout + draw into a 120×40 buffer) takes < 16 ms in
//!   release (< 250 ms in debug, where the test only guards against
//!   regressions of an order of magnitude).
//! - AC2 Syntax highlighting is memoized: laying out the same code twice
//!   returns the same highlighted segments without running syntect again
//!   (`highlight_cached` returns the same `Arc`); editing the code or changing
//!   the language/theme invalidates it.
//! - AC3 The status bar reuses the frame's layout instead of laying the
//!   document out a second time (`viewport_warnings_for`).
//! - AC4 Canvas cells store their grapheme inline (no heap allocation per
//!   cell); clusters longer than 22 bytes still work (heap fallback).
//!
//! Before: 222 ms per frame (98 ms syntect, a second layout for the status
//! bar, ~10 ms of `String` allocations for 260,000 cells). After: ~7 ms.

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tiza_core::canvas::Sym;
use tiza_core::highlight::highlight_cached;
use tiza_qa::Live;
use two_face::theme::EmbeddedThemeName;

fn big_doc() -> String {
    let mut s = String::new();
    for i in 0..400 {
        s.push_str(&format!(
            "fn f{i}(s: String) -> usize {{\n    let n = s.len(); // {i}\n    println!(\"{{n}}\");\n    takes(s);\n}}\n"
        ));
    }
    s
}

fn big_live(name: &str) -> Live {
    let live = Live::from_code(&big_doc(), "rs", name);
    assert_eq!(live.with(|a| a.doc.code.len()), 2000);
    for i in 0..200u32 {
        let line = 1 + (i * 10) % 1990;
        let r = match i % 5 {
            0 => live.api(
                "annotation.add",
                json!({"type":"note","line":line,"text":format!("note {i}")}),
            ),
            1 => live.api(
                "annotation.add",
                json!({"type":"box","from_line":line,"to_line":line+3,"label":"box"}),
            ),
            2 => live.api(
                "annotation.add",
                json!({"type":"arrow","from":{"line":line,"col":4},"to":{"line":line+3,"col":4},"label":"mv"}),
            ),
            3 => live.api(
                "annotation.add",
                json!({"type":"highlight","at":{"line":line,"start":0,"end":5}}),
            ),
            _ => live.api(
                "annotation.add",
                json!({"type":"mark","kind":"move","at":{"line":line+1,"start":4,"end":7},"label":"m"}),
            ),
        };
        assert!(r.error.is_none(), "{:?}", r.error);
    }
    assert_eq!(live.with(|a| a.doc.annotations.len()), 200);
    live
}

#[test]
fn ac1_frame_under_budget() {
    let live = big_live("t054-budget");
    let _ = live.screen(120, 40); // warm the caches (syntect grammars, highlight)
    let mut samples: Vec<Duration> = (0..21)
        .map(|_| {
            let t = Instant::now();
            let _ = live.screen(120, 40);
            t.elapsed()
        })
        .collect();
    samples.sort();
    let median = samples[samples.len() / 2];
    let budget = if cfg!(debug_assertions) {
        Duration::from_millis(250)
    } else {
        Duration::from_millis(16)
    };
    eprintln!("median frame: {median:?} (budget {budget:?})");
    assert!(
        median < budget,
        "median frame {median:?} over the budget {budget:?}"
    );
    // Scrolling to the end and moving the cursor keep the same cost.
    live.command("step 1");
    live.with(|a| a.cursor_line = 1990);
    let t = Instant::now();
    let _ = live.screen(120, 40);
    assert!(t.elapsed() < budget * 2, "{:?}", t.elapsed());
}

#[test]
fn ac2_highlight_is_memoized_and_invalidated() {
    let code: Vec<String> = big_doc().lines().map(str::to_string).collect();
    let a = highlight_cached(&code, "rs", EmbeddedThemeName::OneHalfDark);
    let t = Instant::now();
    let b = highlight_cached(&code, "rs", EmbeddedThemeName::OneHalfDark);
    let hit = t.elapsed();
    assert!(Arc::ptr_eq(&a, &b), "second call returns the cached Arc");
    assert!(hit < Duration::from_millis(5), "cache hit took {hit:?}");
    // Another language, theme or code: a different result.
    let c = highlight_cached(&code, "py", EmbeddedThemeName::OneHalfDark);
    assert!(!Arc::ptr_eq(&a, &c));
    let d = highlight_cached(&code, "rs", EmbeddedThemeName::OneHalfLight);
    assert!(!Arc::ptr_eq(&a, &d));
    let mut edited = code.clone();
    edited[1000].push('x');
    let e = highlight_cached(&edited, "rs", EmbeddedThemeName::OneHalfDark);
    assert!(!Arc::ptr_eq(&a, &e));
    assert_ne!(e[1000], a[1000]);
    // The first entry is still cached after three more (cache of four).
    let f = highlight_cached(&code, "rs", EmbeddedThemeName::OneHalfDark);
    assert!(Arc::ptr_eq(&a, &f));
}

#[test]
fn ac3_status_bar_reuses_the_layout() {
    let live = big_live("t054-status");
    live.with(|a| a.size = (120, 40));
    // The warnings computed from a given layout equal the standalone ones.
    let (from_layout, standalone) = live.with(|a| {
        let l = a.layout(118);
        (a.viewport_warnings_for(&l), a.viewport_warnings())
    });
    assert_eq!(from_layout, standalone);
    // Drawing with the status bar costs about the same as presentation mode
    // (which has no status bar): no second layout hides in the bar.
    let time = |present: bool| {
        live.with(move |a| a.present = present);
        let _ = live.screen(120, 40);
        let mut s: Vec<Duration> = (0..11)
            .map(|_| {
                let t = Instant::now();
                let _ = live.screen(120, 40);
                t.elapsed()
            })
            .collect();
        s.sort();
        s[5]
    };
    let with_bar = time(false);
    let without = time(true);
    assert!(
        with_bar < without * 2 + Duration::from_millis(2),
        "status bar {with_bar:?} vs presentation {without:?}"
    );
}

#[test]
fn ac4_cells_are_inline_and_long_clusters_still_work() {
    assert!(matches!(Sym::new("╭"), Sym::Inline { .. }));
    assert!(matches!(Sym::new("名"), Sym::Inline { .. }));
    assert!(matches!(Sym::new("🔥"), Sym::Inline { .. }));
    assert_eq!(Sym::new("a\u{0336}").as_str(), "a\u{0336}");
    // A family emoji ZWJ sequence is 25 bytes: heap, same text.
    let family = "👨\u{200d}👩\u{200d}👧\u{200d}👦";
    let s = Sym::new(family);
    assert!(matches!(s, Sym::Heap(_)));
    assert_eq!(s.as_str(), family);
    assert!(Sym::EMPTY.is_empty());
    assert_eq!(Sym::SPACE, " ");
    assert_eq!(
        std::mem::size_of::<Sym>(),
        24,
        "three words per cell symbol"
    );
    assert!(
        matches!(Sym::new("👩\u{200d}💻"), Sym::Inline { .. }),
        "11 bytes"
    );
    // Through a canvas: the long cluster round-trips into plain text.
    let mut c = tiza_core::canvas::Canvas::new(10, 1);
    c.put(0, 0, family, tiza_core::canvas::CellStyle::default());
    assert!(c.to_plain_text().starts_with(family));
}
