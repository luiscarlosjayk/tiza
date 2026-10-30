//! Some terminals draw ⚠, ↪ and ⏸ two cells wide, though Unicode gives
//! them one cell (each has an emoji form). The status bar counts one cell
//! for each, so on such a terminal it ran past the edge: with a message the
//! screen scrolled one row, and without autowrap the end of the bar was
//! lost.
//!
//! Acceptance criteria:
//! - AC1 At start-up Tiza measures how wide the terminal draws them (a
//!   cursor position request after each one). Where one is wide, Tiza's
//!   own text outside the code shows its plain form (`!`, `>`, `|`): the
//!   bar is whole, with no scroll. Where they are narrow, or where the
//!   terminal does not answer, the bar is as before.
//! - AC2 An answer of the probe that comes late, whole or cut by the
//!   wait, is never taken as keys.
//! - AC3 The code and the annotations keep the glyphs (`render.text`).
//!
//! The terminal is an emulator in the pty (`tiza_qa::WIDETERM`). Its
//! answers to the probe are written before `tiza` starts (the cursor is at
//! row 1), as a terminal that answers at once: no race with the wait. The
//! other texts outside the code (a message, supervised mode, a panel, the
//! message line of presentation mode) are checked in process, with the
//! signs set as measured wide.

use serde_json::{Value, json};
use tiza_qa::{bin, scratch};

const MAIN: &str =
    "fn main() {\n    let s = String::from(\"hi\");\n    let r = &s;\n    println!(\"{r}\");\n}\n";
const OTHER: &str = "fn other() {\n    let n = 1;\n}\n";
const TITLE: &str = "\u{26A0} the borrow \u{26A0}";
/// A light terminal's answers to the colour query (OSC 11, then OSC 10).
const LIGHT: &[u8] = b"\x1b]11;rgb:f7f7/f5f5/efef\x1b\\\x1b]10;rgb:1f1f/2323/2828\x1b\\";

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `main.rs` with a step 1 titled [`TITLE`] and a note with ⚠ in it, or
/// (`link`) with a `code.link` to `other.rs` at step 1.
fn files(name: &str, link: bool) -> std::path::PathBuf {
    let dir = scratch(name);
    std::fs::write(dir.join("main.rs"), MAIN).unwrap();
    std::fs::write(dir.join("other.rs"), OTHER).unwrap();
    let mut doc = json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "step": 0,
        "annotations": [
            {"id": "n1", "step": 1, "type": "note", "line": 3, "text": "\u{26A0} a shared borrow"}
        ],
        "steps": [{"step": 1, "title": TITLE}]
    });
    if link {
        doc["moves"] = json!([{"at_step": 1, "action": "code.link", "file": "other.rs"}]);
        doc["annotations"] = json!([]);
    }
    std::fs::write(dir.join("doc.tiza"), doc.to_string()).unwrap();
    dir
}

const BODY: &str = r##"
import json
binary, cwd, cfg = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color")
env.pop("TIZA_NO_OSC", None)
if cfg.get("no_osc"):
    env["TIZA_NO_OSC"] = "1"
rows, cols = cfg["rows"], cfg["cols"]
term = Term(rows, cols, wide=cfg["wide"], autowrap=cfg["autowrap"], cpr=cfg["cpr"])
# The answers of a terminal that draws `wide` two cells wide, to the probe
# (the cursor at row 1, column 1; then each of ⚠ ↪ ⏸ ▶ from there), after
# the colour answers when the theme asks for them.
reports = bytes.fromhex(cfg["colors"]) + b"\x1b[1;1R" + b"".join(
    b"\x1b[1;%dR" % (1 + (2 if g in cfg["wide"] else 1)) for g in "⚠↪⏸▶"
) if cfg["cpr"] else b""
early, after = cfg["early"].encode(), cfg["after"].encode()
when = cfg["answers"]
seed, rest = {
    "now": (early + reports + after, b""),
    "cut": (early + reports[: len(reports) - cfg["rest"]], reports[len(reports) - cfg["rest"]:]),
    "late": (early, reports),
}[when]
argv = [binary, "--theme", cfg["theme"], "--no-save", "--socket", "s.sock", "--step", str(cfg["step"]), "doc.tiza"]
s = Pty(argv, rows, cols, env=env, ctty=True, raw=True, seed=seed)
seen = 0
def pump():
    # The output so far through the emulator. Its own answers are not
    # sent: the seed has them.
    global seen
    out = s.out
    term.feed(out[seen:])
    seen = len(out)
    return out
def bar_has(pred):
    return lambda out: (pump(), pred(term.lines()[-1]))[1]
if not cfg.get("no_osc"):
    s.wait_for(lambda out: (pump(), term.cpr_asked >= 5)[1], 10, "the probe")
if rest:
    # The rest after the end of the start-up read: the timed read set
    # VTIME before it wrote the probe, and puts the old mode (VTIME 0)
    # back when it stops.
    def read_over(out):
        try:
            return termios.tcgetattr(s._slave)[6][termios.VTIME] == 0
        except (termios.error, TypeError):
            return True
    s.wait_for(read_over, 10, "the end of the start-up read")
    s.send(rest)
up = s.wait_for(lambda out: (pump(), b"\x1b[?1049h" in out and os.path.exists("s.sock"))[1], 10, "the TUI")
print("UP", up, flush=True)
# Before the TUI takes the screen, the probe puts the cursor back and
# clears only what it wrote: the text before the cursor stays.
main = s.out.split(b"\x1b[?1049h")[0]
print("CLEARED", main.rfind(b"\x1b8\x1b[K") > main.rfind(b"\x1b[6n") >= 0, flush=True)
if up and cfg["early"]:
    # The command line that the early `:` opened: what reaches it.
    s.send(b"x")
    s.wait_for(bar_has(lambda b: b.rstrip().endswith("x")), 10, "the x")
if up and cfg["after"]:
    # Keys typed after the answers, the last one an ESC: they stay keys.
    s.wait_for(bar_has(lambda b: "NORMAL" in b), 10, "normal mode")
if up and cfg["message"]:
    s.send((":" + cfg["message"] + "\r").encode())
    s.wait_for(bar_has(lambda b: cfg["shown"] in b and "numbers" in b), 10, "the message")
elif up and not cfg["early"] and not cfg["after"]:
    s.wait_for(bar_has(lambda b: cfg["shown"] in b), 10, "the bar")
pump()
print("ASKED", term.cpr_asked, flush=True)
print("BAR", json.dumps(term.lines()[-1]), flush=True)
print("SCROLLS", term.scrolls, flush=True)
print("SCREEN", json.dumps(term.lines()), flush=True)
r = subprocess.run([binary, "--socket", "s.sock", "api", "render.text"], capture_output=True, text=True, timeout=10)
print("TAKE", json.dumps((json.loads(r.stdout or "{}") or {}).get("text", "")), flush=True)
# Out of the command line first: `ESC :` in one read is Alt+:.
if term.lines()[-1].startswith(":"):
    s.send(b"\x1b")
    s.wait_for(bar_has(lambda b: not b.startswith(":")), 10, "the command line closed")
s.send(b":q!\r")
print("EXIT", s.finish(10), flush=True)
"##;

struct Run {
    bar: String,
    scrolls: u64,
    take: String,
    out: String,
}

fn run(name: &str, link: bool, cfg: Value) -> Run {
    let dir = files(name, link);
    let script = format!("{}\n{BODY}", tiza_qa::WIDETERM);
    let out = tiza_qa::pty_script(
        &dir,
        "wide.py",
        &script,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &cfg.to_string(),
        ],
    );
    let field = |key: &str| -> Value {
        out.lines()
            .find_map(|l| l.strip_prefix(&format!("{key} ")))
            .and_then(|l| serde_json::from_str(l).ok())
            .unwrap_or_else(|| panic!("no {key}\n{out}"))
    };
    assert!(out.contains("EXIT 0"), "{out}");
    Run {
        bar: field("BAR").as_str().unwrap_or_default().to_string(),
        scrolls: field("SCROLLS").as_u64().unwrap_or(u64::MAX),
        take: field("TAKE").as_str().unwrap_or_default().to_string(),
        out: out.clone(),
    }
}

fn cfg(wide: &str, cols: u16, autowrap: bool, message: &str, shown: &str) -> Value {
    json!({
        "rows": 10, "cols": cols, "wide": wide, "autowrap": autowrap, "cpr": true,
        "answers": "now", "rest": 0, "early": "", "after": "", "step": 1,
        "theme": "dark", "colors": "",
        "message": message, "shown": shown,
    })
}

#[test]
fn where_the_warning_sign_is_wide_the_bar_shows_it_plain_and_whole() {
    for (autowrap, message) in [(true, ""), (true, "numbers off"), (false, "numbers off")] {
        let r = run(
            "wide-warn",
            false,
            cfg("\u{26A0}", 80, autowrap, message, "! the borrow !"),
        );
        let what = format!("autowrap {autowrap}, message {message:?}");
        // The plain form, and the bar ends in its row: nothing ran past the
        // edge (no scroll, and the message is there whole).
        assert!(r.bar.contains("! the borrow !"), "{what}\n{}", r.out);
        assert!(!r.bar.contains('\u{26A0}'), "{what}\n{}", r.out);
        assert_eq!(r.scrolls, 0, "{what}\n{}", r.out);
        if !message.is_empty() {
            assert!(r.bar.contains("numbers"), "{what}\n{}", r.out);
        }
        // The code and the annotations keep the glyph.
        assert!(
            r.take.contains("\u{26A0} a shared borrow"),
            "{what}\n{}",
            r.out
        );
        // The probe put the cursor back and cleared only what it wrote.
        assert!(r.out.contains("CLEARED True"), "{what}\n{}", r.out);
    }
    // The `▶` at the start of the bar too.
    let r = run(
        "wide-arrow",
        false,
        cfg(
            "\u{26A0}\u{25B6}",
            80,
            true,
            "numbers off",
            "! the borrow !",
        ),
    );
    assert!(r.bar.starts_with(" \u{256D}> "), "{}", r.out);
    assert_eq!(r.scrolls, 0, "{}", r.out);
}

#[test]
fn where_the_link_sign_is_wide_the_view_shows_it_plain() {
    let r = run(
        "wide-link",
        true,
        cfg("\u{21AA}", 125, true, "", "> other.rs"),
    );
    assert!(r.bar.contains("> other.rs"), "{}", r.out);
    assert!(!r.bar.contains('\u{21AA}'), "{}", r.out);
    assert_eq!(r.scrolls, 0, "{}", r.out);
}

#[test]
fn where_the_signs_are_narrow_or_not_measured_the_bar_keeps_them() {
    // A terminal that draws them one cell wide.
    let r = run("narrow", false, cfg("", 80, true, "", TITLE));
    assert!(r.bar.contains(TITLE), "{}", r.out);
    // A terminal that does not answer the probe: as before, and the TUI
    // comes up after the wait.
    let mut c = cfg("", 80, true, "", TITLE);
    c["cpr"] = json!(false);
    let r = run("no-answer", false, c);
    assert!(r.out.contains("UP True"), "{}", r.out);
    assert!(r.bar.contains(TITLE), "{}", r.out);
    // `TIZA_NO_OSC` (tests, pipelines): Tiza asks nothing, the probe
    // neither.
    let mut c = cfg("", 80, true, "", TITLE);
    c["no_osc"] = json!(true);
    let r = run("no-osc", false, c);
    assert!(r.out.contains("ASKED 0"), "{}", r.out);
    assert!(r.bar.contains(TITLE), "{}", r.out);
}

#[test]
fn an_answer_of_the_probe_that_comes_late_is_not_keys() {
    // A `:` typed at start-up opens the command line before the answer
    // ends: a part of the answer taken as keys would be typed there
    // (`:3Rx` in place of `:x`). The last report (`ESC [ 1 ; 3 R`) comes
    // whole and late, or is cut at each place: after the ESC, after the
    // `[`, after the row, after the `;`, before the `R`. Or every report
    // comes late.
    let mut cases: Vec<(String, Value)> = [6usize, 5, 4, 3, 2, 1]
        .into_iter()
        .map(|rest| {
            let mut c = cfg("\u{26A0}", 80, true, "", "");
            c["answers"] = json!("cut");
            c["rest"] = json!(rest);
            c["early"] = json!(":");
            (format!("cut, {rest} bytes later"), c)
        })
        .collect();
    let mut late = cfg("\u{26A0}", 80, true, "", "");
    late["answers"] = json!("late");
    late["early"] = json!(":");
    cases.push(("every report late".into(), late));
    // With the colour query too: the colours come whole, and the first
    // report (the cursor before the glyphs) is cut right after its ESC.
    let mut colors = cfg("\u{26A0}", 80, true, "", "");
    colors["theme"] = json!("auto");
    colors["colors"] = json!(hex(LIGHT));
    colors["answers"] = json!("cut");
    colors["rest"] = json!(5 * 6 - 1);
    colors["early"] = json!(":");
    cases.push((
        "the colours, then the first report cut after its ESC".into(),
        colors,
    ));
    for (what, c) in cases {
        let r = run("late-answer", false, c);
        assert_eq!(r.bar.trim_end(), ":x", "{what}\n{}", r.out);
    }
}

/// Every sign measured wide, as `tiza_tui::appearance::query_host` gives
/// it from a terminal that drew each one two cells wide (the cursor at
/// column 1 first).
fn all_wide() -> tiza_tui::appearance::Wide {
    tiza_tui::appearance::Wide::from_reports(&[(1, 1), (1, 3), (1, 3), (1, 3), (1, 3)])
}

/// The screen of the live instance with `set` done first, at 100×12.
fn screen_with(
    live: &tiza_qa::Live,
    set: impl FnOnce(&mut tiza_tui::app::App) + Send + 'static,
) -> String {
    live.with(set);
    live.screen(100, 12)
}

fn opened(name: &str) -> tiza_qa::Live {
    let dir = files(name, false);
    let tiza = dir.join("doc.tiza");
    let doc = tiza_core::fs::load_document(&tiza).unwrap();
    tiza_qa::Live::open(doc, tiza, dir.join("live.sock"))
}

#[test]
fn the_other_text_outside_the_code_shows_the_plain_form_too() {
    // The bar's message, with the user's text in it.
    let live = opened("plain-message");
    let screen = screen_with(&live, |app| {
        app.wide = all_wide();
        app.message = Some(("\u{26A0} a note of mine \u{23F8}".into(), false));
    });
    let bar = screen.lines().last().unwrap_or_default().to_string();
    assert!(bar.contains("! a note of mine ||"), "{screen}");
    // Supervised mode, on the bar: the plain form of `⏸` is two cells.
    let screen = screen_with(&live, |app| {
        app.message = None;
        app.supervised = true;
    });
    let bar = screen.lines().last().unwrap_or_default().to_string();
    assert!(bar.contains("  || supervised"), "{screen}");
    assert!(!bar.contains('\u{23F8}'), "{screen}");
    // A panel: the annotation list, with the note that has ⚠ in it. The
    // code under it keeps the sign.
    live.with(|app| app.supervised = false);
    live.api_ok("step.set", json!({"step": 1}));
    let code = live.screen(100, 12);
    assert!(code.contains("\u{26A0} a shared borrow"), "{code}");
    live.keys(&[crossterm::event::KeyCode::Char('L')]);
    let panel = live.screen(100, 12);
    assert!(panel.contains("! a shared borrow"), "{panel}");
    // The hint of an annotation selected with Tab names its text, at a
    // width where the hint fits whole.
    live.keys(&[
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyCode::Tab,
    ]);
    let screen = live.screen(160, 12);
    let bar = screen.lines().last().unwrap_or_default().to_string();
    assert!(bar.contains("! a shared borrow"), "{screen}");
    assert!(!bar.contains('\u{26A0}'), "{screen}");
    // Narrower, the hint is fitted to its room (its keys kept), and its
    // label still fits: still plain.
    let screen = live.screen(135, 12);
    let bar = screen.lines().last().unwrap_or_default().to_string();
    assert!(bar.contains("\u{AB}! a shared borrow\u{BB}"), "{screen}");
    assert!(!bar.contains('\u{26A0}'), "{screen}");
    // A message on the command line while it is open (a completion, a
    // notice).
    live.keys(&[
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyCode::Char(':'),
    ]);
    let screen = screen_with(&live, |app| {
        app.message = Some(("\u{21AA} a notice".into(), false));
    });
    let bar = screen.lines().last().unwrap_or_default().to_string();
    assert!(
        bar.starts_with(':') && bar.contains("> a notice"),
        "{screen}"
    );
    // The message line of presentation mode.
    live.keys(&[crossterm::event::KeyCode::Esc]);
    let screen = screen_with(&live, |app| {
        app.present = true;
        app.present_message = Some(("\u{21AA} other.rs \u{26A0}".into(), false));
        app.present_message_until =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
    });
    assert!(screen.contains("> other.rs !"), "{screen}");
    // With nothing measured wide, the same texts keep the signs.
    let live = opened("plain-none");
    let screen = screen_with(&live, |app| {
        app.message = Some(("\u{26A0} a note of mine".into(), false));
    });
    assert!(screen.contains("\u{26A0} a note of mine"), "{screen}");
}

#[test]
fn keys_typed_after_the_answers_stay_keys() {
    // Every report came, then the user typed `:abc` and ESC: the ESC closes
    // the command line. Taken for the start of an answer, it left the
    // command line open.
    let mut c = cfg("", 80, true, "", "");
    c["after"] = json!(":abc\x1b");
    let r = run("after-answers", false, c);
    assert!(r.bar.contains("NORMAL"), "{}", r.out);
}
