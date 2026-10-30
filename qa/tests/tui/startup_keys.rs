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
//! - AC3 Nothing changes when nothing is typed early (the start-up race,
//!   theme detection and hang-up tests stay green).
//! - AC4 This file; CHANGELOG (Fixed).
//!
//! Corner cases: a terminal that never answers (the 100 ms timeout) still
//! gives the keys back; an ESC typed early is a key, not part of an answer.
//!
//! Review (`rev_*`): an early key that quits ends them (nothing after it
//! runs, nothing is saved); an early ESC stays Esc when the next key comes
//! in a later read or after the answer; an early `:mouse on` turns the
//! mouse capture on.
//!
//! An answer that the end of the wait cuts short: its rest comes as keys
//! later, and the event loop takes it as the rest of the answer, also
//! after a slow first frame and with slow frames (a busy machine used the
//! second of the capture before the event loop read the rest). When the
//! rest never comes, the keys work again a second after the first frame.

use std::path::Path;

use tiza_qa::{bin, fixture, scratch};

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
    std::fs::copy(
        fixture("shared/ownership/ownership.rs.tiza"),
        dir.join("doc.tiza"),
    )
    .unwrap();
    std::fs::copy(
        fixture("shared/ownership/ownership.rs"),
        dir.join("ownership.rs"),
    )
    .unwrap();
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
# The light theme can come after the step: an answer that the start-up
# read cut short is taken with the keys, and its frame follows. Waited
# for, when the light answer was sent.
if b"]11;rgb:f7f7" in seed + typed + after:
    light = s.wait_for(lambda out: "light·auto" in plain(out), 10, "light theme")
else:
    light = "light·auto" in plain(s.out)
print("LIGHT", light, flush=True)
print("MOUSE_ON", b"\x1b[?1000h" in s.out, flush=True)
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
fn nn_before_the_start_moves_two_steps() {
    // Test step 1: in the input before Tiza reads it, so the query's read
    // takes it; the terminal never answers (the 100 ms timeout path).
    let out = run("seed", &["--present"], b"nn", b"", 0, b"");
    assert_eq!(step_of(&out), "2", "{out}");
    // Without `--mouse`, the terminal keeps its own mouse (text selection):
    // Tiza does not turn the capture on (moved here from the terminal setup tests).
    assert!(out.contains("MOUSE_ON False"), "{out}");
}

#[test]
fn nn_at_every_moment_of_the_first_100_ms() {
    for delay in [0, 20, 50, 100] {
        let out = run(
            &format!("at-{delay}"),
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
fn keys_on_both_sides_of_the_answer_and_the_answer_still_counts() {
    let mut seed = b"n".to_vec();
    seed.extend_from_slice(LIGHT);
    seed.push(b'n');
    let out = run("answer", &[], &seed, b"", 0, b"");
    assert_eq!(step_of(&out), "2", "{out}");
    assert!(out.contains("LIGHT True"), "{out}");
}

#[test]
fn an_esc_typed_early_is_a_key() {
    // `n`, a command line opened and closed with ESC, then `n` after the
    // first frame: two steps. Had the ESC been taken for an answer, the
    // second `n` would go into the command line.
    let out = run("esc", &[], b"n:abc\x1b", b"", 0, b"n");
    assert_eq!(step_of(&out), "2", "{out}");
}

// ---------- AC2 ----------

#[test]
fn with_no_terminal_a_plain_error() {
    // Test step 2: a new session (no controlling terminal), stdin from
    // /dev/null, output to pipes.
    let dir = scratch("no-tty");
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
# The slave stays open here until its output is read: on macOS the last
# close of the slave drops what the master has not read, and the child
# writes one line and exits at once.
out = b""
end = time.monotonic() + 30
while time.monotonic() < end and p.poll() is None:
    r, _, _ = select.select([master], [], [], 0.05)
    if r:
        out += os.read(master, 65536)
try:
    code = p.wait(timeout=10)
except subprocess.TimeoutExpired:
    p.kill()
    code = "KILLED"
# The child is gone: all it wrote waits on the master.
while select.select([master], [], [], 0)[0]:
    chunk = os.read(master, 65536)
    if not chunk:
        break
    out += chunk
os.close(slave)
os.close(master)
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
    assert!(
        !runtime.join("tiza").join("panics.log").exists(),
        "a panic was logged"
    );
    assert!(
        !Path::new(&dir.join("s.sock")).exists(),
        "the socket was left"
    );
}

// ---------- review ----------

#[test]
fn an_early_key_that_quits_ends_them() {
    // Ctrl-C (or `q`) first: the keys after it never run, and nothing is
    // saved (`x` would delete the selected annotation, autosaved).
    for (name, seed) in [("ctrl-c", b"\x03n\tx".as_slice()), ("q", b"qn\tx")] {
        let dir = scratch(&format!("quit-{name}"));
        std::fs::copy(
            fixture("shared/ownership/ownership.rs.tiza"),
            dir.join("doc.tiza"),
        )
        .unwrap();
        std::fs::copy(
            fixture("shared/ownership/ownership.rs"),
            dir.join("ownership.rs"),
        )
        .unwrap();
        let before = std::fs::read(dir.join("doc.tiza")).unwrap();
        let body = r##"
binary, cwd, seed = sys.argv[1], sys.argv[2], bytes.fromhex(sys.argv[3])
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color")
env.pop("TIZA_NO_OSC", None)
s = Pty([binary, "--theme", "auto", "--step", "2", "--socket", "s.sock", "doc.tiza"], 30, 120,
        env=env, ctty=True, raw=True, seed=seed)
print("EXIT", s.finish(10), flush=True)
"##;
        let out = tiza_qa::pty_script(
            &dir,
            &format!("quit-{name}.py"),
            body,
            &[bin().to_str().unwrap(), dir.to_str().unwrap(), &hex(seed)],
        );
        assert!(out.contains("EXIT 0"), "{name}: {out}");
        assert_eq!(
            std::fs::read(dir.join("doc.tiza")).unwrap(),
            before,
            "{name}: the document was written"
        );
    }
}

#[test]
fn an_early_esc_stays_esc_when_the_next_key_comes_later() {
    // The next key after the terminal's answer. (One in a later read of the
    // query is the same rule, `appearance::without_answers`, tested there:
    // in the same read, `ESC n` is Alt+n, as crossterm reads it too, so the
    // read boundary cannot be timed from here.)
    let mut seed = b"n:\x1b".to_vec();
    seed.extend_from_slice(LIGHT);
    seed.push(b'n');
    let out = run("esc-answer", &[], &seed, b"", 0, b"");
    assert_eq!(step_of(&out), "2", "{out}");
    assert!(out.contains("LIGHT True"), "{out}");
}

#[test]
fn an_early_mouse_on_turns_the_capture_on() {
    let out = run("mouse", &[], b":mouse on\r", b"", 0, b"nn");
    assert!(out.contains("MOUSE_ON True"), "{out}");
}

#[test]
fn the_rest_of_an_answer_cut_by_the_wait_is_not_keys() {
    // A slow terminal, or a busy machine: the answer comes in two parts,
    // 500 ms apart, so the wait (200 ms at most) ends inside an answer and
    // its rest comes as keys. Cut inside the background answer, the theme
    // stayed dark (CI on Linux saw it once in some hundred runs). Cut
    // inside the text answer, the `:` of `rgb:` opened the command line,
    // and the `nn` after it went there.
    for (cut, place) in [(9, "the background answer"), (33, "the text answer")] {
        let out = cut_answer_run(cut, "", "later");
        assert_eq!(step_of(&out), "2", "{place}: {out}");
        assert!(out.contains("LIGHT True"), "{place}: {out}");
    }
}

#[test]
fn after_a_slow_first_frame_the_rest_of_an_answer_is_still_not_keys() {
    // A busy machine: the second of the capture started before the first
    // frame, and a first frame of more than a second
    // (`draw:slow-first-frame`, a test hook, takes 1.5 s) dropped the
    // capture before the event loop read the rest. Under load the test
    // above failed in about a third of the runs.
    for (cut, place) in [(9, "the background answer"), (33, "the text answer")] {
        let out = cut_answer_run(cut, "draw:slow-first-frame", "later");
        assert_eq!(step_of(&out), "2", "{place}: {out}");
        assert!(out.contains("LIGHT True"), "{place}: {out}");
    }
}

#[test]
fn with_slow_frames_the_rest_of_an_answer_is_still_not_keys() {
    // Each key of the rest drew a frame: with frames of 100 ms more
    // (`draw:slow-frames`, a test hook) the keys of the rest used the
    // second of the capture before its end came.
    for (cut, place) in [(9, "the background answer"), (33, "the text answer")] {
        let out = cut_answer_run(cut, "draw:slow-frames", "later");
        assert_eq!(step_of(&out), "2", "{place}: {out}");
        assert!(out.contains("LIGHT True"), "{place}: {out}");
    }
}

#[test]
fn when_the_rest_never_comes_the_keys_work_after_a_second() {
    // The terminal never sends the rest: the capture takes the keys for a
    // second after the first frame, not more. A second that started at
    // the first key took `nn` too.
    let out = cut_answer_run(33, "", "never");
    assert_eq!(step_of(&out), "2", "{out}");
}

/// [`LIGHT`] in two parts: `cut` bytes after the colour query, then the
/// rest 500 ms later (`rest` is `later`) or never (`never`); then `nn`,
/// when the TUI is up (`later`) or more than a second after its first
/// frame (`never`). `hook` names a test hook, or is empty.
fn cut_answer_run(cut: usize, hook: &str, rest: &str) -> String {
    let dir = scratch("cut-answer");
    std::fs::copy(
        fixture("shared/ownership/ownership.rs.tiza"),
        dir.join("doc.tiza"),
    )
    .unwrap();
    std::fs::copy(
        fixture("shared/ownership/ownership.rs"),
        dir.join("ownership.rs"),
    )
    .unwrap();
    let body = r##"
import json
binary, cwd, light, cut, hook, rest = sys.argv[1], sys.argv[2], bytes.fromhex(sys.argv[3]), int(sys.argv[4]), sys.argv[5], sys.argv[6]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color")
env.pop("TIZA_NO_OSC", None)
if hook:
    env["TIZA_TEST_HOOK"] = hook
argv = [binary, "--theme", "auto", "--step", "0", "--no-save", "--socket", "s.sock", "doc.tiza"]
s = Pty(argv, 30, 120, env=env, ctty=True, raw=True)
def status():
    r = subprocess.run([binary, "--socket", "s.sock", "api", "status"], capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout) if r.returncode == 0 else {}
step = None
if s.wait_for(lambda out: b"\x1b]10;?" in out, 10, "colour query"):
    s.send(light[:cut])
    if rest == "later":
        time.sleep(0.5)  # the delay under test, not a synchronisation
        s.send(light[cut:])
        up = s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "the TUI")
    else:
        # The first frame shows the theme of the background answer, which
        # came whole; then more than the second of the capture goes by.
        up = s.wait_for(lambda out: "light·auto" in plain(out) and os.path.exists("s.sock"), 10, "first frame")
        time.sleep(1.5)  # the delay under test, not a synchronisation
    if up:
        s.send(b"nn")
        end = time.monotonic() + 10
        while time.monotonic() < end:
            step = status().get("step")
            if step == 2:
                break
            s.settle(0.05)
print("STEP", step, flush=True)
# The frame of the light theme follows the rest of the answer: waited for.
shown = s.wait_for(lambda out: "light·auto" in plain(out), 10, "light theme")
print("LIGHT", shown, flush=True)
if step != 2 or not shown:
    # The end of the screen, for the cause of a failure.
    print("SCREEN", repr(plain(s.out)[-2000:]), flush=True)
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
"##;
    tiza_qa::pty_script(
        &dir,
        "cut-answer.py",
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &hex(LIGHT),
            &cut.to_string(),
            hook,
            rest,
        ],
    )
}
