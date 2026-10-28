//! Enter effects (experimental) on the real TUI: a key ends the effect of
//! the step at once and does nothing else, going back shows the final
//! drawing with no effect, the auto-advance waits for the effect, and a
//! step without an effect starts none.
//!
//! The binary of the tests slows the clock of an effect 100 times
//! (`TIZA_TEST_HOOK=effect:slow`), so the effect still plays when the
//! next key arrives.

use serde_json::Value;
use tiza_qa::{fixture, scratch};

/// Runs the TUI on a copy of `fixture` (a path in `qa/fixtures/`) with
/// `--no-save --present --theme dark` and `args`, then the steps of
/// `script`: `key` sends keys, `api` calls a socket method, `until` waits
/// for a status, and `until_code` waits until the code holds a text. Each
/// wait prints `STATUS <json>`.
fn run(name: &str, fixture_path: &str, args: &[&str], script: &str) -> Vec<Value> {
    let dir = scratch(name);
    let source = fixture(fixture_path);
    let file = source.file_name().unwrap().to_str().unwrap().to_string();
    std::fs::copy(&source, dir.join(&file)).unwrap();
    let args = serde_json::to_string(args).unwrap();
    let body = r##"
import json
binary, cwd, script = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
file, args = sys.argv[4], json.loads(sys.argv[5])
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="effect:slow")
s = Pty([binary, "--no-save", "--present", "--theme", "dark"] + args
        + ["--socket", "s.sock", file], 30, 100, env=env)

def call(method, params=None):
    cmd = [binary, "--socket", "s.sock", "api", method]
    if params is not None:
        cmd.append(json.dumps(params))
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout) if r.returncode == 0 else {}

def status():
    return call("status")

def until(pred, what, timeout=10):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        st = status()
        if pred(st):
            return st
        s.settle(0.05)
    print("WAIT_TIMEOUT", what, flush=True)
    return status()

ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame")
for step in script if ok else []:
    kind, arg = step[0], step[1]
    if kind == "key":
        s.send(arg.encode())
    elif kind == "until":
        want = arg
        st = until(lambda st: all(st.get(k, False) == v for k, v in want.items()), json.dumps(want))
        print("STATUS", json.dumps({k: st.get(k, False) for k in ("step", "effect")}), flush=True)
    elif kind == "api":
        call(arg[0], arg[1])
    elif kind == "until_code":
        end = time.monotonic() + 10
        while arg not in call("code.get").get("text", "") and time.monotonic() < end:
            s.settle(0.05)
        if arg not in call("code.get").get("text", ""):
            print("WAIT_TIMEOUT code", arg, flush=True)
        st = status()
        print("STATUS", json.dumps({k: st.get(k, False) for k in ("step", "effect")}), flush=True)
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        &format!("{name}.py"),
        body,
        &[
            tiza_qa::bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            script,
            &file,
            &args,
        ],
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    out.lines()
        .filter_map(|l| l.strip_prefix("STATUS "))
        .map(|j| serde_json::from_str(j).unwrap())
        .collect()
}

#[test]
fn a_key_ends_the_effect_and_does_nothing_else() {
    // `n` starts the effect of step 2; `n` again only ends it; the next `n`
    // goes to step 3.
    let seen = run(
        "effect-key",
        "shared/enter_effects/drop.tiza",
        &["--step", "1"],
        r#"[["key", "n"], ["until", {"step": 2, "effect": true}],
            ["key", "n"], ["until", {"step": 2, "effect": false}],
            ["key", "n"], ["until", {"step": 3, "effect": true}],
            ["key", "x"], ["until", {"step": 3, "effect": false}]]"#,
    );
    let pairs: Vec<(u64, bool)> = seen
        .iter()
        .map(|v| (v["step"].as_u64().unwrap(), v["effect"].as_bool().unwrap()))
        .collect();
    assert_eq!(pairs, [(2, true), (2, false), (3, true), (3, false)]);
}

#[test]
fn going_back_shows_the_final_drawing_at_once() {
    let seen = run(
        "effect-back",
        "shared/enter_effects/drop.tiza",
        &["--step", "1"],
        r#"[["key", "n"], ["until", {"step": 2, "effect": true}],
            ["key", "x"], ["until", {"step": 2, "effect": false}],
            ["key", "n"], ["until", {"step": 3, "effect": true}],
            ["key", "x"], ["until", {"step": 3, "effect": false}],
            ["key", "p"], ["until", {"step": 2}]]"#,
    );
    assert_eq!(seen.last().unwrap()["effect"], false, "{seen:?}");
}

#[test]
fn auto_advance_waits_for_the_effect() {
    // Step 1 advances after 1 ms, but only once its effect is over: the
    // effect still plays at step 1, and `x` ends it.
    let seen = run(
        "effect-auto",
        "tui/enter_effects/auto.tiza",
        &["--step", "0"],
        r#"[["key", "n"], ["until", {"step": 1, "effect": true}],
            ["key", "x"], ["until", {"step": 2}]]"#,
    );
    let steps: Vec<u64> = seen.iter().map(|v| v["step"].as_u64().unwrap()).collect();
    assert_eq!(steps, [1, 2], "{seen:?}");
}

#[test]
fn a_step_without_an_effect_starts_no_effect() {
    // Only the callout of step 3 has an effect: step 2 starts none.
    let seen = run(
        "effect-none",
        "shared/enter_effects/drop-mixed.tiza",
        &["--step", "1"],
        r#"[["key", "n"], ["until", {"step": 2}],
            ["key", "n"], ["until", {"step": 3, "effect": true}]]"#,
    );
    let pairs: Vec<(u64, bool)> = seen
        .iter()
        .map(|v| (v["step"].as_u64().unwrap(), v["effect"].as_bool().unwrap()))
        .collect();
    assert_eq!(pairs, [(2, false), (3, true)]);
}

#[test]
fn the_step_the_tui_opens_at_plays_no_effect() {
    // drop.tiza is saved at step 1: `--step 2` opens it one step on, at a
    // step with an effect, and shows the final drawing.
    let seen = run(
        "effect-open",
        "shared/enter_effects/drop.tiza",
        &["--step", "2"],
        r#"[["until", {"step": 2}]]"#,
    );
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0]["effect"], false, "{seen:?}");
}

#[test]
fn a_step_change_to_the_step_on_screen_keeps_the_auto_advance() {
    // `step.set` to the step that plays its effect ends the effect; the
    // auto-advance of 1 ms then goes on to step 2.
    let seen = run(
        "effect-same-step",
        "tui/enter_effects/auto.tiza",
        &["--step", "0"],
        r#"[["key", "n"], ["until", {"step": 1, "effect": true}],
            ["api", ["step.set", {"step": 1}]], ["until", {"step": 2}]]"#,
    );
    let steps: Vec<u64> = seen.iter().map(|v| v["step"].as_u64().unwrap()).collect();
    assert_eq!(steps, [1, 2], "{seen:?}");
}

#[test]
fn a_goto_to_the_next_step_plays_its_effect() {
    let seen = run(
        "effect-goto",
        "shared/enter_effects/drop.tiza",
        &["--step", "1"],
        r#"[["api", ["step.goto", {"id": 2}]], ["until", {"step": 2}]]"#,
    );
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0]["effect"], true, "{seen:?}");
}
