//! The annotation list (`L`) and the agent review (`:review`) as panels
//! (docs/ui-guidelines.md): rows grouped by step and file, in columns, one
//! of them selected. `App::list_lines` stays as it is: `:list` and the
//! agents read its text.

use tiza_core::model::{Annotation, Shape};

use crate::app::App;
use crate::panel::{Panel, PanelRow};

/// A row of the list that can be selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListItem {
    Annotation {
        id: String,
        step: u32,
    },
    /// A move, by its 1-based index.
    Move {
        index: usize,
        step: u32,
    },
}

impl ListItem {
    /// What the selection keeps of the row, to find it again when the rows
    /// above it change (an agent adds one, a deletion is undone).
    pub fn key(&self) -> String {
        match self {
            ListItem::Annotation { id, .. } => format!("a:{id}"),
            ListItem::Move { index, .. } => format!("m:{index}"),
        }
    }

    pub fn step(&self) -> u32 {
        match self {
            ListItem::Annotation { step, .. } | ListItem::Move { step, .. } => *step,
        }
    }
}

/// The selected row among `keys`: the one the selection keeps, or else the
/// one at its place.
pub fn selected(app: &App, keys: &[String]) -> Option<usize> {
    if keys.is_empty() {
        return None;
    }
    app.overlay_pick
        .as_ref()
        .and_then(|k| keys.iter().position(|x| x == k))
        .or(Some(app.overlay_sel.min(keys.len() - 1)))
}

/// The type and the lines of an annotation, as the list's columns show
/// them, and its label.
fn columns(a: &Annotation) -> (String, String, String) {
    let label = |l: &Option<String>| l.clone().unwrap_or_default();
    match &a.shape {
        Shape::Highlight { span, label: l, .. } => (
            "highlight".into(),
            format!("L{} {}..{}", span.line, span.start, span.end),
            label(l),
        ),
        Shape::Mark {
            kind,
            span,
            label: l,
            ..
        } => (
            format!("mark {kind}"),
            format!("L{} {}..{}", span.line, span.start, span.end),
            label(l),
        ),
        Shape::Box {
            from_line,
            to_line,
            label: l,
            ..
        } => ("box".into(), format!("L{from_line}-{to_line}"), label(l)),
        Shape::Arrow {
            from, to, label: l, ..
        } => (
            "arrow".into(),
            format!("L{}:{}→L{}:{}", from.line, from.col, to.line, to.col),
            label(l),
        ),
        Shape::Note { line, text } => ("note".into(), format!("L{line}"), text.clone()),
        Shape::Callout {
            from_line,
            to_line,
            text,
            title,
            ..
        } => (
            "callout".into(),
            format!("L{from_line}-{}", to_line.unwrap_or(*from_line)),
            match title {
                Some(t) => format!("{t}: {text}"),
                None => text.clone(),
            },
        ),
        Shape::Spotlight {
            from_line,
            to_line,
            ids,
            ..
        } => (
            "spotlight".into(),
            from_line
                .map(|f| format!("L{f}-{}", to_line.unwrap_or(f)))
                .unwrap_or_default(),
            ids.join(" "),
        ),
    }
}

/// The action and the detail of a move, from its listing
/// (`#1 [step 2] code.link src/queue.rs:10-36`).
fn move_columns(index: usize, m: &tiza_core::moves::Move) -> (String, String) {
    let line = tiza_core::protocol::describe_move(index, m);
    let line = match &m.stale {
        // The reason shows on the message line when the row is selected.
        Some(s) => line
            .strip_suffix(&format!(" (stale: {})", s.reason))
            .unwrap_or(&line)
            .to_string(),
        None => line,
    };
    let rest = line
        .split_once("] ")
        .map(|(_, rest)| rest.to_string())
        .unwrap_or(line);
    match rest.split_once(' ') {
        Some((action, detail)) => (action.to_string(), detail.to_string()),
        None => (rest, String::new()),
    }
}

/// Column widths: the widest text plus two blank cells, kept between `min`
/// and `max`.
fn width(texts: impl Iterator<Item = usize>, min: usize, max: usize) -> usize {
    (texts.max().unwrap_or(0) + 2).clamp(min, max)
}

/// The list as a panel, with its selectable rows in order. A filter keeps
/// the rows whose id, type, lines, label, file or step header holds it, in
/// any case, under their headers.
pub fn list(app: &App) -> (Panel, Vec<ListItem>) {
    let doc = &app.doc;
    let f = app.overlay_filter.to_lowercase();
    let matches =
        |texts: &[&str]| f.is_empty() || texts.iter().any(|t| t.to_lowercase().contains(&f));
    // Without moves, every step shows the document as it is.
    let view = (!doc.moves.is_empty()).then(|| doc.view_at(doc.step));
    // The main file as the document names it (`src/stack.rs`), as the
    // annotations of other files and the `code.link` moves name theirs.
    let main = match &doc.code_src {
        tiza_core::model::CodeSource::File { file, .. } => file.clone(),
        tiza_core::model::CodeSource::Embedded(_) => doc
            .source
            .path
            .as_deref()
            .map(|p| {
                std::path::Path::new(p)
                    .file_name()
                    .map_or(p.to_string(), |f| f.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "(stdin)".into()),
    };
    // File sub-headers only when the document shows more than one file.
    let files = doc.annotations.iter().any(|a| a.file.is_some())
        || doc
            .moves
            .iter()
            .any(|m| matches!(m.action, tiza_core::moves::MoveAction::CodeLink(_)));
    let entries: Vec<(&Annotation, (String, String, String))> = doc
        .annotations
        .iter()
        .map(|a| {
            // The lines as the step on screen shows them, when it does.
            let shown = view
                .as_ref()
                .and_then(|v| v.doc.find_annotation(&a.id))
                .unwrap_or(a);
            (a, columns(shown))
        })
        .collect();
    let id_w = width(entries.iter().map(|(a, _)| a.id.chars().count()), 4, 12);
    let type_w = width(entries.iter().map(|(_, c)| c.0.chars().count()), 12, 16);
    let lines_w = width(entries.iter().map(|(_, c)| c.1.chars().count()), 12, 20);
    let printable = |t: &str| tiza_core::printable::printable_line(t).into_owned();
    let mut rows = Vec::new();
    let mut items = Vec::new();
    // Step 0 shows only when it holds annotations; the step on screen
    // shows even past the last one (after its annotations are deleted).
    for step in 0..=doc.max_step().max(1).max(doc.step) {
        if step == 0 && !entries.iter().any(|(a, _)| a.step == 0) {
            continue;
        }
        let header = match doc.step_title(step) {
            Some(t) => format!("Step {step} · {}", printable(&t)),
            None => format!("Step {step}"),
        };
        let header = if step == doc.step {
            format!("{header} · current")
        } else {
            header
        };
        let in_step: Vec<_> = entries.iter().filter(|(a, _)| a.step == step).collect();
        let header_matches = !f.is_empty() && header.to_lowercase().contains(&f);
        let mut body = Vec::new();
        let mut groups: Vec<&str> = Vec::new();
        for (a, _) in &in_step {
            let file = a.file.as_deref().unwrap_or(&main);
            if !groups.contains(&file) {
                groups.push(file);
            }
        }
        for file in groups {
            let mut group = Vec::new();
            for (a, (kind, lines, label)) in &in_step {
                if a.file.as_deref().unwrap_or(&main) != file {
                    continue;
                }
                if !header_matches && !matches(&[&a.id, kind, lines, label, file]) {
                    continue;
                }
                let flag = view
                    .as_ref()
                    .is_some_and(|v| v.unanchored.contains(&a.id))
                    .then(|| ("! unanchored".to_string(), true));
                group.push(PanelRow::Item {
                    cols: vec![
                        (a.id.clone(), id_w),
                        (kind.clone(), type_w),
                        (lines.clone(), lines_w),
                    ],
                    label: tiza_core::printable::printable_line(label).into_owned(),
                    flag,
                    item: items.len(),
                });
                items.push(ListItem::Annotation {
                    id: a.id.clone(),
                    step,
                });
            }
            if !group.is_empty() {
                if files {
                    body.push(PanelRow::Sub(printable(file)));
                }
                body.extend(group);
            }
        }
        if body.is_empty() && !f.is_empty() {
            continue;
        }
        if !rows.is_empty() {
            rows.push(PanelRow::Blank);
        }
        rows.push(PanelRow::Section(header));
        if body.is_empty() {
            rows.push(PanelRow::Sub("no annotations".into()));
        }
        rows.extend(body);
    }
    // ---- moves ----
    let moves: Vec<(usize, &tiza_core::moves::Move, String, String)> = doc
        .moves
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let (action, detail) = move_columns(i + 1, m);
            (i + 1, m, action, detail)
        })
        .filter(|(i, m, action, detail)| {
            matches(&[
                &format!("#{i}"),
                &format!("step {}", m.at_step),
                action,
                detail,
            ])
        })
        .collect();
    if !moves.is_empty() {
        let action_w = width(moves.iter().map(|(_, _, a, _)| a.chars().count()), 11, 16);
        let num_w = width(moves.iter().map(|(i, ..)| format!("#{i}").len()), 4, 8);
        let step_w = width(
            moves
                .iter()
                .map(|(_, m, ..)| format!("step {}", m.at_step).len()),
            8,
            12,
        );
        if !rows.is_empty() {
            rows.push(PanelRow::Blank);
        }
        rows.push(PanelRow::Section("Moves".into()));
        for (i, m, action, detail) in moves {
            rows.push(PanelRow::Item {
                cols: vec![
                    (format!("#{i}"), num_w),
                    (format!("step {}", m.at_step), step_w),
                    (action, action_w),
                ],
                label: tiza_core::printable::printable_line(&detail).into_owned(),
                flag: m.stale.is_some().then(|| ("! stale".to_string(), true)),
                item: items.len(),
            });
            items.push(ListItem::Move {
                index: i,
                step: m.at_step,
            });
        }
    }
    // ---- empty states ----
    if items.is_empty() {
        rows.clear();
        if f.is_empty() {
            rows.push(PanelRow::Text("no annotations yet".into()));
            rows.push(PanelRow::Blank);
            rows.push(PanelRow::Text(
                "Select text with v, then press m, n, b or a.".into(),
            ));
        } else {
            rows.push(PanelRow::Text(format!(
                "no annotation or move matches «{}»",
                app.overlay_filter
            )));
            rows.push(PanelRow::Blank);
            rows.push(PanelRow::Text("Press Esc to clear the filter.".into()));
        }
    }
    let title = if doc.moves.is_empty() {
        format!("Annotations · {}", doc.annotations.len())
    } else {
        format!(
            "Annotations · {} · Moves · {}",
            doc.annotations.len(),
            doc.moves.len()
        )
    };
    let keys: Vec<String> = items.iter().map(ListItem::key).collect();
    let selected = selected(app, &keys);
    let panel = Panel {
        title,
        chip: chip(app),
        filter: app.overlay_filter.clone(),
        typing: app.overlay_typing,
        placeholder: "type to filter annotations and moves",
        rows,
        selected,
        footer: if app.overlay_typing {
            typing_footer()
        } else {
            vec![
                ("j/k", "select"),
                ("enter", "go"),
                ("x", "delete"),
                ("u", "undo"),
                ("U", "redo"),
            ]
        },
        position: Some(position(app, selected, items.len())),
    };
    (panel, items)
}

/// The agent changes that wait, as a panel; the selectable rows are the
/// changes, in order, by their place in the queue.
pub fn review(app: &App) -> (Panel, Vec<usize>) {
    let f = app.overlay_filter.to_lowercase();
    let mut rows = Vec::new();
    let mut items = Vec::new();
    let id_w = width(
        app.proposals.iter().map(|p| format!("p{}", p.id).len()),
        4,
        8,
    );
    for (i, p) in app.proposals.iter().enumerate() {
        if !f.is_empty() && !p.summary.to_lowercase().contains(&f) {
            continue;
        }
        rows.push(PanelRow::Item {
            cols: vec![(format!("p{}", p.id), id_w)],
            label: tiza_core::printable::printable_line(&p.summary).into_owned(),
            flag: Some((format!("{}s", p.since.elapsed().as_secs()), false)),
            item: items.len(),
        });
        items.push(i);
    }
    if items.is_empty() && !f.is_empty() {
        rows.push(PanelRow::Text(format!(
            "no agent change matches «{}»",
            app.overlay_filter
        )));
        rows.push(PanelRow::Blank);
        rows.push(PanelRow::Text("Press Esc to clear the filter.".into()));
    } else {
        rows.push(PanelRow::Blank);
        rows.push(PanelRow::Text(format!(
            "Tiza rejects an agent change {} s after it arrives.",
            crate::app::PROPOSAL_TIMEOUT.as_secs()
        )));
    }
    let keys: Vec<String> = items
        .iter()
        .map(|i| format!("p:{}", app.proposals[*i].id))
        .collect();
    let selected = selected(app, &keys);
    let panel = Panel {
        title: format!("Agent changes · {} waiting", app.proposals.len()),
        chip: if app.overlay_typing || !app.overlay_filter.is_empty() {
            ("esc", "clear")
        } else {
            ("esc", "hide")
        },
        filter: app.overlay_filter.clone(),
        typing: app.overlay_typing,
        placeholder: "type to filter",
        rows,
        selected,
        footer: if app.overlay_typing {
            typing_footer()
        } else {
            vec![
                ("y", "accept"),
                ("n", "reject"),
                ("a", "accept all"),
                ("d", "reject all"),
                ("j/k", "select"),
            ]
        },
        position: Some(position(app, selected, items.len())),
    };
    (panel, items)
}

fn chip(app: &App) -> (&'static str, &'static str) {
    if app.overlay_typing || !app.overlay_filter.is_empty() {
        ("esc", "clear")
    } else {
        ("esc", "close")
    }
}

fn typing_footer() -> Vec<(&'static str, &'static str)> {
    vec![
        ("", "type to filter"),
        ("↑/↓", "select"),
        ("enter", "keep"),
        ("esc", "clear"),
    ]
}

/// `3 of 13`, or `6 matches` with a filter.
fn position(app: &App, selected: Option<usize>, n: usize) -> String {
    if !app.overlay_filter.is_empty() {
        return match n {
            1 => "1 match".into(),
            n => format!("{n} matches"),
        };
    }
    match selected {
        Some(i) => format!("{} of {n}", i + 1),
        None => "0 of 0".into(),
    }
}
