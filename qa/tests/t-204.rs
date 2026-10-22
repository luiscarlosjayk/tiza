//! Start-up leftovers: keys typed while the start-up colour query read the
//! terminal were eaten with its answer (`nn` sent right after launch left
//! the presentation at step 0), and with no terminal at all Tiza said
//! «tiza crashed» from `ratatui::init`.
//!
//! Acceptance criteria:
//! - AC1 Keys typed while the query waits are not lost: what the read took
//!   besides the answer is handed to the event loop, in order. `nn` sent
//!   0–100 ms after launch in presentation moves two steps.
//! - AC2 With no controlling terminal, a plain error: one line saying a
//!   terminal is needed and pointing at `tiza render` / `tiza check`; exit
//!   code 1; nothing in `panics.log`.
//! - AC3 Nothing changes when nothing is typed early (t-180, t-119 and t-205
//!   stay green).
//! - AC4 This file; CHANGELOG (Fixed).
//!
//! Corner cases: a terminal that never answers (the 100 ms timeout) still
//! gives the keys back; an ESC typed early is a key, not part of an answer.

use std::path::Path;

use tiza_qa::{bin, example, scratch};

/// Starts `tiza --theme auto --step 0 ARGS doc.tiza` on a pty that is its
/// controlling terminal (so the colour query runs), writes `seed` to its
/// input before it starts and `typed` `delay_ms` after, then, after the
/// first frame, `after`; prints the step `status` settles on (`STEP n`) and
/// whether the screen showed `light·auto` (`LIGHT …`).
fn run(
    name: &str,
    args: &[&str],
    seed: &[u8],
    typed: &[u8],
    delay_ms: u64,
    after: &[u8],
) -> String {
    let dir = scratch(name);
    std::fs::copy(example("ownership.rs.tiza"), dir.join("doc.tiza")).unwrap();
    std::fs::copy(example("ownership.rs"), dir.join("ownership.rs")).unwrap();
    let body = r##"
import json
binary, cwd, args, seed, typed, delay, after, want = sys.argv[1:9]
os.chdir(cwd)
args, seed, typed, after = json.loads(args), bytes.fromhex(seed), bytes.fromhex(typed), bytes.fromhex(after)
delay, want = int(delay), int(want)
env = dict(os.environ, TERM="xterm-256color")
env.pop("TIZA_NO_OSC", None)
argv = [binary, "--theme", "auto", "--step", "0", "--no-save", "--socket", "s.sock", *args, "doc.tiza"]
s = Pty(argv, 30, 120, env=env, ctty=True, raw=True, seed=seed)
if typed:
    time.sleep(delay / 1000)  # the delay under test, not a synchronisation
    s.send(typed)
def status():
    r = subprocess.run([binary, "--socket", "s.sock", "api", "status"], capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout) if r.returncode == 0 else {}
ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame")
if ok and after:
    s.send(after)
step = None
end = time.monotonic() + 10
while ok and time.monotonic() < end:
    step = status().get("step")
    if step == want:
        break
    s.settle(0.05)
print("STEP", step, flush=True)
print("LIGHT", "light·auto" in plain(s.out), flush=True)
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
"##;
    let want = 2;
    tiza_qa::pty_script(
        &dir,
        &format!("{name}.py"),
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &serde_json::to_string(args).unwrap(),
            &hex(seed),
            &hex(typed),
            &delay_ms.to_string(),
            &hex(after),
            &want.to_string(),
        ],
    )
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|c| format!("{c:02x}")).collect()
}

fn step_of(out: &str) -> String {
    out.lines()
        .find_map(|l| l.strip_prefix("STEP "))
        .unwrap_or("?")
        .to_string()
}

/// A light terminal's answer: background, then foreground.
const LIGHT: &[u8] = b"\x1b]11;rgb:f7f7/f5f5/efef\x1b\\\x1b]10;rgb:1f1f/2323/2828\x1b\\";

#[test]
fn ac1_nn_before_the_start_moves_two_steps() {
    // Test step 1: in the input before Tiza reads it, so the query's read
    // takes it; the terminal never answers (the 100 ms timeout path).
    let out = run("t204-seed", &["--present"], b"nn", b"", 0, b"");
    assert_eq!(step_of(&out), "2", "{out}");
}

#[test]
fn ac1_nn_at_every_moment_of_the_first_100_ms() {
    for delay in [0, 20, 50, 100] {
        let out = run(
            &format!("t204-at-{delay}"),
            &["--present"],
            b"",
            b"nn",
            delay,
            b"",
        );
        assert_eq!(step_of(&out), "2", "{delay} ms:\n{out}");
    }
}

#[test]
fn ac1_keys_on_both_sides_of_the_answer_and_the_answer_still_counts() {
    let mut seed = b"n".to_vec();
    seed.extend_from_slice(LIGHT);
    seed.push(b'n');
    let out = run("t204-answer", &[], &seed, b"", 0, b"");
    assert_eq!(step_of(&out), "2", "{out}");
    assert!(out.contains("LIGHT True"), "{out}");
}

#[test]
fn corner_an_esc_typed_early_is_a_key() {
    // `n`, a command line opened and closed with ESC, then `n` after the
    // first frame: two steps. Had the ESC been taken for an answer, the
    // second `n` would go into the command line.
    let out = run("t204-esc", &[], b"n:abc\x1b", b"", 0, b"n");
    assert_eq!(step_of(&out), "2", "{out}");
}

#[test]
fn ac3_nothing_typed_early_changes_nothing() {
    let out = run("t204-quiet", &[], b"", b"", 0, b"nn");
    assert_eq!(step_of(&out), "2", "{out}");
    assert!(out.contains("LIGHT False"), "{out}");
}

// ---------- AC2 ----------

#[test]
fn ac2_with_no_terminal_a_plain_error() {
    // Test step 2: a new session (no controlling terminal), stdin from
    // /dev/null, output to pipes.
    let dir = scratch("t204-no-tty");
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    let runtime = dir.join("run");
    std::fs::create_dir_all(&runtime).unwrap();
    // Its output on a terminal (a pty that is not its controlling one: a
    // service writing to one), so it gets past the CLI's own check of
    // stdout; no terminal to read keys from.
    let script = dir.join("no_tty.py");
    std::fs::write(
        &script,
        r#"
import os, pty, select, subprocess, sys, time
binary, cwd, runtime = sys.argv[1:4]
env = dict(os.environ, XDG_RUNTIME_DIR=runtime, TERM="xterm-256color")
master, slave = pty.openpty()
p = subprocess.Popen([binary, "--socket", os.path.join(cwd, "s.sock"), "main.rs"], cwd=cwd, env=env,
                     stdin=subprocess.DEVNULL, stdout=slave, stderr=slave,
                     start_new_session=True, close_fds=True)
os.close(slave)
out = b""
end = time.monotonic() + 30
while time.monotonic() < end:
    r, _, _ = select.select([master], [], [], 0.05)
    if r:
        try:
            chunk = os.read(master, 65536)
        except OSError:
            break
        if not chunk:
            break
        out += chunk
    elif p.poll() is not None:
        break
try:
    code = p.wait(timeout=10)
except subprocess.TimeoutExpired:
    p.kill()
    code = "KILLED"
print("CODE", code)
print("STDERR", repr(out.decode(errors="replace")))
"#,
    )
    .unwrap();
    let run = tiza_qa::python(
        &script,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            runtime.to_str().unwrap(),
        ],
        std::time::Duration::from_secs(60),
    );
    let out = run.stdout;
    assert!(out.contains("CODE 1"), "{out}");
    let stderr = out
        .lines()
        .find_map(|l| l.strip_prefix("STDERR "))
        .unwrap_or_default();
    assert!(!stderr.contains("crashed"), "{stderr}");
    assert!(
        stderr.contains("needs a terminal to read keys from"),
        "{stderr}"
    );
    assert!(
        stderr.contains("tiza render") && stderr.contains("tiza check"),
        "{stderr}"
    );
    // One line and nothing else: no screen set up, no escape codes.
    assert_eq!(stderr.matches("\\n").count(), 1, "{stderr}");
    assert!(!stderr.contains("\\x1b"), "{stderr}");
    assert!(!runtime.join("panics.log").exists(), "a panic was logged");
    assert!(
        !Path::new(&dir.join("s.sock")).exists(),
        "the socket was left"
    );
}
