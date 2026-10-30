//! What a spotlight dims: the code outside its lines, and each annotation
//! that does not touch its lines.
//!
//! Acceptance criteria:
//! - AC1 A spotlight with a line range also dims each annotation that does
//!   not touch one of its lines. An annotation inside the range, or one that
//!   crosses it, keeps its color.
//! - AC2 `ids` keep an annotation of the list in its color, also when it is
//!   outside the lines.
//! - AC3 The dimmed color is the `dimmed` token of the theme: dark, light
//!   and terminal.
//!
//! An annotation touches the lines when the lines from its first line to
//! its last line overlap the range of an active spotlight: the line of a
//! highlight, a mark or a note, the lines of a box or a callout, and the
//! lines from one end of an arrow to the other. Only `from_line`/`to_line`
//! count: the lines that a spotlight with only `ids` shows do not.
//!
//! The fixtures share one piece of code. `shapes.tiza` has one annotation of
//! each shape inside and outside a spotlight on lines 10-14 (step 2), and no
//! spotlight at step 3. `rules.tiza` has four notes and a highlight at step
//! 1, and a different spotlight at each step after it.

use ratatui::style::Color;
use serde_json::json;
use tiza_core::Theme;
use tiza_core::canvas::Rgb;
use tiza_core::theme::{HostColor, host_color};
use tiza_qa::{Live, fixture, scratch, tiza_ok};

/// One cell of a render: its character and the color that draws it.
type Cell<C> = (char, C);

/// The rows of a render, cell by cell.
struct Grid<C> {
    rows: Vec<Vec<Cell<C>>>,
}

impl<C: Copy + std::fmt::Debug> Grid<C> {
    fn text(&self, y: usize) -> String {
        self.rows[y].iter().map(|c| c.0).collect()
    }

    fn all_text(&self) -> String {
        (0..self.rows.len())
            .map(|y| self.text(y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The row that contains `row`; there must be exactly one.
    fn row(&self, row: &str) -> usize {
        let found: Vec<usize> = (0..self.rows.len())
            .filter(|&y| self.text(y).contains(row))
            .collect();
        assert_eq!(
            found.len(),
            1,
            "«{row}» must be on exactly one row:\n{}",
            self.all_text()
        );
        found[0]
    }

    /// The colors of the characters of `sub` (spaces left out), at its
    /// first place on the row that contains `row`.
    fn colors(&self, row: &str, sub: &str) -> Vec<C> {
        let y = self.row(row);
        let chars: Vec<char> = self.text(y).chars().collect();
        let want: Vec<char> = sub.chars().collect();
        let x = (0..chars.len())
            .find(|&x| chars[x..].starts_with(&want))
            .unwrap_or_else(|| panic!("no «{sub}» on the row «{}»", self.text(y)));
        self.rows[y][x..x + want.len()]
            .iter()
            .filter(|c| !c.0.is_whitespace())
            .map(|c| c.1)
            .collect()
    }

    /// The one color of `sub` on the row that contains `row`.
    fn color(&self, row: &str, sub: &str) -> C
    where
        C: PartialEq,
    {
        let cs = self.colors(row, sub);
        assert!(
            cs.windows(2).all(|w| w[0] == w[1]),
            "«{sub}» has more than one color: {cs:?}"
        );
        cs[0]
    }
}

/// Foreground and background of a cell of an ANSI export.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Paint {
    fg: Option<Rgb>,
    bg: Option<Rgb>,
}

/// Reads the 24-bit colors of an ANSI export, cell by cell.
fn parse_ansi(ansi: &str) -> Grid<Paint> {
    let mut rows = Vec::new();
    for line in ansi.lines() {
        let mut row = Vec::new();
        let mut paint = Paint { fg: None, bg: None };
        let mut it = line.chars().peekable();
        while let Some(ch) = it.next() {
            if ch != '\x1b' {
                row.push((ch, paint));
                continue;
            }
            assert_eq!(it.next(), Some('['), "only SGR sequences: {line:?}");
            let mut params = String::new();
            for c in it.by_ref() {
                if c == 'm' {
                    break;
                }
                params.push(c);
            }
            let nums: Vec<u8> = params.split(';').map(|p| p.parse().unwrap_or(0)).collect();
            let mut i = 0;
            while i < nums.len() {
                match nums[i] {
                    0 => paint = Paint { fg: None, bg: None },
                    38 | 48 if nums.get(i + 1) == Some(&2) && i + 4 < nums.len() => {
                        let c = Some(Rgb(nums[i + 2], nums[i + 3], nums[i + 4]));
                        if nums[i] == 38 {
                            paint.fg = c;
                        } else {
                            paint.bg = c;
                        }
                        i += 4;
                    }
                    _ => {}
                }
                i += 1;
            }
        }
        rows.push(row);
    }
    Grid { rows }
}

/// `tiza render FIXTURE --step N --width W --theme T --format ansi`.
fn render(name: &str, step: u32, width: u32, theme: &str) -> Grid<Paint> {
    let path = fixture(&format!("render/spotlight_dimming/{name}"));
    let (step, width) = (step.to_string(), width.to_string());
    parse_ansi(&tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--step",
        &step,
        "--width",
        &width,
        "--theme",
        theme,
        "--format",
        "ansi",
    ]))
}

fn fg(g: &Grid<Paint>, row: &str, sub: &str) -> Rgb {
    g.color(row, sub)
        .fg
        .unwrap_or_else(|| panic!("«{sub}» has no foreground"))
}

fn named(th: &Theme, name: &str) -> Rgb {
    th.named_color(name)
        .unwrap_or_else(|| panic!("no color {name} in the theme"))
}

#[test]
fn a_spotlight_by_lines_dims_each_annotation_outside_its_lines() {
    let th = Theme::dark();
    let g = render("shapes.tiza", 2, 140, "dark");
    // Outside lines 10-14: a box, an arrow, a note, and the label of a
    // highlight of an earlier step.
    assert_eq!(fg(&g, "╭─ main ─", "main"), th.dimmed, "box outside");
    assert_eq!(fg(&g, "stays out", "stays out"), th.dimmed, "arrow outside");
    assert_eq!(fg(&g, "prints it", "prints it"), th.dimmed, "note outside");
    assert_eq!(
        fg(&g, "earlier step", "earlier step"),
        th.dimmed,
        "label of a highlight of an earlier step"
    );
    // Inside the range, or across it: each keeps its color.
    assert_eq!(fg(&g, "covers part", "covers part"), named(&th, "green"));
    assert_eq!(
        fg(&g, "enters", "enters"),
        named(&th, "cyan"),
        "an arrow from a line outside to a line inside"
    );
    assert_eq!(
        fg(&g, "crosses", "crosses"),
        named(&th, "yellow"),
        "an arrow from line 2 to line 16 crosses lines 10-14"
    );
    assert_eq!(
        fg(&g, "╭─ inside ─", "inside"),
        named(&th, "green"),
        "a callout in the margin on a line inside"
    );
    assert_eq!(fg(&g, "falls back", "falls back"), named(&th, "pink"));
    // The code: dimmed outside the lines, not inside.
    assert_eq!(fg(&g, "println!", "println"), th.dimmed);
    assert!(
        g.colors("let line = text", "let line")
            .iter()
            .all(|c| c.fg != Some(th.dimmed)),
        "the code inside the lines is not dimmed"
    );
}

#[test]
fn without_a_spotlight_nothing_is_dimmed() {
    let th = Theme::dark();
    let g = render("shapes.tiza", 3, 140, "dark");
    assert_eq!(fg(&g, "╭─ main ─", "main"), named(&th, "blue"));
    assert_eq!(fg(&g, "stays out", "stays out"), named(&th, "red"));
    assert_eq!(fg(&g, "prints it", "prints it"), named(&th, "pink"));
    assert_eq!(fg(&g, "earlier step", "earlier step"), named(&th, "purple"));
}

#[test]
fn a_highlight_outside_the_lines_dims_the_code_that_it_colors() {
    let th = Theme::dark();
    let on = render("shapes.tiza", 2, 140, "dark");
    let off = render("shapes.tiza", 3, 140, "dark");
    // A plain highlight on line 9 colors `first` with the text color on the
    // highlight background. Under the spotlight, the dimmed color.
    let lit = off.color("// parse the first line", "first");
    assert_eq!(lit.fg, Some(th.fg));
    assert_eq!(lit.bg, Some(th.highlight_bg));
    let dim = on.color("// parse the first line", "first");
    assert_eq!(dim.fg, Some(th.dimmed), "the code of the highlight");
    assert_eq!(
        dim.bg,
        Some(th.tint(th.dimmed, 0.12)),
        "its background: the tint of the dimmed color"
    );
    // An underline of an earlier step on line 1.
    assert_eq!(fg(&off, "use std::fs;", "fs"), named(&th, "purple"));
    assert_eq!(fg(&on, "use std::fs;", "fs"), th.dimmed);
}

#[test]
fn a_dimmed_mark_keeps_its_label_in_the_margin_and_under_its_line() {
    // In the margin (140 columns) a dimmed mark kept only its legend, and
    // under its line (60 columns) it kept its label: now both keep it, in
    // the dimmed color.
    let th = Theme::dark();
    let off = render("shapes.tiza", 3, 140, "dark");
    let row = "let path = \"app.toml\"";
    assert!(off.text(off.row(row)).contains("read: reads the file"));
    let on = render("shapes.tiza", 2, 140, "dark");
    let text = on.text(on.row(row));
    assert!(text.contains("read: reads the file"), "{text}");
    assert_eq!(fg(&on, row, "reads the file"), th.dimmed);
    // The code of the mark is dimmed too.
    assert_eq!(fg(&on, row, "app.toml"), th.dimmed);
    assert_ne!(fg(&off, row, "app.toml"), th.dimmed);
    let narrow = render("shapes.tiza", 2, 60, "dark");
    let label = "read: reads the file";
    assert_eq!(
        narrow.row(label),
        narrow.row(row) + 1,
        "the label goes under its line:\n{}",
        narrow.all_text()
    );
    assert_eq!(fg(&narrow, label, "reads the file"), th.dimmed);
}

#[test]
fn a_note_under_a_line_inside_keeps_its_color() {
    // At 60 columns the notes and the callout go under their lines.
    let th = Theme::dark();
    let g = render("shapes.tiza", 2, 60, "dark");
    // The row of a note comes right after the row of the code of its line.
    let under = |text: &str, code: &str| {
        assert_eq!(
            g.row(text),
            g.row(code) + 1,
            "«{text}» is not under «{code}»:\n{}",
            g.all_text()
        );
        fg(&g, text, text)
    };
    assert_eq!(
        under("falls back", "value.parse().unwrap_or(DEFAULT)"),
        named(&th, "pink"),
        "under line 13"
    );
    assert_eq!(
        under("prints it", "println!(\"port {port}\");"),
        th.dimmed,
        "under line 7"
    );
    assert_eq!(
        fg(&g, "╭─ inside ─", "inside"),
        named(&th, "green"),
        "a callout under line 11"
    );
}

#[test]
fn ids_keep_an_annotation_outside_the_lines_in_its_color() {
    // Step 4: lines 10-12 and ids [n6].
    let th = Theme::dark();
    let g = render("rules.tiza", 4, 100, "dark");
    let orange = named(&th, "orange");
    assert_eq!(
        fg(&g, "calls parse_port", "calls"),
        orange,
        "in ids, line 6"
    );
    assert_eq!(
        fg(&g, "only the first line", "only"),
        orange,
        "inside the lines and not in ids"
    );
    assert_eq!(fg(&g, "the default port", "the default"), th.dimmed);
    assert_eq!(fg(&g, "the path", "the path"), th.dimmed);
    // The code of n6 stays in focus too, outside the lines.
    assert!(
        g.colors("let port = parse_port", "let")
            .iter()
            .all(|c| c.fg != Some(th.dimmed)),
        "the line of n6 is not dimmed"
    );
    assert_eq!(fg(&g, "println!", "println"), th.dimmed);
}

#[test]
fn two_spotlights_at_one_step_show_the_union_of_their_lines() {
    // Step 3: lines 1-2 and lines 10-12.
    let th = Theme::dark();
    let g = render("rules.tiza", 3, 100, "dark");
    let orange = named(&th, "orange");
    assert_eq!(fg(&g, "the default port", "the default"), orange);
    assert_eq!(fg(&g, "only the first line", "only"), orange);
    assert_eq!(fg(&g, "calls parse_port", "calls"), th.dimmed);
    assert_eq!(fg(&g, "the path", "the path"), th.dimmed);
    assert!(
        g.colors("const DEFAULT", "const")
            .iter()
            .all(|c| c.fg != Some(th.dimmed)),
        "line 2 is in the first spotlight"
    );
    assert_eq!(fg(&g, "println!", "println"), th.dimmed);
}

#[test]
fn a_spotlight_with_only_ids_dims_the_other_annotations_on_its_lines() {
    // Step 5: ids [h4] only. Line 4 shows, but the note on it is not in ids.
    let th = Theme::dark();
    let g = render("rules.tiza", 5, 100, "dark");
    assert_eq!(fg(&g, "the path", "the path"), named(&th, "cyan"));
    assert_eq!(fg(&g, "same line as h4", "same line"), th.dimmed);
    assert_eq!(fg(&g, "only the first line", "only"), th.dimmed);
    assert!(
        g.colors("let path = ", "let")
            .iter()
            .all(|c| c.fg != Some(th.dimmed)),
        "the line of h4 is not dimmed"
    );
}

#[test]
fn a_spotlight_by_lines_dims_until_its_last_step_and_not_after() {
    // Step 6: lines 10-12 with until 7. Step 8 has no spotlight.
    let th = Theme::dark();
    let orange = named(&th, "orange");
    for step in [6, 7] {
        let g = render("rules.tiza", step, 100, "dark");
        assert_eq!(
            fg(&g, "the default port", "the default"),
            th.dimmed,
            "step {step}"
        );
        assert_eq!(fg(&g, "only the first line", "only"), orange, "step {step}");
    }
    for step in [1, 8] {
        let g = render("rules.tiza", step, 100, "dark");
        assert_eq!(
            fg(&g, "the default port", "the default"),
            orange,
            "step {step}"
        );
    }
}

#[test]
fn the_light_theme_dims_with_its_own_color() {
    let th = Theme::light();
    assert_ne!(th.dimmed, Theme::dark().dimmed);
    let g = render("shapes.tiza", 2, 140, "light");
    assert_eq!(fg(&g, "prints it", "prints it"), th.dimmed);
    assert_eq!(fg(&g, "╭─ main ─", "main"), th.dimmed);
    assert_eq!(fg(&g, "falls back", "falls back"), named(&th, "pink"));
    assert_eq!(fg(&g, "enters", "enters"), named(&th, "cyan"));
}

/// The foreground of each cell of the TUI screen.
fn screen(live: &Live, cols: u16, rows: u16) -> Grid<Color> {
    let buf = live.buffer(cols, rows);
    let rows = (0..rows)
        .map(|y| {
            (0..cols)
                .flat_map(|x| {
                    let c = &buf[(x, y)];
                    let fg = c.fg;
                    c.symbol()
                        .chars()
                        .collect::<Vec<_>>()
                        .into_iter()
                        .map(move |ch| (ch, fg))
                })
                .collect()
        })
        .collect();
    Grid { rows }
}

#[test]
fn the_terminal_theme_dims_with_the_terminal_palette() {
    let doc = tiza_core::fs::open(
        fixture("render/spotlight_dimming/rules.tiza")
            .to_str()
            .unwrap(),
    )
    .unwrap()
    .document;
    let dir = scratch("spotlight-terminal");
    let live = Live::open(doc, dir.join("doc.tiza"), dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": 2}));
    live.command("theme terminal");
    let th = live.with(|a| a.theme.clone());
    assert!(th.is_terminal());
    let host = |c: Rgb| match host_color(c) {
        Some(HostColor::Indexed(n)) => Color::Indexed(n),
        other => panic!("not a palette color: {other:?}"),
    };
    let g = screen(&live, 100, 30);
    assert_eq!(
        g.color("the default port", "the default"),
        host(th.dimmed),
        "a note outside lines 10-12"
    );
    assert_eq!(
        g.color("only the first line", "only"),
        host(named(&th, "orange")),
        "a note inside"
    );
}
