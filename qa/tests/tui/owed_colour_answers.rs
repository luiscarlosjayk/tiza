//! The colour answers that the terminal owes: each colour query asks for
//! two answers, and an answer can come cut, with its `ESC` alone at the end
//! of a read. Tiza holds such an `ESC` while an answer is owed, also when
//! two queries are in flight (two resizes in a row), when the query at
//! start ran out of time before its answers came, and when an answer
//! started in its second and ends after it. The `ESC` of the `ESC \` that
//! ends an answer can come alone too. An Escape typed during the start-up
//! read is the user's, also when the read cut an answer short, and an
//! Escape that the capture gives back shows at once.
//!
//! Each test drives `tiza --theme auto` on a pty that is its controlling
//! terminal, and writes the answers of the terminal as the test says: the
//! pauses between them are the delays under test, not a synchronisation.
//! The witness: two `n` keys at the end reach the step that the test says
//! (an answer that went to the keys opened the command line, and the keys
//! went there), no rest of an answer is on the screen, and the bar says the
//! theme of the last answer.

use tiza_qa::{bin, fixture, scratch};

const LIGHT_BG: &str = "]11;rgb:f7f7/f5f5/efef\x1b\\";
const LIGHT_FG: &str = "]10;rgb:1f1f/2323/2828\x1b\\";
const DARK_BG: &str = "]11;rgb:1515/1717/1b1b\x1b\\";
const DARK_FG: &str = "]10;rgb:d0d0/d0d0/d0d0\x1b\\";

/// One step of a scenario.
enum Step {
    /// Wait until the bar shows the theme (`·auto`): the TUI listens.
    Bar,
    /// Wait until Tiza wrote this many colour queries in all.
    Queries(usize),
    /// Resize the terminal (rows, columns).
    Resize(u16, u16),
    /// Write these bytes as the terminal's, then pause this many ms.
    Send(String, u64),
    /// Type `:abc` and wait until the command line shows it.
    Line,
    /// Wait until the screen shows the command line closed.
    LineClosed,
}

/// Runs `tiza --theme auto --step 0` on the ownership fixture at 30 × 120,
/// with `seed` in its input before it starts, then `steps`, then two `n`,
/// and waits for step `want`. Returns the output of the script: `STEP n`,
/// `BAR …` (the last row of the screen) and `LEAK …` (a rest of an answer
/// drawn on the screen).
fn scenario(name: &str, seed: &str, steps: &[Step], want: u32) -> String {
    let dir = scratch(name);
    for f in ["ownership.rs.tiza", "ownership.rs"] {
        std::fs::copy(fixture(&format!("shared/ownership/{f}")), dir.join(f)).unwrap();
    }
    let steps: Vec<serde_json::Value> = steps
        .iter()
        .map(|s| match s {
            Step::Bar => serde_json::json!(["bar"]),
            Step::Queries(n) => serde_json::json!(["queries", n]),
            Step::Resize(r, c) => serde_json::json!(["resize", r, c]),
            Step::Send(b, ms) => serde_json::json!(["send", hex(b.as_bytes()), ms]),
            Step::Line => serde_json::json!(["line"]),
            Step::LineClosed => serde_json::json!(["closed"]),
        })
        .collect();
    let body = r##"
import json
binary, cwd, seed, steps, want = sys.argv[1:6]
want = int(want)
seed = bytes.fromhex(seed)
steps = json.loads(steps)
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color")
env.pop("TIZA_NO_OSC", None)
argv = [binary, "--theme", "auto", "--step", "0", "--no-save", "--socket", "s.sock", "ownership.rs.tiza"]
rows, cols = 30, 120
s = Pty(argv, rows, cols, env=env, ctty=True, raw=True, seed=seed)
Q = b"\x1b]11;?"
ok = True
for step in steps:
    if not ok:
        break
    if step[0] == "bar":
        ok = s.wait_for(lambda out: "·auto" in plain(out) and os.path.exists("s.sock"), 10, "the bar")
    elif step[0] == "queries":
        ok = s.wait_for(lambda out, n=step[1]: out.count(Q) >= n, 10, f"{step[1]} colour queries")
    elif step[0] == "resize":
        rows, cols = step[1], step[2]
        s.resize(rows, cols)
    elif step[0] == "send":
        s.send(bytes.fromhex(step[1]))
        time.sleep(step[2] / 1000)  # the delay under test, not a synchronisation
    elif step[0] == "line":
        s.send(b":abc")
        ok = s.wait_for(lambda out: ":abc" in terminal_rows(out, rows, cols)[-1], 10, "the command line")
    elif step[0] == "closed":
        ok = s.wait_for(lambda out: ":abc" not in terminal_rows(out, rows, cols)[-1], 10, "the command line closed")
s.send(b"nn")
def status():
    r = subprocess.run([binary, "--socket", "s.sock", "api", "status"], capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout) if r.returncode == 0 else {}
step = None
end = time.monotonic() + 10
while ok and time.monotonic() < end:
    step = status().get("step")
    if step == want:
        break
    s.settle(0.05)
print("STEP", step, flush=True)
screen = plain(s.out)
print("LEAK", any(w in screen for w in ("f7f7", "1f1f", "1515", "d0d0", "rgb")), flush=True)
print("BAR", terminal_rows(s.out, rows, cols)[-1], flush=True)
print("QUERIES", s.out.count(Q), flush=True)
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
"##;
    tiza_qa::pty_script(
        &dir,
        &format!("{name}.py"),
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &hex(seed.as_bytes()),
            &serde_json::Value::Array(steps).to_string(),
            &want.to_string(),
        ],
    )
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The answer `body` (without its first `ESC`) and the `ESC` of the next
/// one: a read that ends right after that `ESC`.
fn then_esc(body: &str) -> String {
    format!("{body}\x1b")
}

/// Checks the witness: step 2, nothing of an answer on the screen, and the
/// bar with `theme`.
fn assert_clean(out: &str, theme: &str) {
    assert!(
        out.contains("STEP 2\n"),
        "the keys went to the command line:\n{out}"
    );
    assert!(
        out.contains("LEAK False"),
        "a rest of an answer is on the screen:\n{out}"
    );
    let bar = out
        .lines()
        .find_map(|l| l.strip_prefix("BAR "))
        .unwrap_or("");
    assert!(
        bar.contains(theme),
        "the bar is not {theme}: {bar:?}\n{out}"
    );
    assert!(out.contains("EXIT 0"), "{out}");
}

/// Two resizes in a row; each answer cut right after its `ESC`, 300 ms
/// apart; the text answer of the first query comes after the second
/// query. The first query gets `first`, the second `second`.
fn two_resizes(name: &str, first: [&str; 2], second: [&str; 2]) -> String {
    let start = format!("\x1b{LIGHT_BG}\x1b{LIGHT_FG}");
    scenario(
        name,
        &start,
        &[
            Step::Bar,
            Step::Resize(30, 110),
            Step::Queries(2),
            Step::Send("\x1b".into(), 300),
            Step::Send(then_esc(first[0]), 300),
            Step::Resize(30, 120),
            Step::Queries(3),
            Step::Send(then_esc(first[1]), 300),
            Step::Send(then_esc(second[0]), 300),
            Step::Send(second[1].into(), 300),
        ],
        2,
    )
}

#[test]
fn two_resizes_in_a_row_keep_cut_answers_out_of_the_keys() {
    let out = two_resizes("two-resizes", [DARK_BG, DARK_FG], [LIGHT_BG, LIGHT_FG]);
    assert_clean(&out, "light·auto");
    // The other way round: the theme is the one of the last answer, not of
    // the first.
    let out = two_resizes("two-resizes-back", [LIGHT_BG, LIGHT_FG], [DARK_BG, DARK_FG]);
    assert_clean(&out, "dark·auto");
}

#[test]
fn an_answer_that_starts_in_the_second_and_ends_after_it_still_counts() {
    // No answer in the start-up read: both are owed. The background answer
    // starts 0.6 s after the bar (in the second after the first frame) and
    // ends at 1.3 s (after it); the `ESC` of the text answer comes with its
    // end.
    let out = scenario(
        "late-in-the-second",
        "",
        &[
            Step::Bar,
            Step::Send(String::new(), 600),
            Step::Send("\x1b".into(), 700),
            Step::Send(then_esc(LIGHT_BG), 300),
            Step::Send(LIGHT_FG.into(), 300),
        ],
        2,
    );
    assert_clean(&out, "light·auto");
}

#[test]
fn a_resize_after_the_start_up_wait_ran_out_does_not_let_late_answers_in() {
    // No answer in the start-up read, and none in the second after the
    // first frame. A resize asks again. Then the answers of the start-up
    // query come, and those of the resize, each cut right after its `ESC`.
    let out = scenario(
        "resize-after-the-wait",
        "",
        &[
            Step::Bar,
            Step::Send(String::new(), 1500),
            Step::Resize(30, 110),
            Step::Queries(2),
            Step::Send("\x1b".into(), 300),
            Step::Send(then_esc(DARK_BG), 300),
            Step::Send(then_esc(DARK_FG), 300),
            Step::Send(then_esc(LIGHT_BG), 300),
            Step::Send(LIGHT_FG.into(), 300),
        ],
        2,
    );
    assert_clean(&out, "light·auto");
}

#[test]
fn the_esc_of_the_end_of_an_answer_alone_is_not_a_key() {
    // Dark at start. After a resize, the light answers, each cut right
    // before the `\` of its end: the `ESC` of `ESC \` ends a read alone.
    let start = format!("\x1b{DARK_BG}\x1b{DARK_FG}");
    let cut = |a: &str| format!("\x1b{}", &a[..a.len() - 1]);
    let out = scenario(
        "end-esc-alone",
        &start,
        &[
            Step::Bar,
            Step::Resize(30, 110),
            Step::Queries(2),
            Step::Send(cut(LIGHT_BG), 300),
            Step::Send(format!("\\{}", cut(LIGHT_FG)), 300),
            Step::Send("\\".into(), 300),
        ],
        2,
    );
    assert_clean(&out, "light·auto");
}

#[test]
fn an_escape_typed_during_the_start_up_read_is_kept_when_an_answer_is_cut() {
    // Typed before Tiza reads: `n`, `:` and Escape. Then the background
    // answer, whole, and the text answer, cut short at the end of the read.
    // The Escape closes the command line: the `n` keys move the steps.
    let seed = format!("n:\x1b\x1b{LIGHT_BG}\x1b]10;rgb:1f1f/");
    let out = scenario(
        "early-escape",
        &seed,
        &[Step::Bar, Step::Send("2323/2828\x1b\\".into(), 300)],
        3,
    );
    // The early `n` and the two at the end make step 3. When the Escape is
    // lost, the command line stays open, the two `n` go there, and the
    // step stays 1.
    assert!(
        out.contains("STEP 3\n"),
        "the Escape typed early was lost: the command line stayed open\n{out}"
    );
    assert!(
        out.contains("LEAK False") && out.contains("EXIT 0"),
        "{out}"
    );
}

#[test]
fn an_escape_given_back_while_an_answer_starts_shows_at_once() {
    // The command line is open. After a resize, the user types Escape: held,
    // an answer is owed. Then the `ESC` of the answer, alone: the Escape
    // goes to the editor and closes the command line, while the capture
    // holds the new `ESC`. The body of the answer keeps the capture open.
    // The screen must show the command line closed now: when the capture
    // runs out, it gives nothing back, and no frame comes.
    let start = format!("\x1b{DARK_BG}\x1b{DARK_FG}");
    let out = scenario(
        "escape-given-back",
        &start,
        &[
            Step::Bar,
            Step::Line,
            Step::Resize(30, 110),
            Step::Queries(2),
            Step::Send("\x1b".into(), 300),
            Step::Send("\x1b".into(), 300),
            Step::Send("]11;rgb:f7f7".into(), 0),
            Step::LineClosed,
            Step::Send("/f5f5/efef\x1b\\".into(), 300),
            Step::Send(format!("\x1b{LIGHT_FG}"), 300),
        ],
        2,
    );
    assert_clean(&out, "light·auto");
}
