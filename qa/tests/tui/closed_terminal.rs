//! Closing the terminal without `:q` left `tiza` spinning at 100 %
//! of a CPU.
//!
//! Where no SIGHUP came (a pty that is not the process's controlling
//! terminal, SIGHUP ignored as under `nohup`), crossterm took the hung-up
//! tty for one that is always readable and empty and polled it again at
//! once; with a step typing, the draw failed instead and the exit aborted
//! (a panic writing to the gone terminal, inside the panic hook). With
//! SIGHUP the process died where it stood and left its socket file.
//!
//! Acceptance criteria:
//! - AC1 When the controlling terminal goes away (master side of the pty
//!   closed, SIGHUP, read/poll on the tty returning EOF, EIO or hang-up),
//!   Tiza exits within 1 s and CPU stays idle meanwhile.
//! - AC2 What is saved follows the rules of Ctrl-C (autosave on: the
//!   document is saved; `--no-save`/autosave off: nothing is written); the
//!   socket file is removed.
//! - AC3 No write to the gone terminal makes the exit hang or panic; any
//!   message goes to the panic/log path, not stderr.
//! - AC4 This file: pty closed from the master side (and SIGHUP) in normal
//!   and presentation mode, with and without `--no-save`, during a typing
//!   animation; the process is gone within the deadline and the CPU it used
//!   is small; CHANGELOG (Fixed).
//!
//! - AC5 SIGTERM and SIGINT from outside end Tiza as Ctrl-C does (the
//!   terminal comes back, the socket goes, Ctrl-C's save rules, what is
//!   not saved on stderr), with exit code 143 and 130; also when one comes
//!   before Tiza takes the screen (it never takes it). A second signal ends
//!   the process at once. When the main thread is blocked in a write to a
//!   terminal that nobody reads, a watchdog removes the socket and ends the
//!   process 5 s after the signal. SIGHUP exits with 0.
//!
//! Corner cases: an MCP server (`tiza mcp`) connected to the instance gets
//! a clean disconnect, not a hang.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use tiza_qa::scratch;

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n}\n";

/// The process has to exit on its own after the close: the bug never did
/// (it polled the dead tty at 100 % CPU). A condition with a generous
/// timeout, as the reaper waits for it: how soon it goes (the AC's 1 s, on
/// an idle machine) is printed, not asserted, so a loaded machine running
/// the whole suite is no false red.
const GONE_WITHIN_S: f64 = 10.0;

/// CPU the whole process may use, start-up included (a debug build). A
/// spin burns one CPU second per second until the 10 s wait gives up.
const CPU_AT_MOST_S: f64 = 2.0;

/// `main.rs` with an existing sidecar, as Tiza itself wrote it.
fn project(name: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(name);
    let src = dir.join("main.rs");
    std::fs::write(&src, CODE).unwrap();
    let mut doc = tiza_core::Document::from_code(CODE, "rs", Some(src.display().to_string()));
    doc.reference_file("main.rs", None);
    let tiza = dir.join("main.rs.tiza");
    tiza_core::fs::save_document(&doc, &tiza).unwrap();
    (dir, tiza)
}

/// `typed.tiza`: step 1 types three lines, one an hour, so that it still
/// types when the terminal goes, also on a slow machine.
fn typing_project(name: &str) -> (PathBuf, PathBuf) {
    let dir = scratch(name);
    let doc = json!({
        "schema": 1,
        "source": {"language": "rs"},
        "code": CODE.lines().collect::<Vec<_>>(),
        "step": 0,
        "annotations": [{"id": "a1", "step": 1, "type": "note", "line": 2, "text": "typed"}],
        "moves": [{
            "at_step": 1, "action": "code.patch", "op": "insert", "after": "let b = 2;",
            "content": "    let c = 3;\n    let d = 4;\n    let e = 5;",
            "typing": "line", "speed_ms": 3_600_000
        }],
    });
    let path = dir.join("typed.tiza");
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    (dir, path)
}

/// How the terminal goes away.
#[derive(Clone, Copy)]
enum Close {
    /// The pty is the controlling terminal: closing the master sends SIGHUP.
    Sighup,
    /// The same with SIGHUP inherited as ignored (as under `nohup`): Tiza
    /// catches it all the same.
    SighupIgnored,
    /// The pty is only stdin/stdout/stderr, not the controlling terminal:
    /// no SIGHUP, the tty just hangs up.
    NoSighup,
    /// The code piped in (`cat main.rs | tiza --lang rs`) by a shell that
    /// ignores SIGHUP, so the keys come from `/dev/tty`; the pty closes.
    Piped,
    /// The terminal stays: `kill -HUP` to Tiza.
    KillHup,
    /// The terminal stays: `kill -TERM` to Tiza.
    KillTerm,
    /// The terminal stays: `kill -INT` to Tiza.
    KillInt,
}

struct Outcome {
    out: String,
}

impl Outcome {
    fn field(&self, key: &str) -> Option<&str> {
        self.out.lines().find_map(|l| l.strip_prefix(key))
    }

    /// What the terminal shows after Tiza left the alternate screen (only
    /// with [`Close::KillHup`] and the other kills, where the terminal
    /// stays).
    fn tail(&self) -> &str {
        self.field("TAIL ").unwrap_or("")
    }

    /// The process exited on its own, soon, cleanly, without burning CPU.
    fn gone_cleanly(&self) {
        self.gone_with(0);
    }

    /// The same, with the exit code `want`.
    fn gone_with(&self, want: i32) {
        let gone = self
            .field("GONE ")
            .unwrap_or_else(|| panic!("tiza did not exit after the close:\n{}", self.out));
        let v: Vec<&str> = gone.split_whitespace().collect();
        let (elapsed, cpu, code): (f64, f64, i32) = (
            v[0].parse().unwrap(),
            v[1].parse().unwrap(),
            v[2].parse().unwrap(),
        );
        assert_eq!(
            code, want,
            "exit status (-6 is the abort, -1 SIGHUP, -15 SIGTERM):\n{}",
            self.out
        );
        assert!(
            elapsed < GONE_WITHIN_S,
            "gone {elapsed:.2} s after the close:\n{}",
            self.out
        );
        assert!(cpu < CPU_AT_MOST_S, "used {cpu:.2} s of CPU:\n{}", self.out);
        // How soon (the AC's 1 s), for `--nocapture`.
        println!("gone {elapsed:.3} s after the close, {cpu:.3} s of CPU");
        assert!(
            !self.out.contains("WAIT_TIMEOUT") && !self.out.contains("API_ERR"),
            "{}",
            self.out
        );
    }
}

/// Runs `tiza ARGS` on a 30×100 pty in `dir` (socket `s.sock`, runtime
/// directory `run/` for the log), does `steps`, closes the terminal from
/// the master side as `close` says and waits (10 s at most) for the process
/// to exit, with `wait4` for its CPU.
///
/// A step is `["api", method, params]`, `["k", keys]`, `["status", field,
/// value]` (waits until `status` gives that value), `["chmod", file, mode]`
/// or `["wait_file", name]` (waits until the test creates that file).
fn run(dir: &Path, name: &str, args: &[&str], close: Close, steps: Value) -> Outcome {
    let body = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
args, close, steps = json.loads(sys.argv[3])
os.chdir(cwd)
env = dict(os.environ, XDG_RUNTIME_DIR=os.path.join(cwd, "run"), TERM="xterm-256color", TIZA_NO_OSC="1")
if close == "nohup":
    signal.signal(signal.SIGHUP, signal.SIG_IGN)  # inherited through exec
argv = [binary, "--socket", "s.sock", *args]
if close == "piped":
    # sh leads the session and ignores SIGHUP, so it waits for tiza and
    # exits with its status.
    argv = ["sh", "-c", 'trap "" HUP; cat main.rs | "$0" --socket s.sock "$@"', binary, *args]
s = Pty(argv, 30, 100, env=env, ctty=close != "tty")

def api(method, params):
    r = subprocess.run([binary, "--socket", "s.sock", "api", method, json.dumps(params)],
                       capture_output=True, text=True, timeout=10)
    if r.returncode != 0:
        print("API_ERR", method, r.stderr.strip(), flush=True)
        return None
    return json.loads(r.stdout)

def until(pred, what, timeout=10):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        if pred():
            return True
        s.settle(0.05)
    print("WAIT_TIMEOUT", what, flush=True)
    return False

ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame and socket")
for step in steps if ok else []:
    op = step[0]
    if op == "api":
        api(step[1], step[2])
    elif op == "k":
        s.send(step[1].encode())
    elif op == "status":
        until(lambda: (api("status", {}) or {}).get(step[1]) == step[2], f"status {step[1]}")
    elif op == "chmod":
        os.chmod(step[1], int(step[2], 8))
    elif op == "wait_file":
        until(lambda: os.path.exists(step[1]), step[1])

kills = {"kill": signal.SIGHUP, "term": signal.SIGTERM, "int": signal.SIGINT}
if close in kills:
    # The terminal stays: the signal to tiza alone.
    t0 = time.monotonic()
    os.kill(s.p.pid, kills[close])
else:
    # The terminal goes away: the master side closes, nobody reads it any more.
    s._stop_reader()
    os.close(s.master)
    s.closed = True
    t0 = time.monotonic()
done = {}
def reap():
    done["r"] = os.wait4(s.p.pid, 0)
    done["t"] = time.monotonic()
th = threading.Thread(target=reap, daemon=True)
th.start()
th.join(10)
if "r" in done:
    _, status, ru = done["r"]
    s.p.returncode = os.waitstatus_to_exitcode(status)
    print("GONE %.3f %.3f %d" % (done["t"] - t0, ru.ru_utime + ru.ru_stime, s.p.returncode), flush=True)
    if not s.closed:
        s.finish(10)
        out = s.out
        left = out.rfind(b"\x1b[?1049l")
        print("TAIL", plain(out[left + 8:] if left >= 0 else b"NOT_LEFT").replace("\n", " ").strip(), flush=True)
else:
    cpu = subprocess.run(["ps", "-o", "%cpu=", "-p", str(s.p.pid)], capture_output=True, text=True).stdout.strip()
    print("STILL_ALIVE %cpu", cpu, flush=True)
    os.killpg(s.p.pid, signal.SIGKILL)
    th.join(5)
"##;
    let close = match close {
        Close::Sighup => "hup",
        Close::SighupIgnored => "nohup",
        Close::NoSighup => "tty",
        Close::Piped => "piped",
        Close::KillHup => "kill",
        Close::KillTerm => "term",
        Close::KillInt => "int",
    };
    let out = tiza_qa::pty_script(
        dir,
        name,
        body,
        &[
            tiza_qa::bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &json!([args, close, steps]).to_string(),
        ],
    );
    Outcome { out }
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_default()
}

fn note(text: &str) -> Value {
    json!(["api", "annotation.add", {"type": "note", "line": 2, "text": text}])
}

#[test]
fn no_sighup_autosave_on_saves_and_exits() {
    // Test step 2, the failed-autosave half: the sidecar is read-only when
    // the note is added, writable again when the terminal goes; the exit
    // saves it, as Ctrl-C does.
    for close in [Close::NoSighup, Close::Sighup] {
        let (dir, tiza) = project("autosave");
        let r = run(
            &dir,
            "autosave.py",
            &["main.rs"],
            close,
            json!([
                ["chmod", "main.rs.tiza", "444"],
                note("hello"),
                ["status", "annotations", 1],
                ["chmod", "main.rs.tiza", "644"],
            ]),
        );
        r.gone_cleanly();
        assert!(read(&tiza).contains("hello"), "{}", read(&tiza));
        assert!(!dir.join("s.sock").exists(), "the socket file is removed");
    }
}

#[test]
fn no_save_writes_nothing_and_says_so_in_the_log() {
    // `--no-save`: nothing written; what was not saved goes to the log, not
    // to the terminal that is gone.
    for (close, name) in [
        (Close::NoSighup, "no-save-tty"),
        (Close::Sighup, "no-save-hup"),
    ] {
        let (dir, tiza) = project(name);
        let before = std::fs::read(&tiza).unwrap();
        let r = run(
            &dir,
            "no-save.py",
            &["--no-save", "main.rs"],
            close,
            json!([note("hello"), ["status", "annotations", 1]]),
        );
        r.gone_cleanly();
        assert_eq!(std::fs::read(&tiza).unwrap(), before);
        assert!(!dir.join("s.sock").exists());
        let log = read(&dir.join("run/tiza/panics.log"));
        assert!(
            log.contains(
                "the terminal went away: quit without saving main.rs.tiza: 1 change not saved (note a1)"
            ),
            "{log}"
        );
    }
}

#[test]
fn presentation_while_a_step_types() {
    // Test step 3: `--present`, `n` starts a step that types (one line an
    // hour), and the terminal goes meanwhile — each way it can go. Before, without
    // SIGHUP the draw on the gone terminal ended in an abort.
    for (close, name) in [
        (Close::Sighup, "typing-hup"),
        (Close::SighupIgnored, "typing-nohup"),
        (Close::NoSighup, "typing-tty"),
    ] {
        let (dir, path) = typing_project(name);
        let before = std::fs::read(&path).unwrap();
        let r = run(
            &dir,
            "typing.py",
            &["--present", "--no-save", "typed.tiza"],
            close,
            json!([["k", "n"], ["status", "step", 1]]),
        );
        r.gone_cleanly();
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!dir.join("s.sock").exists());
    }
}

#[test]
fn no_sighup_and_idle_does_not_spin() {
    // The 100 % case: nothing to draw, no SIGHUP, the tty hung up.
    let (dir, tiza) = project("idle");
    let r = run(&dir, "idle.py", &["main.rs"], Close::NoSighup, json!([]));
    r.gone_cleanly();
    assert!(read(&tiza).contains("\"code\""));
}

#[test]
fn autosave_off_live_writes_nothing() {
    let (dir, tiza) = project("autosave-off");
    let before = std::fs::read(&tiza).unwrap();
    let r = run(
        &dir,
        "off.py",
        &["main.rs"],
        Close::NoSighup,
        json!([
            ["k", ":autosave off\r"],
            ["status", "autosave", false],
            note("hello"),
            ["status", "annotations", 1],
        ]),
    );
    r.gone_cleanly();
    assert_eq!(std::fs::read(&tiza).unwrap(), before);
    let log = read(&dir.join("run/tiza/panics.log"));
    assert!(log.contains("1 change not saved (note a1)"), "{log}");
}

#[test]
fn a_retry_that_fails_again_goes_to_the_log() {
    // The sidecar stays read-only: the save tried on the way out fails
    // again, and says so in the log.
    let (dir, tiza) = project("retry-fails");
    let r = run(
        &dir,
        "retry.py",
        &["main.rs"],
        Close::NoSighup,
        json!([
            ["chmod", "main.rs.tiza", "444"],
            note("hello"),
            ["status", "annotations", 1],
        ]),
    );
    r.gone_cleanly();
    assert!(!read(&tiza).contains("hello"));
    let log = read(&dir.join("run/tiza/panics.log"));
    assert!(
        log.contains("the terminal went away: could not save")
            && log.contains("1 change not saved (note a1)"),
        "{log}"
    );
}

#[test]
fn piped_stdin_only_the_tty_matters() {
    let dir = scratch("piped");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let r = run(
        &dir,
        "piped.py",
        &["--lang", "rs", "--no-save"],
        Close::Piped,
        json!([note("hello"), ["status", "annotations", 1]]),
    );
    r.gone_cleanly();
    assert!(!dir.join("s.sock").exists());
}

#[test]
fn kill_hup_on_a_live_terminal_still_prints_there() {
    // SIGHUP with the terminal still there (`kill -HUP`): it quits in
    // order, and stderr is that terminal, not the log.
    let (dir, _) = project("kill-hup");
    let r = run(
        &dir,
        "kill.py",
        &["--no-save", "main.rs"],
        Close::KillHup,
        json!([note("hello"), ["status", "annotations", 1]]),
    );
    r.gone_cleanly();
    assert!(
        r.tail()
            .contains("tiza: quit without saving main.rs.tiza: 1 change not saved (note a1)"),
        "{}",
        r.out
    );
    assert!(!read(&dir.join("run/tiza/panics.log")).contains("went away"));
    assert!(!dir.join("s.sock").exists());
}

#[test]
fn sigterm_and_sigint_end_tiza_as_ctrl_c_does() {
    for (close, code, sig) in [(Close::KillTerm, 143, "term"), (Close::KillInt, 130, "int")] {
        for present in [false, true] {
            // `--no-save`: nothing written, and stderr says what is lost.
            let name = format!("{sig}-{}", if present { "present" } else { "normal" });
            let (dir, tiza) = project(&name);
            let before = std::fs::read(&tiza).unwrap();
            let mut args = vec!["--no-save", "main.rs"];
            if present {
                args.insert(0, "--present");
            }
            let r = run(
                &dir,
                &format!("{name}.py"),
                &args,
                close,
                json!([note("hello"), ["status", "annotations", 1]]),
            );
            r.gone_with(code);
            assert_ne!(
                r.tail(),
                "NOT_LEFT",
                "{name}: the screen came back:\n{}",
                r.out
            );
            assert!(
                r.tail().contains(
                    "tiza: quit without saving main.rs.tiza: 1 change not saved (note a1)"
                ),
                "{name}:\n{}",
                r.out
            );
            assert_eq!(std::fs::read(&tiza).unwrap(), before, "{name}");
            assert!(!dir.join("s.sock").exists(), "{name}: the socket stays");
        }
        // Autosave on: the change is on disk.
        let (dir, tiza) = project(&format!("{sig}-autosave"));
        let r = run(
            &dir,
            &format!("{sig}-autosave.py"),
            &["main.rs"],
            close,
            json!([note("hello"), ["status", "annotations", 1]]),
        );
        r.gone_with(code);
        assert!(read(&tiza).contains("hello"), "{}", read(&tiza));
        assert!(!dir.join("s.sock").exists());
    }
}

/// Runs `tiza --theme auto --socket s.sock main.rs` on a pty with
/// `TIZA_TEST_HOOK=start:hold`: Tiza asks the terminal for its colours
/// (nobody answers), then holds before it takes the screen. The test sends
/// SIGTERM there and lets it go on.
#[test]
fn a_signal_before_the_screen_exits_without_taking_it() {
    let (dir, _) = project("term-start");
    let body = r##"
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
run = os.path.join(cwd, "run")
os.makedirs(os.path.join(run, "tiza"), mode=0o700)
os.chmod(run, 0o700)
os.chmod(os.path.join(run, "tiza"), 0o700)
hold = os.path.join(run, "tiza", "start-hold")
open(hold, "w").close()
env = dict(os.environ, XDG_RUNTIME_DIR=run, TERM="xterm-256color", TIZA_TEST_HOOK="start:hold")
for k in ("TIZA_SOCKET", "TIZA_SESSION", "TIZA_NO_OSC"):
    env.pop(k, None)
s = Pty([binary, "--theme", "auto", "--socket", "s.sock", "--no-save", "main.rs"], 30, 100, env=env, ctty=True)
if s.wait_for(lambda out: b"\x1b]11;?" in out and os.path.exists("s.sock"), 10, "the colour query"):
    os.kill(s.p.pid, signal.SIGTERM)
os.remove(hold)
print("EXIT", s.finish(10))
print("SCREEN", b"\x1b[?1049h" in s.out)
print("SOCKET", os.path.exists("s.sock"))
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "term-start.py",
        body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(out.contains("EXIT 143"), "{out}");
    assert!(out.contains("SCREEN False"), "it took the screen:\n{out}");
    assert!(out.contains("SOCKET False"), "{out}");
}

/// Runs `tiza --theme dark --socket s.sock main.rs` on a 500×200 pty whose master side
/// nobody reads after the start of the first frame (`?2026h`): the frame is
/// larger than the buffer of the pty, so its write blocks. The signal comes
/// after the start of that frame, thus after the check of the loop before
/// it. The test sends `signals` (one after the
/// other) and waits 10 s at most for the exit. Prints `GONE elapsed code`
/// and `SOCKET exists`.
fn blocked(dir: &Path, name: &str, signals: &[&str]) -> String {
    let body = r##"
import json
binary, cwd, sigs = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
os.chdir(cwd)
env = dict(os.environ, XDG_RUNTIME_DIR=os.path.join(cwd, "run"), TERM="xterm-256color", TIZA_NO_OSC="1")
for k in ("TIZA_SOCKET", "TIZA_SESSION", "TIZA_TEST_HOOK"):
    env.pop(k, None)
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 200, 500, 0, 0))
# A theme with a background: the first frame writes each of the 100 000
# cells, more than the buffer of a pty holds.
p = subprocess.Popen([binary, "--theme", "dark", "--socket", "s.sock", "--no-save", "main.rs"], stdin=slave, stdout=slave, stderr=slave, env=env, start_new_session=True)
os.close(slave)
seen = b""
end = time.monotonic() + 10
# Read only until the first frame starts: the rest of it stays unread.
while b"\x1b[?2026h" not in seen and time.monotonic() < end:
    r, _, _ = select.select([master], [], [], 0.05)
    if r:
        try:
            seen += os.read(master, 1)
        except OSError:
            break
if b"\x1b[?2026h" not in seen or not os.path.exists("s.sock"):
    print("NOT_STARTED", repr(seen[-200:]), flush=True)
t0 = time.monotonic()
for name in sigs:
    os.kill(p.pid, {"term": signal.SIGTERM, "int": signal.SIGINT}[name])
try:
    code = p.wait(timeout=10)
    print("GONE %.3f %d" % (time.monotonic() - t0, code), flush=True)
except subprocess.TimeoutExpired:
    print("STILL_ALIVE", flush=True)
    os.killpg(p.pid, signal.SIGKILL)
    p.wait(5)
print("SOCKET", os.path.exists("s.sock"), flush=True)
os.close(master)
"##;
    tiza_qa::pty_script(
        dir,
        name,
        body,
        &[
            tiza_qa::bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &json!(signals).to_string(),
        ],
    )
}

fn gone(out: &str) -> (f64, i32) {
    let g = out
        .lines()
        .find_map(|l| l.strip_prefix("GONE "))
        .unwrap_or_else(|| panic!("tiza did not exit:\n{out}"));
    let (t, c) = g.split_once(' ').unwrap();
    (t.parse().unwrap(), c.parse().unwrap())
}

#[test]
fn a_watchdog_ends_tiza_when_a_write_blocks_after_sigterm() {
    let (dir, _) = project("term-blocked");
    let out = blocked(&dir, "blocked.py", &["term"]);
    assert!(!out.contains("NOT_STARTED"), "{out}");
    let (elapsed, code) = gone(&out);
    assert_eq!(code, 143, "{out}");
    assert!(out.contains("SOCKET False"), "the socket stays:\n{out}");
    // The watchdog waits `SIGNAL_EXIT_WAIT` first.
    assert!(
        elapsed >= tiza_tui::signals::SIGNAL_EXIT_WAIT.as_secs_f64() - 0.1,
        "gone after {elapsed:.2} s: the write did not block\n{out}"
    );
}

#[test]
fn a_second_signal_ends_tiza_at_once() {
    let (dir, _) = project("term-twice");
    let out = blocked(&dir, "twice.py", &["term", "int"]);
    assert!(!out.contains("NOT_STARTED"), "{out}");
    let (elapsed, code) = gone(&out);
    // The two can come in either order: 143 or 130, never the watchdog.
    assert!(code == 143 || code == 130, "{out}");
    assert!(
        elapsed < tiza_tui::signals::SIGNAL_EXIT_WAIT.as_secs_f64() - 1.0,
        "gone after {elapsed:.2} s: the watchdog ended it, not the second signal\n{out}"
    );
}

#[test]
fn mcp_client_gets_a_clean_disconnect() {
    let (dir, _) = project("mcp");
    let sock = dir.join("s.sock");
    let script_dir = dir.clone();
    let tty = std::thread::spawn(move || {
        run(
            &script_dir,
            "mcp.py",
            &["--no-save", "main.rs"],
            Close::NoSighup,
            json!([["wait_file", "close-now"]]),
        )
    });
    // The instance is up once its socket is.
    let up = (0..200).any(|_| {
        if sock.exists() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
        false
    });
    assert!(up, "the instance never opened its socket");
    let mut mcp = tiza_qa::McpClient::start(&sock);
    let (err, text, _) = mcp.call("status", json!({}));
    assert!(!err, "{text}");
    std::fs::write(dir.join("close-now"), "").unwrap();
    tty.join().unwrap().gone_cleanly();
    // The next call answers (an error: nobody listens), it does not hang.
    let reply = mcp
        .request_within(
            "tools/call",
            json!({"name": "status", "arguments": {}}),
            Duration::from_secs(10),
        )
        .expect("tiza mcp answers after the instance is gone");
    let is_error = reply["result"]["isError"] == true || !reply["error"].is_null();
    assert!(is_error, "{reply}");
}
