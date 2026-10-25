//! A terminal resize in the first milliseconds after start was
//! lost: the first frame stayed laid out for the old size until the next
//! key. crossterm sets up its input source (and starts catching SIGWINCH)
//! on the first `event::poll`, and the event loop drew the first frame
//! before that poll; a resize while that frame was laid out produced no
//! `Resize` event, and nothing redrew.
//!
//! Acceptance criteria:
//! - AC1 A resize at any moment of startup ends in the same frame as
//!   starting at the final size: in a pty, start at W+1 (and W-1, and two
//!   resizes in a row), resize to W at 0, 5, 10, 20, 50, 100 and 200 ms;
//!   the screen equals a fresh start at W. Release and debug builds both
//!   (this file runs the build `cargo test` makes; the release build goes
//!   through the same script by hand, see the card).
//! - AC2 The fix does not depend on timing luck: the input source is set up
//!   (a zero-timeout poll) before the first draw. No sleeps.
//! - AC3 Same for rows, not only columns, in presentation and normal mode.
//! - AC4 `scripts/herdr-matrix.sh --check --tiza <release build>` gives no
//!   differences 3 runs in a row (Luis's Mac, not here).
//! - AC5 This file; CHANGELOG (Fixed).
//!
//! Corner cases: a resize while the host-appearance query reads /dev/tty
//! (the answer and the resize both land); a resize before the socket server
//! is ready and while a big document loads; piped stdin; two resizes within
//! 1 ms (the last size wins); `--smoke` keeps drawing one frame only.
//!
//! How the race is run: each case starts every delay at once (one Tiza per
//! delay, on its own pty that is its controlling terminal, so the kernel
//! sends SIGWINCH as a real terminal does), waits the delay — counted from
//! the terminal setup (`ESC[?2004h`, written right before the first frame,
//! so the resize lands while it is laid out), or from the spawn as the card
//! measured it — and sets the final size. Then the emulated screen,
//! characters and styles, has to come to equal a fresh start at the final
//! size, waited for as a condition (`WAIT_S`, 10 s, from the end of the
//! first frame), then stay so once the output goes quiet; and `tiza api
//! status` has to give that size for the last frame drawn. No time budget:
//! the bug never redraws without input, so the frame at the final size does
//! not come at all, however long the wait, and a loaded machine (the whole
//! suite alongside, a slow CI runner) only makes a correct frame come later.
//! Before the fix, in the debug build `cargo test` makes, a resize in the
//! first 200 ms after the setup was lost at most delays (a late one may land
//! after the first poll and be an event), so every case but the host-query
//! and smoke ones fails, every time. With the
//! fix it cannot fail by timing: a resize before the poll is read by the
//! first draw (ratatui asks the size on every draw) and one after it is an
//! event. The host-query and `--smoke` cases pass on `main` too: the query
//! runs before the terminal setup, and `--smoke` has no event loop; they
//! guard what the fix must not change.

use std::path::Path;
use std::sync::Mutex;

use serde_json::{Value, json};
use tiza_qa::{bin, fixture, scratch};

/// One case at a time: each one already starts up to seven Tizas at once.
static ONE_CASE_AT_A_TIME: Mutex<()> = Mutex::new(());

const DELAYS: [u64; 7] = [0, 5, 10, 20, 50, 100, 200];
/// How long the frame at the final size is waited for after the first
/// frame: a condition's generous timeout, not a budget (see above).
const WAIT_S: f64 = 10.0;
const W: u16 = 60;
const H: u16 = 30;

/// The pty side: references, the races, the screen emulation.
const BODY: &str = r##"
import json, threading, unicodedata

binary, spec_file, tmp = sys.argv[1], sys.argv[2], sys.argv[3]
spec = json.load(open(spec_file))
ENV = dict(os.environ, TERM="xterm-256color", HOME=tmp, XDG_CONFIG_HOME=os.path.join(tmp, "config"))
ENV.pop("TIZA_SOCKET", None)
ENV.pop("TIZA_SESSION", None)
if not spec.get("osc"):
    ENV["TIZA_NO_OSC"] = "1"
# Bracketed paste on: the last thing Tiza writes before its first frame.
SETUP = b"\x1b[?2004h"
QUERY = b"\x1b]11;?"
# ratatui ends every frame by hiding (or placing) the cursor.
FRAME_END = (b"\x1b[?25l", b"\x1b[?25h")
WAIT = spec["wait_s"]
ANSWER = spec.get("answer", "").encode()
COUNT = [0]


def sock_path():
    COUNT[0] += 1
    return os.path.join(tmp, "s%d.sock" % COUNT[0])


def argv_for(sock):
    return [a.replace("{bin}", binary).replace("{sock}", sock) for a in spec["argv"]]


def sgr(style, params):
    """Applies one SGR sequence to `style` (a dict: fg, bg, attrs)."""
    p = [int(x) if x.isdigit() else 0 for x in params.replace(":", ";").split(";")] if params else [0]
    i = 0
    while i < len(p):
        n = p[i]
        if n == 0:
            style.clear()
        elif n in (38, 48, 58) and i + 1 < len(p):
            key = {38: "fg", 48: "bg", 58: "ul"}[n]
            if p[i + 1] == 5 and i + 2 < len(p):
                style[key] = ("i", p[i + 2]); i += 2
            elif p[i + 1] == 2 and i + 4 < len(p):
                style[key] = tuple(p[i + 2:i + 5]); i += 4
        elif n == 39:
            style.pop("fg", None)
        elif n == 49:
            style.pop("bg", None)
        elif 30 <= n <= 37 or 90 <= n <= 97:
            style["fg"] = ("i", n)
        elif 40 <= n <= 47 or 100 <= n <= 107:
            style["bg"] = ("i", n)
        elif n in (22, 23, 24, 25, 27, 28, 29):
            style.pop("a%d" % (n - 20), None)
            if n == 22:
                style.pop("a2", None)
        else:
            style["a%d" % n] = True
        i += 1


def screen(raw, rows, cols, keep_rows, keep_cols):
    """What a rows x cols terminal shows after `raw`, cropped to the final
    size: (character, style) per cell. Enough of a VT for what crossterm
    writes: CUP, ED, EL, cursor moves, SGR; OSC/DCS/APC skipped; writes past
    the right edge are clipped."""
    blank = (" ", ())
    g = [[blank] * cols for _ in range(rows)]
    r = c = 0
    style = {}
    s = raw.decode(errors="replace")
    i, n = 0, len(s)
    while i < n:
        ch = s[i]
        if ch == "\x1b" and i + 1 < n:
            k = s[i + 1]
            if k == "[":
                j = i + 2
                while j < n and not ("\x40" <= s[j] <= "\x7e"):
                    j += 1
                if j >= n:
                    break
                params, final = s[i + 2:j], s[j]
                i = j + 1
                if final == "m":
                    sgr(style, params)
                    continue
                nums = [int(x) if x.isdigit() else 0 for x in params.lstrip("?").split(";")] if params else []
                arg = lambda k, d=1: nums[k] if len(nums) > k and nums[k] else d
                if final in "Hf":
                    r, c = arg(0) - 1, arg(1) - 1
                elif final == "J":
                    m = arg(0, 0)
                    if m in (2, 3):
                        g = [[blank] * cols for _ in range(rows)]
                    elif m == 0 and r < rows:
                        g[r][c:] = [blank] * max(0, cols - c)
                        for rr in range(r + 1, rows):
                            g[rr] = [blank] * cols
                elif final == "K" and r < rows:
                    m = arg(0, 0)
                    lo, hi = (c, cols) if m == 0 else (0, c + 1) if m == 1 else (0, cols)
                    for cc in range(lo, min(hi, cols)):
                        g[r][cc] = blank
                elif final == "A":
                    r = max(0, r - arg(0))
                elif final == "B":
                    r += arg(0)
                elif final == "C":
                    c += arg(0)
                elif final == "D":
                    c = max(0, c - arg(0))
                elif final == "G":
                    c = arg(0) - 1
                continue
            if k in "]P_^X":
                j = i + 2
                while j < n and s[j] != "\x07" and not (s[j] == "\x1b" and j + 1 < n and s[j + 1] == "\\"):
                    j += 1
                i = j + (1 if j < n and s[j] == "\x07" else 2)
                continue
            i += 3 if k in "()" else 2
            continue
        i += 1
        if ch == "\r":
            c = 0
        elif ch == "\n":
            r += 1
        elif ch < " " or ch == "\x7f" or unicodedata.combining(ch):
            pass
        else:
            st = tuple(sorted(style.items()))
            if r < rows and c < cols:
                g[r][c] = (ch, st)
            if unicodedata.east_asian_width(ch) in "WF":
                if r < rows and c + 1 < cols:
                    g[r][c + 1] = ("", st)
                c += 1
            c += 1
    return tuple(tuple(row[:keep_cols]) for row in g[:keep_rows])


def text(scr):
    return "\n".join("".join(ch for ch, _ in row).rstrip() for row in scr)


def status(sock):
    """`tiza api status`: (rows, cols) of the last frame drawn, or None."""
    try:
        r = subprocess.run([binary, "api", "status", "--socket", sock],
                           capture_output=True, text=True, timeout=60, env=ENV)
        j = json.loads(r.stdout)
    except (ValueError, subprocess.TimeoutExpired):
        return None
    j = j.get("result", j)
    return (j.get("rows"), j.get("cols"))


def quiet(p, quiet_s=0.3, timeout=10):
    """Lets the reader thread take the frame already written: returns once
    the child wrote nothing for `quiet_s`."""
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        with p._cv:
            q = time.monotonic() - p._last
        if q >= quiet_s:
            return True
        p.settle(quiet_s - q)
    return False


def reference(size, seed):
    """A fresh start at `size`: the screen once its first frame is out (the
    socket answers only after the loop drew it)."""
    sock = sock_path()
    p = Pty(argv_for(sock), size[0], size[1], env=ENV, ctty=True, raw=bool(seed), seed=seed)
    p.wait_for(lambda o: SETUP in o, 30, "reference setup")
    end = time.monotonic() + 30
    st = None
    while st is None and time.monotonic() < end and p.p.poll() is None:
        st = status(sock)
        if st is None:
            p.settle(0.05)
    quiet(p)
    scr = screen(p.out, size[0], size[1], size[0], size[1])
    p.kill()
    p.finish(2)
    if st != tuple(size):
        print("REFERENCE_STATUS", size, st, flush=True)
    return scr


REFS = {}
for run in spec["runs"]:
    final = tuple(run["resizes"][-1])
    if final not in REFS:
        REFS[final] = reference(final, ANSWER)
if spec.get("answer"):
    # The answer has to show: without it the frame is another one.
    final = tuple(spec["runs"][0]["resizes"][-1])
    if reference(final, b"") == REFS[final]:
        print("ANSWER_INVISIBLE the reference without the host answer is the same frame", flush=True)


def drew(o):
    """A whole frame is out (after the terminal setup)."""
    i = o.find(SETUP)
    return i >= 0 and any(m in o[i:] for m in FRAME_END)


def race(run, p, t_spawn, results):
    name = "%s@%dms" % (run["name"], run["delay_ms"])
    start = tuple(run["start"])
    sizes = [tuple(s) for s in run["resizes"]]
    final = sizes[-1]
    t0 = t_spawn
    anchor = run["anchor"]
    if anchor in ("setup", "query"):
        mark = SETUP if anchor == "setup" else QUERY
        if not p.wait_for(lambda o: mark in o, 30, name + " " + anchor):
            results[name] = "WRONG no %s seen" % anchor
            return
        t0 = time.monotonic()
    left = t0 + run["delay_ms"] / 1000 - time.monotonic()
    if left > 0:
        time.sleep(left)  # the delay under test, not a synchronisation
    for rows, cols in sizes:
        p.resize(rows, cols)
    if anchor == "query" and ANSWER:
        p.send(ANSWER)
    ref = REFS[final]
    rows, cols = max(start[0], *(s[0] for s in sizes)), max(start[1], *(s[1] for s in sizes))
    same = lambda o: screen(o, rows, cols, final[0], final[1]) == ref
    # Conditions only: the first frame, then the frame at the final size
    # (the bug never draws it without input), then it stays once the
    # output is quiet.
    if not p.wait_for(drew, 30, name + " first frame"):
        results[name] = "WRONG no frame drawn"
        return
    t_base = time.monotonic()
    ok = p.wait_for(same, WAIT, name + " frame at the final size")
    took = time.monotonic() - t_base
    if ok:
        quiet(p)
        ok = same(p.out)
    st = status(run["sock"])
    if ok and st == final:
        results[name] = "OK %.2fs" % took
    else:
        got = screen(p.out, rows, cols, final[0], final[1])
        path = os.path.join(tmp, "wrong-%s.txt" % name.replace("@", "-"))
        with open(path, "w") as f:
            f.write("got:\n%s\n\nexpected:\n%s\n" % (text(got), text(ref)))
        results[name] = "WRONG status=%s frame_at_final_size=%s (%s)" % (st, ok, path)


results = {}
runs = spec["runs"]
BATCH = 7
for b in range(0, len(runs), BATCH):
    threads, ptys = [], []
    for run in runs[b:b + BATCH]:
        run["sock"] = sock_path()
        t_spawn = time.monotonic()
        p = Pty(argv_for(run["sock"]), run["start"][0], run["start"][1], env=ENV, ctty=True)
        ptys.append(p)
        t = threading.Thread(target=race, args=(run, p, t_spawn, results))
        t.start()
        threads.append(t)
    for t in threads:
        t.join(150)
    for p in ptys:
        p.kill()
        p.finish(2)
for run in runs:
    name = "%s@%dms" % (run["name"], run["delay_ms"])
    print("RESULT", name, results.get(name, "WRONG no result"), flush=True)
"##;

/// One race: a start size, the resizes sent back to back, the delay before
/// them and what the delay counts from (`setup`, `spawn`, `query`).
fn run(name: &str, start: (u16, u16), resizes: &[(u16, u16)], anchor: &str, delay: u64) -> Value {
    json!({
        "name": name,
        "start": [start.0, start.1],
        "resizes": resizes.iter().map(|(r, c)| json!([r, c])).collect::<Vec<_>>(),
        "anchor": anchor,
        "delay_ms": delay,
    })
}

/// The same race at every delay of the card.
fn at_every_delay(
    name: &str,
    start: (u16, u16),
    resizes: &[(u16, u16)],
    anchor: &str,
) -> Vec<Value> {
    DELAYS
        .iter()
        .map(|&d| run(name, start, resizes, anchor, d))
        .collect()
}

/// `tiza <args>` in presentation mode at step 7, as the card measured it.
fn present_args(doc: &Path) -> Vec<String> {
    let doc = doc.to_str().unwrap();
    [
        "{bin}",
        doc,
        "--present",
        "--step",
        "7",
        "--no-save",
        "--socket",
        "{sock}",
    ]
    .map(String::from)
    .to_vec()
}

fn normal_args(doc: &Path) -> Vec<String> {
    let doc = doc.to_str().unwrap();
    ["{bin}", doc, "--no-save", "--socket", "{sock}"]
        .map(String::from)
        .to_vec()
}

fn ownership() -> std::path::PathBuf {
    fixture("tui/resize_at_start/ownership.rs.tiza")
}

/// Runs the races of one case and asserts every one of them ended in the
/// frame of a fresh start at its final size.
fn race(case: &str, argv: Vec<String>, runs: Vec<Value>, extra: Value) {
    let _one = ONE_CASE_AT_A_TIME
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let dir = scratch(case);
    let mut spec = json!({ "argv": argv, "runs": runs, "wait_s": WAIT_S });
    if let (Some(s), Some(e)) = (spec.as_object_mut(), extra.as_object()) {
        s.extend(e.clone());
    }
    let spec_file = dir.join("spec.json");
    std::fs::write(&spec_file, spec.to_string()).unwrap();
    let out = tiza_qa::pty_script_with_deadline(
        &dir,
        "race.py",
        BODY,
        &[
            bin().to_str().unwrap(),
            spec_file.to_str().unwrap(),
            dir.to_str().unwrap(),
        ],
        240,
    );
    let n = spec["runs"].as_array().map_or(0, Vec::len);
    let results: Vec<&str> = out.lines().filter(|l| l.starts_with("RESULT ")).collect();
    // How long each frame took (shown with --nocapture).
    println!("{case}:\n{}", results.join("\n"));
    let wrong: Vec<&&str> = results.iter().filter(|l| !l.contains(" OK ")).collect();
    assert_eq!(results.len(), n, "every race reports a result:\n{out}");
    assert!(
        wrong.is_empty(),
        "{} of {n} races ended in a frame for another size:\n{}\n\nwhole output:\n{out}",
        wrong.len(),
        wrong
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(!out.contains("REFERENCE_STATUS"), "{out}");
    assert!(!out.contains("ANSWER_INVISIBLE"), "{out}");
}

#[test]
fn a_wider_start_ends_at_the_final_width() {
    race(
        "wider",
        present_args(&ownership()),
        at_every_delay("61to60", (H, W + 1), &[(H, W)], "setup"),
        json!({}),
    );
}

#[test]
fn two_resizes_in_a_row_the_last_one_wins() {
    // Both sizes are set back to back (well within 1 ms): two SIGWINCH,
    // and the frame that stays is the last size's.
    race(
        "twice",
        present_args(&ownership()),
        at_every_delay("62to59to60", (H, W + 2), &[(H, W - 1), (H, W)], "setup"),
        json!({}),
    );
}

#[test]
fn delays_counted_from_the_spawn_as_the_card_measured() {
    // Includes resizes before the socket server is up and while the
    // document loads.
    race(
        "spawn",
        present_args(&ownership()),
        at_every_delay("61to60", (H, W + 1), &[(H, W)], "spawn"),
        json!({}),
    );
}

#[test]
fn rows_in_presentation_mode() {
    let mut runs = at_every_delay("31to30", (H + 1, W), &[(H, W)], "setup");
    runs.extend(at_every_delay("29to30", (H - 1, W), &[(H, W)], "setup"));
    race("rows", present_args(&ownership()), runs, json!({}));
}

#[test]
fn piped_stdin_reads_the_resize_from_dev_tty() {
    let src = fixture("tui/resize_at_start/ownership.rs");
    let line = format!(
        "cat '{}' | exec {{bin}} --lang rs --no-save --socket {{sock}}",
        src.display()
    );
    race(
        "piped",
        vec!["sh".into(), "-c".into(), line],
        at_every_delay("61to60", (H, W + 1), &[(H, W)], "setup"),
        json!({}),
    );
}

#[test]
fn a_resize_while_the_host_query_reads_dev_tty() {
    // `--theme auto` asks the terminal for its colours before crossterm
    // starts (up to 100 ms). The resize comes while that read waits, and the
    // answer (a light terminal) right after it: the frame must be the one a
    // fresh start at 60 columns with that answer draws, colours included
    // (without the answer it would be dark: checked in the script).
    let doc = ownership();
    let argv = [
        "{bin}",
        doc.to_str().unwrap(),
        "--theme",
        "auto",
        "--present",
        "--step",
        "7",
        "--no-save",
        "--socket",
        "{sock}",
    ]
    .map(String::from)
    .to_vec();
    let runs = [0, 20, 50]
        .iter()
        .map(|&d| run("61to60", (H, W + 1), &[(H, W)], "query", d))
        .collect();
    race(
        "query",
        argv,
        runs,
        json!({
            "osc": true,
            "answer": "\u{1b}]11;rgb:ffff/ffff/ffff\u{1b}\\\u{1b}]10;rgb:0000/0000/0000\u{1b}\\",
        }),
    );
}

#[test]
fn a_resize_while_a_big_document_loads() {
    // 1,000 lines: past the 800 from which highlighting runs on several
    // threads (chunked highlighting), and still quick to open in a debug build.
    let dir = scratch("big-src");
    let mut code = String::from("fn main() {\n");
    for i in 0..1_000 {
        code.push_str(&format!("    let v{i} = String::from(\"line {i}\");\n"));
    }
    code.push_str("}\n");
    let src = dir.join("big.rs");
    std::fs::write(&src, code).unwrap();
    // While its first frame is laid out (a resize while it loads, before
    // the socket server is up, is the `spawn` case of AC1). Its lines are
    // narrower than both sizes, so the frames for 61 and 60 columns look the
    // same: here `tiza api status` (the size of the last frame) is the guard.
    race(
        "big",
        normal_args(&src),
        at_every_delay("61to60", (H, W + 1), &[(H, W)], "setup"),
        json!({}),
    );
}

#[test]
fn smoke_still_draws_one_frame() {
    // `--smoke` does not go through the event loop: one frame (the code
    // once on the screen), one poll, exit 0.
    let dir = scratch("smoke");
    let out = tiza_qa::pty_script(
        &dir,
        "smoke.py",
        r#"
binary, doc = sys.argv[1], sys.argv[2]
env = dict(os.environ, TIZA_NO_OSC="1", TERM="xterm-256color")
s = Pty([binary, doc, "--smoke", "--no-socket", "--no-save", "--present", "--step", "7"], 30, 60, env=env, ctty=True)
code = s.finish(30)
print("EXIT", code)
print("TAKES", plain(s.out).count("fn takes("))
"#,
        &[bin().to_str().unwrap(), ownership().to_str().unwrap()],
    );
    assert!(out.contains("EXIT 0"), "{out}");
    assert!(out.contains("TAKES 1\n"), "one frame: {out}");
}
