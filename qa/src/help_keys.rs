//! The keys of the help (`tiza_tui::keys::HELP`), pressed in the state that
//! each one is for: the tests of the help panel and the check of the keys
//! page (`site/checks`) use them.

use crossterm::event::{KeyCode, KeyModifiers};
use tiza_tui::keys::{HELP, Row, When};

use crate::Live;

pub const CODE: &str = "fn main() {\n    let total = count(items) + 1;\n    report(total);\n    println!(\"{total}\");\n}\n";

/// Code of five lines, two annotations (steps 1 and 2), one change to
/// undo, the cursor in the middle of a word on line 2, at step 1.
pub fn fresh(name: &str) -> Live {
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
pub fn state(live: &Live) -> String {
    live.with(|a| {
        format!(
            "{:?}|{:?}|{}:{}|{}|{}|{:?}|{}|{}|{:?}|{}|{:?}|{:?}|{}|{}|{:?}|{}|{}|{:?}|{:?}",
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
            // A key of the view in presentation mode moves it at the next
            // frame.
            a.view_request(),
        )
    })
}

/// The key presses a help key names: `j/k  ↓/↑` is four.
pub fn presses(keys: &str) -> Vec<(KeyCode, KeyModifiers)> {
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
pub fn enter(live: &Live, when: When) {
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
pub fn documented(when: When) -> Vec<(KeyCode, KeyModifiers)> {
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
