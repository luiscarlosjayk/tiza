//! t-051 — `.cast` (asciicast v3) and `.tape` (VHS) generated from the
//! document's steps.
//!
//! Acceptance criteria:
//! - AC1 `tiza render FILE --out x.cast` writes asciicast v3: a JSON
//!   header (`version: 3`, `term.cols` = width, `term.rows` = tallest frame,
//!   title from the document or `--title`), then one `"m"` marker and one
//!   `"o"` frame per step from 0 (code only) to the last, each frame clearing
//!   the screen with the theme background and using `\r\n`; intervals are
//!   `--interval` seconds (2 by default) between frames; a final `"x"` event.
//!   `--skip-zero` starts at step 1. The file replays without scrolling.
//! - AC2 `--out x.tape` writes a VHS tape that runs the real binary in
//!   presentation mode (`--present --no-socket --step 0`), one `Type "n"` +
//!   `Sleep` per step, `Output x.gif`, dimensions derived from the width and
//!   the tallest frame; paths with spaces are quoted.
//! - AC3 `--format cast|tape` forces the format; `-` writes to stdout; the
//!   live instance (no FILE) can be recorded too.
//! - AC4 Neither format needs a pty, vhs, ttyd or ffmpeg to be *generated*;
//!   the README says what is needed to *play* them.

use serde_json::Value;
use tiza_qa::{Live, example, scratch, tiza, tiza_ok};

fn ownership() -> String {
    example("ownership.rs.tiza").to_string_lossy().into_owned()
}

fn parse_cast(text: &str) -> (Value, Vec<Value>) {
    let mut lines = text.lines();
    let header: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    let events = lines.map(|l| serde_json::from_str(l).unwrap()).collect();
    (header, events)
}

#[test]
fn ac1_cast_has_one_frame_per_step_and_replays_without_scrolling() {
    let dir = scratch("t051-cast");
    let out = dir.join("own.cast");
    tiza_ok(&[
        "render",
        &ownership(),
        "--out",
        out.to_str().unwrap(),
        "--width",
        "100",
    ]);
    let text = std::fs::read_to_string(&out).unwrap();
    let (header, events) = parse_cast(&text);
    assert_eq!(header["version"], 3);
    assert_eq!(header["term"]["cols"], 100);
    assert_eq!(header["title"], "Ownership in Rust: move, borrow and &mut");
    let rows = header["term"]["rows"].as_u64().unwrap() as usize;
    let frames: Vec<&Value> = events.iter().filter(|e| e[1] == "o").collect();
    let markers: Vec<&Value> = events.iter().filter(|e| e[1] == "m").collect();
    let doc = tiza_core::fs::open(&ownership()).unwrap().document;
    let steps = doc.max_step() as usize + 1;
    assert_eq!(frames.len(), steps, "step 0 … {}", doc.max_step());
    assert_eq!(markers.len(), steps);
    assert_eq!(markers[0][2], "step 0");
    assert_eq!(frames[0][0], 0.0, "first frame at once");
    assert_eq!(frames[1][0], 2.0, "default interval");
    assert_eq!(events.last().unwrap()[1], "x");
    for f in &frames {
        let data = f[2].as_str().unwrap();
        assert!(
            data.starts_with("\x1b[48;2;21;23;27m\x1b[2J\x1b[H"),
            "clear + home with the theme bg"
        );
        let bare_lf = data
            .as_bytes()
            .windows(2)
            .any(|w| w[1] == b'\n' && w[0] != b'\r');
        assert!(data.contains("\r\n") && !bare_lf, "CRLF line ends");
        assert!(!data.ends_with('\n'), "no trailing newline (would scroll)");
        // Rows never exceed the declared terminal height.
        assert!(
            data.matches("\r\n").count() < rows,
            "{} rows in a {rows}-row terminal",
            data.matches("\r\n").count() + 1
        );
    }
    assert!(
        !frames[0][2].as_str().unwrap().contains("s is born"),
        "step 0 = code only"
    );
    assert!(
        frames[steps - 1][2].as_str().unwrap().contains("drop"),
        "last step has the drop mark"
    );

    // --interval, --title, --skip-zero, --theme.
    let out2 = dir.join("own2.cast");
    tiza_ok(&[
        "render",
        &ownership(),
        "--out",
        out2.to_str().unwrap(),
        "--interval",
        "0.5",
        "--title",
        "Demo",
        "--skip-zero",
        "--theme",
        "light",
    ]);
    let (header, events) = parse_cast(&std::fs::read_to_string(&out2).unwrap());
    assert_eq!(header["title"], "Demo");
    let frames: Vec<&Value> = events.iter().filter(|e| e[1] == "o").collect();
    assert_eq!(frames.len(), steps - 1);
    assert_eq!(frames[1][0], 0.5);
    assert!(
        frames[0][2]
            .as_str()
            .unwrap()
            .starts_with("\x1b[48;2;247;245;239m"),
        "light bg"
    );
    assert_eq!(events.iter().find(|e| e[1] == "m").unwrap()[2], "step 1");

    // Optional: a real terminal emulator (python3 + pyte) replays the cast
    // and the last screen shows the last step with its first row intact.
    let script = dir.join("replay.py");
    std::fs::write(
        &script,
        r#"
import json, sys
try:
    import pyte
except ImportError:
    print("NO_PYTE"); sys.exit(0)
lines = open(sys.argv[1], encoding="utf-8").read().splitlines()
h = json.loads(lines[0])
screen = pyte.Screen(h["term"]["cols"], h["term"]["rows"]); stream = pyte.Stream(screen)
for l in lines[1:]:
    ev = json.loads(l)
    if ev[1] == "o": stream.feed(ev[2])
print("FIRST:" + screen.display[0].rstrip())
print("ALL:" + "|".join(r.rstrip() for r in screen.display))
"#,
    )
    .unwrap();
    if let Ok(o) = std::process::Command::new("python3")
        .args(["-I", script.to_str().unwrap(), out.to_str().unwrap()])
        .output()
    {
        let s = String::from_utf8_lossy(&o.stdout);
        if !s.contains("NO_PYTE") {
            assert!(s.contains("FIRST:  1"), "first row still visible: {s}");
            assert!(s.contains("drop: s dies here"), "{s}");
        }
    }
}

#[test]
fn ac2_tape_drives_the_real_binary() {
    let dir = scratch("t051-tape");
    let out = dir.join("own.tape");
    tiza_ok(&[
        "render",
        &ownership(),
        "--out",
        out.to_str().unwrap(),
        "--interval",
        "1",
    ]);
    let tape = std::fs::read_to_string(&out).unwrap();
    let doc = tiza_core::fs::open(&ownership()).unwrap().document;
    // AC5: `Output` is always quoted (an absolute path, one with spaces).
    assert!(
        tape.contains(&format!("Output \"{}\"", dir.join("own.gif").display())),
        "{tape}"
    );
    assert!(
        tape.contains(&format!(
            "Type \"tiza {} --present --no-socket --step 0\"",
            ownership()
        )),
        "{tape}"
    );
    assert_eq!(tape.matches("Type \"n\"").count(), doc.max_step() as usize);
    assert_eq!(
        tape.matches("Sleep 1s").count(),
        doc.max_step() as usize + 1 + 1 + 1,
        "one per step + the first + startup + the hold of the last step (AC8)"
    );
    // AC6: the window leaves slack for the real glyph advance, two rows
    // and the padding: Width ≥ cols × 12 + 2 × pad.
    let width: usize = tape
        .lines()
        .find_map(|l| l.strip_prefix("Set Width "))
        .unwrap()
        .parse()
        .unwrap();
    assert!(width >= 100 * 12 + 2 * 10, "Set Width {width}");
    let height: usize = tape
        .lines()
        .find_map(|l| l.strip_prefix("Set Height "))
        .unwrap()
        .parse()
        .unwrap();
    assert!(height >= (10 + 2) * 24 + 2 * 10, "Set Height {height}");
    // AC7: the window takes the document theme's colours, not a VHS preset.
    assert!(
        tape.contains("Set Theme { \"background\": \"#15171b\", \"foreground\": \"#"),
        "{tape}"
    );
    assert!(!tape.contains("Catppuccin"));
    assert!(tape.trim_end().ends_with("Type \"q\""));
    // The tape types only the start command, `n` and `q`, never a `:`
    // command (moved here from t-107).
    let start = format!(
        "Type \"tiza {} --present --no-socket --step 0\"",
        ownership()
    );
    for l in tape.lines().filter(|l| l.starts_with("Type ")) {
        assert!(
            l == start || l == "Type \"n\"" || l == "Type \"q\"",
            "{l}\n{tape}"
        );
    }
    let hold = tape.lines().rev().nth(3).unwrap_or("");
    assert_eq!(
        hold, "Sleep 1s",
        "the last step holds one more interval:\n{tape}"
    );
    assert!(tape.contains("# Ownership in Rust"));
    // A path with a space is quoted for the shell.
    let spaced = dir.join("my demo.tiza");
    std::fs::copy(ownership(), &spaced).unwrap();
    std::fs::copy(example("ownership.rs"), dir.join("ownership.rs")).unwrap();
    let t2 = tiza_ok(&[
        "render",
        spaced.to_str().unwrap(),
        "--out",
        "-",
        "--format",
        "tape",
    ]);
    assert!(
        t2.contains(&format!("Type \"tiza '{}' --present", spaced.display())),
        "{t2}"
    );
    assert!(
        t2.contains("Output \"demo.gif\""),
        "stdout tape names demo.gif"
    );
    // `--theme light` colours the window from the light theme.
    let t3 = tiza_ok(&[
        "render",
        &ownership(),
        "--out",
        "-",
        "--format",
        "tape",
        "--theme",
        "light",
    ]);
    assert!(
        t3.contains("Set Theme { \"background\": \"#f7f5ef\""),
        "{t3}"
    );
    // AC9: one hint line after writing a file.
    let out2 = dir.join("hint.tape");
    let o = tiza(&["render", &ownership(), "--out", out2.to_str().unwrap()]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains(&format!(
            "play it with `vhs {}` (needs vhs, ttyd, ffmpeg)",
            out2.display()
        )),
        "{err}"
    );
    let cast = dir.join("hint.cast");
    let o = tiza(&["render", &ownership(), "--out", cast.to_str().unwrap()]);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(
        err.contains(&format!("play it with `asciinema play {}`", cast.display()))
            && err.contains("agg "),
        "{err}"
    );
}

#[test]
fn ac3_format_flag_stdout_and_live_instance() {
    let cast = tiza_ok(&[
        "render",
        &ownership(),
        "--out",
        "-",
        "--format",
        "cast",
        "--width",
        "80",
    ]);
    let (header, events) = parse_cast(&cast);
    assert_eq!(header["term"]["cols"], 80);
    assert!(events.len() > 2);
    // A .cast extension on a path with another case still works.
    let dir = scratch("t051-ext");
    let out = dir.join("X.CAST");
    tiza_ok(&["render", &ownership(), "--out", out.to_str().unwrap()]);
    assert!(
        std::fs::read_to_string(&out)
            .unwrap()
            .starts_with("{\"env\"")
    );
    // Live instance: the current document, every step.
    let live = Live::from_code("fn a() {}\nfn b() {}\n", "rs", "t051-live");
    live.command("note 1 one");
    live.command("step 2");
    live.command("note 2 two");
    let text = tiza_ok(&[
        "--socket",
        live.socket.to_str().unwrap(),
        "render",
        "--out",
        "-",
        "--format",
        "cast",
    ]);
    let (_, events) = parse_cast(&text);
    let frames: Vec<&Value> = events.iter().filter(|e| e[1] == "o").collect();
    assert_eq!(frames.len(), 3, "steps 0, 1, 2 of the live document");
    assert!(frames[2][2].as_str().unwrap().contains("two"));
}
