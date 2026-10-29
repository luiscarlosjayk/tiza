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
//!
//! In presentation mode, a key that only moves the view (a trackpad comes
//! as ↑ and ↓) lets the effect play, unless the live pointer is on.

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
fn a_change_of_the_pointer_ends_the_effect() {
    // An agent points while the effect plays: the effect ends, and the
    // step stays. So does the end of the pointer of the presenter.
    let seen = run(
        "effect-pointer-set",
        "shared/enter_effects/drop.tiza",
        &[],
        r#"[["key", "n"], ["until", {"step": 2, "effect": true}],
            ["api", ["pointer.set", {"line": 3}]], ["until", {"step": 2, "effect": false}]]"#,
    );
    assert_eq!(seen.last().unwrap()["effect"], false, "{seen:?}");
    assert_eq!(seen.last().unwrap()["step"], 2, "{seen:?}");
    let seen = run(
        "effect-pointer-clear",
        "shared/enter_effects/drop.tiza",
        &[],
        r#"[["key", "f"], ["key", "n"], ["until", {"step": 2, "effect": true}],
            ["api", ["pointer.clear", null]], ["until", {"step": 2, "effect": false}]]"#,
    );
    assert_eq!(seen.last().unwrap()["effect"], false, "{seen:?}");
    assert_eq!(seen.last().unwrap()["step"], 2, "{seen:?}");
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

/// [`scrolled_effect`] in presentation mode, with the live pointer on
/// when `pointer`.
fn presenting_effect(name: &str, pointer: bool) -> Live {
    let dir = scratch(name);
    let doc = tiza_core::fs::load_document(&fixture("tui/enter_effects/scroll.tiza")).unwrap();
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("s.sock"));
    live.with(|a| {
        a.autosave = false;
        a.set_present(true);
        a.cursor_line = 60;
        draw(a);
    });
    if pointer {
        live.keys(&[KeyCode::Char('f')]);
        assert!(live.with(|a| a.pointer.is_some()), "the pointer is off");
    }
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
    let p = tiza_core::effects::screen_frame_period(30);
    let t0 = Instant::now();
    let next = tiza_tui::next_frame_due;
    // The first frame of an effect starts the grid.
    let due = next(None, t0, p);
    assert_eq!(due, t0 + p);
    // One more frame before the next is due (the mouse wheel, a resize):
    // the grid stays.
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
    let p = tiza_core::effects::screen_frame_period(30);
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
fn a_new_effect_starts_with_no_frame_due() {
    let t0 = Instant::now();
    let due = t0 + Duration::from_millis(33);
    let of = tiza_tui::frame_due_of;
    // The effect that drew the last frame: its next frame.
    assert_eq!(of(Some((t0, due)), Some(t0)), Some(due));
    // Another effect started since (a step during an effect): its first
    // frame shows at once.
    let later = t0 + Duration::from_millis(5);
    assert_eq!(of(Some((t0, due)), Some(later)), None);
    // No effect plays, or no frame yet.
    assert_eq!(of(Some((t0, due)), None), None);
    assert_eq!(of(None, Some(t0)), None);
}

/// The gaps (ms, sorted) between the frames of the effect of step 2 of
/// `drop.tiza`, from the second frame on, with the test hook `hook` and
/// `effect_fps = 30` in the config file. With `effect:late-frames`, each
/// frame takes 50 ms more, longer than the 33 ms between two frames, so
/// every frame is late. With `mouse`, the TUI has mouse capture on and the
/// mouse moves about every 5 ms during the effect.
fn frame_gaps(name: &str, hook: &str, mouse: bool) -> Vec<f64> {
    let dir = scratch(name);
    let source = fixture("shared/enter_effects/drop.tiza");
    std::fs::copy(&source, dir.join("drop.tiza")).unwrap();
    std::fs::write(dir.join("cfg.toml"), "[defaults]\neffect_fps = 30\n").unwrap();
    let body = r##"
import json
binary, cwd, mouse, hook = sys.argv[1], sys.argv[2], sys.argv[3] == "mouse", sys.argv[4]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK=hook,
           TIZA_CONFIG=os.path.join(cwd, "cfg.toml"))
argv = [binary, "--no-save", "--present", "--theme", "dark", "--socket", "s.sock"]
s = Pty(argv + (["--mouse"] if mouse else []) + ["drop.tiza"], 30, 100, env=env)
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

    if mouse:
        end = time.monotonic() + 10
        x = 0
        while not frames(s.out) and time.monotonic() < end:
            # A motion of the mouse with no button (SGR encoding).
            x = x % 60 + 1
            s.send(b"\x1b[<35;%d;5M" % x)
            s.settle(0.005)
        if not frames(s.out):
            print("WAIT_TIMEOUT 12 frames of the effect", flush=True)
    else:
        s.wait_for(frames, 10, "12 frames of the effect")
print("TIMES", json.dumps(times), flush=True)
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "late.py",
        body,
        &[
            tiza_qa::bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            if mouse { "mouse" } else { "keys" },
            hook,
        ],
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
    gaps
}

#[test]
fn after_a_late_frame_the_loop_rests_one_period() {
    // The loop rests one period after each late frame, so a frame comes
    // about 83 ms after the one before. A loop that does not rest draws
    // again at once, about 50 ms after, at 100 % of a core. A busy machine
    // makes most gaps longer; the median is the check.
    let gaps = frame_gaps("effect-late", "effect:late-frames", false);
    let median = gaps[gaps.len() / 2];
    assert!(median >= 70.0, "median gap {median:.1} ms: {gaps:?}");
}

#[test]
fn a_moving_mouse_draws_no_frame_more_during_an_effect() {
    // With mouse capture on, the terminal sends each motion of the mouse.
    // A motion changes nothing on the screen: the frames still come one
    // period after a late frame, not each time the mouse moves.
    let gaps = frame_gaps("effect-mouse", "effect:late-frames", true);
    let median = gaps[gaps.len() / 2];
    assert!(median >= 70.0, "median gap {median:.1} ms: {gaps:?}");
}

#[test]
fn in_presentation_the_keys_that_move_the_view_let_the_effect_play() {
    // The keys that only move the view do not end the effect in
    // presentation mode: the view moves and the effect goes on. A
    // trackpad or a mouse wheel comes as ↑ and ↓.
    let live = presenting_effect("effect-view-keys", false);
    let first = live.with(|a| {
        draw(a);
        a.scroll
    });
    let mut scrolls = Vec::new();
    for key in [
        KeyCode::Char('G'),
        KeyCode::Up,
        KeyCode::Char('k'),
        KeyCode::Char('K'),
        KeyCode::Down,
        KeyCode::Char('j'),
        KeyCode::Char('J'),
        KeyCode::Home,
        KeyCode::End,
        KeyCode::Char('g'),
    ] {
        live.keys(&[key]);
        let (playing, scroll) = live.with(|a| {
            draw(a);
            (a.is_playing_effect(), a.scroll)
        });
        assert!(playing, "{key:?} ended the effect");
        scrolls.push(scroll);
    }
    // `G` went down to the end, `g` back up to the start.
    assert!(
        scrolls[0] > first,
        "G did not move the view: {first} then {scrolls:?}"
    );
    assert_eq!(scrolls.last(), Some(&0), "{scrolls:?}");
}

#[test]
fn in_presentation_with_the_pointer_on_a_key_that_moves_the_view_ends_the_effect() {
    // With the live pointer on, ↓ would move the pointer: a change of the
    // pointer ends the effect, and the key does nothing else.
    let live = presenting_effect("effect-view-pointer", true);
    let line = live.with(|a| a.cursor_line);
    live.keys(&[KeyCode::Down]);
    assert!(
        !live.with(|a| a.is_playing_effect()),
        "the effect still plays"
    );
    assert_eq!(
        live.with(|a| a.cursor_line),
        line,
        "the key moved the cursor"
    );
}

#[test]
fn in_presentation_another_key_still_ends_the_effect() {
    for key in [KeyCode::Char('x'), KeyCode::Char('l')] {
        let live = presenting_effect("effect-other-key", false);
        live.keys(&[key]);
        assert!(
            !live.with(|a| a.is_playing_effect()),
            "{key:?} did not end it"
        );
    }
}

#[test]
fn outside_presentation_a_key_that_moves_the_view_still_ends_the_effect() {
    let live = scrolled_effect("effect-author-key");
    let line = live.with(|a| a.cursor_line);
    live.keys(&[KeyCode::Down]);
    assert!(
        !live.with(|a| a.is_playing_effect()),
        "the effect still plays"
    );
    assert_eq!(
        live.with(|a| a.cursor_line),
        line,
        "the key moved the cursor"
    );
}

#[test]
fn effect_fps_in_the_config_sets_the_frames_a_second_of_an_effect() {
    // `effect_fps = 30` in `[defaults]`: the frames of an effect come
    // about 33 ms apart, not about 17 ms as at the 60 of the default. A
    // busy machine only makes the gaps longer.
    let gaps = frame_gaps("effect-fps", "effect:slow", false);
    let median = gaps[gaps.len() / 2];
    assert!(median >= 25.0, "median gap {median:.1} ms: {gaps:?}");
}

#[test]
fn an_effect_plays_at_60_frames_a_second_unless_the_config_says_30() {
    let live = scrolled_effect("effect-fps-default");
    let (default, thirty) = live.with(|a| {
        let default = a.effect_frame_period();
        a.config_defaults.effect_fps = Some(30);
        (default, a.effect_frame_period())
    });
    assert_eq!(default, Duration::from_micros(16_667));
    assert_eq!(thirty, Duration::from_micros(33_333));
}

/// With the mouse on, in normal mode, a button ends the effect as a key
/// does and does nothing else; the wheel lets it play. Presentation mode
/// ignores the mouse.
#[test]
fn a_mouse_button_ends_the_effect_and_the_wheel_lets_it_play() {
    use tiza_tui::app::{MouseKind, MouseOutcome};
    for (kind, ends) in [
        (MouseKind::Down, true),
        (MouseKind::Drag, true),
        (MouseKind::OtherButton, true),
        (MouseKind::ScrollDown, false),
        (MouseKind::ScrollUp, false),
    ] {
        let live = scrolled_effect(&format!("mouse-{kind:?}"));
        let (out, playing, moved) = live.with(move |a| {
            a.mouse = true;
            let cursor = (a.cursor_line, a.cursor_col);
            let out = a.on_mouse(kind, 10, 3);
            (
                out,
                a.is_playing_effect(),
                (a.cursor_line, a.cursor_col) != cursor,
            )
        });
        assert_eq!(playing, !ends, "{kind:?}: {out:?}");
        if ends {
            assert_eq!(out, MouseOutcome::EndedEffect, "{kind:?}");
            assert!(!moved, "{kind:?}: the button did something else");
        }
    }
    let live = presenting_effect("mouse-present", false);
    let (out, playing) = live.with(|a| {
        a.mouse = true;
        (a.on_mouse(MouseKind::Down, 10, 3), a.is_playing_effect())
    });
    assert_eq!((out, playing), (MouseOutcome::Ignored, true));
}

/// The real TUI with the mouse on, in normal mode: a right click (SGR)
/// during an effect ends it, and the wheel does not. The effect plays 100
/// times slower, so it is still playing when the mouse comes.
#[test]
fn a_right_click_ends_the_effect_in_the_real_tui() {
    let dir = scratch("mouse-right-click");
    std::fs::copy(
        fixture("shared/enter_effects/drop.tiza"),
        dir.join("drop.tiza"),
    )
    .unwrap();
    let body = r##"
import json
binary, cwd = sys.argv[1], sys.argv[2]
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1", TIZA_TEST_HOOK="effect:slow")
s = Pty([binary, "--no-save", "--mouse", "--theme", "dark", "--socket", "s.sock", "drop.tiza"], 30, 100, env=env)

def effect():
    r = subprocess.run([binary, "--socket", "s.sock", "api", "status"], capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout).get("effect", False) if r.returncode == 0 else None

def until(want, what):
    end = time.monotonic() + 10
    while time.monotonic() < end:
        if effect() == want:
            return True
        s.settle(0.05)
    print("WAIT_TIMEOUT", what, flush=True)
    return False

if s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame"):
    s.send(b"n")
    if until(True, "the effect starts"):
        s.send(b"\x1b[<65;20;5M")
        s.settle(0.3)
        print("AFTER_WHEEL", effect(), flush=True)
        s.send(b"\x1b[<2;20;5M\x1b[<2;20;5m")
        until(False, "the right click ends the effect")
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
"##;
    let out = tiza_qa::pty_script(
        &dir,
        "click.py",
        body,
        &[tiza_qa::bin().to_str().unwrap(), dir.to_str().unwrap()],
    );
    assert!(!out.contains("WAIT_TIMEOUT"), "{out}");
    assert!(
        out.contains("AFTER_WHEEL True"),
        "the wheel ended it: {out}"
    );
}

/// Effects that this version does not know: their names go to stderr
/// before the TUI starts (so they are on the terminal after it ends, also
/// in presentation mode), and the bar says it in a few words, so that it
/// fits. In presentation mode the last row says it for a few seconds.
#[test]
fn unknown_effects_are_named_on_stderr_and_counted_on_the_bar() {
    let dir = scratch("unknown-effects-open");
    std::fs::copy(
        fixture("compat/v1-newer-unknown-effect.tiza"),
        dir.join("newer.tiza"),
    )
    .unwrap();
    let body = r##"
binary, cwd, present = sys.argv[1], sys.argv[2], sys.argv[3] == "present"
os.chdir(cwd)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
argv = [binary, "--no-save", "--no-socket", "--theme", "dark"] + (["--present"] if present else []) + ["newer.tiza"]
s = Pty(argv, 30, 120, env=env)
s.wait_for(lambda out: b"\x1b[?1049h" in out, 10, "first frame")
s.wait_for(lambda out: b"see tiza check" in out, 10, "the bar or the last row")
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
sys.stdout.flush()
out = s.out.decode("utf-8", "replace")
cut = out.find("\x1b[?1049h")
print("BEFORE", repr(out[:cut]), flush=True)
print("SCREEN", repr(out[cut:]), flush=True)
"##;
    for present in [false, true] {
        let out = tiza_qa::pty_script(
            &dir,
            "unknown.py",
            body,
            &[
                tiza_qa::bin().to_str().unwrap(),
                dir.to_str().unwrap(),
                if present { "present" } else { "normal" },
            ],
        );
        let before = out.lines().find_map(|l| l.strip_prefix("BEFORE ")).unwrap();
        let screen = out.lines().find_map(|l| l.strip_prefix("SCREEN ")).unwrap();
        assert!(
            before.contains("3 annotations have an effect that this version of Tiza does not know")
                && before.contains("b1 (\"slide\")"),
            "present {present}: {before}"
        );
        // On the bar, or on the last row for a few seconds in presentation
        // mode.
        assert!(
            screen.contains("3 entries are not known: see tiza check"),
            "present {present}: {screen}"
        );
    }
}

/// After a button ends the effect, the rest of the gesture (a drag, the
/// release) selects nothing, also from a drag that an earlier lost
/// release left open.
#[test]
fn the_gesture_that_ends_an_effect_selects_nothing() {
    use tiza_tui::app::{MouseKind, MouseOutcome};
    let live = scrolled_effect("mouse-gesture");
    let outs = live.with(|a| {
        a.mouse = true;
        a.drag_from = Some((1, 0));
        [
            a.on_mouse(MouseKind::Down, 10, 3),
            a.on_mouse(MouseKind::Drag, 14, 6),
            a.on_mouse(MouseKind::Up, 14, 6),
        ]
    });
    assert_eq!(
        outs,
        [
            MouseOutcome::EndedEffect,
            MouseOutcome::Ignored,
            MouseOutcome::Ignored
        ]
    );
    assert!(live.with(|a| a.selection().is_none()), "a selection");
}

/// In presentation mode the bar is hidden: when the TUI opens there, the
/// effects that this version does not know show on the last row for a few
/// seconds, as the result of a `:` command. The times are the test's, not
/// the clock's.
#[test]
fn in_presentation_unknown_effects_show_for_a_few_seconds_at_the_start() {
    use tiza_tui::app::PRESENT_MESSAGE_TTL;
    let dir = scratch("unknown-effects-present");
    let doc =
        tiza_core::fs::load_document(&fixture("compat/v1-newer-unknown-effect.tiza")).unwrap();
    let mut app = App::new(doc.clone(), dir.join("doc.tiza"));
    app.set_present(true);
    let start = Instant::now();
    app.show_unknown_effects(start);
    assert!(app.present_message_shown(), "not on the last row");
    let message = app.message.clone().map(|(m, _)| m).unwrap_or_default();
    assert!(
        message.contains("entries are not known: see tiza check"),
        "{message}"
    );
    app.present_message_tick(start + PRESENT_MESSAGE_TTL - Duration::from_millis(1));
    assert!(app.present_message_shown(), "gone before its time");
    app.present_message_tick(start + PRESENT_MESSAGE_TTL);
    assert!(!app.present_message_shown(), "still there after its time");
    // Normal mode: the bar shows it, with no time.
    let mut app = App::new(doc, dir.join("doc2.tiza"));
    app.show_unknown_effects(start);
    assert!(app.present_message_until.is_none());
}
