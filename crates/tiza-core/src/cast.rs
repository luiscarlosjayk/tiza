//! Terminal recordings generated from the document's steps, without a pty
//! (t-051): an **asciicast v3** file (`.cast`, what asciinema and its web
//! player read) written straight from the canvas, and a **VHS tape**
//! (`.tape`) that scripts the real binary for charmbracelet/vhs to turn into
//! a GIF or MP4 (needs `vhs`, `ttyd` and `ffmpeg` on the machine).

use std::fmt::Write as _;

use crate::Document;
use crate::layout::{LayoutOptions, layout};
use crate::theme::Theme;

/// Options shared by both formats.
#[derive(Debug, Clone)]
pub struct CastOptions {
    /// Seconds each step stays on screen.
    pub interval: f64,
    /// Columns of the recorded terminal.
    pub width: usize,
    /// Rows: `None` = tallest frame.
    pub rows: Option<usize>,
    pub line_numbers: bool,
    /// Recording title (asciicast header, VHS comment).
    pub title: Option<String>,
    /// Start at step 0 (code only) before the first annotated step.
    pub from_zero: bool,
}

impl Default for CastOptions {
    fn default() -> Self {
        Self {
            interval: 2.0,
            width: 100,
            rows: None,
            line_numbers: true,
            title: None,
            from_zero: true,
        }
    }
}

/// The frames of every step, as ANSI text with `\r\n` line ends, plus the
/// tallest one (rows).
fn frames(doc: &Document, theme: &Theme, opts: &CastOptions) -> (Vec<(u32, String)>, usize) {
    let first = if opts.from_zero { 0 } else { 1 };
    let last = doc.max_step().max(first);
    let mut out = Vec::new();
    let mut rows = 0;
    for step in first..=last {
        let l = layout(
            doc,
            theme,
            &LayoutOptions {
                width: opts.width,
                step,
                focus: None,
                line_numbers: opts.line_numbers,
                tab_width: 4,
                typing: false,
                redact: true,
            },
        );
        rows = rows.max(l.canvas.height);
        // No newline after the last row: it would scroll the frame up by one.
        let ansi = l.canvas.to_ansi();
        let ansi = ansi.trim_end_matches('\n').replace('\n', "\r\n");
        out.push((step, ansi));
    }
    (out, rows)
}

/// asciicast v3: one JSON header line, then `[interval, "o", text]` events
/// (intervals are relative to the previous event in v3) and a `"m"` marker
/// per step so players can jump between them.
pub fn to_asciicast(doc: &Document, theme: &Theme, opts: &CastOptions) -> String {
    let (frames, tallest) = frames(doc, theme, opts);
    let rows = opts.rows.unwrap_or(tallest).max(1);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let title = opts
        .title
        .clone()
        .or_else(|| doc.title.clone())
        .unwrap_or_else(|| "Tiza".into());
    let header = serde_json::json!({
        "version": 3,
        "term": {"cols": opts.width, "rows": rows, "type": "xterm-256color"},
        "timestamp": timestamp,
        "title": title,
        "env": {"SHELL": "/bin/sh", "TERM": "xterm-256color"},
    });
    let mut out = String::new();
    let _ = writeln!(out, "{header}");
    let bg = theme.bg;
    for (i, (step, ansi)) in frames.iter().enumerate() {
        let interval = if i == 0 { 0.0 } else { opts.interval };
        let marker = serde_json::json!([0.0, "m", format!("step {step}")]);
        let _ = writeln!(out, "{marker}");
        // Clear with the theme background, home, then the frame.
        let data = format!("\x1b[48;2;{};{};{}m\x1b[2J\x1b[H{ansi}", bg.0, bg.1, bg.2);
        let event = serde_json::json!([interval, "o", data]);
        let _ = writeln!(out, "{event}");
    }
    // Hold the last frame, then end.
    let _ = writeln!(out, "{}", serde_json::json!([opts.interval, "x", "0"]));
    out
}

/// Margin added after a typing animation before the next `n`, so the tape
/// never sends a key while the TUI is still typing (that key would only
/// finish the animation and the step would be lost).
const TYPING_MARGIN_MS: u64 = 500;

/// How long the tape holds `step`: the interval, or the typing time of the
/// step's moves plus a margin when that is longer.
fn step_sleep(doc: &Document, step: u32, interval: f64) -> String {
    let typing = doc.typing_ms_at(step);
    if typing == 0 {
        return format_secs(interval);
    }
    let needed = (typing + TYPING_MARGIN_MS) as f64 / 1000.0;
    format_secs(needed.max(interval))
}

/// VHS window padding, in pixels.
pub const TAPE_PADDING: usize = 10;
/// Pixels per column at `FontSize 18`: the real glyph advance is ≈11.3 px,
/// so 12 leaves slack and the TUI keeps its margin layout at 100 columns.
pub const TAPE_PX_PER_COL: usize = 12;
/// Pixels per row at `FontSize 18`.
pub const TAPE_PX_PER_ROW: usize = 24;

/// A VHS tape that opens the document in the real TUI (presentation mode)
/// and advances through the steps with `n`. A step whose moves are typed
/// sleeps at least as long as the typing takes (plus a margin); the last
/// step stays twice the interval so the GIF loop does not cut it short.
/// The window takes the document theme's colours and is sized with slack
/// for the real glyph advance, two more rows and the padding.
pub fn to_vhs_tape(
    doc: &Document,
    theme: &Theme,
    document_path: &str,
    output: &str,
    opts: &CastOptions,
) -> String {
    let first = if opts.from_zero { 0 } else { 1 };
    let last = doc.max_step().max(first);
    let (_, tallest) = frames(doc, theme, opts);
    let rows = opts.rows.unwrap_or(tallest + 1).max(10);
    let sleep = format_secs(opts.interval);
    let hex = |c: crate::canvas::Rgb| format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2);
    let mut out = String::new();
    let title = opts.title.clone().or_else(|| doc.title.clone());
    let _ = writeln!(
        out,
        "# VHS tape generated by tiza (https://github.com/charmbracelet/vhs)"
    );
    if let Some(t) = title {
        let _ = writeln!(out, "# {t}");
    }
    let _ = writeln!(out, "# Run: vhs {}", output.replace(".gif", ".tape"));
    let _ = writeln!(out);
    let _ = writeln!(out, "Output \"{}\"", output.replace('"', "\\\""));
    let _ = writeln!(out);
    let _ = writeln!(out, "Set Shell \"bash\"");
    let _ = writeln!(out, "Set FontSize 18");
    let _ = writeln!(
        out,
        "Set Width {}",
        opts.width * TAPE_PX_PER_COL + 2 * TAPE_PADDING
    );
    let _ = writeln!(
        out,
        "Set Height {}",
        (rows + 2) * TAPE_PX_PER_ROW + 2 * TAPE_PADDING
    );
    let _ = writeln!(out, "Set Padding {TAPE_PADDING}");
    // The window matches the document theme, so the padding does not show
    // as a lighter frame around the TUI.
    let _ = writeln!(
        out,
        "Set Theme {{ \"background\": \"{}\", \"foreground\": \"{}\", \"cursor\": \"{}\" }}",
        hex(theme.bg),
        hex(theme.fg),
        hex(theme.fg)
    );
    let _ = writeln!(out, "Set TypingSpeed 0");
    let _ = writeln!(out);
    let _ = writeln!(out, "Hide");
    let _ = writeln!(
        out,
        "Type \"tiza {} --present --no-socket{}\"",
        shell_quote(document_path),
        if first == 0 { " --step 0" } else { "" }
    );
    let _ = writeln!(out, "Enter");
    let _ = writeln!(out, "Sleep 1s");
    let _ = writeln!(out, "Show");
    let _ = writeln!(out);
    let _ = writeln!(out, "# step {first}");
    let _ = writeln!(out, "Sleep {sleep}");
    for step in (first + 1)..=last {
        let typing = doc.typing_ms_at(step);
        if typing > 0 {
            let _ = writeln!(out, "# step {step} (typed: {typing} ms)");
        } else {
            let _ = writeln!(out, "# step {step}");
        }
        let _ = writeln!(out, "Type \"n\"");
        let _ = writeln!(out, "Sleep {}", step_sleep(doc, step, opts.interval));
    }
    // The last step stays one more interval (2× in all) before the tape
    // ends, so the GIF loop does not jump back to step 0 right away.
    let _ = writeln!(out, "# hold the last step");
    let _ = writeln!(out, "Sleep {sleep}");
    let _ = writeln!(out);
    let _ = writeln!(out, "Hide");
    let _ = writeln!(out, "Type \"q\"");
    out
}

fn format_secs(s: f64) -> String {
    if (s - s.round()).abs() < 1e-9 {
        format!("{}s", s.round() as u64)
    } else {
        format!("{}ms", (s * 1000.0).round() as u64)
    }
}

fn shell_quote(path: &str) -> String {
    if path
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "./_-:".contains(c))
    {
        path.to_string()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        let mut d = Document::from_code("fn a() {}\nfn b() {}\nfn c() {}\n", "rs", None);
        d.add(crate::model::Annotation {
            id: "n1".into(),
            step: 1,
            color: None,
            markers: None,
            shape: crate::model::Shape::Note {
                line: 1,
                text: "one".into(),
            },
        })
        .unwrap();
        d.add(crate::model::Annotation {
            id: "n2".into(),
            step: 2,
            color: None,
            markers: None,
            shape: crate::model::Shape::Note {
                line: 3,
                text: "two".into(),
            },
        })
        .unwrap();
        d
    }

    #[test]
    fn asciicast_has_header_and_one_frame_per_step() {
        let cast = to_asciicast(&doc(), &Theme::dark(), &CastOptions::default());
        let lines: Vec<&str> = cast.lines().collect();
        let header: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(header["version"], 3);
        assert_eq!(header["term"]["cols"], 100);
        assert!(header["term"]["rows"].as_u64().unwrap() >= 3);
        let events: Vec<serde_json::Value> = lines[1..]
            .iter()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let outputs: Vec<&serde_json::Value> = events.iter().filter(|e| e[1] == "o").collect();
        assert_eq!(outputs.len(), 3, "steps 0, 1, 2");
        assert_eq!(outputs[0][0], 0.0);
        assert_eq!(outputs[1][0], 2.0);
        assert!(outputs[1][2].as_str().unwrap().contains("one"));
        assert!(!outputs[1][2].as_str().unwrap().contains("two"));
        assert!(outputs[2][2].as_str().unwrap().contains("two"));
        assert!(outputs[0][2].as_str().unwrap().contains("\x1b[2J\x1b[H"));
        assert!(outputs[0][2].as_str().unwrap().contains("\r\n"));
        assert!(
            !outputs[0][2].as_str().unwrap().ends_with('\n'),
            "no scroll after the last row"
        );
        let markers: Vec<&serde_json::Value> = events.iter().filter(|e| e[1] == "m").collect();
        assert_eq!(markers.len(), 3);
        assert_eq!(markers[2][2], "step 2");
        assert_eq!(events.last().unwrap()[1], "x");
    }

    #[test]
    fn tape_scripts_the_real_binary() {
        let tape = to_vhs_tape(
            &doc(),
            &Theme::dark(),
            "examples/demo.tiza",
            "/tmp/my demo.gif",
            &CastOptions {
                interval: 1.5,
                ..Default::default()
            },
        );
        assert!(tape.contains("Output \"/tmp/my demo.gif\""), "{tape}");
        assert!(tape.contains("Type \"tiza examples/demo.tiza --present --no-socket --step 0\""));
        assert_eq!(tape.matches("Type \"n\"").count(), 2);
        assert_eq!(
            tape.matches("Sleep 1500ms").count(),
            4,
            "steps 0, 1, 2 + hold"
        );
        assert!(tape.trim_end().ends_with("Type \"q\""));
        assert!(
            tape.contains("Set Width 1220"),
            "100 cols × 12 + 2 × 10:\n{tape}"
        );
        assert!(
            tape.contains("Set Theme { \"background\": \"#15171b\""),
            "{tape}"
        );
        let quoted = to_vhs_tape(
            &doc(),
            &Theme::dark(),
            "my demo's.tiza",
            "x.gif",
            &CastOptions::default(),
        );
        assert!(quoted.contains("Type \"tiza 'my demo'\\''s.tiza'"));
    }

    #[test]
    fn tape_waits_for_a_typed_step() {
        // 20 lines typed at 160 ms = 3.2 s: the 2 s default would send the
        // next `n` mid-typing and lose a step (t-098 AC10).
        let mut d = doc();
        let content = (0..20)
            .map(|i| format!("    let x{i} = {i};"))
            .collect::<Vec<_>>()
            .join("\n");
        d.add_move(
            serde_json::from_value(serde_json::json!({
                "at_step": 2, "action": "code.patch", "op": "insert",
                "after": "fn b() {}", "content": content, "typing": "line", "speed_ms": 160
            }))
            .unwrap(),
        )
        .unwrap();
        let tape = to_vhs_tape(
            &d,
            &Theme::dark(),
            "d.tiza",
            "d.gif",
            &CastOptions::default(),
        );
        let lines: Vec<&str> = tape.lines().collect();
        let i = lines
            .iter()
            .position(|l| *l == "# step 2 (typed: 3200 ms)")
            .unwrap();
        assert_eq!(lines[i + 1], "Type \"n\"");
        assert_eq!(lines[i + 2], "Sleep 3700ms");
        // Step 1 has nothing typed: the plain interval.
        let j = lines.iter().position(|l| *l == "# step 1").unwrap();
        assert_eq!(lines[j + 2], "Sleep 2s");
    }
}
