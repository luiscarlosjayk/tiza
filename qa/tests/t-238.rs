//! The help (`?`) was a block of text that wrapped into itself: no
//! columns, a long line broken under the key column, no way to find a key,
//! and its keys could drift from the key handler.
//!
//! Acceptance criteria (option A «Marco», docs/ui-guidelines.md):
//! - AC1 Sections that follow the work: Move, Steps, Select and annotate,
//!   Edit an annotation (Tab), View, Commands (:), Save and quit.
//! - AC2 Two columns: the key or command and its description; a long
//!   description wraps in its own column, a key never wraps.
//! - AC3 `/` filters by key, description or section; Esc clears the
//!   filter, then closes.
//! - AC4 The footer has the panel's keys and the position.
//! - AC5 At 40 to 200 columns the panel never draws past the screen; below
//!   64 cells of content a row stacks (key above description); below
//!   40×12 the help does not open and the bar says how large to make it.
//! - AC7 The keys and commands come from one table (`tiza_tui::keys`):
//!   every `:` command is in it, every key of it does something where it
//!   says, and every key that does something is in it.
//!
//! Corner cases: a filter that matches nothing says so; `?` in
//! presentation mode.

use crossterm::event::{KeyCode, KeyModifiers};
use tiza_qa::Live;
use tiza_tui::app::{Mode, Overlay};
use tiza_tui::keys::{HELP, Row, When};

const CODE: &str = "fn main() {\n    let total = count(items) + 1;\n    report(total);\n    println!(\"{total}\");\n}\n";

/// Code of five lines, two annotations (steps 1 and 2), one change to
/// undo, the cursor in the middle of a word on line 2, at step 1.
fn fresh(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.command("note 2 the total");
    live.command("note 3 printed");
    live.command("step a2 2");
    live.command("label a1 the sum");
    live.command("step 1");
    live.with(|a| {
        a.autosave = false;
        a.size = (100, 30);
        a.cursor_line = 2;
        a.cursor_col = 10;
        a.message = None;
    });
    live
}

/// Everything a key can change, as text.
fn state(live: &Live) -> String {
    live.with(|a| {
        format!(
            "{:?}|{:?}|{}:{}|{}|{}|{:?}|{}|{}|{:?}|{}|{:?}|{:?}|{}|{}|{:?}|{}|{}|{:?}",
            a.mode,
            a.overlay,
            a.cursor_line,
            a.cursor_col,
            a.doc.step,
            a.revision,
            a.selected,
            a.present,
            a.pointer.is_some(),
            a.theme.name,
            a.quit,
            a.arrow_from,
            a.focus,
            a.doc.annotations.len(),
            a.overlay_scroll,
            a.message,
            a.dirty,
            a.layout_stable,
            a.pending_sel,
        )
    })
}

/// The key presses a help key names: `j/k  ↓/↑` is four.
fn presses(keys: &str) -> Vec<(KeyCode, KeyModifiers)> {
    let mut out = Vec::new();
    for word in keys.split_whitespace() {
        let parts: Vec<&str> = if word.chars().count() > 1 && word.contains('/') && word != "/" {
            word.split('/').collect()
        } else {
            vec![word]
        };
        for p in parts {
            out.push(match p {
                "↓" => (KeyCode::Down, KeyModifiers::NONE),
                "↑" => (KeyCode::Up, KeyModifiers::NONE),
                "←" => (KeyCode::Left, KeyModifiers::NONE),
                "→" => (KeyCode::Right, KeyModifiers::NONE),
                "PgDn" => (KeyCode::PageDown, KeyModifiers::NONE),
                "PgUp" => (KeyCode::PageUp, KeyModifiers::NONE),
                "Esc" => (KeyCode::Esc, KeyModifiers::NONE),
                "Enter" => (KeyCode::Enter, KeyModifiers::NONE),
                "Tab" => (KeyCode::Tab, KeyModifiers::NONE),
                "S-Tab" => (KeyCode::BackTab, KeyModifiers::NONE),
                "Ctrl-C" => (KeyCode::Char('c'), KeyModifiers::CONTROL),
                one if one.chars().count() == 1 => (
                    KeyCode::Char(one.chars().next().unwrap()),
                    KeyModifiers::NONE,
                ),
                other => panic!("no key for «{other}» in «{keys}»"),
            });
        }
    }
    out
}

/// Puts `live` in the state `when` names.
fn enter(live: &Live, when: When) {
    let press = |c: KeyCode| live.with(move |a| tiza_tui::on_key(a, c, KeyModifiers::NONE));
    match when {
        When::Normal => {}
        When::Selection => {
            press(KeyCode::Char('v'));
            press(KeyCode::Char('l'));
        }
        When::Annotation => press(KeyCode::Tab),
        When::Presenting => press(KeyCode::Char('P')),
    }
}

/// The keys the help documents for `when`: its own rows, the keys of rows
/// of normal mode, and the keys a description names (`Home and End
/// do the same`, `Esc also stops it`, `Ctrl-R also redoes`).
fn documented(when: When) -> Vec<(KeyCode, KeyModifiers)> {
    let mut out = Vec::new();
    for (_, rows) in HELP {
        for r in *rows {
            let Row::Key {
                keys,
                desc,
                when: w,
            } = r
            else {
                continue;
            };
            // A key of normal mode works in the other states too, unless a
            // row of that state gives it another meaning.
            if *w != when && *w != When::Normal {
                continue;
            }
            out.extend(presses(keys));
            if desc.contains("Home and End") {
                out.push((KeyCode::Home, KeyModifiers::NONE));
                out.push((KeyCode::End, KeyModifiers::NONE));
            }
            if desc.contains("Esc also") {
                out.push((KeyCode::Esc, KeyModifiers::NONE));
            }
            if desc.contains("Ctrl-R") {
                out.push((KeyCode::Char('r'), KeyModifiers::CONTROL));
            }
        }
    }
    out
}

#[test]
fn ac7_every_command_is_in_the_table() {
    let mut named: Vec<String> = Vec::new();
    for (_, rows) in HELP {
        for r in *rows {
            if let Row::Command { .. } = r {
                for word in r.key_text().split_whitespace() {
                    if let Some(name) = word.strip_prefix(':') {
                        named.push(name.trim_end_matches('!').to_string());
                    }
                }
            }
        }
    }
    for c in tiza_tui::app::App::COMMANDS {
        assert!(named.iter().any(|n| n == c), ":{c} is not in the help");
    }
}

#[test]
fn ac7_every_key_of_the_table_does_something_where_it_says() {
    for (_, rows) in HELP {
        for r in *rows {
            let Row::Key { keys, when, .. } = r else {
                continue;
            };
            for (code, mods) in presses(keys) {
                let live = fresh("t238-key");
                enter(&live, *when);
                // A redo needs a change undone first.
                if code == KeyCode::Char('U') {
                    live.keys(&[KeyCode::Char('u')]);
                }
                let before = state(&live);
                live.with(move |a| tiza_tui::on_key(a, code, mods));
                assert_ne!(
                    state(&live),
                    before,
                    "«{keys}» ({code:?} {mods:?}) does nothing in {when:?}"
                );
            }
        }
    }
}

#[test]
fn ac7_every_key_that_does_something_is_in_the_table() {
    let mut keys: Vec<(KeyCode, KeyModifiers)> = (0x20u8..0x7f)
        .map(|b| (KeyCode::Char(b as char), KeyModifiers::NONE))
        .collect();
    keys.extend(
        [
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::PageUp,
            KeyCode::PageDown,
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Enter,
            KeyCode::Esc,
            KeyCode::Backspace,
            KeyCode::Delete,
        ]
        .map(|k| (k, KeyModifiers::NONE)),
    );
    keys.extend(('a'..='z').map(|c| (KeyCode::Char(c), KeyModifiers::CONTROL)));
    let mut missing = Vec::new();
    for when in [
        When::Normal,
        When::Selection,
        When::Annotation,
        When::Presenting,
    ] {
        let docs = documented(when);
        for &(code, mods) in &keys {
            let live = fresh("t238-reverse");
            enter(&live, when);
            let before = state(&live);
            live.with(move |a| tiza_tui::on_key(a, code, mods));
            if state(&live) != before && !docs.contains(&(code, mods)) {
                missing.push(format!("{when:?}: {code:?} {mods:?}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "keys that act but are not in the help:\n{}",
        missing.join("\n")
    );
}

#[test]
fn ac1_the_sections_follow_the_work() {
    let titles: Vec<&str> = HELP.iter().map(|(s, _)| s.title()).collect();
    assert_eq!(
        titles,
        [
            "Move",
            "Steps",
            "Select and annotate",
            "Edit an annotation (Tab)",
            "View",
            "Commands (:)",
            "Save and quit",
        ]
    );
}

/// The screen with the help open, `keys` typed in it.
fn help_screen(live: &Live, cols: u16, rows: u16, keys: &[KeyCode]) -> String {
    live.with(move |a| a.size = (cols, rows));
    let _ = live.screen(cols, rows);
    live.keys(&[KeyCode::Char('?')]);
    let _ = live.screen(cols, rows);
    live.keys(keys);
    live.screen(cols, rows)
}

#[test]
fn step1_ac2_ac4_two_columns_the_footer_and_the_position() {
    let live = fresh("t238-columns");
    let s = help_screen(&live, 80, 24, &[]);
    assert!(s.contains("╭─ Keys and commands "), "{s}");
    assert!(s.contains("esc close ─╮"), "{s}");
    assert!(s.contains("│ / type to filter keys and commands"), "{s}");
    assert!(
        s.contains("│ j/k  ↓/↑      Move the cursor one line down or up."),
        "{s}"
    );
    // A long description wraps in its own column.
    assert!(
        s.contains("│ w  b  e       Go to the next word, the previous word or the end of the   │")
            && s.contains("│               word.  "),
        "{s}"
    );
    assert!(s.contains("╰─ / filter · j/k scroll · g/G top/end "), "{s}");
    assert!(s.contains(" 1-14 of "), "{s}");
}

#[test]
fn step2_ac3_the_filter_keeps_the_step_rows_then_esc_clears_then_closes() {
    let live = fresh("t238-filter");
    let mut keys = vec![KeyCode::Char('/')];
    keys.extend("step".chars().map(KeyCode::Char));
    let s = help_screen(&live, 80, 40, &keys);
    assert!(s.contains("│ / step▏"), "{s}");
    assert!(s.contains("esc clear ─╮"), "{s}");
    assert!(s.contains("│ n  PgDn       Go to the next step."), "{s}");
    assert!(!s.contains("Move the cursor one line"), "{s}");
    assert!(s.contains(" matches ─╯"), "{s}");
    // Enter keeps the filter; Esc clears it; Esc again closes.
    live.keys(&[KeyCode::Enter]);
    assert_eq!(live.with(|a| a.overlay_filter.clone()), "step");
    live.keys(&[KeyCode::Esc]);
    assert_eq!(
        live.with(|a| (a.overlay, a.overlay_filter.clone())),
        (Overlay::Help, String::new())
    );
    live.keys(&[KeyCode::Esc]);
    assert_eq!(live.with(|a| a.overlay), Overlay::None);
}

#[test]
fn corner_a_filter_that_matches_nothing_says_so() {
    let live = fresh("t238-empty");
    let s = help_screen(
        &live,
        80,
        24,
        &[
            KeyCode::Char('/'),
            KeyCode::Char('z'),
            KeyCode::Char('z'),
            KeyCode::Char('z'),
        ],
    );
    assert!(s.contains("│ no key or command matches «zzz»"), "{s}");
    assert!(s.contains("│ Press Esc to clear the filter."), "{s}");
    assert!(s.contains(" 0 matches ─╯"), "{s}");
    // While typing, `q` and `?` are letters of the filter.
    live.keys(&[KeyCode::Char('q'), KeyCode::Char('?')]);
    assert_eq!(live.with(|a| a.overlay_filter.clone()), "zzzq?");
    assert_eq!(live.with(|a| a.overlay), Overlay::Help);
}

#[test]
fn step3_j_k_and_page_down_scroll_and_the_footer_follows() {
    let live = fresh("t238-scroll");
    let s = help_screen(&live, 80, 24, &[KeyCode::PageDown]);
    assert!(s.contains(" 15-28 of "), "{s}");
    live.keys(&[KeyCode::Char('j')]);
    assert!(live.screen(80, 24).contains(" 16-29 of "));
    live.keys(&[KeyCode::Char('G')]);
    let end = live.screen(80, 24);
    assert!(
        end.contains("Tiza creates the folders that are missing."),
        "{end}"
    );
    live.keys(&[KeyCode::Char('g')]);
    assert!(live.screen(80, 24).contains(" 1-14 of "));
}

#[test]
fn ac5_at_every_width_no_key_is_split_and_nothing_passes_the_screen() {
    for cols in [40u16, 60, 80, 120, 200] {
        let live = fresh(&format!("t238-width-{cols}"));
        let panel = tiza_tui::panel::rect(ratatui::layout::Rect::new(0, 0, cols, 30)).unwrap();
        let (left, right) = (panel.x as usize, (panel.x + panel.width - 1) as usize);
        // Every row of the panel has its frame at the panel's edges, on
        // every page.
        let framed = |screen: &str| {
            let lines: Vec<&str> = screen.lines().collect();
            for y in panel.y..panel.y + panel.height {
                let line = lines[y as usize];
                let cells: Vec<char> = line.chars().collect();
                assert!("│╭╰".contains(cells[left]), "{cols}: {line}");
                assert!("│╮╯".contains(cells[right]), "{cols}: {line}");
            }
        };
        let mut seen: Vec<String> = Vec::new();
        let first = help_screen(&live, cols, 30, &[]);
        framed(&first);
        seen.extend(first.lines().map(str::to_string));
        for _ in 0..80 {
            live.keys(&[KeyCode::PageDown]);
            let screen = live.screen(cols, 30);
            framed(&screen);
            seen.extend(screen.lines().map(str::to_string));
        }
        // A key wider than the content is cut with …, never wrapped.
        let room = panel.width as usize - 4;
        for (_, rows) in HELP {
            for row in *rows {
                if matches!(row, Row::Key { .. } | Row::Command { .. }) {
                    let key = row.key_text();
                    let shown = if key.chars().count() > room {
                        key.chars().take(room - 1).collect::<String>()
                    } else {
                        key.clone()
                    };
                    assert!(
                        seen.iter().any(|l| l.contains(&format!("│ {shown}"))),
                        "{cols}: «{key}» not whole on one line"
                    );
                }
            }
        }
    }
}

#[test]
fn ac5_a_narrow_panel_stacks_the_key_above_its_description() {
    let live = fresh("t238-stacked");
    let s = help_screen(&live, 60, 30, &[]);
    assert!(s.contains("│ j/k  ↓/↑"), "{s}");
    assert!(
        s.contains("│     Move the cursor one line down or up."),
        "{s}"
    );
    assert!(s.contains("│ / type to filter "), "{s}");
}

#[test]
fn ac5_below_40x12_the_help_does_not_open_and_says_why() {
    let live = fresh("t238-small");
    live.with(|a| a.size = (39, 11));
    live.keys(&[KeyCode::Char('?')]);
    assert_eq!(live.with(|a| a.overlay), Overlay::None);
    let (msg, err) = live.message().unwrap();
    assert!(err, "{msg}");
    assert_eq!(msg, "use 40×12 or more to show the help");
    // A bar of 39 cells still shows the size: the other segments give way.
    let bar = live.screen(39, 11).lines().last().unwrap().to_string();
    assert!(bar.contains("NORMAL  no-save  ! use 40×12 "), "{bar}");
}

#[test]
fn ac5_a_resize_below_40x12_closes_the_help_and_says_why() {
    let live = fresh("t238-resize");
    let mut keys = vec![KeyCode::Char('/')];
    keys.extend("ste".chars().map(KeyCode::Char));
    help_screen(&live, 80, 24, &keys);
    let bar = live.screen(39, 11).lines().last().unwrap().to_string();
    assert_eq!(live.with(|a| a.overlay), Overlay::None);
    assert!(bar.contains("use 40×12 "), "{bar}");
    // The next keys are not lost in a help that cannot be seen.
    live.keys(&[KeyCode::Char('j')]);
    assert_eq!(live.with(|a| a.cursor_line), 3);
}

#[test]
fn corner_the_help_opens_in_presentation_mode_and_arrows_do_not_move_it() {
    let live = fresh("t238-present");
    live.keys(&[KeyCode::Char('P')]);
    let s = help_screen(&live, 80, 24, &[]);
    assert!(s.contains("╭─ Keys and commands "), "{s}");
    let step = live.with(|a| a.doc.step);
    live.keys(&[KeyCode::Right, KeyCode::Left]);
    assert_eq!(
        live.with(|a| (a.doc.step, a.overlay)),
        (step, Overlay::Help)
    );
    assert!(live.with(|a| matches!(a.mode, Mode::Normal)));
}

#[test]
fn ac2_placeholders_are_in_the_note_color() {
    let live = fresh("t238-colors");
    live.with(|a| a.size = (120, 40));
    live.keys(&[KeyCode::Char('?'), KeyCode::Char('/')]);
    live.keys(&":mark".chars().map(KeyCode::Char).collect::<Vec<_>>());
    let note = live.with(|a| a.theme.note);
    let cells = live.with(move |a| {
        use ratatui::{Terminal, backend::TestBackend};
        let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
        t.draw(|f| tiza_tui::ui::draw(f, a)).unwrap();
        let buf = t.backend().buffer().clone();
        let mut found = None;
        for y in 0..40 {
            let row: String = (0..120).map(|x| buf[(x, y)].symbol().to_string()).collect();
            if let Some(i) = row.find(":mark KIND") {
                let x = row[..i].chars().count() as u16 + ":mark ".len() as u16;
                found = Some((buf[(x, y)].symbol().to_string(), buf[(x, y)].fg));
            }
        }
        found
    });
    let (sym, fg) = cells.expect(":mark row");
    assert_eq!(sym, "K");
    assert_eq!(fg, ratatui::style::Color::Rgb(note.0, note.1, note.2));
}
