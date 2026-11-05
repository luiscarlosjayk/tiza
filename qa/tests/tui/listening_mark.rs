//! The mark of the test build that says the event loop listens. The first
//! `?1049h` only says that the screen is set up: the loop reads keys and
//! resizes only after its first poll. The test build writes a private OSC
//! (`Pty.wait_listening` of the pty harness waits for it) right after that
//! poll, once. A test that sends a key or a resize to a listening TUI waits
//! for this mark.
//!
//! The order is fixed by the code: the mark is the line after the poll. A
//! mark that comes before the poll is not caught here, because a resize
//! before the poll is also drawn (by the first frame).

use tiza_qa::{bin, fixture, scratch};

/// What the script prints, one `KEY value` per line.
fn run(name: &str, socket: bool) -> String {
    let dir = scratch(name);
    for f in ["ownership.rs.tiza", "ownership.rs"] {
        std::fs::copy(fixture(&format!("shared/ownership/{f}")), dir.join(f)).unwrap();
    }
    let body = r##"
import json
binary, cwd, socket = sys.argv[1:4]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
args = ["--theme", "dark", "--no-save"]
args += ["--socket", "s.sock"] if socket == "1" else ["--no-socket"]
s = Pty([binary] + args + ["ownership.rs.tiza"], 30, 120, env=env)
def api(method):
    cmd = [binary, "--socket", "s.sock", "api", method]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
    except subprocess.TimeoutExpired:
        return None
    try:
        return json.loads(r.stdout) if r.returncode == 0 else {}
    except ValueError:
        return {}
listens = s.wait_listening()
print("LISTENS", listens, flush=True)
if listens:
    out = s.out
    setup = out.find(b"\x1b[?1049h")
    mark = out.find(s.LISTENING)
    print("AFTER_SETUP", 0 <= setup < mark, flush=True)
    # The first frame comes after the mark: the resize must come after it, or
    # that frame reads the new size and the test proves nothing.
    first = s.wait_for(lambda out: b"\x1b[?2026l" in out[mark:], 10, "the first frame")
    print("FIRST_FRAME", first, flush=True)
    before = len(s.out)
    s.resize(24, 100)
    if socket == "1":
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
    else:
        # The frame of the resize: the status bar on the last of 24 rows (the
        # first frame drew it on row 30), in an update that ends.
        frame = s.wait_for(lambda out: b"\x1b[24;1H" in out[before:] and b"\x1b[?2026l" in out[before:], 10, "the frame of the resize")
        print("FRAME", frame, flush=True)
s.send(b":q!\r")
print("EXIT", s.finish(10), flush=True)
print("MARKS", s.out.count(s.LISTENING), flush=True)
"##;
    tiza_qa::pty_script(
        &dir,
        &format!("{name}.py"),
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            if socket { "1" } else { "0" },
        ],
    )
}

fn has(out: &str, line: &str) -> bool {
    out.lines().any(|l| l.trim() == line)
}

#[test]
fn a_resize_after_the_listening_mark_is_drawn() {
    let out = run("listening-mark-socket", true);
    for want in [
        "LISTENS True",
        "AFTER_SETUP True",
        "FIRST_FRAME True",
        "SIZE (100, 24)",
        "EXIT 0",
        "MARKS 1",
    ] {
        assert!(has(&out, want), "no `{want}`:\n{out}");
    }
}

#[test]
fn the_mark_comes_once_also_without_a_socket() {
    let out = run("listening-mark-no-socket", false);
    for want in [
        "LISTENS True",
        "AFTER_SETUP True",
        "FIRST_FRAME True",
        "FRAME True",
        "EXIT 0",
        "MARKS 1",
    ] {
        assert!(has(&out, want), "no `{want}`:\n{out}");
    }
}
