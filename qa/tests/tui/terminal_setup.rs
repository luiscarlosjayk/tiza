//! Terminal support: the claims that docs/reference/terminals.md
//! makes about the code stay true.
//!
//! The text of the page (what is verified, the caveats, the report line) is
//! a manual check: since the test rule of AGENTS.md, no test pins prose.
//! These tests pin the claims the page makes about the code: a plain (not
//! tmux-wrapped) OSC 11/10 query, no `COLORTERM` check, no kitty keyboard
//! flags, and `--theme terminal` drawing without a single 24-bit colour.
//! Bracketed paste on and mouse capture only with `--mouse` are checked on
//! a real pty, in the resize-at-start and start-up keys tests. Each frame in
//! one synchronized update, also after a crash, is checked here on a real
//! pty. If one of these changes, docs/reference/terminals.md has to change
//! with it.

use std::path::{Path, PathBuf};

use tiza_qa::Live;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Every `.rs` file under `crates/`, as (path, contents).
fn crate_sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push((p.clone(), std::fs::read_to_string(&p).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("crates"), &mut out);
    assert!(!out.is_empty());
    out
}

#[test]
fn the_colour_query_is_plain_osc_not_wrapped_for_tmux() {
    // docs/reference/terminals.md says `allow-passthrough` changes neither
    // tmux 3.3a nor 3.5a: that holds only while the query is not a
    // `ESC P tmux;` passthrough
    // (wrapped, tmux 3.5a loses the answer).
    assert_eq!(
        tiza_tui::appearance::QUERY,
        "\x1b]11;?\x1b\\\x1b]10;?\x1b\\"
    );
    for (path, src) in crate_sources() {
        assert!(
            !src.contains("Ptmux;"),
            "{} wraps something for tmux passthrough: update the tmux caveat in docs/reference/terminals.md",
            path.display()
        );
    }
}

#[test]
fn no_colorterm_check_and_no_kitty_keyboard_flags() {
    for (path, src) in crate_sources() {
        assert!(
            !src.contains("\"COLORTERM\""),
            "{} reads COLORTERM: docs/reference/terminals.md says there is no 256-colour fallback",
            path.display()
        );
        assert!(
            !src.contains("KeyboardEnhancementFlags"),
            "{} enables the kitty keyboard protocol: docs/reference/terminals.md says Tiza does not",
            path.display()
        );
    }
}

#[test]
fn the_terminal_theme_sends_no_24_bit_colour() {
    let live = Live::from_code(
        "fn main() {\n    let s = String::from(\"hi\");\n    println!(\"{s}\");\n}\n",
        "rs",
        "terminal-theme",
    );
    live.api_ok(
        "annotation.add",
        serde_json::json!({"type": "note", "line": 2, "text": "s is born"}),
    );
    live.api_ok(
        "annotation.add",
        serde_json::json!({"type": "box", "from_line": 1, "to_line": 4, "label": "main"}),
    );
    live.command("theme terminal");
    let (symbols, styles) = live.snapshot(100, 20);
    assert!(symbols.contains("s is born"), "{symbols}");
    assert!(
        !styles.contains('#'),
        "`--theme terminal` drew a 24-bit colour:\n{styles}"
    );
    // The dark theme, for contrast, is all RGB.
    live.command("theme dark");
    let (_, dark) = live.snapshot(100, 20);
    assert!(dark.contains('#'), "{dark}");
}

/// A document of 200 lines `let vN = N;` with a box of step 2 that plays
/// `draw`, in a scratch folder: its path.
fn sync_doc(dir: &Path) -> PathBuf {
    let code: Vec<String> = (1..=200).map(|i| format!("let v{i} = {i};")).collect();
    let doc = serde_json::json!({
        "schema": 1, "source": {"language": "rs", "first_line": 1}, "code": code,
        "annotations": [
            {"id": "b2", "step": 2, "type": "box", "from_line": 3, "to_line": 5, "label": "drawn", "effect": "draw"}
        ],
        "step": 1
    });
    let path = dir.join("doc.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    path
}

/// What the TUI wrote between the moments it entered and left the
/// alternate screen, cut at the marks of a synchronized update: the marks
/// in order, the frames (each draw of ratatui hides or shows the cursor
/// once) inside the updates and between them, the frames that show the
/// cursor (the `:` command line) inside them, and the text drawn outside
/// them (what is left once the escape sequences are taken out). Each line
/// starts with `tag`.
const SYNC_ANALYSIS: &str = r##"
import re
SEQ = rb"\x1b\[[0-9;?<>=]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[()][0-9A-Za-z]|\x1b[=>78]"
OPEN, CLOSE = b"\x1b[?2026h", b"\x1b[?2026l"
def closed_since(out, before, updates=1):
    """The updates after `before` ended: the last mark is an end, and at
    least `updates` ends came after `before`."""
    last = out.rfind(CLOSE)
    return last >= before and last > out.rfind(OPEN) and out[before:].count(CLOSE) >= updates
def ordered(out):
    """The last end comes before the TUI leaves the alternate screen."""
    end, leave = out.rfind(CLOSE), out.rfind(b"\x1b[?1049l")
    return end != -1 and leave != -1 and end < leave
def analyse(raw, tag=""):
    start, leave = raw.find(b"\x1b[?1049h"), raw.rfind(b"\x1b[?1049l")
    body = raw[start:leave] if leave > start else raw[start:]
    marks = b"".join(re.findall(rb"\x1b\[\?2026([hl])", body)).decode()
    parts = re.split(rb"\x1b\[\?2026[hl]", body)
    inside, outside = parts[1::2], parts[0::2]
    frames = lambda ps: sum(p.count(b"\x1b[?25l") + p.count(b"\x1b[?25h") for p in ps)
    text = b"".join(re.sub(SEQ, b"", p) for p in outside).strip()
    print(tag + "MARKS", marks, flush=True)
    print(tag + "FRAMES", frames(inside), frames(outside[1:-1]), flush=True)
    print(tag + "CURSOR_INSIDE", sum(p.count(b"\x1b[?25h") for p in inside), flush=True)
    print(tag + "TEXT_OUTSIDE", len(text), repr(text[:80]), flush=True)
"##;

/// The value of the line `NAME value` of a pty script.
fn field(out: &str, name: &str) -> String {
    out.lines()
        .find_map(|l| l.strip_prefix(name))
        .unwrap_or_else(|| panic!("{name}: {out}"))
        .trim()
        .to_string()
}

/// docs/reference/terminals.md: each frame comes in a synchronized update
/// (mode 2026), so the terminal paints it at once. On a real pty: a jump
/// to the end, back to the top, five frames of an effect (slow, with the
/// `effect:slow` test hook) and a key that ends it, the command line with
/// its cursor, a resize and `:q!`. After each one the update of its frame
/// ends: an update that stays open until the next frame fails here. Every
/// frame is inside one update, and no text is drawn outside them.
#[test]
fn each_frame_is_one_synchronized_update() {
    let dir = tiza_qa::scratch("sync-frames");
    sync_doc(&dir);
    let body = format!(
        "{SYNC_ANALYSIS}{}",
        r##"
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="effect:slow")
s = Pty([binary, "--no-save", "--socket", "s.sock", "doc.tiza"], 30, 100, env=env)
def key(keys, updates=1, what=None):
    before = len(s.out)
    s.send(keys)
    return s.wait_for(lambda out: closed_since(out, before, updates), 10, what or repr(keys))
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    key(b"G")
    key(b"g")
    key(b"n", 5, "five frames of the effect")
    key(b"\x1b")
    key(b":")
    key(b"\x1b")
    before = len(s.out)
    s.resize(24, 90)
    s.wait_for(lambda out: closed_since(out, before), 10, "the frame of the resize")
s.send(b":q!\r")
print("EXIT", s.finish(10), flush=True)
analyse(s.out)
"##
    );
    let out = tiza_qa::pty_script(
        &dir,
        "sync.py",
        &body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    assert!(out.lines().any(|l| l == "EXIT 0"), "{out}");
    // Begin, end, begin, end…: the last update ends before the TUI exits.
    let marks = field(&out, "MARKS");
    assert!(
        marks.len() >= 20 && marks == "hl".repeat(marks.len() / 2),
        "{marks}"
    );
    // One frame in each update, and none between two of them.
    assert_eq!(
        field(&out, "FRAMES"),
        format!("{} 0", marks.len() / 2),
        "{out}"
    );
    let cursor: usize = field(&out, "CURSOR_INSIDE").parse().unwrap();
    assert!(cursor >= 1, "the frame of the command line: {out}");
    assert!(field(&out, "TEXT_OUTSIDE").starts_with("0 "), "{out}");
}

/// A panic in a frame: in its layout, before its update begins, no update
/// opens; after it begins, the panic hook ends it. Either way the last
/// update ends before the TUI leaves the alternate screen, so the terminal
/// does not keep the frame before on screen. (`draw:panic-in-layout` and
/// `draw:panic-after-begin` are test hooks of the `test-hooks` feature.)
#[test]
fn a_panic_in_a_frame_leaves_no_synchronized_update_open() {
    let dir = tiza_qa::scratch("sync-panic");
    sync_doc(&dir);
    let body = format!(
        "{SYNC_ANALYSIS}{}",
        r##"
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
for tag, hook in [("layout ", "draw:panic-in-layout"), ("update ", "draw:panic-after-begin")]:
    env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK=hook)
    s = Pty([binary, "--no-save", "--socket", tag.strip() + ".sock", "doc.tiza"], 30, 100, env=env)
    s.wait_for(lambda out: b"tiza crashed" in out, 10, tag + "crash")
    print(tag + "EXIT", s.finish(10), flush=True)
    analyse(s.out, tag)
    print(tag + "ORDER", ordered(s.out), flush=True)
"##
    );
    let out = tiza_qa::pty_script(
        &dir,
        "panic.py",
        &body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    // In the layout no update began: the only mark is the end that the
    // panic hook sends in any case.
    assert_eq!(field(&out, "layout MARKS"), "l", "{out}");
    assert_eq!(field(&out, "update MARKS"), "hl", "{out}");
    for tag in ["layout", "update"] {
        assert_eq!(field(&out, &format!("{tag} ORDER")), "True", "{out}");
    }
}
