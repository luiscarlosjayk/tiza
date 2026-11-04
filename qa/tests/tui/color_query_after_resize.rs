//! The color query after a resize goes to the terminal of the keys, and
//! `TIZA_NO_OSC` stops it.

use std::path::Path;

use tiza_qa::{bin, scratch};

const CODE: &str = "fn main() {\n    println!(\"hi\");\n}\n";

// ---------------------------------------------------------------- AC4

/// What one run of `tiza --theme auto` sent: colour queries to the
/// terminal of the keys after a resize and in all, and to stdout in all.
struct Queries {
    keys_after_resize: usize,
    keys_total: usize,
    stdout_total: usize,
}

/// `tiza --theme auto main.rs` with the keys (stdin, the controlling
/// terminal) on one pty and stdout on a second one, resized once after
/// the first frame. Counts the colour queries that each pty receives.
fn resized(dir: &Path, name: &str, no_osc: bool) -> Queries {
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let body = r##"
binary, cwd, no_osc = sys.argv[1], sys.argv[2], sys.argv[3] == "1"
os.chdir(cwd)
Q = b"\x1b]11;?"


class Screen:
    """The second pty, tiza's stdout, drained all the time."""

    def __init__(self, rows, cols):
        self.master, self.slave = pty.openpty()
        self.size(rows, cols)
        self.name = os.ttyname(self.slave)
        self.cv = threading.Condition()
        self.out = b""
        threading.Thread(target=self.pump, daemon=True).start()

    def size(self, rows, cols):
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

    def pump(self):
        while True:
            r, _, _ = select.select([self.master], [], [], 0.05)
            if not r:
                continue
            try:
                chunk = os.read(self.master, 65536)
            except OSError:
                chunk = b""
            with self.cv:
                self.out += chunk
                self.cv.notify_all()
            if not chunk:
                return

    def wait_for(self, pred, timeout, what):
        end = time.monotonic() + timeout
        with self.cv:
            while not pred(self.out):
                left = min(end, DEADLINE) - time.monotonic()
                if left <= 0:
                    print("WAIT_TIMEOUT", what, flush=True)
                    return False
                self.cv.wait(min(left, 0.05))
        return True


screen = Screen(30, 100)
env = dict(os.environ, TERM="xterm-256color")
env.pop("TIZA_NO_OSC", None)
if no_osc:
    env["TIZA_NO_OSC"] = "1"
cmd = 'exec "$0" --theme auto --no-save --no-socket main.rs > "$1"'
s = Pty(["sh", "-c", cmd, binary, screen.name], 30, 100, env=env, ctty=True)
first = screen.wait_for(lambda out: b"\x1b[?1049h" in out and b"main.rs" in out, 10, "first frame")
# The query at start comes before the first frame; without TIZA_NO_OSC it
# must be read before the mark, so that it is not counted after the resize.
if first and not no_osc:
    first = s.wait_for(lambda out: Q in out, 10, "the query at start")
if first:
    keys_mark = len(s.out)
    with screen.cv:
        screen_mark = len(screen.out)
    screen.size(30, 118)
    s.resize(30, 118)
    # The redraw at the new size comes after the handler that asks.
    if screen.wait_for(lambda out: b"\x1b[2J" in out[screen_mark:], 10, "redraw after the resize"):
        if not no_osc:
            s.wait_for(lambda out: Q in out[keys_mark:], 10, "a query after the resize")
        print("KEYS_AFTER", s.out[keys_mark:].count(Q))
    s.send(b":q\r")
print("EXIT", s.finish(10))
print("KEYS_TOTAL", s.out.count(Q))
with screen.cv:
    print("STDOUT_TOTAL", screen.out.count(Q))
"##;
    let out = tiza_qa::pty_script(
        dir,
        name,
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            if no_osc { "1" } else { "0" },
        ],
    );
    let count = |key: &str| -> usize {
        out.lines()
            .find_map(|l| l.strip_prefix(key))
            .unwrap_or_else(|| panic!("no {key}in:\n{out}"))
            .trim()
            .parse()
            .unwrap()
    };
    assert!(out.contains("EXIT 0"), "{out}");
    Queries {
        keys_after_resize: count("KEYS_AFTER "),
        keys_total: count("KEYS_TOTAL "),
        stdout_total: count("STDOUT_TOTAL "),
    }
}

#[test]
fn tiza_no_osc_stops_the_query_after_a_resize_too() {
    let dir = scratch("no-osc");
    let q = resized(&dir, "no_osc.py", true);
    assert_eq!(
        (q.keys_total, q.stdout_total),
        (0, 0),
        "TIZA_NO_OSC: no colour query at start or after a resize"
    );
}

#[test]
fn the_query_after_a_resize_goes_to_the_terminal_of_the_keys() {
    // stdout on another pty: the query after a resize goes where the query
    // at start goes, to the terminal that the answer comes back from.
    let dir = scratch("tty");
    let q = resized(&dir, "tty.py", false);
    assert_eq!(q.stdout_total, 0, "a colour query went to stdout");
    assert!(
        q.keys_after_resize >= 1,
        "no colour query reached the terminal of the keys after the resize"
    );
}

// ------------------------------------------- a resize at the very start

/// `tiza --theme auto` on one pty, its controlling terminal. When
/// `resize`, the window changes size while the start-up read waits for
/// the answers (after the colour query, before the event loop listens for
/// a resize), and the test waits for the second query. Then the terminal
/// answers each query: dark for the one at start (light with no resize),
/// light for the second, so a light bar is the second answer's.
/// Returns how many colour queries Tiza wrote when its bar shows the theme
/// of the last answer, and the output.
fn queries_with_a_resize_at_start(name: &str, resize: bool) -> (usize, String) {
    let dir = scratch(name);
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let body = r##"
binary, cwd, resize = sys.argv[1], sys.argv[2], sys.argv[3] == "1"
os.chdir(cwd)
Q = b"\x1b]11;?"
env = dict(os.environ, TERM="xterm-256color")
env.pop("TIZA_NO_OSC", None)
s = Pty([binary, "--theme", "auto", "--no-save", "--no-socket", "main.rs"], 30, 120, env=env, ctty=True, raw=True)
ok = s.wait_for(lambda out: Q in out, 10, "the query at start")
if ok and resize:
    s.resize(30, 110)
    # The second query: from the check of the size after the first poll
    # when the resize came before it, or from the resize event after it.
    ok = s.wait_for(lambda out: out.count(Q) >= 2, 10, "the second query")
if ok:
    light = b"\x1b]11;rgb:f7f7/f5f5/efef\x1b\\\x1b]10;rgb:1f1f/2323/2828\x1b\\"
    dark = b"\x1b]11;rgb:1515/1717/1b1b\x1b\\\x1b]10;rgb:d0d0/d0d0/d0d0\x1b\\"
    # With a resize, the answers to the second query set the theme.
    s.send(dark + light if resize else light)
    ok = s.wait_for(lambda out: "light·auto" in plain(out), 10, "the bar")
print("QUERIES", s.out.count(Q), flush=True)
s.send(b":q\r")
print("EXIT", s.finish(10), flush=True)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        &format!("{name}.py"),
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            if resize { "1" } else { "0" },
        ],
    );
    assert!(
        out.contains("EXIT 0") && !out.contains("WAIT_TIMEOUT"),
        "{out}"
    );
    let n = out
        .lines()
        .find_map(|l| l.strip_prefix("QUERIES "))
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or_else(|| panic!("no QUERIES in:\n{out}"));
    (n, out)
}

#[test]
fn a_resize_while_the_start_up_read_waits_asks_for_the_colours_again() {
    // Before the event loop listens, a resize comes as no event: Tiza
    // compares the size with the one when it asked, and asks again. A
    // resize signal right after the first poll can ask a third time.
    let (resized, out) = queries_with_a_resize_at_start("resize-at-start", true);
    assert!(resized >= 2, "{out}");
    // With no resize, one query: the second one is the resize's.
    let (still, out) = queries_with_a_resize_at_start("no-resize-at-start", false);
    assert_ne!(still, resized, "{out}");
    assert_eq!(still, 1, "{out}");
}
