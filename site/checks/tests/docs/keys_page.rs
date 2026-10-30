//! The keys page, `docs/reference/keys.md`, against the keys of the help
//! (`tiza_tui::keys::HELP`) and the TUI: each key of its tables works in the
//! state of its section, each key of the help is in one of its tables, and
//! the page gives the size of the command history (`HISTORY_COMMANDS`).

use crossterm::event::{KeyCode, KeyModifiers};
use tiza_qa::help_keys::{documented, enter, fresh, presses, state};
use tiza_qa::markdown::{cited, section, table_rows};
use tiza_qa::repo_file;
use tiza_tui::app::HISTORY_COMMANDS;
use tiza_tui::keys::{HELP, Row, When};

/// The sections of `docs/reference/keys.md` with a table of keys, and
/// the state of their keys.
const PAGE_SECTIONS: [(&str, When); 7] = [
    ("## Move the cursor", When::Normal),
    ("## Go to a step", When::Normal),
    ("## Select code", When::Normal),
    ("## Annotate a selection", When::Selection),
    ("## Edit an annotation", When::Annotation),
    ("## Present", When::Presenting),
    ("## Other keys", When::Normal),
];

/// A key that a table of the keys page names.
struct PageKey {
    when: When,
    name: String,
    key: (KeyCode, KeyModifiers),
    line: usize,
}

/// The key press of a name of the keys page: `PageDown`, `Shift-Tab`,
/// `Ctrl-C`, `↓`, `x`.
fn key_named(name: &str) -> Option<(KeyCode, KeyModifiers)> {
    let none = |code| Some((code, KeyModifiers::NONE));
    match name {
        "↓" => none(KeyCode::Down),
        "↑" => none(KeyCode::Up),
        "←" => none(KeyCode::Left),
        "→" => none(KeyCode::Right),
        "PageDown" => none(KeyCode::PageDown),
        "PageUp" => none(KeyCode::PageUp),
        "Home" => none(KeyCode::Home),
        "End" => none(KeyCode::End),
        "Tab" => none(KeyCode::Tab),
        "Shift-Tab" => none(KeyCode::BackTab),
        "Enter" => none(KeyCode::Enter),
        "Esc" => none(KeyCode::Esc),
        "Backspace" => none(KeyCode::Backspace),
        "Space" => none(KeyCode::Char(' ')),
        _ => {
            let (name, mods) = match name.strip_prefix("Ctrl-") {
                Some(letter) => (letter, KeyModifiers::CONTROL),
                None => (name, KeyModifiers::NONE),
            };
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if mods == KeyModifiers::CONTROL && c.is_ascii_alphabetic() => {
                    Some((KeyCode::Char(c.to_ascii_lowercase()), mods))
                }
                (Some(c), None) if mods == KeyModifiers::NONE => Some((KeyCode::Char(c), mods)),
                _ => None,
            }
        }
    }
}

/// The keys of the tables of the keys page: each name between backticks
/// in the first cell of a row. A row that says «in a selection» is of a
/// selection, whatever its section.
fn page_keys(md: &str) -> Vec<PageKey> {
    let mut out = Vec::new();
    for (heading, when) in PAGE_SECTIONS {
        let s = section(md, heading).unwrap_or_else(|| panic!("keys.md has no {heading:?}"));
        let before = out.len();
        for (n, cells) in table_rows(s.text) {
            let line = s.page_line(n);
            let when = if cells[0].contains("in a selection") {
                When::Selection
            } else {
                when
            };
            for name in cells[0].split('`').skip(1).step_by(2) {
                let key = key_named(name)
                    .unwrap_or_else(|| panic!("keys.md:{line}: unknown key name «{name}»"));
                out.push(PageKey {
                    when,
                    name: name.to_string(),
                    key,
                    line,
                });
            }
        }
        assert!(
            out.len() > before,
            "keys.md:{}: {heading:?} has no table with keys",
            s.line
        );
    }
    out
}

/// In normal mode, with an annotation selected and in presentation mode,
/// the other keys of normal mode work too (the keys page says so), so a
/// key of the help for that state is enough. In a selection, other keys
/// do nothing: there the key must act.
#[test]
fn every_key_on_the_keys_page_works_in_the_state_of_its_section() {
    let mut wrong = Vec::new();
    for k in page_keys(&repo_file("docs/reference/keys.md")) {
        if k.when != When::Selection {
            if !documented(k.when).contains(&k.key) {
                wrong.push(format!(
                    "keys.md:{}: «{}» ({:?}) is not a key of the help in that state",
                    k.line, k.name, k.when
                ));
            }
            continue;
        }
        let live = fresh("page-key");
        enter(&live, When::Selection);
        let before = state(&live);
        let (code, mods) = k.key;
        live.with(move |a| tiza_tui::on_key(a, code, mods));
        if state(&live) == before {
            wrong.push(format!(
                "keys.md:{}: «{}» in a selection does nothing there",
                k.line, k.name
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_key_of_the_help_is_on_the_keys_page() {
    let page: Vec<(KeyCode, KeyModifiers)> = page_keys(&repo_file("docs/reference/keys.md"))
        .into_iter()
        .map(|k| k.key)
        .collect();
    let mut missing = Vec::new();
    for (_, rows) in HELP {
        for r in *rows {
            let Row::Key { keys, when, .. } = r else {
                continue;
            };
            for (code, mods) in presses(keys) {
                if !page.contains(&(code, mods)) {
                    missing.push(format!("«{keys}» ({when:?}): {code:?} {mods:?}"));
                }
            }
        }
    }
    assert!(
        missing.is_empty(),
        "keys of the help that no table of docs/reference/keys.md has:\n{}",
        missing.join("\n")
    );
}

/// The keys page gives the size of the command history.
#[test]
fn the_keys_page_gives_the_size_of_the_command_history() {
    let cites = cited(
        &repo_file("docs/reference/keys.md"),
        "keeps the last ",
        " commands",
    );
    let [(line, n)] = cites[..] else {
        panic!(
            "keys.md gives the size of the history {} times, not once: {cites:?}",
            cites.len()
        );
    };
    assert_eq!(
        n, HISTORY_COMMANDS as u64,
        "keys.md:{line} says that the history keeps {n} commands, and HISTORY_COMMANDS is {HISTORY_COMMANDS}"
    );
}
