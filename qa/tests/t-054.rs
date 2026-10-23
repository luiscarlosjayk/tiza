//! t-054 — Render budget: a frame in under 16 ms with 2,000 lines and 200
//! annotations (release build).
//!
//! Acceptance criteria:
//! - AC1 With 2,000 lines of Rust and 200 annotations of every kind, the
//!   median frame (layout + draw into a 120×40 buffer) takes < 16 ms in
//!   release (`scripts/bench.sh` times it). The test checks what made it
//!   slow, not the time: one layout per frame, the highlighted code kept,
//!   a frame far below highlighting the code.
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

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde_json::json;
use tiza_core::canvas::Sym;
use tiza_core::highlight::highlight_cached;
use tiza_qa::Live;
use two_face::theme::EmbeddedThemeName;

/// The highlight cache is the process's, and the other tests here
/// highlight too (each `annotation.add` lays the take out to say whether
/// the annotation is drawn): `ac2` runs alone, or it counts entries they
/// put there.
static CACHE: RwLock<()> = RwLock::new(());

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

/// One frame of `live` and how long it took.
fn frame(live: &Live) -> Duration {
    let t = Instant::now();
    let _ = live.screen(120, 40);
    t.elapsed()
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

#[test]
fn ac1_a_frame_lays_out_once_and_does_not_highlight() {
    // What made a frame slow is checked, not a time budget, which failed on
    // a loaded machine (the time is the bench's to judge: `scripts/bench.sh`
    // reads the median printed below). A frame lays the document out once
    // (the status bar reuses that layout), keeps the highlighted code, and
    // takes far less than highlighting the code: both are timed here, in
    // turns, so a load slows both alike.
    let _shared = CACHE.read().unwrap_or_else(|e| e.into_inner());
    let live = big_live("t054-budget");
    let _ = live.screen(120, 40); // warm the caches (syntect grammars, highlight)
    let (code, language, theme) = live.with(|a| {
        (
            a.doc.code.clone(),
            a.doc.source.language.clone(),
            a.theme.syntax,
        )
    });
    let kept = highlight_cached(&code, &language, theme);
    let layouts = || live.with(|a| a.layouts.get());
    let before = layouts();
    let frames: Vec<Duration> = (0..21).map(|_| frame(&live)).collect();
    assert_eq!(
        layouts() - before,
        21,
        "one layout per frame (the status bar reuses it)"
    );
    assert!(
        Arc::ptr_eq(&kept, &highlight_cached(&code, &language, theme)),
        "a frame highlighted the code again"
    );
    let median_frame = median(frames);
    eprintln!("median frame: {median_frame:?}");
    let mut frames = Vec::new();
    let mut highlights = Vec::new();
    for _ in 0..5 {
        frames.push(frame(&live));
        let t = Instant::now();
        let _ = tiza_core::highlight::highlight(&code, &language, theme);
        highlights.push(t.elapsed());
    }
    let (f, h) = (median(frames), median(highlights));
    assert!(
        f * 2 < h,
        "a frame ({f:?}) is not far below highlighting the code ({h:?})"
    );
    // Scrolling to the end and moving the cursor: still one layout, and the
    // same highlighted code.
    live.command("step 1");
    live.with(|a| a.cursor_line = 1990);
    let before = layouts();
    let _ = frame(&live);
    assert_eq!(layouts() - before, 1);
    assert!(Arc::ptr_eq(
        &kept,
        &highlight_cached(&code, &language, theme)
    ));
}

#[test]
fn ac2_highlight_is_memoized_and_invalidated() {
    let _alone = CACHE.write().unwrap_or_else(|e| e.into_inner());
    let code: Vec<String> = big_doc().lines().map(str::to_string).collect();
    let a = highlight_cached(&code, "rs", EmbeddedThemeName::OneHalfDark);
    let t = Instant::now();
    let b = highlight_cached(&code, "rs", EmbeddedThemeName::OneHalfDark);
    let hit = t.elapsed();
    // The same Arc is the hit; its time is only reported (a loaded machine
    // can stop any thread for milliseconds).
    eprintln!("cache hit: {hit:?}");
    assert!(Arc::ptr_eq(&a, &b), "second call returns the cached Arc");
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
    let _shared = CACHE.read().unwrap_or_else(|e| e.into_inner());
    let live = big_live("t054-status");
    live.with(|a| a.size = (120, 40));
    // The warnings computed from a given layout equal the standalone ones.
    let (from_layout, standalone) = live.with(|a| {
        let l = a.layout(118);
        (a.viewport_warnings_for(&l), a.viewport_warnings())
    });
    assert_eq!(from_layout, standalone);
    // No second layout hides in the bar: a frame with the status bar lays
    // out once, as one in presentation mode (which has no status bar).
    for present in [false, true] {
        live.with(move |a| a.present = present);
        let _ = live.screen(120, 40);
        let before = live.with(|a| a.layouts.get());
        for _ in 0..11 {
            let _ = live.screen(120, 40);
        }
        assert_eq!(
            live.with(|a| a.layouts.get()) - before,
            11,
            "present {present}: one layout per frame"
        );
    }
}

#[test]
fn ac4_cells_are_inline_and_long_clusters_still_work() {
    let _shared = CACHE.read().unwrap_or_else(|e| e.into_inner());
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
