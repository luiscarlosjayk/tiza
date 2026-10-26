//! The keys and the `:` commands that the help (`?`) shows: one table. The
//! help panel draws it, and the tests compare it with the key handler
//! (`on_key`) and with `App::COMMANDS`, so the help cannot drift from the
//! real bindings. Texts follow ASD-STE100 (see crates/tiza-tui/UI-GUIDELINES.md).

use crate::panel::{Panel, PanelRow};

/// A section of the help, in the order of the work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Move,
    Steps,
    SelectAndAnnotate,
    EditAnnotation,
    View,
    Commands,
    SaveAndQuit,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Section::Move => "Move",
            Section::Steps => "Steps",
            Section::SelectAndAnnotate => "Select and annotate",
            Section::EditAnnotation => "Edit an annotation (Tab)",
            Section::View => "View",
            Section::Commands => "Commands (:)",
            Section::SaveAndQuit => "Save and quit",
        }
    }

    /// Its keys and commands line up with the commands: the wide column.
    pub fn wide(self) -> bool {
        matches!(self, Section::Commands | Section::SaveAndQuit)
    }
}

/// Where a key works: the state a test puts the TUI in before it presses
/// the key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// Normal mode, nothing selected.
    Normal,
    /// A `v`/`V` selection is open. Most motions work there too, but `b`
    /// adds a box: `B` goes back a word.
    Selection,
    /// An annotation is selected with Tab.
    Annotation,
    /// Presentation mode (`P`).
    Presenting,
}

/// One row of the help.
#[derive(Clone, Copy, Debug)]
pub enum Row {
    /// Keys as the user presses them: a pair joined with `/` (`j/k`),
    /// alternatives two cells apart (`n  PgDn`).
    Key {
        keys: &'static str,
        desc: &'static str,
        when: When,
    },
    /// A `:` command as the user types it. A placeholder is between `{` and
    /// `}`: drawn in the `note` color, the braces left out.
    Command {
        syntax: &'static str,
        desc: &'static str,
    },
    /// A sub-header: `With a selection:`, `Annotate`.
    Sub(&'static str),
    /// A line of text: the legend of the placeholders.
    Note(&'static str),
}

impl Row {
    /// The key or the command as it is drawn (no braces).
    pub fn key_text(&self) -> String {
        match self {
            Row::Key { keys, .. } => keys.to_string(),
            Row::Command { syntax, .. } => syntax.replace(['{', '}'], ""),
            Row::Sub(t) | Row::Note(t) => t.to_string(),
        }
    }

    pub fn desc(&self) -> &'static str {
        match self {
            Row::Key { desc, .. } | Row::Command { desc, .. } => desc,
            Row::Sub(_) | Row::Note(_) => "",
        }
    }
}

use Row::{Command, Key, Note, Sub};
use When::{Annotation, Normal, Presenting, Selection};

pub const HELP: &[(Section, &[Row])] = &[
    (
        Section::Move,
        &[
            Key {
                keys: "j/k  ↓/↑",
                desc: "Move the cursor one line down or up.",
                when: Normal,
            },
            Key {
                keys: "h/l  ←/→",
                desc: "Move the cursor one column left or right.",
                when: Normal,
            },
            Key {
                keys: "w  b  e",
                desc: "Go to the next word, the previous word or the end of the word.",
                when: Normal,
            },
            Key {
                keys: "0  $",
                desc: "Go to the start or the end of the line.",
                when: Normal,
            },
            Key {
                keys: "g/G",
                desc: "Go to the first or the last line. Home and End do the same.",
                when: Normal,
            },
            Key {
                keys: "J/K",
                desc: "Move half a screen down or up.",
                when: Normal,
            },
        ],
    ),
    (
        Section::Steps,
        &[
            Key {
                keys: "n  PgDn",
                desc: "Go to the next step.",
                when: Normal,
            },
            Key {
                keys: "p  PgUp",
                desc: "Go to the previous step. Step 0 shows only the code.",
                when: Normal,
            },
            Key {
                keys: "→/←",
                desc: "In presentation mode, go to the next or the previous step.",
                when: Presenting,
            },
        ],
    ),
    (
        Section::SelectAndAnnotate,
        &[
            Key {
                keys: "v",
                desc: "Select text.",
                when: Normal,
            },
            Key {
                keys: "V",
                desc: "Select lines.",
                when: Normal,
            },
            Key {
                keys: "i",
                desc: "Select the word under the cursor.",
                when: Normal,
            },
            Sub("With a selection:"),
            Key {
                keys: "h",
                desc: "Highlight the text. On a line selection, focus the lines.",
                when: Selection,
            },
            Key {
                keys: "m",
                desc: "Add a mark. Then type the kind and a label.",
                when: Selection,
            },
            Key {
                keys: "n",
                desc: "Add a note. Then type the text.",
                when: Selection,
            },
            Key {
                keys: "C",
                desc: "Add a callout. Then type the text.",
                when: Selection,
            },
            Key {
                keys: "b",
                desc: "Add a box. Then type a label.",
                when: Selection,
            },
            Key {
                keys: "B",
                desc: "Go to the previous word, as b does without a selection.",
                when: Selection,
            },
            Key {
                keys: "a",
                desc: "Start an arrow. Select the target and press a again.",
                when: Selection,
            },
            Key {
                keys: "s",
                desc: "Add a spotlight. The rest of the step dims.",
                when: Selection,
            },
            Key {
                keys: "Esc",
                desc: "Cancel the selection.",
                when: Selection,
            },
        ],
    ),
    (
        Section::EditAnnotation,
        &[
            Key {
                keys: "Tab/S-Tab",
                desc: "Select the next or the previous annotation.",
                when: Normal,
            },
            Key {
                keys: "Enter",
                desc: "Edit the label.",
                when: Annotation,
            },
            Key {
                keys: "e",
                desc: "Change the properties with :set.",
                when: Annotation,
            },
            Key {
                keys: "c",
                desc: "Change the color.",
                when: Annotation,
            },
            Key {
                keys: "x",
                desc: "Delete the annotation. Press u to undo.",
                when: Annotation,
            },
            Key {
                keys: "+/-",
                desc: "Move the annotation to the next or the previous step.",
                when: Annotation,
            },
            Key {
                keys: "u/U",
                desc: "Undo or redo the last change. Ctrl-R also redoes.",
                when: Normal,
            },
            Key {
                keys: "Esc",
                desc: "Clear the selection.",
                when: Annotation,
            },
        ],
    ),
    (
        Section::View,
        &[
            Key {
                keys: "f",
                desc: "Turn the pointer on or off. The code around it dims. It is never saved.",
                when: Normal,
            },
            Key {
                keys: "P",
                desc: "Start or stop presentation mode. Esc also stops it.",
                when: Normal,
            },
            Key {
                keys: "t",
                desc: "Change between the dark and the light theme.",
                when: Normal,
            },
            Key {
                keys: "L",
                desc: "Show the annotation list. Enter on a row goes to its step.",
                when: Normal,
            },
            Key {
                keys: "?",
                desc: "Show this help.",
                when: Normal,
            },
            Key {
                keys: ":",
                desc: "Open the command line. Tab completes a command.",
                when: Normal,
            },
        ],
    ),
    (
        Section::Commands,
        &[
            Note("L line   A-B lines   L:text text on line L"),
            Note("ID annotation, as a3   N step number"),
            Sub("Annotate"),
            Command {
                syntax: ":mark {KIND} {L:text} [{label}]",
                desc: "Add a mark. KIND is info, warn, error, ok, question, important, read, \
                       write, move, new, drop, scope, flow, todo or a style.",
            },
            Command {
                syntax: ":style {NAME} glyph={G} legend=\"{text}\" color={C}",
                desc: "Define a mark style. Use NAME as a KIND.",
            },
            Command {
                syntax: ":hl {L:text} [{label}]",
                desc: "Highlight the text. emphasis=underline, brackets or strike changes how \
                       it shows.",
            },
            Command {
                syntax: ":arrow {L:text} {L:text} [{label}]",
                desc: "Add an arrow between two points.",
            },
            Command {
                syntax: ":box {A-B} [{label}]",
                desc: "Add a box around the lines.",
            },
            Command {
                syntax: ":note {L} {text}",
                desc: "Add a note on the line. Use . for the cursor line.",
            },
            Command {
                syntax: ":callout {A}[-{B}] [@margin|@below|@auto] {text}",
                desc: "Add a paragraph in a frame. title=\"…\" gives it a title.",
            },
            Command {
                syntax: ":spot {A-B} [{ID}…]",
                desc: "Add a spotlight. The rest of the step dims.",
            },
            Command {
                syntax: ":until {ID} {N}",
                desc: "Keep the spotlight on until step N.",
            },
            Sub("Change"),
            Command {
                syntax: ":label {ID} {text}",
                desc: "Change the label.",
            },
            Command {
                syntax: ":color {ID} {COLOR}",
                desc: "Change the color. COLOR is a name, #rrggbb or auto.",
            },
            Command {
                syntax: ":set {ID} {key}={value}…",
                desc: "Change properties: stroke, route, heads, border, corners, fill or emphasis.",
            },
            Command {
                syntax: ":step {ID} {N}",
                desc: "Move the annotation to step N.",
            },
            Command {
                syntax: ":del {ID}…",
                desc: "Delete the annotations.",
            },
            Command {
                syntax: ":clear [{N}|.]",
                desc: "Delete all annotations, or only the annotations of one step.",
            },
            Sub("Steps and files"),
            Command {
                syntax: ":step {N}",
                desc: "Go to step N.",
            },
            Command {
                syntax: ":next  :prev",
                desc: "Go to the next or the previous step.",
            },
            Command {
                syntax: ":goto {ID}",
                desc: "Go to a named step.",
            },
            Command {
                syntax: ":link {path}[:{A-B}]",
                desc: "Show another file from this step on.",
            },
            Command {
                syntax: ":ref {path}[:{A-B}]",
                desc: "Use this file as the code.",
            },
            Command {
                syntax: ":embed",
                desc: "Copy the code into the .tiza file.",
            },
            Command {
                syntax: ":follow on|off",
                desc: "Reload the code when the file changes on disk.",
            },
            Sub("View"),
            Command {
                syntax: ":focus {A-B}",
                desc: "Dim all lines except these. :focus alone clears it.",
            },
            Command {
                syntax: ":theme dark|light|terminal|auto",
                desc: "Set the theme.",
            },
            Command {
                syntax: ":theme file {PATH}",
                desc: "Load the theme colors from a file.",
            },
            Command {
                syntax: ":lang {LANG}|auto",
                desc: "Set the language of the code.",
            },
            Command {
                syntax: ":title {text}",
                desc: "Set the title of the document.",
            },
            Command {
                syntax: ":numbers on|off",
                desc: "Show or hide the line numbers.",
            },
            Command {
                syntax: ":pad {V} {H}|default",
                desc: "Set the blank cells around the drawing.",
            },
            Command {
                syntax: ":margin {N}|auto",
                desc: "Set the largest right margin, from 12 to 200 cells.",
            },
            Command {
                syntax: ":layout stable|dynamic",
                desc: "Use the layout of the take while you edit.",
            },
            Command {
                syntax: ":mouse on|off",
                desc: "Use the mouse to move, select and scroll.",
            },
            Sub("Export and agents"),
            Command {
                syntax: ":export [{FILE}] [step={N}] [width={N}] [theme={T}]",
                desc: "Write the step to a file: .txt, .ansi, .md, .svg, .png or .webp.",
            },
            Command {
                syntax: ":supervise on|off",
                desc: "Make agent changes wait for your approval.",
            },
            Command {
                syntax: ":review",
                desc: "Show the agent changes that wait.",
            },
            Command {
                syntax: ":list  :present  :help",
                desc: "Do the same as L, P and ?.",
            },
            Command {
                syntax: ":undo  :redo",
                desc: "Do the same as u and U.",
            },
        ],
    ),
    (
        Section::SaveAndQuit,
        &[
            Key {
                keys: "q",
                desc: "Quit. Unsaved changes stop the quit.",
                when: Normal,
            },
            Key {
                keys: "Ctrl-C",
                desc: "Close what is open, as Esc does. With nothing open, quit without saving.",
                when: Normal,
            },
            Command {
                syntax: ":w",
                desc: "Save now.",
            },
            Command {
                syntax: ":w {PATH}",
                desc: "Write a copy to PATH.",
            },
            Command {
                syntax: ":saveas {PATH}",
                desc: "Save to PATH from now on.",
            },
            Command {
                syntax: ":wq",
                desc: "Save and quit.",
            },
            Command {
                syntax: ":q",
                desc: "Quit, as q does.",
            },
            Command {
                syntax: ":q!",
                desc: "Quit without saving.",
            },
            Command {
                syntax: ":autosave on|off",
                desc: "Save every change, or save only with :w.",
            },
            Command {
                syntax: ":w! {PATH}",
                desc: "Add ! to overwrite a file that exists.",
            },
            Note(
                "With --no-save, :saveas PATH saves to another file, and every change saves there after it.",
            ),
            Note("With --no-save, :q! discards the changes."),
            Note("A relative PATH starts at the folder where you started Tiza."),
            Note("Tiza creates the folders that are missing."),
        ],
    ),
];

/// The placeholders of a command's syntax: `(text, is a placeholder)` in
/// order, the braces left out.
pub fn syntax_parts(syntax: &str) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut cur = String::new();
    let mut inside = false;
    for c in syntax.chars() {
        match c {
            '{' | '}' => {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), inside));
                }
                inside = c == '{';
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push((cur, inside));
    }
    out
}

/// The rows of the help, filtered by `filter`: a substring of the key or
/// command, of the description or of the section title, in any case. A
/// filter keeps the matching keys and commands under their section header,
/// without the sub-headers and the legend. Also the number of keys and
/// commands that match.
pub fn help_rows(filter: &str) -> (Vec<PanelRow>, usize) {
    let f = filter.to_lowercase();
    let mut out = Vec::new();
    let mut matches = 0;
    for (section, rows) in HELP {
        let in_title = section.title().to_lowercase().contains(&f);
        let kept: Vec<&Row> = rows
            .iter()
            .filter(|r| match r {
                Row::Key { .. } | Row::Command { .. } => {
                    f.is_empty()
                        || in_title
                        || r.key_text().to_lowercase().contains(&f)
                        || r.desc().to_lowercase().contains(&f)
                }
                Row::Sub(_) | Row::Note(_) => f.is_empty(),
            })
            .collect();
        if !kept
            .iter()
            .any(|r| matches!(r, Row::Key { .. } | Row::Command { .. }))
        {
            continue;
        }
        if !out.is_empty() {
            out.push(PanelRow::Blank);
        }
        out.push(PanelRow::Section(section.title().to_string()));
        for r in kept {
            out.push(match r {
                Row::Key { keys, desc, .. } => {
                    matches += 1;
                    PanelRow::Pair {
                        key: vec![(keys.to_string(), false)],
                        desc: desc.to_string(),
                        wide: section.wide(),
                    }
                }
                Row::Command { syntax, desc } => {
                    matches += 1;
                    PanelRow::Pair {
                        key: syntax_parts(syntax),
                        desc: desc.to_string(),
                        wide: section.wide(),
                    }
                }
                Row::Sub(t) => PanelRow::Sub(t.to_string()),
                Row::Note(t) => PanelRow::Note(t.to_string()),
            });
        }
    }
    if out.is_empty() {
        out = vec![
            PanelRow::Text(format!("no key or command matches «{filter}»")),
            PanelRow::Blank,
            PanelRow::Text("Press Esc to clear the filter.".into()),
        ];
    }
    (out, matches)
}

/// The help panel as it shows now: its filter, whether it is typed, and
/// how wide its content is (a narrow one says less on the filter line).
pub fn help_panel(filter: &str, typing: bool, content_w: usize) -> Panel {
    let (rows, matches) = help_rows(filter);
    let filtered = !filter.is_empty();
    let footer: Vec<(&'static str, &'static str)> = if typing {
        vec![
            ("", "type to filter"),
            ("↑/↓", "scroll"),
            ("enter", "keep"),
            ("esc", "clear"),
        ]
    } else if filtered && matches == 0 {
        vec![("/", "filter"), ("esc", "clear")]
    } else {
        vec![("/", "filter"), ("j/k", "scroll"), ("g/G", "top/end")]
    };
    Panel {
        title: "Keys and commands".into(),
        chip: ("esc", if filtered || typing { "clear" } else { "close" }),
        filter: filter.to_string(),
        typing,
        placeholder: if content_w >= crate::panel::STACK_BELOW {
            "type to filter keys and commands"
        } else {
            "type to filter"
        },
        rows,
        selected: None,
        footer,
        position: filtered.then(|| match matches {
            1 => "1 match".to_string(),
            n => format!("{n} matches"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braces_are_balanced_and_parts_rebuild_the_text() {
        for (_, rows) in HELP {
            for r in *rows {
                if let Row::Command { syntax, .. } = r {
                    assert_eq!(syntax.matches('{').count(), syntax.matches('}').count());
                    let joined: String = syntax_parts(syntax).into_iter().map(|p| p.0).collect();
                    assert_eq!(joined, r.key_text(), "{syntax}");
                }
            }
        }
    }
}
