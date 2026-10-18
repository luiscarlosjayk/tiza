//! t-046 — Catalogue of demo scenarios: ways to use Tiza with agents
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
//!   After Luis's rework (2026-10-18) it names only what will be built
//!   (t-166, t-168), and no scenario depends on a discarded feature: no
//!   arrows between files or two files at once, no author metadata, no
//!   resource notifications, no diff import, no `:redact auto` for agents,
//!   no `--no-save` (the scenarios work on a copy and keep the sidecar the
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
fn ac1_between_10_and_15_scenarios_each_with_every_field() {
    let s = scenarios();
    assert!((10..=15).contains(&s.len()), "{} scenarios", s.len());
    for (i, (title, body)) in s.iter().enumerate() {
        assert!(
            title.starts_with(&format!("{}. ", i + 1)),
            "scenarios are numbered in order: {title}"
        );
        for field in [
            "**For:**",
            "**Setup:**",
            "**Script:**",
            "**What the audience sees:**",
            "**Features:**",
            "**Recording:**",
        ] {
            assert!(body.contains(field), "«{title}» has no {field}");
        }
    }
}

#[test]
fn ac1_every_path_it_names_exists() {
    let text = doc();
    let mut n = 0;
    for chunk in text.split('`').skip(1).step_by(2) {
        for word in chunk.split_whitespace() {
            let w = word.trim_matches(|c: char| "()[],:;\"'".contains(c));
            let w = w.split(':').next().unwrap().trim_end_matches('.');
            if w.starts_with("examples/") || w.starts_with("docs/") || w.starts_with("scripts/") {
                n += 1;
                assert!(
                    root().join(w).exists(),
                    "the catalogue names {w}, which does not exist"
                );
            }
        }
    }
    assert!(n >= 8, "{n} paths checked");
}

#[test]
fn ac2_the_two_scenarios_luis_named_are_there() {
    let s = scenarios();
    let titles: Vec<String> = s.iter().map(|(t, _)| t.to_lowercase()).collect();
    assert!(
        titles.iter().any(|t| t.contains("tour")),
        "a codebase tour: {titles:?}"
    );
    assert!(
        titles
            .iter()
            .any(|t| t.contains("agent") && (t.contains("review") || t.contains("another"))),
        "an agent explaining code to another: {titles:?}"
    );
}

#[test]
fn ac4_the_closing_list_names_only_what_will_be_built() {
    let text = doc();
    let start = text
        .find("\n## Planned changes that touch these scenarios\n")
        .expect("the closing section");
    let items: Vec<&str> = text[start..]
        .lines()
        .filter(|l| l.starts_with("- "))
        .collect();
    assert_eq!(items.len(), 2, "{items:?}");
    assert!(items[0].contains("t-166"), "{items:?}");
    assert!(items[1].contains("t-168"), "{items:?}");
    assert!(scenarios().iter().all(|(t, _)| !t.starts_with("Planned")));
}

#[test]
fn ac4_no_scenario_depends_on_a_discarded_feature() {
    let text = doc();
    let lower = text.to_lowercase();
    for gone in [
        "--no-save",
        "split view",
        "two files on screen",
        "arrows between files",
        "subscription",
        "notification",
        "import of a diff",
        "scene get >",
        "t-169",
    ] {
        assert!(
            !lower.contains(gone),
            "the catalogue still mentions «{gone}»"
        );
    }
    // Agents never turn masking on or off: `:redact auto` is not in a script.
    assert!(
        !text.contains(":redact auto"),
        "scenario 6 builds the masks"
    );
    let s = scenarios();
    let body = |n: usize| &s[n - 1].1;
    // Scenario 2: each agent signs its own labels and notes.
    assert!(body(2).contains("author:") && body(2).contains("review:"));
    // Scenario 4: the agent writes the moves from the diff.
    assert!(body(4).contains("git diff") && body(4).contains("writes the moves"));
    // Scenario 6: the .tiza is built with its masks before it is opened.
    assert!(body(6).contains("demo.env.tiza") && body(6).contains("redact"));
    // Every scenario that opens a sample opens a working copy.
    for (title, b) in &s {
        for line in b.lines() {
            assert!(
                !line.contains("`tiza examples/"),
                "«{title}» opens examples/ itself: {line}"
            );
        }
    }
}

#[test]
fn ac5_no_channel_tasks() {
    let text = doc().to_lowercase();
    for word in ["thumbnail", "devlog", "subscribe", "upload to youtube"] {
        assert!(!text.contains(word), "the catalogue mentions «{word}»");
    }
}

#[test]
fn ac3_the_worked_example_checks_clean_at_its_recording_width() {
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

#[test]
fn ac3_each_step_shows_its_file() {
    let ex = example("tally-tour.tiza");
    let expect = [
        (1, "fn main()", "flow: 1 parse the flags"),
        (
            2,
            "pub fn parse(",
            "every argument is a path, except the flag",
        ),
        (3, "split_whitespace()", "bytes come free"),
        (4, "pub fn table(", "--by-words: most words first"),
        (5, "fn main()", "the whole flow"),
    ];
    for (step, code, label) in expect {
        let text = render_step(&ex, step);
        assert!(text.contains(code), "step {step}:\n{text}");
        assert!(text.contains(label), "step {step}:\n{text}");
    }
    assert!(!render_step(&ex, 2).contains("fn main()"));
    // Step 5's window starts at `fn main` (line 9), not at the module docs.
    assert!(!render_step(&ex, 5).contains("mod args;"));
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
fn ac3_the_prompts_tool_calls_rebuild_the_shipped_example() {
    let main = tally_copy("t046-prompts");
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
            // Each one is drawn as the presenter watches (t-155).
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
    let main = tally_copy("t046-s12");
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
    live.api_ok("scene.patch", fast);
    let mut seen = vec![live.with(|a| a.doc.step)];
    wait_until("a whole pass and the start of the next", || {
        let now = live.with(|a| a.doc.step);
        if seen.last() != Some(&now) {
            seen.push(now);
        }
        seen.ends_with(&[5, 0, 1])
    });
    let first = seen.iter().position(|&s| s == 0).unwrap();
    assert_eq!(&seen[first..], &[0, 1, 2, 3, 4, 5, 0, 1], "{seen:?}");
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

#[test]
fn scenario_6_asks_for_redacts_without_a_label() {
    // QA 2 (2026-10-18): an agent labelled the masks, and the notes the
    // second prompt puts on those lines had no room and were not drawn.
    let s = scenarios();
    let prompt1 = s[5]
        .1
        .lines()
        .find(|l| l.starts_with("- To the agent, before Tiza is open"))
        .expect("scenario 6's first prompt");
    assert!(
        prompt1.to_lowercase().contains("give the redacts no label"),
        "{prompt1}"
    );
    // Why: the same take with labelled masks loses its notes.
    let dir = scratch("t046-s6");
    std::fs::create_dir_all(&dir).unwrap();
    let env = [
        ("DATABASE_URL=postgres://app:", "Qz7rW4xv", "@db.local/app"),
        ("API_TOKEN=", "tk_R7vQ2mXz9LpW4nB8", ""),
        ("LOG_LEVEL=info", "", ""),
    ];
    let text: String = env.iter().map(|(a, v, b)| format!("{a}{v}{b}\n")).collect();
    std::fs::write(dir.join("demo.env"), text).unwrap();
    // The span of each secret value, from the text before it (0-based
    // columns, end exclusive), as the prompt asks the agent to compute it.
    // The fake values share no four characters in a row with anything else
    // in the scenario (nor with the notes and labels below, nor with the
    // file name): a value that does masks those words in every label and
    // note too (t-174, fail closed). `hunter2` masked «counter», «enter».
    let words = s[5]
        .1
        .replace("Qz7rW4xv", "")
        .replace("tk_R7vQ2mXz9LpW4nB8", "")
        + " demo.env where the app's Postgres lives the token the API client sends how chatty the logs are database password API token";
    for (_, value, _) in env.iter().take(2) {
        assert!(s[5].1.contains(value), "scenario 6 writes {value}");
        let c: Vec<char> = value.chars().collect();
        for w in c.windows(4) {
            let piece: String = w.iter().collect();
            assert!(
                !words.contains(&piece),
                "{value} shares «{piece}» with the scenario's text"
            );
        }
    }
    let span = |line: usize| {
        let (before, value, _) = env[line - 1];
        json!({"line": line, "start": before.len(), "end": before.len() + value.len()})
    };
    let notes = [
        (1, "where the app's Postgres lives"),
        (2, "the token the API client sends"),
        (3, "how chatty the logs are"),
    ];
    for labelled in [false, true] {
        let mut r1 = json!({"id": "r1", "type": "redact", "step": 0, "span": span(1)});
        let mut r2 = json!({"id": "r2", "type": "redact", "step": 0, "span": span(2)});
        if labelled {
            r1["label"] = json!("database password");
            r2["label"] = json!("API token");
        }
        let mut annotations = vec![r1, r2];
        for (line, text) in notes {
            annotations.push(json!({"id": format!("a{line}"), "type": "note", "step": 1, "line": line, "text": text}));
        }
        let path = dir.join("demo.env.tiza");
        let d = json!({"schema": 1, "source": {"first_line": 1}, "code": {"file": "demo.env"}, "annotations": annotations});
        std::fs::write(&path, d.to_string()).unwrap();
        let frame = render_step(&path, 1);
        let drawn = notes.iter().filter(|(_, t)| frame.contains(t)).count();
        // Exactly the value is masked: the text around it stays, and no
        // piece of a value (two characters in a row) is left on its line.
        for (n, (before, value, after)) in env.iter().take(2).enumerate() {
            let masked = format!("{before}••••••••{after}");
            let line = frame
                .lines()
                .find(|l| l.contains(before))
                .unwrap_or_else(|| panic!("line {} is not drawn:\n{frame}", n + 1));
            // The code as drawn: from the key to the first blank after it.
            let at = line.find(before).unwrap();
            let code: String = line[at..]
                .chars()
                .take_while(|c| !c.is_whitespace())
                .collect();
            assert_eq!(code, masked, "line {}: {line}", n + 1);
            let shown = &code[before.len()..];
            let chars: Vec<char> = value.chars().collect();
            for w in chars.windows(2) {
                let piece: String = w.iter().collect();
                assert!(
                    !shown.contains(&piece),
                    "a piece of the value «{piece}» shows: {line}"
                );
            }
        }
        if labelled {
            assert!(
                drawn < notes.len(),
                "labelled masks: {drawn} notes drawn\n{frame}"
            );
        } else {
            assert_eq!(drawn, notes.len(), "unlabelled masks:\n{frame}");
        }
    }
}

#[test]
fn scenario_4_makes_the_review_folder_before_writing_into_it() {
    let s = scenarios();
    let body = &s[3].1;
    let show = body
        .find("git show HEAD~1:src/lib.rs > /tmp/review/")
        .expect("scenario 4 writes the old version into /tmp/review");
    let mkdir = body
        .find("mkdir -p /tmp/review &&")
        .expect("mkdir -p /tmp/review");
    assert!(mkdir < show, "the folder is made first");
}

#[test]
fn every_copy_of_tally_starts_fresh() {
    // `cp -r` onto a folder that exists copies into /tmp/tally/tally, and
    // the next scenario would open the previous one's sidecar.
    let text = doc();
    let cp = "cp -r examples/tally /tmp/tally";
    let mut n = 0;
    for (at, _) in text.match_indices(cp) {
        n += 1;
        assert!(
            text[..at].ends_with("rm -rf /tmp/tally && "),
            "a copy of tally without rm -rf first: …{}",
            &text[at.saturating_sub(40)..at + cp.len()]
        );
    }
    assert!(n >= 7, "{n} copies");
}

#[test]
fn scenario_3_reads_the_take_where_its_setup_leaves_it() {
    let s = scenarios();
    let body = &s[2].1;
    // Two sources, one variable: the sidecar of scenario 1, or the finished
    // take next to the copied folder (it names tally/src/main.rs relative
    // to itself, so it cannot become main.rs's sidecar).
    assert!(body.contains("`W=/tmp/tally/src/main.rs.tiza`"), "{body}");
    assert!(
        body.contains("cp examples/tally-tour.tiza /tmp/` and `W=/tmp/tally-tour.tiza`"),
        "{body}"
    );
    let agent_b = body
        .lines()
        .find(|l| l.starts_with("- Agent B"))
        .expect("agent B's lines");
    for cmd in [
        "tiza check \"$W\"",
        "tiza render \"$W\"",
        "tiza \"$W\" --session b",
    ] {
        assert!(agent_b.contains(cmd), "{agent_b}");
    }
    assert!(!agent_b.contains("main.rs.tiza"), "{agent_b}");
}

#[test]
fn the_t166_list_names_exactly_the_scenarios_that_use_code_link() {
    let s = scenarios();
    let using: Vec<usize> = s
        .iter()
        .enumerate()
        .filter(|(_, (_, body))| {
            body.lines()
                .find(|l| l.starts_with("**Features:**"))
                .is_some_and(|l| l.contains("code.link"))
        })
        .map(|(i, _)| i + 1)
        .collect();
    let text = doc();
    let item = text
        .lines()
        .find(|l| l.starts_with("- **t-166"))
        .expect("the t-166 item");
    let listed: Vec<usize> = item[item.find("** (").unwrap() + 4..item.find(").").unwrap()]
        .split(", ")
        .map(|n| n.parse().unwrap())
        .collect();
    assert_eq!(listed, using, "{item}");
}

#[test]
fn scenario_10_ends_as_the_example_does() {
    let s = scenarios();
    let body = &s[9].1;
    let d: Value =
        serde_json::from_str(&std::fs::read_to_string(example("stack-and-queue.tiza")).unwrap())
            .unwrap();
    // The comparing note on stack.rs's peek is quoted as the example has it.
    let peek = d["annotations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["step"] == 4 && a["type"] == "note")
        .expect("the note on peek");
    assert!(body.contains(peek["text"].as_str().unwrap()), "{body}");
    // The last move is the narrower window of queue.rs, in the script too.
    let last = d["moves"].as_array().unwrap().last().unwrap();
    assert_eq!(last["file"], "src/queue.rs");
    let lines = last["lines"].as_str().unwrap();
    assert!(body.contains(&format!("(lines {lines})")), "{body}");
    let script = body.lines().find(|l| l.starts_with("**Script:**")).unwrap();
    assert!(script.contains("last, link queue.rs again"), "{script}");
}
