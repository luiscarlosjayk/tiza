//! t-178 — docs/zed.md: the step tasks (`tiza next|prev|goto`) as tested in
//! Zed, with `reveal_target: center` so a Tiza in the terminal dock (alone or
//! inside Herdr) does not flicker.
//!
//! Acceptance criteria:
//! - AC1 docs/zed.md drops «Not yet run in Zed» and says what was checked
//!   (Zed version, macOS, 2026-10-18).
//! - AC2 The three step tasks use `"reveal": "never"`, `"reveal_target":
//!   "center"`, `"hide": "on_success"`, with one sentence on why.
//! - AC3 One line on the leftover: the editor's tab bar shows the task tab
//!   for an instant; to record, capture only Tiza's area or step with `n`,
//!   `auto_advance_ms` or an agent (MCP `goto_step`).
//! - AC4 The error with no Tiza running stays documented (exit 1, the
//!   message in the task's terminal).
//! - AC5 No code change.
//!
//! Zed itself is checked by hand (Luis and QA 1). This file checks what the
//! guide promises outside Zed: every JSON block parses, the step tasks carry
//! the tested settings, and the commands they run — against
//! `examples/tally-tour.tiza`, whose step id the `goto` task names — do and
//! print what the guide says.

use std::path::{Path, PathBuf};

use serde_json::Value;
use tiza_core::fs;
use tiza_qa::{Live, example, scratch, tiza};

fn guide() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::fs::read_to_string(root.join("docs/zed.md")).unwrap()
}

/// Every ```json block of the guide, parsed.
fn json_blocks(md: &str) -> Vec<Value> {
    md.split("```json\n")
        .skip(1)
        .map(|rest| {
            let block = rest.split("```").next().unwrap();
            serde_json::from_str(block).unwrap_or_else(|e| panic!("{e}:\n{block}"))
        })
        .collect()
}

/// The tasks that run `tiza next|prev|goto`.
fn step_tasks(md: &str) -> Vec<Value> {
    json_blocks(md)
        .into_iter()
        .filter_map(|b| b.as_array().cloned())
        .flatten()
        .filter(|t| {
            t["command"] == "tiza"
                && matches!(t["args"][0].as_str(), Some("next" | "prev" | "goto"))
        })
        .collect()
}

fn run(socket: &Path, args: &[&str]) -> (Option<i32>, String, String) {
    let mut all = args.to_vec();
    all.extend(["--socket", socket.to_str().unwrap()]);
    let out = tiza(&all);
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        String::from_utf8_lossy(&out.stderr).trim().to_owned(),
    )
}

#[test]
fn ac1_says_what_was_checked() {
    let md = guide();
    assert!(
        !md.contains("Not yet run"),
        "the old warning is still there"
    );
    let section = md.split("## Driving the demo from Zed").nth(1).unwrap();
    for needle in ["Zed 1.24.2", "macOS 27.0.1", "2026-10-18", "Herdr"] {
        assert!(
            section.contains(needle),
            "the step tasks section lacks {needle}"
        );
    }
}

#[test]
fn ac2_the_three_step_tasks_carry_the_tested_settings() {
    let md = guide();
    let tasks = step_tasks(&md);
    let kinds: Vec<&str> = tasks
        .iter()
        .map(|t| t["args"][0].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["next", "prev", "goto"]);
    for t in &tasks {
        assert_eq!(t["reveal"], "never", "{t}");
        assert_eq!(t["reveal_target"], "center", "{t}");
        assert_eq!(t["hide"], "on_success", "{t}");
    }
    // The why, and the worse alternative.
    for needle in [
        "Zed runs every task in a terminal tab",
        "terminal dock",
        "`\"hide\": \"never\"` is worse",
    ] {
        assert!(md.contains(needle), "docs/zed.md lacks {needle}");
    }
}

#[test]
fn ac3_the_leftover_and_how_to_record_around_it() {
    let md = guide();
    for needle in [
        "tab bar",
        "for an instant",
        "`auto_advance_ms`",
        "`goto_step`",
    ] {
        assert!(md.contains(needle), "docs/zed.md lacks {needle}");
    }
}

/// The tasks' commands against the example the `goto` task names: they step
/// it, and an unknown id or no instance exits 1 with the message the guide
/// quotes (AC4; `hide: on_success` keeps it on screen).
#[test]
fn ac2_ac4_the_commands_do_what_the_guide_says() {
    let md = guide();
    let goto = step_tasks(&md)
        .into_iter()
        .find(|t| t["args"][0] == "goto")
        .unwrap();
    let id = goto["args"][1].as_str().unwrap().to_owned();
    assert!(goto["label"].as_str().unwrap().contains(&id), "{goto}");

    let dir = scratch("t178");
    let tour = example("tally-tour.tiza");
    let doc = fs::open(tour.to_str().unwrap()).unwrap().document;
    // The step the `goto` task names, and the line `tiza` prints for it.
    let (n, title) = doc
        .steps
        .iter()
        .find(|s| s.id.as_deref() == Some(id.as_str()))
        .map(|s| (s.step, s.title.clone().unwrap()))
        .unwrap_or_else(|| panic!("no step {id} in tally-tour.tiza"));
    let last = doc.steps.iter().map(|s| s.step).max().unwrap();
    let first = doc
        .steps
        .iter()
        .find(|s| s.step == 1)
        .unwrap()
        .title
        .clone()
        .unwrap();
    let live = Live::open(doc, dir.join("tour.tiza"), dir.join("live.sock"));
    let sock = live.socket.clone();

    // The guide's tasks pass no socket (the session's); here each command
    // gets the test instance's with --socket, the same client path.
    assert_eq!(
        run(&sock, &["next"]),
        (Some(0), format!("step 1/{last} · {first}"), String::new())
    );
    assert_eq!(
        run(&sock, &["goto", &id]),
        (Some(0), format!("step {n}/{last} · {title}"), String::new())
    );
    assert_eq!(live.with(|a| a.doc.step), n, "goto {id}");
    assert_eq!(run(&sock, &["prev"]).0, Some(0));
    assert_eq!(live.with(|a| a.doc.step), n - 1);

    let (code, _, err) = run(&sock, &["goto", "nope"]);
    assert_eq!(code, Some(1));
    let quoted = "no step is named \"nope\" (named steps: main, args, count, report, recap)";
    assert!(err.contains(quoted), "{err}");
    assert!(md.contains(quoted), "docs/zed.md does not quote: {quoted}");

    let none: PathBuf = dir.join("nobody.sock");
    let (code, _, err) = run(&none, &["next"]);
    assert_eq!(code, Some(1));
    assert!(err.contains("no Tiza instance is listening on"), "{err}");
    assert!(md.contains("`no Tiza instance is listening on …`"));
}

/// Every task of the guide.
fn all_tasks(md: &str) -> Vec<Value> {
    json_blocks(md)
        .into_iter()
        .filter_map(|b| b.as_array().cloned())
        .flatten()
        .collect()
}

#[test]
fn corner_no_task_gives_sh_its_script_in_args() {
    // Zed 1.25.3 runs a task as `$SHELL -i -c` with `command` and `args`
    // on one line: `"command": "sh"` with `-c` in `args` runs `sh` alone.
    let md = guide();
    let tasks = all_tasks(&md);
    assert!(tasks.len() >= 7, "{tasks:?}");
    for t in &tasks {
        let shell = matches!(
            t["command"].as_str(),
            Some("sh" | "bash" | "zsh" | "/bin/sh" | "/bin/bash" | "/bin/zsh")
        );
        let dash_c = t["args"]
            .as_array()
            .is_some_and(|a| a.iter().any(|x| x == "-c"));
        assert!(!(shell && dash_c), "a shell with -c in args: {t}");
    }
    assert!(md.contains("`$SHELL -i -c`") && md.contains("Zed 1.25.3"));
}

#[test]
fn corner_the_selected_text_and_40_lines_tasks_run_as_zed_runs_them() {
    let md = guide();
    let tasks = all_tasks(&md);
    let task = |label: &str| {
        tasks
            .iter()
            .find(|t| t["label"] == label)
            .unwrap_or_else(|| panic!("no task {label}"))
            .clone()
    };
    let dir = scratch("t178-zed-shell");
    let file = dir.join("demo.rs");
    let code: String = (1..=100).map(|n| format!("let v{n} = {n};\n")).collect();
    std::fs::write(&file, code).unwrap();
    // As Zed does: its variables replaced, then `command` and `args` on
    // one line for the shell, with the task's `env`.
    let zed = |s: &str| {
        s.replace("$ZED_FILE", file.to_str().unwrap())
            .replace("$ZED_ROW", "30")
    };
    let line = |t: &Value| {
        let mut l = zed(t["command"].as_str().unwrap());
        for a in t["args"].as_array().unwrap() {
            l.push_str(&format!(" \"{}\"", zed(a.as_str().unwrap())));
        }
        l
    };
    let selected = task("Tiza: selected text");
    assert_eq!(selected["env"]["SEL"], "$ZED_SELECTED_TEXT", "{selected}");
    let forty = task("Tiza: 40 lines from the cursor");
    let bin_dir = tiza_qa::bin().parent().unwrap().to_path_buf();
    let body = r##"
import json
bin_dir, sel_line, forty_line = sys.argv[1], sys.argv[2], sys.argv[3]
path = bin_dir + os.pathsep + os.environ.get("PATH", "")
smoke = " --smoke --no-socket --no-save"
env = dict(os.environ, PATH=path, TERM="xterm-256color", TIZA_NO_OSC="1",
           SEL='fn sel() {\n    let q = "$HOME";\n}', ZED_ROW="30")
env.pop("TIZA_TEST_HOOK", None)
for name, line in [("SEL", sel_line), ("FORTY", forty_line)]:
    s = Pty(["sh", "-c", line + smoke], 30, 120, env=env, ctty=True)
    code = s.finish(10)
    print(name, code, json.dumps(plain(s.out)), flush=True)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "zed_shell.py",
        body,
        &[bin_dir.to_str().unwrap(), &line(&selected), &line(&forty)],
    );
    let frame = |name: &str| -> (String, String) {
        let l = out
            .lines()
            .find(|l| l.starts_with(&format!("{name} ")))
            .unwrap_or_else(|| panic!("{out}"));
        let (code, text) = l[name.len() + 1..].split_once(' ').unwrap();
        (code.to_string(), serde_json::from_str(text).unwrap())
    };
    // The selection reaches Tiza through stdin, quotes and `$` as typed.
    let (code, text) = frame("SEL");
    assert_eq!(code, "0", "{text}");
    assert!(text.contains(r#"let q = "$HOME";"#), "{text}");
    // The 40 lines start at the cursor line: 30-69.
    let (code, text) = frame("FORTY");
    assert_eq!(code, "0", "{text}");
    assert!(
        text.contains("let v30 = 30;") && text.contains("let v40 = 40;"),
        "{text}"
    );
    assert!(!text.contains("let v29 = 29;"), "{text}");
}
