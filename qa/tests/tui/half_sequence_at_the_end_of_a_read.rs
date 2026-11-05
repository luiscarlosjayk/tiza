//! A half sequence at the end of a read: `ESC [` in one write, which
//! Alt+[ sends in many terminals, or `ESC O` (Alt+O). crossterm reads again
//! while its bytes make no event; on a blocking descriptor that second read
//! waited for the next key, and the event loop, the screen and the API
//! stopped with it. Tiza now reads the keys from its own non-blocking open
//! of the terminal; the descriptor of the shell does not change, and the
//! colour query of a resize still goes through an open that blocks.
//!
//! Each test drives `tiza` on a pty that is its controlling terminal,
//! writes the half sequence, waits until Tiza read it (nothing left in the
//! input queue of the pty), and then asks for several answers of the API in
//! a row: on Linux the queue can look empty a moment before the bytes reach
//! it, and only a later call would hang. The half sequence itself waits in
//! crossterm for the next key, which can be lost or join it: that is a
//! limit of the docs, and no test here asks for it. Before Tiza quits, a
//! key closes the half sequence, and Tiza must quit with `:q!`.

use tiza_qa::{bin, fixture, scratch};

const ESC_BRACKET: &str = "\x1b[";
const ESC_O: &str = "\x1bO";

/// What the script prints, one `KEY value` per line.
fn run(name: &str, mode: &str, half: &str, present: bool) -> String {
    let dir = scratch(name);
    for f in ["ownership.rs.tiza", "ownership.rs"] {
        std::fs::copy(fixture(&format!("shared/ownership/{f}")), dir.join(f)).unwrap();
    }
    let body = r##"
import json
binary, cwd, mode, half, present = sys.argv[1:6]
half = bytes.fromhex(half)
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color")
if mode == "keys":
    # The theme that follows the terminal, which never answers: the colour
    # query owes its answers while the half sequence waits.
    env.pop("TIZA_NO_OSC", None)
    theme = "auto"
else:
    env["TIZA_NO_OSC"] = "1"
    theme = "dark"
args = ["--theme", theme, "--step", "0", "--no-save", "--socket", "s.sock"]
args += ["--present"] if present == "1" else []
args += ["ownership.rs.tiza"]
argv = [binary] + args
if mode == "stdin":
    # stdin is not a terminal: Tiza opens /dev/tty, its controlling terminal.
    argv = ["/bin/sh", "-c", 'exec "$0" "$@" < /dev/null', binary] + args
rows, cols = 30, 120
s = Pty(argv, rows, cols, env=env, ctty=True, raw=True)
# A second descriptor of the open that the test made and Tiza inherited (the
# shell's): the harness may close its own when Tiza is gone.
shell = os.dup(s._slave)

def api(method, params=None, timeout=10):
    cmd = [binary, "--socket", "s.sock", "api", method] + ([json.dumps(params)] if params is not None else [])
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return None
    try:
        return json.loads(r.stdout) if r.returncode == 0 else {}
    except ValueError:
        return {}

def answers(n=3):
    # n answers of `status` in a row, a moment apart; the last one, or None.
    st = None
    for _ in range(n):
        st = api("status")
        if not (st or {}).get("max_step"):
            return None
        s.settle(0.1)
    return st

def pending():
    # Bytes in the input queue of the pty that Tiza has not read yet.
    return struct.unpack("i", fcntl.ioctl(shell, termios.FIONREAD, b"\0\0\0\0"))[0]

def read_all(what):
    return s.wait_for(lambda out: pending() == 0, 10, what)

def nonblocking():
    return bool(fcntl.fcntl(shell, fcntl.F_GETFL) & os.O_NONBLOCK)

end = time.monotonic() + 10
ready = False
while time.monotonic() < end and not ready:
    ready = os.path.exists("s.sock") and bool((api("status", timeout=2) or {}).get("max_step"))
    if not ready:
        s.settle(0.05)
print("READY", ready, flush=True)
if ready:
    s.send(half)
    print("READ", read_all("Tiza reads the half sequence"), flush=True)
    if mode == "resize":
        # The resize first: its frame is what the test looks for.
        s.resize(24, 100)
        end = time.monotonic() + 10
        size = None
        while time.monotonic() < end:
            got = api("status")
            if got is None:
                break
            size = (got.get("cols"), got.get("rows"))
            if size == (100, 24):
                break
            s.settle(0.05)
        print("SIZE", size, flush=True)
    st = answers()
    print("API", "ok" if st else "none", flush=True)
    if st is not None and "query_blocks" in st:
        print("QUERY_BLOCKS", st["query_blocks"], flush=True)
    if mode in ("api", "stdin") and st is not None:
        before = len(s.out)
        api("step.next", {})
        print("STEP", (api("status") or {}).get("step"), flush=True)
        print("FRAME", s.wait_for(lambda out: len(out) > before, 10, "a frame"), flush=True)
    if mode == "keys" and st is not None:
        # The first `n` may join the half sequence (the limit): after it the
        # loop goes on, and the next two are keys.
        s.send(b"n")
        read_all("Tiza reads n")
        print("API_AFTER_KEY", "ok" if answers() else "none", flush=True)
        s.send(b"nn")
        end = time.monotonic() + 10
        step = None
        while time.monotonic() < end:
            step = (api("status") or {}).get("step")
            if step is not None and step >= 2:
                break
            s.settle(0.05)
        print("STEP", step, flush=True)
    if mode == "shell":
        print("NONBLOCK_RUNNING", nonblocking(), flush=True)
        s.kill()
        print("NONBLOCK_AFTER_KILL", nonblocking(), flush=True)
        sys.exit(0)
    if mode != "keys":
        # A key that closes the half sequence (it can be lost with it).
        s.send(b"x")
        read_all("Tiza reads x")
s.send(b":q!\r")
print("EXIT", s.finish(10), flush=True)
"##;
    let half: String = half.bytes().map(|b| format!("{b:02x}")).collect();
    tiza_qa::pty_script(
        &dir,
        &format!("{name}.py"),
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            mode,
            &half,
            if present { "1" } else { "0" },
        ],
    )
}

fn has(out: &str, line: &str) -> bool {
    out.lines().any(|l| l == line)
}

/// Tiza listened, read the half sequence, and the API answered after it.
fn assert_goes_on(out: &str) {
    assert!(has(out, "READY True"), "Tiza did not start:\n{out}");
    assert!(
        has(out, "READ True"),
        "Tiza did not read the half sequence:\n{out}"
    );
    assert!(
        has(out, "API ok"),
        "the API did not answer after the half sequence:\n{out}"
    );
}

/// The step moved by the API, a frame came, and `:q!` quit.
fn assert_step_and_exit(out: &str) {
    assert!(has(out, "STEP 1"), "step.next did not apply:\n{out}");
    assert!(has(out, "FRAME True"), "no frame after step.next:\n{out}");
    assert!(has(out, "EXIT 0"), "Tiza did not quit with :q!:\n{out}");
}

#[test]
fn an_escape_and_a_bracket_in_one_read_do_not_stop_the_api() {
    for (name, half, present) in [
        ("half_sequence_api", ESC_BRACKET, false),
        ("half_sequence_api_o", ESC_O, false),
        ("half_sequence_api_present", ESC_BRACKET, true),
    ] {
        let out = run(name, "api", half, present);
        assert_goes_on(&out);
        assert_step_and_exit(&out);
        // The colour query of a resize writes through an open that blocks.
        assert!(
            has(&out, "QUERY_BLOCKS True"),
            "the colour query would write through a non-blocking open:\n{out}"
        );
    }
}

#[test]
fn a_resize_after_an_escape_and_a_bracket_is_drawn() {
    let out = run("half_sequence_resize", "resize", ESC_BRACKET, false);
    assert!(has(&out, "READ True"), "Tiza did not read ESC [:\n{out}");
    assert!(
        has(&out, "SIZE (100, 24)"),
        "the resize was not drawn:\n{out}"
    );
    assert_goes_on(&out);
    assert!(has(&out, "EXIT 0"), "Tiza did not quit with :q!:\n{out}");
}

#[test]
fn the_keys_after_an_escape_and_a_bracket_are_read() {
    let out = run("half_sequence_keys", "keys", ESC_BRACKET, false);
    assert_goes_on(&out);
    assert!(
        has(&out, "API_AFTER_KEY ok"),
        "the loop stopped again after the key that closed ESC [:\n{out}"
    );
    let step: Option<u32> = out
        .lines()
        .find_map(|l| l.strip_prefix("STEP "))
        .and_then(|s| s.parse().ok());
    assert!(
        step.is_some_and(|s| s >= 2),
        "the keys after ESC [ did not move the step:\n{out}"
    );
    assert!(has(&out, "EXIT 0"), "Tiza did not quit with :q!:\n{out}");
}

#[test]
fn stdin_that_is_not_a_terminal_does_not_stop_the_api() {
    let out = run("half_sequence_stdin", "stdin", ESC_BRACKET, false);
    assert_goes_on(&out);
    assert_step_and_exit(&out);
    assert!(
        has(&out, "QUERY_BLOCKS True"),
        "the colour query would write through a non-blocking open:\n{out}"
    );
}

#[test]
fn the_terminal_of_the_shell_stays_blocking_also_after_a_kill() {
    let out = run("half_sequence_shell", "shell", ESC_BRACKET, false);
    assert_goes_on(&out);
    assert!(
        has(&out, "NONBLOCK_RUNNING False"),
        "Tiza made the shell's descriptor non-blocking:\n{out}"
    );
    assert!(
        has(&out, "NONBLOCK_AFTER_KILL False"),
        "the shell's descriptor stayed non-blocking after kill -9:\n{out}"
    );
}
