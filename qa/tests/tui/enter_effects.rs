//! Enter effects (experimental) on the real TUI: a key ends the effect of
//! the step at once and does nothing else, going back shows the final
//! drawing with no effect, the auto-advance waits for the effect, and a
//! step without an effect starts none.
//!
//! The binary of the tests slows the clock of an effect 100 times
//! (`TIZA_TEST_HOOK=effect:slow`), so the effect still plays when the
//! next key arrives.
//!
//! In the process, the frames of an effect paint the rows in view on one
//! work layout: they lay nothing out, they show the effect at its moment
//! wherever the view scrolls, they keep to a grid of frames, and with no
//! test holding its moment the effect goes by its clock. The first frame
//! shows at once, and after a late frame the loop rests one period.

use std::time::{Duration, Instant};

use crossterm::event::KeyCode;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use serde_json::Value;
use tiza_core::canvas::Canvas;
use tiza_core::effects::Plan;
use tiza_core::layout::{Layout, LayoutOptions, layout_traced};
use tiza_qa::{Live, fixture, scratch};
use tiza_tui::app::App;

/// Runs the TUI on a copy of `fixture` (a path in `qa/fixtures/`) with
/// `--no-save --present --theme dark` and `args`, then the steps of
/// `script`: `key` sends keys, `api` calls a socket method, `until` waits
/// for a status, and `until_code` waits until the code holds a text. Each
/// wait prints `STATUS <json>`.
fn run(name: &str, fixture_path: &str, args: &[&str], script: &str) -> Vec<Value> {
    let dir = scratch(name);
    let source = fixture(fixture_path);
    let file = source.file_name().unwrap().to_str().unwrap().to_string();
    std::fs::copy(&source, dir.join(&file)).unwrap();
    let args = serde_json::to_string(args).unwrap();
    let body = r##"
import json
binary, cwd, script = sys.argv[1], sys.argv[2], json.loads(sys.argv[3])
file, args = sys.argv[4], json.loads(sys.argv[5])
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="effect:slow")
s = Pty([binary, "--no-save", "--present", "--theme", "dark"] + args
        + ["--socket", "s.sock", file], 30, 100, env=env)

def call(method, params=None):
    cmd = [binary, "--socket", "s.sock", "api", method]
    if params is not None:
        cmd.append(json.dumps(params))
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout) if r.returncode == 0 else {}

def status():
    return call("status")

def until(pred, what, timeout=10):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        st = status()
        if pred(st):
            return st
        s.settle(0.05)
    print("WAIT_TIMEOUT", what, flush=True)
    return status()

ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame")
for step in script if ok else []:
    kind, arg = step[0], step[1]
    if kind == "key":
        s.send(arg.encode())
    elif kind == "until":
        want = arg
        st = until(lambda st: all(st.get(k, False) == v for k, v in want.items()), json.dumps(want))
        print("STATUS", json.dumps({k: st.get(k, False) for k in ("step", "effect")}), flush=True)
    elif kind == "api":
        call(arg[0], arg[1])
    elif kind == "until_code":
        end = time.monotonic() + 10
        while arg not in call("code.get").get("text", "") and time.monotonic() < end:
            s.settle(0.05)
        if arg not in call("code.get").get("text", ""):
            print("WAIT_TIMEOUT code", arg, flush=True)
        st = status()
        print("STATUS", json.dumps({k: st.get(k, False) for k in ("step", "effect")}), flush=True)
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        &format!("{name}.py"),
        body,
        &[
            tiza_qa::bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            script,
            &file,
            &args,
        ],
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    out.lines()
        .filter_map(|l| l.strip_prefix("STATUS "))
        .map(|j| serde_json::from_str(j).unwrap())
        .collect()
}

#[test]
fn a_key_ends_the_effect_and_does_nothing_else() {
    // `n` starts the effect of step 2; `n` again only ends it; the next `n`
    // goes to step 3.
    let seen = run(
        "effect-key",
        "shared/enter_effects/drop.tiza",
        &["--step", "1"],
        r#"[["key", "n"], ["until", {"step": 2, "effect": true}],
            ["key", "n"], ["until", {"step": 2, "effect": false}],
            ["key", "n"], ["until", {"step": 3, "effect": true}],
            ["key", "x"], ["until", {"step": 3, "effect": false}]]"#,
    );
    let pairs: Vec<(u64, bool)> = seen
        .iter()
        .map(|v| (v["step"].as_u64().unwrap(), v["effect"].as_bool().unwrap()))
        .collect();
    assert_eq!(pairs, [(2, true), (2, false), (3, true), (3, false)]);
}

#[test]
fn going_back_shows_the_final_drawing_at_once() {
    let seen = run(
        "effect-back",
        "shared/enter_effects/drop.tiza",
        &["--step", "1"],
        r#"[["key", "n"], ["until", {"step": 2, "effect": true}],
            ["key", "x"], ["until", {"step": 2, "effect": false}],
            ["key", "n"], ["until", {"step": 3, "effect": true}],
            ["key", "x"], ["until", {"step": 3, "effect": false}],
            ["key", "p"], ["until", {"step": 2}]]"#,
    );
    assert_eq!(seen.last().unwrap()["effect"], false, "{seen:?}");
}

#[test]
fn auto_advance_waits_for_the_effect() {
    // Step 1 advances after 1 ms, but only once its effect is over: the
    // effect still plays at step 1, and `x` ends it.
    let seen = run(
        "effect-auto",
        "tui/enter_effects/auto.tiza",
        &["--step", "0"],
        r#"[["key", "n"], ["until", {"step": 1, "effect": true}],
            ["key", "x"], ["until", {"step": 2}]]"#,
    );
    let steps: Vec<u64> = seen.iter().map(|v| v["step"].as_u64().unwrap()).collect();
    assert_eq!(steps, [1, 2], "{seen:?}");
}

#[test]
fn a_step_without_an_effect_starts_no_effect() {
    // Only the callout of step 3 has an effect: step 2 starts none.
    let seen = run(
        "effect-none",
        "shared/enter_effects/drop-mixed.tiza",
        &["--step", "1"],
        r#"[["key", "n"], ["until", {"step": 2}],
            ["key", "n"], ["until", {"step": 3, "effect": true}]]"#,
    );
    let pairs: Vec<(u64, bool)> = seen
        .iter()
        .map(|v| (v["step"].as_u64().unwrap(), v["effect"].as_bool().unwrap()))
        .collect();
    assert_eq!(pairs, [(2, false), (3, true)]);
}

#[test]
fn the_step_the_tui_opens_at_plays_no_effect() {
    // drop.tiza is saved at step 1: `--step 2` opens it one step on, at a
    // step with an effect, and shows the final drawing.
    let seen = run(
        "effect-open",
        "shared/enter_effects/drop.tiza",
        &["--step", "2"],
        r#"[["until", {"step": 2}]]"#,
    );
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0]["effect"], false, "{seen:?}");
}

#[test]
fn a_step_change_to_the_step_on_screen_keeps_the_auto_advance() {
    // `step.set` to the step that plays its effect ends the effect; the
    // auto-advance of 1 ms then goes on to step 2.
    let seen = run(
        "effect-same-step",
        "tui/enter_effects/auto.tiza",
        &["--step", "0"],
        r#"[["key", "n"], ["until", {"step": 1, "effect": true}],
            ["api", ["step.set", {"step": 1}]], ["until", {"step": 2}]]"#,
    );
    let steps: Vec<u64> = seen.iter().map(|v| v["step"].as_u64().unwrap()).collect();
    assert_eq!(steps, [1, 2], "{seen:?}");
}

#[test]
fn a_goto_to_the_next_step_plays_its_effect() {
    let seen = run(
        "effect-goto",
        "shared/enter_effects/drop.tiza",
        &["--step", "1"],
        r#"[["api", ["step.goto", {"id": 2}]], ["until", {"step": 2}]]"#,
    );
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0]["effect"], true, "{seen:?}");
}

#[test]
fn a_typed_step_the_tui_opens_at_plays_no_effect_after_its_typing() {
    // typed.tiza is saved at step 1: `--step 2` types the move of step 2,
    // and its box (draw) and spotlight (fade) show at once after it.
    let seen = run(
        "effect-open-typed",
        "tui/enter_effects/typed.tiza",
        &["--step", "2"],
        r#"[["until_code", "// typed at 25 ms"]]"#,
    );
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0]["step"], 2, "{seen:?}");
    assert_eq!(seen[0]["effect"], false, "{seen:?}");
}

/// The screen of the in-process tests: 80 columns, 23 rows of code and
/// the status line.
const COLS: u16 = 80;
const ROWS: u16 = 24;

/// `tui/enter_effects/scroll.tiza` (80 lines) open in the process with no
/// padding, the cursor on line 60 and the effect of step 2 playing: a
/// spotlight with `fade` on lines 50-60, so every row in view changes,
/// and a box with `draw` on lines 55-58.
fn scrolled_effect(name: &str) -> Live {
    let dir = scratch(name);
    let doc = tiza_core::fs::load_document(&fixture("tui/enter_effects/scroll.tiza")).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("s.sock"));
    live.command("pad 0 0");
    live.with(|a| {
        a.autosave = false;
        a.cursor_line = 60;
        // A first frame: the effect is planned at the width of the screen.
        draw(a);
    });
    live.keys(&[KeyCode::Char('n')]);
    assert!(
        live.with(|a| a.doc.step == 2 && a.is_playing_effect()),
        "the effect of step 2 does not play"
    );
    live
}

fn draw(a: &mut App) -> Buffer {
    draw_at(a, COLS)
}

fn draw_at(a: &mut App, cols: u16) -> Buffer {
    let mut t = Terminal::new(TestBackend::new(cols, ROWS)).unwrap();
    t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
    t.backend().buffer().clone()
}

/// The final layout of step 2 at `cols` columns and the plan of its
/// effect, from core, with the options of the screen.
fn reference(live: &Live, cols: u16) -> (Layout, Plan) {
    let (doc, theme, numbers, fin_app) = live.with(move |a| {
        (
            a.doc.clone(),
            a.theme.clone(),
            a.numbers(),
            a.layout(usize::from(cols)),
        )
    });
    let opts = LayoutOptions {
        width: usize::from(cols),
        step: 2,
        line_numbers: numbers,
        tab_width: 4,
        margin_cap: doc.margin.map(usize::from),
        ..Default::default()
    };
    let (l, trace) = layout_traced(&doc, &theme, &opts, None);
    // Precondition: these are the options of the screen.
    assert_eq!(
        l.canvas.to_ansi(),
        fin_app.canvas.to_ansi(),
        "the reference is not the screen's layout"
    );
    let plan = Plan::new(&l.canvas, &trace, &doc).expect("step 2 has an effect");
    (l, plan)
}

/// Whether the code cells (from `code_x`) of the screen rows show the rows
/// of `want` from `scroll` on: their symbols and their colours. The cells
/// in motion that they show are added to `in_motion` (those that differ in
/// `fin`).
fn same_rows(
    buf: &Buffer,
    want: &Canvas,
    fin: &Canvas,
    scroll: usize,
    code_x: usize,
    in_motion: &mut usize,
) -> Result<(), String> {
    let rgb = |c: tiza_core::canvas::Rgb| Color::Rgb(c.0, c.1, c.2);
    for r in 0..usize::from(buf.area.height - 1) {
        let y = scroll + r;
        for x in code_x..usize::from(buf.area.width) {
            let (Some(cell), Some(end)) = (want.get(x, y), fin.get(x, y)) else {
                continue;
            };
            if cell.sym.is_empty() {
                continue;
            }
            let shown = &buf[(x as u16, r as u16)];
            let fg = cell.style.fg.map(rgb);
            let bg = cell.style.bg.map(rgb);
            if shown.symbol() != cell.sym.as_str()
                || fg.is_some_and(|c| shown.fg != c)
                || bg.is_some_and(|c| shown.bg != c)
            {
                return Err(format!(
                    "row {y} (screen row {r}), column {x}: {:?} {:?}/{:?} on screen, {:?} {fg:?}/{bg:?} wanted",
                    shown.symbol(),
                    shown.fg,
                    shown.bg,
                    cell.sym.as_str()
                ));
            }
            if cell != end {
                *in_motion += 1;
            }
        }
    }
    Ok(())
}

#[test]
fn the_frames_of_an_effect_lay_nothing_out() {
    // Each frame paints the rows in view on the work layout of the effect
    // and gives it back: no frame lays the document out again.
    let live = scrolled_effect("effect-frames");
    let (before, after) = live.with(|a| {
        let before = a.layouts.get();
        for (t, line) in [(30, 60), (90, 78), (150, 20), (400, 60)] {
            a.cursor_line = line;
            a.hold_effect_at(t);
            draw(a);
        }
        (before, a.layouts.get())
    });
    assert_eq!(
        after, before,
        "the frames of the effect laid the document out"
    );
}

#[test]
fn the_rows_in_view_show_the_effect_at_its_moment() {
    // The frames show the moment that the test holds, on the rows in view
    // wherever the view scrolls: rows come into view after the effect
    // started. The reference is the whole plan of core at that moment.
    let live = scrolled_effect("effect-rows");
    let (l, plan) = reference(&live, COLS);
    let mut scrolls = Vec::new();
    for (t, line) in [(40, 60), (120, 78), (200, 30), (300, 57)] {
        let (buf, scroll) = live.with(move |a| {
            a.cursor_line = line;
            a.hold_effect_at(t);
            let buf = draw(a);
            (buf, a.scroll)
        });
        let mut want = l.canvas.clone();
        plan.paint(&mut want, t);
        let mut in_motion = 0;
        if let Err(e) = same_rows(&buf, &want, &l.canvas, scroll, l.code_x, &mut in_motion) {
            panic!("at {t} ms, scrolled to row {scroll}: {e}");
        }
        assert!(in_motion > 0, "at {t} ms the rows in view show no effect");
        scrolls.push(scroll);
    }
    // The view scrolled down and up during the effect.
    assert!(scrolls.iter().all(|&s| s > 0), "{scrolls:?}");
    assert!(
        scrolls[1] > scrolls[0] && scrolls[2] < scrolls[1],
        "{scrolls:?}"
    );
}

#[test]
fn a_new_width_lays_the_effect_out_once_and_shows_it_at_that_width() {
    // The terminal gets wider while the effect plays: the next frame lays
    // the effect out again at the new width, once, and shows its moment.
    let live = scrolled_effect("effect-width");
    let wide = COLS + 20;
    let (l, plan) = reference(&live, wide);
    let (layouts, buf, scroll) = live.with(move |a| {
        let before = a.layouts.get();
        a.hold_effect_at(120);
        draw_at(a, wide);
        let buf = draw_at(a, wide);
        (a.layouts.get() - before, buf, a.scroll)
    });
    assert_eq!(
        layouts, 1,
        "two frames at a new width made {layouts} layouts"
    );
    let mut want = l.canvas.clone();
    plan.paint(&mut want, 120);
    let mut in_motion = 0;
    if let Err(e) = same_rows(&buf, &want, &l.canvas, scroll, l.code_x, &mut in_motion) {
        panic!("at {wide} columns: {e}");
    }
    assert!(in_motion > 0, "the rows in view show no effect");
}

#[test]
fn with_no_hold_an_effect_goes_by_its_clock() {
    // With no test holding its moment, a frame after the effect's time
    // shows the final drawing, and the tick ends the effect.
    let live = scrolled_effect("effect-clock");
    let (started, duration) = live.with(|a| {
        let e = a.effect.as_ref().unwrap();
        (e.started, Duration::from_millis(e.duration_ms))
    });
    let limit = Instant::now() + Duration::from_secs(10);
    while started.elapsed() < duration {
        assert!(
            Instant::now() < limit,
            "the effect's time did not pass in 10 s"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let (buf, scroll, fin) = live.with(|a| {
        let buf = draw(a);
        (buf, a.scroll, a.layout(usize::from(COLS)))
    });
    let mut in_motion = 0;
    if let Err(e) = same_rows(
        &buf,
        &fin.canvas,
        &fin.canvas,
        scroll,
        fin.code_x,
        &mut in_motion,
    ) {
        panic!("after the effect's time the screen is not the final drawing: {e}");
    }
    assert!(
        live.with(|a| a.is_playing_effect()),
        "the effect ended before its tick"
    );
    assert!(
        !live.with(|a| {
            a.effect_tick();
            a.is_playing_effect()
        }),
        "the tick did not end the effect after its time"
    );
}

#[test]
fn the_frames_of_an_effect_keep_to_a_grid() {
    let p = Duration::from_micros(tiza_core::effects::SCREEN_FRAME_US);
    let t0 = Instant::now();
    let next = tiza_tui::next_frame_due;
    // The first frame of an effect starts the grid.
    let due = next(None, t0, p);
    assert_eq!(due, t0 + p);
    // One more frame before the next is due (a socket request, the
    // mouse): the grid stays.
    assert_eq!(next(Some(due), t0 + p / 2, p), due);
    // A frame on time, a little after it is due: one period on, so the
    // little delays do not add up.
    assert_eq!(next(Some(due), due + Duration::from_millis(2), p), due + p);
    // A late frame: the grid starts again from it, with no burst.
    let late = due + p + Duration::from_millis(1);
    assert_eq!(next(Some(due), late, p), late + p);
}

#[test]
fn the_loop_waits_until_the_next_frame_is_due() {
    let p = Duration::from_micros(tiza_core::effects::SCREEN_FRAME_US);
    let now = Instant::now();
    let wait = tiza_tui::frame_wait;
    // No frame of the effect yet: the first one shows at once, also when
    // the effect starts with no key (a socket request, the auto-advance).
    assert_eq!(wait(None, now), Duration::ZERO);
    // Rounded up to whole milliseconds: never before the frame is due.
    assert_eq!(
        wait(Some(now + Duration::from_micros(10_200)), now),
        Duration::from_millis(11)
    );
    assert_eq!(
        wait(Some(now + Duration::from_millis(10)), now),
        Duration::from_millis(10)
    );
    // A frame due already: no wait.
    assert_eq!(
        wait(Some(now), now + Duration::from_millis(5)),
        Duration::ZERO
    );
    // A frame that took two periods ends late: the loop waits one whole
    // period before the next one, not zero.
    let due = now + p;
    let end = due + 2 * p;
    let next = tiza_tui::next_frame_due(Some(due), end, p);
    assert!(wait(Some(next), end) >= p, "{:?}", wait(Some(next), end));
}

#[test]
fn after_a_late_frame_the_loop_rests_one_period() {
    // With `effect:late-frames`, each frame of the effect takes 50 ms more,
    // longer than the 33 ms between two frames: every frame is late. The
    // loop rests one period after each, so a frame comes about 83 ms after
    // the one before. A loop that does not rest draws again at once, about
    // 50 ms after, at 100 % of a core.
    let dir = scratch("effect-late");
    let source = fixture("shared/enter_effects/drop.tiza");
    std::fs::copy(&source, dir.join("drop.tiza")).unwrap();
    let body = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="effect:late-frames")
s = Pty([binary, "--no-save", "--present", "--theme", "dark", "--socket", "s.sock", "drop.tiza"], 30, 100, env=env)
MARK = b"\x1b[?25l"
times = []
if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    base = s.out.count(MARK)
    s.send(b"n")

    # Each frame draws MARK once: note when each new one comes.
    def frames(out):
        n = out.count(MARK) - base
        while len(times) < n:
            times.append(time.monotonic())
        return n >= 12

    s.wait_for(frames, 10, "12 frames of the effect")
print("TIMES", json.dumps(times), flush=True)
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "late.py",
        body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    let times: Vec<f64> = out
        .lines()
        .find_map(|l| l.strip_prefix("TIMES "))
        .map(|j| serde_json::from_str(j).unwrap())
        .unwrap_or_else(|| panic!("{out}"));
    // From the second frame on: the first one comes after the key.
    let mut gaps: Vec<f64> = times
        .windows(2)
        .skip(1)
        .map(|w| (w[1] - w[0]) * 1000.0)
        .collect();
    gaps.sort_by(f64::total_cmp);
    let median = gaps[gaps.len() / 2];
    // Between 50 ms (no rest) and 83 ms (a rest of one period). A busy
    // machine only makes the gaps longer.
    assert!(median >= 70.0, "median gap {median:.1} ms: {gaps:?}");
}
