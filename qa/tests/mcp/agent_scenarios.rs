//! Catalogue of demo scenarios: ways to use Tiza with agents
//! (codebase tours, agent-to-agent explanations, reviews), as ideas for
//! recordings.
//!
//! Acceptance criteria:
//! - AC1 `docs/demos/scenarios.md` (English) with 10-15 scenarios, each with
//!   who it is for, the setup (a sample under `examples/`, a real demo), the
//!   interaction script, what the audience sees, the features it exercises
//!   and recording notes.
//! - AC2 At least the two scenarios Luis named: a codebase tour while an
//!   agent draws on a few files; one agent explaining code to another for
//!   review or knowledge sharing.
//! - AC3 One scenario fully worked as a runnable example: a multi-file
//!   `.tiza` with steps and links (`examples/tally-tour.tiza` over
//!   `examples/tally/`) plus the exact prompt list.
//! - AC4 A closing list of what each scenario needs that Tiza does not do yet.
//!   After Luis's rework (2026-10-18) it names only what will be built,
//!   and no scenario depends on a discarded feature: no
//!   arrows between files or two files at once, no author metadata, no
//!   resource notifications, no diff import, no `--no-save` (the scenarios work on a copy and keep the sidecar the
//!   TUI autosaves).
//! - AC5 No marketing or channel tasks.
//!
//! The document is prose, so these tests pin what can drift: its shape, the
//! paths it names, and the worked example — `tiza check` clean at the width
//! it is recorded at, each step showing its file, and the MCP tool calls the
//! prompt list leads to rebuilding exactly the frames of the shipped `.tiza`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{Live, McpClient, example, scratch, tiza, tiza_ok};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn doc() -> String {
    std::fs::read_to_string(root().join("docs/demos/scenarios.md"))
        .expect("docs/demos/scenarios.md")
}

/// The numbered scenario sections (`## 1. Title` …), as (title, body).
fn scenarios() -> Vec<(String, String)> {
    let text = doc();
    let mut out = Vec::new();
    let mut parts = text.split("\n## ").skip(1);
    for part in parts.by_ref() {
        let (title, body) = part.split_once('\n').unwrap_or((part, ""));
        let numbered = title
            .split_once(". ")
            .is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()));
        if numbered {
            out.push((title.to_string(), body.to_string()));
        }
    }
    out
}

#[test]
fn the_worked_example_checks_clean_at_its_recording_width() {
    let ex = example("tally-tour.tiza");
    let out = tiza(&["check", ex.to_str().unwrap(), "--width", "130", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["errors"], 0, "{v}");
    assert_eq!(v["warnings"], 0, "{v}");
    // The sample codebase is real: every file the tour links is there.
    let d: Value = serde_json::from_str(&std::fs::read_to_string(&ex).unwrap()).unwrap();
    let links: Vec<&str> = d["moves"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["action"] == "code.link")
        .map(|m| m["file"].as_str().unwrap())
        .collect();
    assert_eq!(links.len(), 4, "{links:?}");
    for f in links {
        assert!(example(f).exists(), "{f}");
    }
}

fn render_step(path: &Path, step: u32) -> String {
    tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--out",
        "-",
        "--step",
        &step.to_string(),
        "--width",
        "130",
    ])
}

/// The MCP tool calls the worked example's prompts lead to, in order.
fn prompt_calls() -> Vec<(&'static str, Value)> {
    vec![
        // Prompt 2: main.rs, step 1.
        ("next_step", json!({})),
        (
            "add_mark",
            json!({"id": "m1", "step": 1, "kind": "flow", "at": {"line": 10, "find": "args::parse"}, "label": "1 parse the flags", "color": "blue"}),
        ),
        (
            "add_mark",
            json!({"id": "m2", "step": 1, "kind": "flow", "at": {"line": 21, "find": "count::count"}, "label": "2 count each file", "color": "green"}),
        ),
        (
            "add_mark",
            json!({"id": "m3", "step": 1, "kind": "flow", "at": {"line": 26, "find": "report::table"}, "label": "3 print the table", "color": "purple"}),
        ),
        (
            "add_note",
            json!({"id": "n1", "step": 1, "line": 14, "text": "a bad flag ends here: exit code 2"}),
        ),
        (
            "add_box",
            json!({"id": "b1", "step": 1, "from_line": 19, "to_line": 24, "label": "a file it cannot read is skipped"}),
        ),
        // Prompt 3: args.rs, step 2.
        (
            "add_move",
            json!({"at_step": 2, "action": "code.link", "file": "args.rs", "lines": "14-30"}),
        ),
        ("next_step", json!({})),
        (
            "add_box",
            json!({"id": "b2", "step": 2, "from_line": 17, "to_line": 25, "label": "every argument is a path, except the flag", "color": "blue"}),
        ),
        (
            "add_mark",
            json!({"id": "k1", "step": 2, "kind": "error", "at": {"line": 21, "find": "Err"}, "label": "main prints it, exits 2"}),
        ),
        (
            "add_note",
            json!({"id": "n2", "step": 2, "line": 26, "text": "no paths at all is an error too"}),
        ),
        // Prompt 4: count.rs, step 3.
        (
            "add_move",
            json!({"at_step": 3, "action": "code.link", "file": "count.rs"}),
        ),
        ("next_step", json!({})),
        (
            "add_callout",
            json!({"id": "c1", "step": 3, "from_line": 11, "to_line": 14, "title": "bytes come free", "text": "text.len() is the byte length of the String, so only lines and words need the loop.", "color": "cyan"}),
        ),
        (
            "add_box",
            json!({"id": "b3", "step": 3, "from_line": 15, "to_line": 18, "label": "one pass over the lines", "color": "green"}),
        ),
        (
            "highlight",
            json!({"id": "h1", "step": 3, "at": {"line": 17, "find": "split_whitespace()"}, "label": "runs of spaces count once"}),
        ),
        // Prompt 5: report.rs, step 4.
        (
            "add_move",
            json!({"at_step": 4, "action": "code.link", "file": "report.rs"}),
        ),
        ("next_step", json!({})),
        (
            "add_mark",
            json!({"id": "k2", "step": 4, "kind": "new", "at": {"line": 7, "find": "rows.to_vec()"}, "label": "a copy: the caller's rows are untouched"}),
        ),
        (
            "add_box",
            json!({"id": "b4", "step": 4, "from_line": 8, "to_line": 11, "label": "--by-words: most words first", "color": "purple"}),
        ),
        (
            "add_note",
            json!({"id": "n3", "step": 4, "line": 14, "text": "right-aligned columns, like wc"}),
        ),
        // Prompt 6: back in main.rs, step 5.
        (
            "add_move",
            json!({"at_step": 5, "action": "code.link", "file": "main.rs", "lines": "9-28"}),
        ),
        (
            "add_move",
            json!({"at_step": 5, "action": "select", "from": {"find": "report::table"}, "to": {"find": "opts.sort)"}}),
        ),
        ("next_step", json!({})),
        (
            "add_callout",
            json!({"id": "c2", "step": 5, "from_line": 18, "to_line": 26, "title": "the whole flow", "text": "args.rs turns the flags into Options, count.rs reads each file once, report.rs sorts and prints. main only wires them together.", "placement": "auto", "color": "cyan"}),
        ),
        (
            "add_spotlight",
            json!({"id": "s1", "step": 5, "ids": ["m1", "m2", "m3", "c2"]}),
        ),
    ]
}

/// A scratch copy of the sample codebase, so the live sidecar never lands
/// in `examples/`.
fn tally_copy(name: &str) -> PathBuf {
    let dir = scratch(name).join("tally");
    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    for f in ["main.rs", "args.rs", "count.rs", "report.rs"] {
        std::fs::copy(example(&format!("tally/src/{f}")), src.join(f)).unwrap();
    }
    src.join("main.rs")
}

#[test]
fn the_prompts_tool_calls_rebuild_the_shipped_example() {
    let main = tally_copy("prompts");
    let opened = tiza_core::fs::open(main.to_str().unwrap()).unwrap();
    let live = Live::open(
        opened.document,
        opened.save_path,
        main.parent().unwrap().join("live.sock"),
    );
    let mut mcp = McpClient::start(&live.socket);
    // Prompt 1 only reads.
    for (tool, args) in [("status", json!({})), ("get_code", json!({}))] {
        let (err, text, _) = mcp.call(tool, args);
        assert!(!err, "{tool}: {text}");
    }
    for (tool, args) in prompt_calls() {
        let (err, text, _) = mcp.call(tool, args.clone());
        assert!(!err, "{tool} {args}: {text}");
        if tool != "next_step" && tool != "add_move" {
            // Each one is drawn as the presenter watches.
            assert!(text.contains("; visible now"), "{tool}: {text}");
        }
    }
    // Prompt 7: the step names, through the shell (no MCP tool sets them).
    live.api_ok(
        "scene.patch",
        json!({"steps": [
            {"step": 1, "id": "main", "title": "main.rs: parse, count, report"},
            {"step": 2, "id": "args", "title": "args.rs: one flag, by hand"},
            {"step": 3, "id": "count", "title": "count.rs: one pass"},
            {"step": 4, "id": "report", "title": "report.rs: sort, then print"},
            {"step": 5, "id": "recap", "title": "Back in main.rs"}
        ]}),
    );
    // Prompt 8: check, then step 0.
    let (err, text, _) = mcp.call("check", json!({"width": 130}));
    assert!(!err, "{text}");
    let report: Value = serde_json::from_str(&text)
        .unwrap_or_else(|_| serde_json::from_str(&text[text.find('{').unwrap()..]).unwrap());
    assert_eq!(report["errors"], 0, "{report}");
    assert_eq!(report["warnings"], 0, "{report}");
    let (err, text, _) = mcp.call("goto_step", json!({"id": "count"}));
    assert!(!err && text.contains("count.rs: one pass"), "{text}");
    let (err, _, _) = mcp.call("set_step", json!({"step": 0}));
    assert!(!err);

    // What the TUI autosaved next to main.rs — the result the catalogue
    // tells you to keep — draws every step exactly as the shipped example.
    let built = live.with(|a| a.save_path.clone());
    assert_eq!(built, main.with_file_name("main.rs.tiza"));
    let shipped = example("tally-tour.tiza");
    for step in 0..=5 {
        assert_eq!(
            render_step(&built, step),
            render_step(&shipped, step),
            "step {step} differs from examples/tally-tour.tiza"
        );
    }
}

/// The JSON that scenario 12 passes to `tiza scene patch '…'`.
fn scenario_12_patch() -> Value {
    let s = scenarios();
    let body = &s[11].1;
    let start = body
        .find("tiza scene patch '")
        .expect("scenario 12 patches the steps")
        + "tiza scene patch '".len();
    let len = body[start..].find('\'').expect("the closing quote");
    serde_json::from_str(&body[start..start + len]).expect("scenario 12's patch is JSON")
}

/// Waits (10 s at most) until `cond` holds.
fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !cond() {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "waiting: {what}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn scenario_12_starts_by_itself_and_loops_round_again() {
    // QA 2 (2026-10-18): without an entry for step 0 the tour neither
    // started on its own nor went on after wrapping back to step 0.
    let patch = scenario_12_patch();
    assert_eq!(patch["loop"], true, "{patch}");
    let shipped: Value =
        serde_json::from_str(&std::fs::read_to_string(example("tally-tour.tiza")).unwrap())
            .unwrap();
    let max_step = shipped["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["step"].as_u64())
        .max()
        .unwrap();
    let steps = patch["steps"].as_array().unwrap();
    for n in 0..=max_step {
        let entry = steps.iter().find(|e| e["step"] == n);
        assert!(
            entry.is_some_and(|e| e["auto_advance_ms"].as_u64().is_some_and(|ms| ms > 0)),
            "step {n} has no auto_advance_ms in scenario 12's patch: {patch}"
        );
    }

    // A live copy of the tour, as scenario 12 opens it.
    let main = tally_copy("s12");
    let tour = main
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tally-tour.tiza");
    std::fs::copy(example("tally-tour.tiza"), &tour).unwrap();
    let opened = tiza_core::fs::open(tour.to_str().unwrap()).unwrap();
    let live = Live::open(
        opened.document,
        opened.save_path,
        tour.with_file_name("live.sock"),
    );
    assert_eq!(live.with(|a| a.doc.step), 0);
    // The same shape with short durations, right after opening: with no key
    // it walks every step, wraps to 0 and starts the next pass.
    let mut fast = patch.clone();
    for e in fast["steps"].as_array_mut().unwrap() {
        e["auto_advance_ms"] = json!(30);
    }
    // Read every step the app went through (`Live::steps`), not samples of
    // `doc.step`: a 30 ms step can fall between two polls on a slow runner.
    let from = live.steps().len() - 1;
    live.api_ok("scene.patch", fast);
    let mut seen = Vec::new();
    wait_until("a whole pass and the start of the next", || {
        seen = live.steps()[from..].to_vec();
        seen.len() >= 8
    });
    assert_eq!(&seen[..8], &[0, 1, 2, 3, 4, 5, 0, 1], "{seen:?}");
    // No step skipped or repeated after that either: steps 0 to 5, round
    // and round.
    let cycle: Vec<u32> = (0..seen.len() as u32).map(|i| i % 6).collect();
    assert_eq!(seen, cycle);
    // Then the patch as written: the copy keeps it, and the GIF holds
    // every step for its duration.
    live.api_ok("scene.patch", patch.clone());
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(&tour).unwrap()).unwrap();
    assert_eq!(
        saved["steps"], patch["steps"],
        "the copy keeps the durations"
    );
    assert_eq!(saved["loop"], true);
    let gif = tour.with_file_name("loop.gif");
    let out = tiza(&[
        "render",
        tour.to_str().unwrap(),
        "--out",
        gif.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{out:?}");
    // The summary line goes to stderr, like every `render` to a file.
    let summary = String::from_utf8_lossy(&out.stderr);
    assert!(
        summary.contains("steps 0, 1, 2, 3, 4, 5 timed by their auto_advance_ms"),
        "{summary}"
    );
}
