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
