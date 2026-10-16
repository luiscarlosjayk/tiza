//! Application state and logic, with no drawing dependencies. Both the
//! keyboard and the local API go through here, so an agent and the presenter
//! do exactly the same thing.

use std::path::PathBuf;

use codedraw_core::history::History;
use codedraw_core::layout::{Layout, LayoutOptions, layout};
use codedraw_core::model::{Annotation, BUILTIN_KINDS, Document, MarkStyle, Shape, ThemeName};
use codedraw_core::protocol::{
    AddParams, IdParams, METHODS, Mutated, Request, Response, Scene, Status, StepParams,
    UpdateParams, Where,
};
use codedraw_core::theme::Theme;
use codedraw_core::{Error, fs};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Command(String),
    /// Visual selection: `anchor` is where `v`/`V` was pressed; the cursor is the other end.
    Select {
        anchor_line: u32,
        anchor_col: u32,
        linewise: bool,
    },
}

/// What the last selection was, so a command typed right after can use it as its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Span(codedraw_core::TextSpan),
    Lines(u32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    List,
}

pub struct App {
    pub doc: Document,
    pub save_path: PathBuf,
    pub history: History,
    pub theme: Theme,
    pub mode: Mode,
    pub overlay: Overlay,
    pub present: bool,
    pub focus: Option<(u32, u32)>,
    pub cursor_line: u32,
    /// Cursor column, in graphemes (0-based).
    pub cursor_col: u32,
    /// Selection captured when a `v`/`V` action opened the command line; commands may use it.
    pub pending_sel: Option<Selection>,
    /// Command-line history (most recent last) and the index while browsing it.
    pub history_cmds: Vec<String>,
    pub history_pos: Option<usize>,
    /// First end of an arrow being built with `a` (select → a → select → a).
    pub arrow_from: Option<codedraw_core::Anchor>,
    /// Annotation selected with Tab (its id), for quick edits.
    pub selected: Option<String>,
    pub scroll: usize,
    pub message: Option<(String, bool)>,
    pub revision: u64,
    pub dirty: bool,
    pub quit: bool,
    pub size: (u16, u16),
    pub socket_path: Option<PathBuf>,
    pub autosave: bool,
}

impl App {
    pub fn new(doc: Document, save_path: PathBuf) -> Self {
        let theme = Theme::from_name(doc.theme.unwrap_or(ThemeName::Dark));
        let cursor_line = doc.source.first_line;
        Self {
            doc,
            save_path,
            history: History::default(),
            theme,
            mode: Mode::Normal,
            overlay: Overlay::None,
            present: false,
            focus: None,
            cursor_line,
            cursor_col: 0,
            pending_sel: None,
            history_cmds: Vec::new(),
            history_pos: None,
            arrow_from: None,
            selected: None,
            scroll: 0,
            message: None,
            revision: 1,
            dirty: false,
            quit: false,
            size: (80, 24),
            socket_path: None,
            autosave: true,
        }
    }

    pub fn info(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), false));
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), true));
    }

    pub fn layout(&self, width: usize) -> Layout {
        layout(
            &self.doc,
            &self.theme,
            &LayoutOptions {
                width,
                step: self.doc.step,
                focus: self.focus,
                line_numbers: true,
                tab_width: 4,
            },
        )
    }

    // ---------- mutations ----------

    fn touch(&mut self) {
        self.revision += 1;
        self.dirty = true;
        if self.autosave
            && let Err(e) = self.save()
        {
            self.error(format!("could not save: {e}"));
        }
    }

    pub fn save(&mut self) -> Result<(), Error> {
        self.doc.theme = Some(self.theme.name);
        fs::save_document(&self.doc, &self.save_path)?;
        self.dirty = false;
        Ok(())
    }

    pub fn add_annotation(
        &mut self,
        id: Option<String>,
        step: Option<u32>,
        shape: Shape,
    ) -> Result<String, Error> {
        self.add_annotation_colored(id, step, None, shape)
    }

    pub fn add_annotation_colored(
        &mut self,
        id: Option<String>,
        step: Option<u32>,
        color: Option<String>,
        shape: Shape,
    ) -> Result<String, Error> {
        if let Some(c) = &color
            && self.theme.named_color(c).is_none()
        {
            return Err(Error::Invalid(format!(
                "unknown color: {c} (use {} or #rrggbb)",
                codedraw_core::theme::PALETTE.join(", ")
            )));
        }
        self.history.record(&self.doc);
        let step = step.unwrap_or(self.doc.step).max(1);
        let res = self.doc.add(Annotation {
            id: id.unwrap_or_default(),
            step,
            color,
            shape,
        });
        match res {
            Ok(id) => {
                self.touch();
                Ok(id)
            }
            Err(e) => {
                self.history.undo(&mut self.doc);
                Err(e)
            }
        }
    }

    pub fn delete_annotation(&mut self, id: &str) -> Result<(), Error> {
        self.history.record(&self.doc);
        match self.doc.delete(id) {
            Ok(_) => {
                self.touch();
                Ok(())
            }
            Err(e) => {
                self.history.undo(&mut self.doc);
                Err(e)
            }
        }
    }

    /// Removes every annotation (or only those of one step). Undo with `u`.
    pub fn clear_annotations(&mut self, step: Option<u32>) -> usize {
        let before = self.doc.annotations.len();
        if before == 0 {
            return 0;
        }
        self.history.record(&self.doc);
        match step {
            Some(st) => self.doc.annotations.retain(|a| a.step != st),
            None => self.doc.annotations.clear(),
        }
        let removed = before - self.doc.annotations.len();
        if removed == 0 {
            self.history.undo(&mut self.doc);
            return 0;
        }
        if self.doc.step > self.doc.max_step() {
            self.doc.step = self.doc.max_step();
        }
        self.touch();
        removed
    }

    pub fn update_annotation(&mut self, p: &UpdateParams) -> Result<(), Error> {
        let mut ann = self
            .doc
            .find_annotation(&p.id)
            .cloned()
            .ok_or_else(|| Error::NotFound(p.id.clone()))?;
        if let Some(s) = p.step {
            ann.step = s.max(1);
        }
        if let Some(c) = &p.color {
            if c.trim().is_empty() {
                ann.color = None;
            } else if self.theme.named_color(c).is_some() {
                ann.color = Some(c.trim().to_string());
            } else {
                return Err(Error::Invalid(format!(
                    "unknown color: {c} (use {} or #rrggbb)",
                    codedraw_core::theme::PALETTE.join(", ")
                )));
            }
        }
        if let Some(spec) = &p.shape {
            ann.shape = spec.resolve(&self.doc)?;
        }
        if let Some(mv) = &p.mv {
            mv.apply(&mut ann.shape, &self.doc)?;
        }
        if let Some(l) = &p.label {
            let l = if l.trim().is_empty() {
                None
            } else {
                Some(l.clone())
            };
            match &mut ann.shape {
                Shape::Highlight { label, .. }
                | Shape::Mark { label, .. }
                | Shape::Box { label, .. }
                | Shape::Arrow { label, .. } => *label = l,
                Shape::Note { text, .. } => {
                    if let Some(l) = l {
                        *text = l;
                    }
                }
                Shape::Callout { title, .. } => *title = l,
                Shape::Spotlight { .. } => {}
            }
        }
        self.history.record(&self.doc);
        match self.doc.update(ann) {
            Ok(()) => {
                self.touch();
                Ok(())
            }
            Err(e) => {
                self.history.undo(&mut self.doc);
                Err(e)
            }
        }
    }

    /// Changes the current step. Step 0 is "code only": no annotation is drawn.
    pub fn set_step(&mut self, step: u32) {
        let max = self.doc.max_step();
        let s = step.min(max.max(1));
        if s != self.doc.step {
            self.doc.step = s;
            self.revision += 1;
        }
    }

    pub fn undo(&mut self) {
        if self.history.undo(&mut self.doc) {
            self.touch();
            self.info("undone");
        } else {
            self.info("nothing to undo");
        }
    }

    pub fn redo(&mut self) {
        if self.history.redo(&mut self.doc) {
            self.touch();
            self.info("redone");
        } else {
            self.info("nothing to redo");
        }
    }

    pub fn toggle_theme(&mut self) {
        self.theme = self.theme.toggled();
        self.doc.theme = Some(self.theme.name);
        self.revision += 1;
    }

    /// Replaces the code (paste, stdin). Only when there are no annotations that would be lost.
    pub fn replace_code(&mut self, text: &str) -> Result<(), Error> {
        if !self.doc.annotations.is_empty() {
            return Err(Error::Invalid(
                "the document already has annotations; open another file to paste new code".into(),
            ));
        }
        let lang = self.doc.source.language.clone();
        let mut d = Document::from_code(text, lang, None);
        d.theme = Some(self.theme.name);
        self.doc = d;
        self.cursor_line = self.doc.source.first_line;
        self.scroll = 0;
        self.touch();
        Ok(())
    }

    // ---------- navigation ----------

    pub fn move_cursor(&mut self, delta: i64) {
        let first = self.doc.source.first_line as i64;
        let last = self.doc.last_line() as i64;
        self.cursor_line = (self.cursor_line as i64 + delta).clamp(first, last) as u32;
    }

    fn clamp_col(&mut self) {
        let len = self.doc.line_len(self.cursor_line);
        self.cursor_col = self.cursor_col.min(len.saturating_sub(1));
    }

    pub fn move_col(&mut self, delta: i64) {
        let len = self.doc.line_len(self.cursor_line) as i64;
        self.cursor_col = (self.cursor_col as i64 + delta).clamp(0, (len - 1).max(0)) as u32;
    }

    pub fn line_start(&mut self) {
        let text = self.doc.line_text(self.cursor_line).unwrap_or("");
        self.cursor_col = text.chars().take_while(|c| c.is_whitespace()).count() as u32;
        self.clamp_col();
    }

    pub fn line_end(&mut self) {
        self.cursor_col = self.doc.line_len(self.cursor_line).saturating_sub(1);
    }

    /// Word classes for `w`/`b`/`e`: identifier chars, punctuation runs, whitespace.
    fn word_class(g: &str) -> u8 {
        let c = g.chars().next().unwrap_or(' ');
        if c.is_alphanumeric() || c == '_' {
            1
        } else if c.is_whitespace() {
            0
        } else {
            2
        }
    }

    fn graphemes_of(&self, line: u32) -> Vec<String> {
        use unicode_segmentation::UnicodeSegmentation;
        self.doc
            .line_text(line)
            .unwrap_or("")
            .graphemes(true)
            .map(str::to_string)
            .collect()
    }

    pub fn word_forward(&mut self) {
        let gs = self.graphemes_of(self.cursor_line);
        if gs.is_empty() {
            return;
        }
        let mut i = self.cursor_col as usize;
        let cls = Self::word_class(&gs[i.min(gs.len() - 1)]);
        while i < gs.len() && Self::word_class(&gs[i]) == cls {
            i += 1;
        }
        while i < gs.len() && Self::word_class(&gs[i]) == 0 {
            i += 1;
        }
        self.cursor_col = i.min(gs.len() - 1) as u32;
    }

    pub fn word_backward(&mut self) {
        let gs = self.graphemes_of(self.cursor_line);
        if gs.is_empty() || self.cursor_col == 0 {
            return;
        }
        let mut i = self.cursor_col as usize;
        i -= 1;
        while i > 0 && Self::word_class(&gs[i]) == 0 {
            i -= 1;
        }
        let cls = Self::word_class(&gs[i]);
        while i > 0 && Self::word_class(&gs[i - 1]) == cls {
            i -= 1;
        }
        self.cursor_col = i as u32;
    }

    pub fn word_end(&mut self) {
        let gs = self.graphemes_of(self.cursor_line);
        if gs.is_empty() {
            return;
        }
        let mut i = (self.cursor_col as usize + 1).min(gs.len() - 1);
        while i < gs.len() - 1 && Self::word_class(&gs[i]) == 0 {
            i += 1;
        }
        let cls = Self::word_class(&gs[i]);
        while i + 1 < gs.len() && Self::word_class(&gs[i + 1]) == cls {
            i += 1;
        }
        self.cursor_col = i as u32;
    }

    /// Selects the identifier under the cursor (`iw`).
    pub fn select_word(&mut self) {
        let gs = self.graphemes_of(self.cursor_line);
        if gs.is_empty() {
            return;
        }
        let i = (self.cursor_col as usize).min(gs.len() - 1);
        let cls = Self::word_class(&gs[i]);
        let mut a = i;
        while a > 0 && Self::word_class(&gs[a - 1]) == cls {
            a -= 1;
        }
        let mut b = i;
        while b + 1 < gs.len() && Self::word_class(&gs[b + 1]) == cls {
            b += 1;
        }
        self.mode = Mode::Select {
            anchor_line: self.cursor_line,
            anchor_col: a as u32,
            linewise: false,
        };
        self.cursor_col = b as u32;
    }

    pub fn start_select(&mut self, linewise: bool) {
        self.clamp_col();
        self.mode = Mode::Select {
            anchor_line: self.cursor_line,
            anchor_col: self.cursor_col,
            linewise,
        };
    }

    /// The current selection as a span (single line, end exclusive) or a line range.
    pub fn selection(&self) -> Option<Selection> {
        let Mode::Select {
            anchor_line,
            anchor_col,
            linewise,
        } = self.mode
        else {
            return None;
        };
        if linewise || anchor_line != self.cursor_line {
            let (a, b) = (
                anchor_line.min(self.cursor_line),
                anchor_line.max(self.cursor_line),
            );
            return Some(Selection::Lines(a, b));
        }
        let (a, b) = (
            anchor_col.min(self.cursor_col),
            anchor_col.max(self.cursor_col),
        );
        let len = self.doc.line_len(self.cursor_line);
        Some(Selection::Span(codedraw_core::TextSpan {
            line: self.cursor_line,
            start: a.min(len),
            end: (b + 1).min(len),
        }))
    }

    /// Leaves selection mode, remembering the selection for the next command.
    pub fn end_select(&mut self) -> Option<Selection> {
        let sel = self.selection();
        self.mode = Mode::Normal;
        self.pending_sel = sel;
        sel
    }

    /// Opens the command line pre-filled, with the selection as the pending anchor.
    pub fn command_with_selection(&mut self, prefix: &str) {
        self.end_select();
        self.mode = Mode::Command(prefix.to_string());
    }

    /// Applies an action key to the current selection. Returns false if the key is not one.
    pub fn select_action(&mut self, key: char) -> bool {
        let Some(sel) = self.selection() else {
            return false;
        };
        match (key, sel) {
            ('h', Selection::Span(span)) => {
                self.mode = Mode::Normal;
                match self.add_annotation(None, None, Shape::Highlight { span, label: None }) {
                    Ok(id) => self.info(format!("{id}: highlight (:label {id} text to name it)")),
                    Err(e) => self.error(e.to_string()),
                }
            }
            ('h', Selection::Lines(a, b)) => {
                self.mode = Mode::Normal;
                self.focus = Some((a, b));
                self.info(format!("focus {a}-{b} (Esc clears)"));
            }
            ('m', _) => self.command_with_selection("mark "),
            ('C', Selection::Lines(a, b)) => {
                self.end_select();
                self.mode = Mode::Command(format!("callout {a}-{b} "));
            }
            ('C', Selection::Span(sp)) => {
                self.end_select();
                self.mode = Mode::Command(format!("callout {} ", sp.line));
            }
            ('n', Selection::Span(sp)) => {
                self.end_select();
                self.mode = Mode::Command(format!("note {} ", sp.line));
            }
            ('n', Selection::Lines(a, _)) => {
                self.end_select();
                self.mode = Mode::Command(format!("note {a} "));
            }
            ('b', Selection::Lines(a, b)) => {
                self.end_select();
                self.mode = Mode::Command(format!("box {a}-{b} "));
            }
            ('b', Selection::Span(sp)) => {
                self.end_select();
                self.mode = Mode::Command(format!("box {}-{} ", sp.line, sp.line));
            }
            ('s', Selection::Lines(a, b)) => {
                self.mode = Mode::Normal;
                self.run_command(&format!("spot {a}-{b}"));
            }
            ('s', Selection::Span(sp)) => {
                self.mode = Mode::Normal;
                self.run_command(&format!("spot {}-{}", sp.line, sp.line));
            }
            ('a', Selection::Span(sp)) => {
                self.mode = Mode::Normal;
                let anchor = codedraw_core::Anchor {
                    line: sp.line,
                    col: sp.start,
                };
                match self.arrow_from.take() {
                    None => {
                        self.arrow_from = Some(anchor);
                        self.info("arrow: now select the target and press a (Esc cancels)");
                    }
                    Some(from) => {
                        self.pending_sel = None;
                        match self.add_annotation(
                            None,
                            None,
                            Shape::Arrow {
                                from,
                                to: anchor,
                                label: None,
                            },
                        ) {
                            Ok(id) => {
                                self.info(format!("{id}: arrow (:label {id} text to name it)"))
                            }
                            Err(e) => self.error(e.to_string()),
                        }
                    }
                }
            }
            _ => return false,
        }
        true
    }

    /// Visible annotations in reading order (first line, then id).
    fn visible_ids(&self) -> Vec<String> {
        let mut v: Vec<(u32, String)> = self
            .doc
            .annotations
            .iter()
            .filter(|a| a.step <= self.doc.step.max(1))
            .map(|a| {
                let first = a.shape.line_ranges().first().map(|r| r.0).unwrap_or(0);
                (first, a.id.clone())
            })
            .collect();
        v.sort();
        v.into_iter().map(|(_, id)| id).collect()
    }

    /// Tab / Shift-Tab: select the next/previous visible annotation and move the cursor to it.
    pub fn select_annotation(&mut self, dir: i32) {
        let ids = self.visible_ids();
        if ids.is_empty() {
            self.selected = None;
            self.info("no annotations in this step");
            return;
        }
        let cur = self
            .selected
            .as_ref()
            .and_then(|s| ids.iter().position(|i| i == s));
        let next = match (cur, dir) {
            (None, d) if d >= 0 => 0,
            (None, _) => ids.len() - 1,
            (Some(i), d) => ((i as i64 + d as i64).rem_euclid(ids.len() as i64)) as usize,
        };
        let id = ids[next].clone();
        if let Some(a) = self.doc.find_annotation(&id)
            && let Some((line, _)) = a.shape.line_ranges().first()
            && self.doc.has_line(*line)
        {
            self.cursor_line = *line;
        }
        self.selected = Some(id);
    }

    /// Describes the selected annotation for the status bar.
    pub fn selected_text(&self) -> Option<String> {
        let id = self.selected.as_ref()?;
        let a = self.doc.find_annotation(id)?;
        Some(codedraw_core::protocol::describe(a))
    }

    /// Quick edits on the selected annotation. Returns false if `key` is not one.
    pub fn selected_action(&mut self, key: char) -> bool {
        let Some(id) = self.selected.clone() else {
            return false;
        };
        let Some(a) = self.doc.find_annotation(&id).cloned() else {
            self.selected = None;
            return false;
        };
        match key {
            '\n' => {
                let current = match &a.shape {
                    Shape::Highlight { label, .. }
                    | Shape::Mark { label, .. }
                    | Shape::Box { label, .. }
                    | Shape::Arrow { label, .. } => label.clone().unwrap_or_default(),
                    Shape::Note { text, .. } => text.clone(),
                    Shape::Callout { title, .. } => title.clone().unwrap_or_default(),
                    Shape::Spotlight { .. } => String::new(),
                };
                self.mode = Mode::Command(format!("label {id} {current}"));
            }
            'x' => {
                match self.delete_annotation(&id) {
                    Ok(()) => self.info(format!("{id} deleted (u to undo)")),
                    Err(e) => self.error(e.to_string()),
                }
                self.selected = None;
            }
            'c' => self.mode = Mode::Command(format!("color {id} ")),
            '+' | '-' => {
                let step = if key == '+' {
                    a.step + 1
                } else {
                    a.step.saturating_sub(1).max(1)
                };
                match self.update_annotation(&UpdateParams {
                    id: id.clone(),
                    step: Some(step),
                    color: None,
                    label: None,
                    shape: None,
                    mv: None,
                }) {
                    Ok(()) => self.info(format!("{id} → step {step}")),
                    Err(e) => self.error(e.to_string()),
                }
            }
            _ => return false,
        }
        true
    }

    /// Every `:` command name, for completion and help.
    pub const COMMANDS: &'static [&'static str] = &[
        "arrow", "autosave", "box", "callout", "clear", "color", "del", "embed", "focus", "help",
        "hl", "label", "lang", "link", "list", "mark", "next", "note", "pad", "present", "prev",
        "q", "redo", "spot", "step", "style", "theme", "title", "undo", "until", "w", "wq",
    ];

    /// Tab completion for the command line: the command name on the first word, then
    /// mark kinds after `mark`/`style`, annotation ids after `del`/`label`/`color`/`step`/`until`/`spot`,
    /// and `dark|light` after `theme`. Returns the completed input, or the candidates when ambiguous.
    pub fn complete(&self, input: &str) -> (String, Vec<String>) {
        let trailing_space = input.ends_with(' ');
        let words: Vec<&str> = input.split_whitespace().collect();
        let (prefix, candidates): (&str, Vec<String>) = match (words.len(), trailing_space) {
            (0, _) | (1, false) => (
                words.first().copied().unwrap_or(""),
                Self::COMMANDS.iter().map(|c| c.to_string()).collect(),
            ),
            _ => {
                let cmd = words[0];
                let cur = if trailing_space {
                    ""
                } else {
                    words[words.len() - 1]
                };
                let pool: Vec<String> = match cmd {
                    "mark" | "m" if words.len() <= 2 => self.doc.kind_names(),
                    "style" | "kinds" => self.doc.styles.keys().cloned().collect(),
                    "del" | "delete" | "rm" | "label" | "color" | "col" | "until" | "spot"
                    | "spotlight" => self.doc.annotations.iter().map(|a| a.id.clone()).collect(),
                    "step" | "s" if words.len() <= 2 => {
                        self.doc.annotations.iter().map(|a| a.id.clone()).collect()
                    }
                    "theme" | "t" => vec!["dark".into(), "light".into()],
                    "focus" | "f" => vec![],
                    _ => vec![],
                };
                (cur, pool)
            }
        };
        let matches: Vec<String> = candidates
            .into_iter()
            .filter(|c| c.starts_with(prefix))
            .collect();
        match matches.as_slice() {
            [] => (input.to_string(), vec![]),
            [one] => {
                let base = if prefix.is_empty() {
                    input
                } else {
                    &input[..input.len() - prefix.len()]
                };
                (format!("{base}{one} "), vec![])
            }
            many => {
                // Longest common prefix.
                let mut lcp = many[0].clone();
                for m in &many[1..] {
                    while !m.starts_with(&lcp) {
                        lcp.pop();
                    }
                }
                let base = if prefix.is_empty() {
                    input
                } else {
                    &input[..input.len() - prefix.len()]
                };
                (format!("{base}{lcp}"), many.to_vec())
            }
        }
    }

    pub fn history_step(&mut self, dir: i32) -> Option<String> {
        if self.history_cmds.is_empty() {
            return None;
        }
        let n = self.history_cmds.len() as i64;
        let next = match self.history_pos {
            None if dir < 0 => n - 1,
            None => return None,
            Some(p) => (p as i64 + dir as i64).clamp(-1, n),
        };
        if next < 0 || next >= n {
            self.history_pos = None;
            return Some(String::new());
        }
        self.history_pos = Some(next as usize);
        Some(self.history_cmds[next as usize].clone())
    }

    // ---------- ':' commands ----------

    pub fn run_command(&mut self, input: &str) {
        let input = input.trim();
        if input.is_empty() {
            return;
        }
        if self.history_cmds.last().map(String::as_str) != Some(input) {
            self.history_cmds.push(input.to_string());
            if self.history_cmds.len() > 100 {
                self.history_cmds.remove(0);
            }
        }
        self.history_pos = None;
        let (cmd, rest) = match input.split_once(char::is_whitespace) {
            Some((c, r)) => (c, r.trim()),
            None => (input, ""),
        };
        let mut exec = || -> Result<String, Error> {
            match cmd {
                "q" | "quit" => {
                    self.quit = true;
                    Ok(String::new())
                }
                "w" | "write" => {
                    if !rest.is_empty() {
                        self.save_path = PathBuf::from(rest);
                    }
                    self.save()
                        .map(|_| format!("saved to {}", self.save_path.display()))
                }
                "wq" => {
                    self.quit = true;
                    self.save().map(|_| String::new())
                }
                "box" => self.cmd_box(rest),
                "arrow" | "ar" => self.cmd_arrow(rest),
                "hl" | "highlight" => self.cmd_highlight(rest),
                "mark" | "m" => self.cmd_mark(rest),
                "style" | "kinds" => self.cmd_style(rest),
                "note" | "n" => self.cmd_note(rest),
                "embed" => {
                    self.doc.embed_code();
                    self.touch();
                    Ok("code frozen inside the .codedraw".into())
                }
                "link" => {
                    let (file, range) = if rest.is_empty() {
                        let Some(f) = self.doc.source.path.clone() else {
                            return Err(Error::Invalid(
                                "usage: :link path[:A-B]   (without a path, uses the source file)"
                                    .into(),
                            ));
                        };
                        let r = (self.doc.source.first_line > 1)
                            .then(|| (self.doc.source.first_line, self.doc.last_line()));
                        (f, r)
                    } else {
                        let (p, r) = fs::parse_target(rest);
                        (p.to_string_lossy().into_owned(), r)
                    };
                    self.doc.reference_file(file.clone(), range);
                    self.touch();
                    Ok(format!("the .codedraw now references {file}"))
                }
                "spot" | "spotlight" => self.cmd_spot(rest),
                "callout" | "co" => self.cmd_callout(rest),
                "until" => {
                    let (id, n) = split_first(rest);
                    let n: u32 = n.parse().map_err(|_| {
                        Error::Invalid(
                            "usage: :until ID N   (last step in which the spotlight stays active)"
                                .into(),
                        )
                    })?;
                    let mut ann = self
                        .doc
                        .find_annotation(id)
                        .cloned()
                        .ok_or_else(|| Error::NotFound(id.to_string()))?;
                    let Shape::Spotlight { until, .. } = &mut ann.shape else {
                        return Err(Error::Invalid(format!("{id} is not a spotlight")));
                    };
                    *until = Some(n.max(ann.step));
                    self.history.record(&self.doc);
                    self.doc.update(ann)?;
                    self.touch();
                    Ok(format!("{id} active until step {n}"))
                }
                "clear" => {
                    let step = match rest.trim() {
                        "" => None,
                        "." => Some(self.doc.step),
                        n => Some(n.parse::<u32>().map_err(|_| {
                            Error::Invalid("usage: :clear  |  :clear N  |  :clear .  (all, one step, the current step)".into())
                        })?),
                    };
                    let n = self.clear_annotations(step);
                    Ok(if n == 0 {
                        "nothing to clear".into()
                    } else {
                        format!("{n} annotation(s) cleared (u to undo)")
                    })
                }
                "del" | "delete" | "rm" => {
                    let mut n = 0;
                    for id in rest.split_whitespace() {
                        self.delete_annotation(id)?;
                        n += 1;
                    }
                    if n == 0 {
                        Err(Error::Invalid("usage: :del ID [ID…]".into()))
                    } else {
                        Ok(format!("{n} deleted"))
                    }
                }
                "color" | "col" => {
                    let (id, c) = split_first(rest);
                    if id.is_empty() {
                        return Err(Error::Invalid(format!(
                            "usage: :color ID name|#rrggbb|auto   (names: {})",
                            codedraw_core::theme::PALETTE.join(", ")
                        )));
                    }
                    let c = if c == "auto" { "" } else { c };
                    self.update_annotation(&UpdateParams {
                        id: id.into(),
                        step: None,
                        color: Some(c.into()),
                        label: None,
                        shape: None,
                        mv: None,
                    })
                    .map(|_| format!("{id}: color {}", if c.is_empty() { "auto" } else { c }))
                }
                "label" => {
                    let (id, text) = rest
                        .split_once(char::is_whitespace)
                        .ok_or_else(|| Error::Invalid("usage: :label ID text".into()))?;
                    self.update_annotation(&UpdateParams {
                        id: id.into(),
                        step: None,
                        color: None,
                        label: Some(text.trim().into()),
                        shape: None,
                        mv: None,
                    })
                    .map(|_| format!("{id} updated"))
                }
                "step" | "s" => {
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    match parts.as_slice() {
                        [n] => {
                            let n: u32 = n.parse().map_err(|_| {
                                Error::Invalid("usage: :step N  |  :step ID N".into())
                            })?;
                            self.set_step(n);
                            Ok(format!("step {}/{}", self.doc.step, self.doc.max_step()))
                        }
                        [id, n] => {
                            let n: u32 = n
                                .parse()
                                .map_err(|_| Error::Invalid("usage: :step ID N".into()))?;
                            self.update_annotation(&UpdateParams {
                                id: id.to_string(),
                                step: Some(n),
                                color: None,
                                label: None,
                                shape: None,
                                mv: None,
                            })
                            .map(|_| format!("{id} now appears at step {n}"))
                        }
                        _ => Err(Error::Invalid("usage: :step N  |  :step ID N".into())),
                    }
                }
                "next" => {
                    self.set_step(self.doc.step + 1);
                    Ok(format!("step {}/{}", self.doc.step, self.doc.max_step()))
                }
                "prev" => {
                    self.set_step(self.doc.step.saturating_sub(1));
                    Ok(format!("step {}/{}", self.doc.step, self.doc.max_step()))
                }
                "focus" | "f" => {
                    if rest.is_empty() {
                        self.focus = None;
                        Ok("focus cleared".into())
                    } else {
                        let (a, b) = parse_range(rest)?;
                        self.focus = Some((a, b));
                        Ok(format!("focus on {a}-{b}"))
                    }
                }
                "theme" | "t" => {
                    match rest {
                        "dark" | "oscuro" => self.theme = Theme::dark(),
                        "light" | "claro" => self.theme = Theme::light(),
                        _ => self.theme = self.theme.toggled(),
                    }
                    self.doc.theme = Some(self.theme.name);
                    self.revision += 1;
                    Ok(format!("theme {}", theme_label(self.theme.name)))
                }
                "present" | "p" => {
                    self.present = !self.present;
                    Ok(if self.present {
                        "presentation mode (P to exit)".into()
                    } else {
                        String::new()
                    })
                }
                "lang" => {
                    if rest.is_empty() {
                        Ok(format!("language: {}", self.doc.source.language))
                    } else {
                        self.doc.source.language =
                            codedraw_core::highlight::normalize_language(rest);
                        self.touch();
                        Ok(format!("language: {}", self.doc.source.language))
                    }
                }
                "title" => {
                    self.doc.title = if rest.is_empty() {
                        None
                    } else {
                        Some(rest.into())
                    };
                    self.touch();
                    Ok("title updated".into())
                }
                "ls" | "list" => {
                    self.overlay = Overlay::List;
                    Ok(String::new())
                }
                "help" | "h" | "?" => {
                    self.overlay = Overlay::Help;
                    Ok(String::new())
                }
                "undo" | "u" => {
                    self.undo();
                    Ok(String::new())
                }
                "redo" => {
                    self.redo();
                    Ok(String::new())
                }
                "pad" | "padding" if matches!(rest.trim(), "default" | "reset") => {
                    self.doc.padding = None;
                    self.touch();
                    let (v, h) = crate::ui::DEFAULT_PADDING;
                    Ok(format!("padding {v} {h} (default)"))
                }
                "pad" | "padding" => {
                    let nums: Vec<u16> = rest
                        .split([' ', ','])
                        .filter(|t| !t.is_empty())
                        .map(|t| t.parse::<u16>())
                        .collect::<Result<_, _>>()
                        .map_err(|_| Error::Invalid("usage: :pad V [H] | :pad default   (blank cells around the drawing; :pad 0 0 removes it)".into()))?;
                    let (v, h) = match nums.as_slice() {
                        [] => {
                            let (v, h) = self.doc.padding.unwrap_or(crate::ui::DEFAULT_PADDING);
                            return Ok(format!(
                                "padding {v} {h}{}",
                                if self.doc.padding.is_none() {
                                    " (default; :pad V H to set, :pad default to reset)"
                                } else {
                                    ""
                                }
                            ));
                        }
                        [v] => (*v, *v * 2),
                        [v, h, ..] => (*v, *h),
                    };
                    // `:pad 0 0` is a real value (no padding); `:pad default` returns to the theme default.
                    self.doc.padding = Some((v, h));
                    self.touch();
                    Ok(format!("padding {v} {h}"))
                }
                "autosave" => {
                    self.autosave = !matches!(rest, "off" | "no");
                    Ok(format!(
                        "autosave {}",
                        if self.autosave { "on" } else { "off" }
                    ))
                }
                other => Err(Error::Invalid(format!(
                    "unknown command: {other} (try :help)"
                ))),
            }
        };
        let result = exec();
        match result {
            Ok(msg) if !msg.is_empty() => self.info(msg),
            Ok(_) => {}
            Err(e) => self.error(e.to_string()),
        }
    }

    fn cmd_box(&mut self, rest: &str) -> Result<String, Error> {
        let (range, label) = split_first(rest);
        let (a, b) =
            parse_range(range).map_err(|_| Error::Invalid("usage: :box A-B [label]".into()))?;
        let id = self.add_annotation(
            None,
            None,
            Shape::Box {
                from_line: a,
                to_line: b,
                label: opt(label),
            },
        )?;
        Ok(format!("{id}: box {a}-{b}"))
    }

    fn cmd_arrow(&mut self, rest: &str) -> Result<String, Error> {
        let (from, rest) = split_first(rest);
        let (to, label) = split_first(rest);
        if from.is_empty() || to.is_empty() {
            return Err(Error::Invalid(
                "usage: :arrow L:text L:text [label]   (e.g. :arrow 2:s 7:takes move)".into(),
            ));
        }
        let f = parse_where(from, self.cursor_line)?.resolve_anchor(&self.doc)?;
        let t = parse_where(to, self.cursor_line)?.resolve_anchor(&self.doc)?;
        let id = self.add_annotation(
            None,
            None,
            Shape::Arrow {
                from: f,
                to: t,
                label: opt(label),
            },
        )?;
        Ok(format!("{id}: arrow L{} → L{}", f.line, t.line))
    }

    fn cmd_highlight(&mut self, rest: &str) -> Result<String, Error> {
        let (at, label) = split_first(rest);
        let (span, label) = match (self.pending_sel.take(), looks_like_anchor(at)) {
            (Some(Selection::Span(sp)), false) => (sp, rest.trim()),
            _ => {
                if at.is_empty() {
                    return Err(Error::Invalid("usage: :hl L:text [label]".into()));
                }
                (
                    parse_where(at, self.cursor_line)?.resolve_span(&self.doc)?,
                    label,
                )
            }
        };
        let id = self.add_annotation(
            None,
            None,
            Shape::Highlight {
                span,
                label: opt(label),
            },
        )?;
        Ok(format!("{id}: highlight at L{}", span.line))
    }

    /// `:mark KIND L:text [label]` where KIND is a built-in kind (`warn`, `read`, `move`…),
    /// an alias (`borrow`), a style defined with `:style`, or a bare glyph/emoji for a
    /// one-off inline style (`:mark 🔥 7:&t hot path`). `KIND@COLOR` sets the color.
    fn cmd_mark(&mut self, rest: &str) -> Result<String, Error> {
        let (kind_tok, rest) = split_first(rest);
        let (at, label) = split_first(rest);
        if kind_tok.is_empty() || (at.is_empty() && self.pending_sel.is_none()) {
            return Err(Error::Invalid(format!(
                "usage: :mark KIND[@COLOR] L:text [label]   KIND: {} | a :style name | a glyph",
                self.doc.kind_names().join(" ")
            )));
        }
        let (kind_tok, color) = match kind_tok.rsplit_once('@') {
            Some((k, c)) if !k.is_empty() => (k, Some(c.to_string())),
            _ => (kind_tok, None),
        };
        let known = self.doc.styles.contains_key(kind_tok)
            || codedraw_core::model::builtin_kind(kind_tok).is_some();
        let (kind, mut style) = if known {
            (kind_tok.to_string(), MarkStyle::default())
        } else {
            // Not a known kind: treat it as a glyph for an inline style.
            let w = codedraw_core::canvas::display_width(kind_tok, 4);
            if w == 0
                || w > 2
                || kind_tok
                    .chars()
                    .any(|c| c.is_alphanumeric() && c.is_ascii())
            {
                return Err(Error::Invalid(format!(
                    "unknown mark kind: {kind_tok} (kinds: {}; or define one with :style, or use a glyph/emoji)",
                    self.doc.kind_names().join(" ")
                )));
            }
            (
                kind_tok.to_string(),
                MarkStyle {
                    glyph: Some(kind_tok.to_string()),
                    legend: opt(label),
                    color: None,
                },
            )
        };
        if let Some(c) = color {
            if self.theme.named_color(&c).is_none() {
                return Err(Error::Invalid(format!("unknown color: {c}")));
            }
            style.color = Some(c);
        }
        let inline = !known;
        let (span, label) = match (self.pending_sel.take(), looks_like_anchor(at)) {
            (Some(Selection::Span(sp)), false) => (sp, rest.trim()),
            _ => (
                parse_where(at, self.cursor_line)?.resolve_span(&self.doc)?,
                label,
            ),
        };
        if inline {
            style.legend = opt(label);
        }
        let legend = self
            .doc
            .resolve_style(&kind, &style)
            .and_then(|st| st.legend)
            .unwrap_or_else(|| kind.clone());
        let id = self.add_annotation(
            None,
            None,
            Shape::Mark {
                kind,
                span,
                label: if inline { None } else { opt(label) },
                style,
            },
        )?;
        Ok(format!("{id}: {legend} at L{}", span.line))
    }

    /// `:style NAME glyph=🔥 legend="hot path" color=red` defines a reusable style.
    /// `:style NAME` removes it; `:style` lists styles and built-in kinds.
    fn cmd_style(&mut self, rest: &str) -> Result<String, Error> {
        let (name, rest) = split_first(rest);
        if name.is_empty() {
            let mut out: Vec<String> = self
                .doc
                .styles
                .iter()
                .map(|(n, st)| {
                    format!(
                        "{n} = {} {}{}",
                        st.glyph.as_deref().unwrap_or(""),
                        st.legend.as_deref().unwrap_or(""),
                        st.color
                            .as_ref()
                            .map(|c| format!(" @{c}"))
                            .unwrap_or_default()
                    )
                })
                .collect();
            out.push(format!(
                "built-in: {}",
                BUILTIN_KINDS
                    .iter()
                    .map(|(k, g, ..)| format!("{k} {g}"))
                    .collect::<Vec<_>>()
                    .join("  ")
            ));
            return Ok(out.join(" | "));
        }
        if rest.is_empty() {
            return match self.doc.styles.remove(name) {
                Some(_) => {
                    self.touch();
                    Ok(format!("style {name} removed"))
                }
                None => Err(Error::Invalid(
                    "usage: :style NAME glyph=G legend=\"text\" color=C   (:style NAME removes it)"
                        .into(),
                )),
            };
        }
        let mut st = self.doc.styles.get(name).cloned().unwrap_or_default();
        for (k, v) in parse_kv(rest) {
            match k.as_str() {
                "glyph" | "g" => st.glyph = Some(v),
                "legend" | "l" | "text" => st.legend = Some(v),
                "color" | "c" => {
                    if self.theme.named_color(&v).is_none() {
                        return Err(Error::Invalid(format!("unknown color: {v}")));
                    }
                    st.color = Some(v);
                }
                other => {
                    return Err(Error::Invalid(format!(
                        "unknown style field: {other} (glyph, legend, color)"
                    )));
                }
            }
        }
        if st.glyph.is_none() && codedraw_core::model::builtin_style(name).is_none() {
            return Err(Error::Invalid("a style needs at least a glyph".into()));
        }
        self.doc.styles.insert(name.to_string(), st);
        self.touch();
        Ok(format!("style {name} defined (use it with :mark {name} …)"))
    }

    /// `:spot 7-12`, `:spot a2 a3`, `:spot 7-12 a2 a3`. Active only in the current step
    /// (`:until ID N` extends it).
    fn cmd_spot(&mut self, rest: &str) -> Result<String, Error> {
        let mut range: Option<(u32, u32)> = None;
        let mut ids = Vec::new();
        for tok in rest.split_whitespace() {
            if tok.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                range = Some(parse_range(tok)?);
            } else {
                ids.push(tok.to_string());
            }
        }
        if range.is_none() && ids.is_empty() {
            return Err(Error::Invalid(
                "usage: :spot A-B [ID…]  |  :spot ID…".into(),
            ));
        }
        let id = self.add_annotation(
            None,
            None,
            Shape::Spotlight {
                from_line: range.map(|r| r.0),
                to_line: range.map(|r| r.1),
                ids,
                until: None,
            },
        )?;
        Ok(format!("{id}: spotlight at step {}", self.doc.step.max(1)))
    }

    /// `:callout A[-B] [@below|@margin|@auto] [title="…"] text…`
    fn cmd_callout(&mut self, rest: &str) -> Result<String, Error> {
        let (range, mut rest) = split_first(rest);
        let (a, b) = if range == "." {
            (self.cursor_line, self.cursor_line)
        } else {
            parse_range(range).map_err(|_| {
                Error::Invalid(
                    "usage: :callout A[-B] [@below|@margin|@auto] [title=\"…\"] text".into(),
                )
            })?
        };
        let mut placement = codedraw_core::model::Placement::Margin;
        let mut title = None;
        loop {
            let (tok, after) = split_first(rest);
            if let Some(pl) = tok
                .strip_prefix('@')
                .and_then(codedraw_core::model::Placement::parse)
            {
                placement = pl;
                rest = after;
            } else if tok.starts_with("title=") {
                let kv = parse_kv(rest);
                let (_, v) = kv.first().cloned().unwrap_or_default();
                title = opt(&v);
                // consume `title=…` (quoted or not) from rest
                let consumed = if rest.trim_start().starts_with("title=\"") {
                    rest.trim_start()
                        .find("\" ")
                        .map(|i| i + 2)
                        .unwrap_or(rest.len())
                } else {
                    rest.trim_start().find(' ').unwrap_or(rest.len())
                };
                rest = rest.trim_start()[consumed.min(rest.trim_start().len())..].trim_start();
            } else {
                break;
            }
        }
        if rest.trim().is_empty() {
            return Err(Error::Invalid("the callout text is empty".into()));
        }
        let id = self.add_annotation(
            None,
            None,
            Shape::Callout {
                from_line: a,
                to_line: if b == a { None } else { Some(b) },
                text: rest.trim().to_string(),
                title,
                placement,
                width: None,
            },
        )?;
        Ok(format!("{id}: callout at L{a}-{b}"))
    }

    fn cmd_note(&mut self, rest: &str) -> Result<String, Error> {
        let (line, text) = split_first(rest);
        let line: u32 = if line == "." {
            self.cursor_line
        } else {
            line.parse().map_err(|_| {
                Error::Invalid("usage: :note L text  (L may be . for the cursor line)".into())
            })?
        };
        if text.is_empty() {
            return Err(Error::Invalid("the note is empty".into()));
        }
        let id = self.add_annotation(
            None,
            None,
            Shape::Note {
                line,
                text: text.into(),
            },
        )?;
        Ok(format!("{id}: note at L{line}"))
    }

    // ---------- local API ----------

    pub fn handle_request(&mut self, req: Request) -> Response {
        let id = req.id.clone();
        match self.dispatch(&req.method, req.params) {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::err(id, e.code(), e.to_string()),
        }
    }

    fn dispatch(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, Error> {
            serde_json::from_value(v)
                .map_err(|e| Error::Invalid(format!("invalid parameters: {e}")))
        }
        Ok(match method {
            "ping" => {
                json!({"type": "pong", "protocol": codedraw_core::protocol::PROTOCOL_VERSION})
            }
            "status" => serde_json::to_value(self.status())?,
            "api.methods" => json!(
                METHODS
                    .iter()
                    .map(|(m, d)| json!({"method": m, "description": d}))
                    .collect::<Vec<_>>()
            ),
            "code.get" => {
                let mut out = String::new();
                for line in self.doc.source.first_line..=self.doc.last_line() {
                    out.push_str(&format!(
                        "{line:>4}  {}\n",
                        self.doc.line_text(line).unwrap_or("")
                    ));
                }
                json!({"language": self.doc.source.language, "first_line": self.doc.source.first_line, "last_line": self.doc.last_line(), "text": out})
            }
            "scene.get" => {
                // On the wire the code travels embedded: a client in another
                // directory could not resolve a relative file reference.
                let mut document = self.doc.clone();
                document.code_src =
                    codedraw_core::model::CodeSource::Embedded(document.code.clone());
                serde_json::to_value(Scene {
                    revision: self.revision,
                    step: self.doc.step,
                    max_step: self.doc.max_step(),
                    document,
                })?
            }
            "scene.list" => json!(
                self.doc
                    .annotations
                    .iter()
                    .map(codedraw_core::protocol::describe)
                    .collect::<Vec<_>>()
            ),
            "annotation.add" => {
                let p: AddParams = parse(params)?;
                let shape = p.shape.resolve(&self.doc)?;
                let id = self.add_annotation_colored(p.id, p.step, p.color, shape)?;
                serde_json::to_value(Mutated {
                    id,
                    revision: self.revision,
                })?
            }
            "annotation.update" => {
                let p: UpdateParams = parse(params)?;
                self.update_annotation(&p)?;
                serde_json::to_value(Mutated {
                    id: p.id,
                    revision: self.revision,
                })?
            }
            "style.set" => {
                let name = params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .ok_or_else(|| Error::Invalid("style.set needs `name`".into()))?;
                let get = |k: &str| {
                    params
                        .get(k)
                        .and_then(|v| v.as_str())
                        .map(|v| v.to_string())
                };
                let mut st = self.doc.styles.get(name).cloned().unwrap_or_default();
                if let Some(g) = get("glyph") {
                    st.glyph = Some(g);
                }
                if let Some(l) = get("legend") {
                    st.legend = Some(l);
                }
                if let Some(c) = get("color") {
                    if self.theme.named_color(&c).is_none() {
                        return Err(Error::Invalid(format!("unknown color: {c}")));
                    }
                    st.color = Some(c);
                }
                if st.glyph.is_none() && codedraw_core::model::builtin_style(name).is_none() {
                    return Err(Error::Invalid("a style needs at least a glyph".into()));
                }
                self.doc.styles.insert(name.to_string(), st.clone());
                self.touch();
                json!({"name": name, "style": st, "revision": self.revision})
            }
            "style.list" => json!({
                "document": self.doc.styles,
                "builtin": BUILTIN_KINDS.iter().map(|(k, g, l, c)| json!({"kind": k, "glyph": g, "legend": l, "color": c})).collect::<Vec<_>>(),
                "aliases": codedraw_core::model::KIND_ALIASES.iter().map(|(a, k)| json!({"alias": a, "kind": k})).collect::<Vec<_>>(),
            }),
            "scene.clear" => {
                let step = params
                    .get("step")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let removed = self.clear_annotations(step);
                json!({"removed": removed, "remaining": self.doc.annotations.len(), "revision": self.revision})
            }
            "annotation.delete" => {
                let p: IdParams = parse(params)?;
                self.delete_annotation(&p.id)?;
                serde_json::to_value(Mutated {
                    id: p.id,
                    revision: self.revision,
                })?
            }
            "step.set" => {
                let p: StepParams = parse(params)?;
                self.set_step(p.step);
                json!({"step": self.doc.step, "max_step": self.doc.max_step(), "revision": self.revision})
            }
            "step.next" => {
                self.set_step(self.doc.step + 1);
                json!({"step": self.doc.step, "max_step": self.doc.max_step(), "revision": self.revision})
            }
            "step.prev" => {
                self.set_step(self.doc.step.saturating_sub(1));
                json!({"step": self.doc.step, "max_step": self.doc.max_step(), "revision": self.revision})
            }
            "render.text" => {
                let width = params
                    .get("width")
                    .and_then(|w| w.as_u64())
                    .map(|w| w as usize)
                    .unwrap_or(self.size.0 as usize);
                json!({"text": self.layout(width).canvas.to_plain_text()})
            }
            "focus.set" => {
                let a = params
                    .get("from_line")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let b = params
                    .get("to_line")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                self.focus = match (a, b) {
                    (Some(a), Some(b)) => Some((a.min(b), a.max(b))),
                    (Some(a), None) => Some((a, a)),
                    _ => None,
                };
                self.revision += 1;
                json!({"focus": self.focus, "revision": self.revision})
            }
            "theme.set" => {
                match params.get("theme").and_then(|t| t.as_str()) {
                    Some("dark") => self.theme = Theme::dark(),
                    Some("light") => self.theme = Theme::light(),
                    _ => self.theme = self.theme.toggled(),
                }
                self.doc.theme = Some(self.theme.name);
                self.revision += 1;
                json!({"theme": self.theme.name, "revision": self.revision})
            }
            other => return Err(Error::Invalid(format!("unknown method: {other}"))),
        })
    }

    pub fn status(&self) -> Status {
        Status {
            app: codedraw_core::APP_NAME.into(),
            version: codedraw_core::VERSION.into(),
            protocol: codedraw_core::protocol::PROTOCOL_VERSION,
            revision: self.revision,
            file: self.doc.source.path.clone(),
            lines: self.doc.code.len(),
            annotations: self.doc.annotations.len(),
            step: self.doc.step,
            max_step: self.doc.max_step(),
            cols: self.size.0,
            rows: self.size.1,
            warnings: self.viewport_warnings(),
        }
    }

    /// Minimum terminal size for the recommended recording profile.
    pub const MIN_COLS: u16 = 100;
    pub const MIN_ROWS: u16 = 30;

    /// Warnings about the current viewport: too small for the recording
    /// profile (labels go under the lines) or code lines being clipped.
    /// Empty when the size is unknown (0×0) or everything fits.
    pub fn viewport_warnings(&self) -> Vec<String> {
        let (cols, rows) = self.size;
        if cols == 0 || rows == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let (_, pad_h) = self.doc.padding.unwrap_or(crate::ui::DEFAULT_PADDING);
        let width = (cols as usize).saturating_sub(2 * pad_h as usize);
        let l = self.layout(width.max(1));
        if let Some(w) = l.clipped_code {
            out.push(format!("code clipped ({w} cols)"));
        }
        if cols < Self::MIN_COLS || rows < Self::MIN_ROWS {
            let mut w = format!(
                "small terminal {cols}×{rows} < {}×{}",
                Self::MIN_COLS,
                Self::MIN_ROWS
            );
            if !l.labels_in_margin {
                w.push_str(": labels go under the lines");
            }
            out.push(w);
        }
        out
    }
}

pub fn theme_label(t: ThemeName) -> &'static str {
    match t {
        ThemeName::Dark => "dark",
        ThemeName::Light => "light",
    }
}

fn opt(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// `key=value key2="value with spaces"` → pairs. Unquoted values end at whitespace.
pub fn parse_kv(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s.trim();
    while !rest.is_empty() {
        let Some((key, after)) = rest.split_once('=') else {
            break;
        };
        let key = key.trim().to_string();
        let after = after.trim_start();
        let (val, next) = if let Some(inner) = after.strip_prefix('"') {
            match inner.find('"') {
                Some(i) => (inner[..i].to_string(), inner[i + 1..].trim_start()),
                None => (inner.to_string(), ""),
            }
        } else {
            match after.find(char::is_whitespace) {
                Some(i) => (after[..i].to_string(), after[i..].trim_start()),
                None => (after.to_string(), ""),
            }
        };
        out.push((key, val));
        rest = next;
    }
    out
}

/// True if `tok` looks like an explicit anchor (`7:text`, `7@3..9`, `.:x`, `7`).
pub fn looks_like_anchor(tok: &str) -> bool {
    let t = tok.trim();
    if t.is_empty() {
        return false;
    }
    let head = t.split([':', '@']).next().unwrap_or("");
    (head == "." || head.chars().all(|c| c.is_ascii_digit())) && !head.is_empty()
}

fn split_first(s: &str) -> (&str, &str) {
    let s = s.trim();
    match s.split_once(char::is_whitespace) {
        Some((a, b)) => (a, b.trim()),
        None => (s, ""),
    }
}

/// `A-B`, `A..B`, `A:B` or `A` → (A, B).
pub fn parse_range(s: &str) -> Result<(u32, u32), Error> {
    let s = s.trim();
    let parts: Vec<&str> = s.split(['-', ':']).filter(|p| !p.is_empty()).collect();
    let parts = if parts.len() == 1 {
        s.split("..").collect()
    } else {
        parts
    };
    let nums: Result<Vec<u32>, _> = parts.iter().map(|p| p.trim().parse::<u32>()).collect();
    match nums
        .map_err(|_| Error::Invalid(format!("invalid range: {s}")))?
        .as_slice()
    {
        [a] => Ok((*a, *a)),
        [a, b] => Ok((*a.min(b), *a.max(b))),
        _ => Err(Error::Invalid(format!("invalid range: {s}"))),
    }
}

/// `L:text[/N]`, `L`, `.:text` (cursor line) or `text` (search the whole document).
pub fn parse_where(s: &str, cursor_line: u32) -> Result<Where, Error> {
    let s = s.trim();
    if s.is_empty() {
        return Err(Error::Invalid("empty anchor".into()));
    }
    // `L@S..E` (grapheme span) or `L@C` (grapheme column), as written by the selection keys.
    if let Some((h, rest)) = s.split_once('@')
        && (h == "." || h.chars().all(|c| c.is_ascii_digit()))
        && !h.is_empty()
    {
        let line = if h == "." {
            cursor_line
        } else {
            h.parse::<u32>()
                .map_err(|_| Error::Invalid(format!("invalid line: {h}")))?
        };
        let (start, end) = match rest.split_once("..") {
            Some((a, b)) => (a.parse::<u32>().ok(), b.parse::<u32>().ok()),
            None => (rest.parse::<u32>().ok(), None),
        };
        let Some(start) = start else {
            return Err(Error::Invalid(format!("invalid anchor: {s}")));
        };
        return Ok(Where {
            line: Some(line),
            col: Some(start),
            find: None,
            occurrence: None,
            end,
        });
    }
    let (head, find) = match s.split_once(':') {
        Some((h, f)) if (h == "." || h.chars().all(|c| c.is_ascii_digit())) && !h.is_empty() => {
            (Some(h), f)
        }
        None if s == "." || s.chars().all(|c| c.is_ascii_digit()) => (Some(s), ""),
        _ => (None, s),
    };
    let line = match head {
        Some(".") => Some(cursor_line),
        Some(h) => Some(
            h.parse::<u32>()
                .map_err(|_| Error::Invalid(format!("invalid line: {h}")))?,
        ),
        None => None,
    };
    let (find, occurrence) = match find.rsplit_once('/') {
        Some((f, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => {
            (f, n.parse().ok())
        }
        _ => (find, None),
    };
    if find.is_empty() && line.is_none() {
        return Err(Error::Invalid(format!("invalid anchor: {s}")));
    }
    Ok(Where {
        line,
        col: None,
        find: if find.is_empty() {
            None
        } else {
            Some(find.to_string())
        },
        occurrence,
        end: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut a = App::new(
            Document::from_code(
                "fn main() {\n    let s = String::from(\"hola\");\n    takes(s);\n}\nfn takes(t: String) {}\n",
                "rs",
                None,
            ),
            PathBuf::from("/dev/null"),
        );
        a.autosave = false;
        a
    }

    #[test]
    fn parse_where_forms() {
        let w = parse_where("3:takes(s)", 1).unwrap();
        assert_eq!(
            (w.line, w.find.as_deref(), w.occurrence),
            (Some(3), Some("takes(s)"), None)
        );
        let w = parse_where("takes/2", 1).unwrap();
        assert_eq!(
            (w.line, w.find.as_deref(), w.occurrence),
            (None, Some("takes"), Some(2))
        );
        let w = parse_where(".:s", 9).unwrap();
        assert_eq!((w.line, w.find.as_deref()), (Some(9), Some("s")));
        let w = parse_where("4", 1).unwrap();
        assert_eq!((w.line, w.find), (Some(4), None));
    }

    #[test]
    fn commands_create_annotations() {
        let mut a = app();
        a.run_command("arrow 2:s 5:t move");
        a.run_command("mark move 3:s");
        a.run_command("box 5-5 takes");
        a.run_command("note . starts here");
        assert_eq!(a.doc.annotations.len(), 4);
        assert!(matches!(a.message, Some((_, false))));
        a.run_command("mark warn 2:String careful");
        assert!(matches!(a.message, Some((_, false))), "{:?}", a.message);
        a.run_command("mark 🔥 3:takes hot path");
        assert!(matches!(a.message, Some((_, false))), "{:?}", a.message);
        match &a.doc.annotations.last().unwrap().shape {
            Shape::Mark { kind, style, .. } => {
                assert_eq!(kind, "🔥");
                assert_eq!(style.glyph.as_deref(), Some("🔥"));
                assert_eq!(style.legend.as_deref(), Some("hot path"));
            }
            other => panic!("{other:?}"),
        }
        a.run_command("style hot glyph=🔥 legend=\"hot path\" color=red");
        assert!(a.doc.styles.contains_key("hot"), "{:?}", a.message);
        a.run_command("mark hot@blue 5:takes");
        assert!(matches!(a.message, Some((_, false))), "{:?}", a.message);
        a.run_command("mark nope 5:takes");
        assert!(
            matches!(a.message, Some((_, true))),
            "unknown kind must fail"
        );
        a.run_command("del a5 a6 a7");
        assert_eq!(
            parse_kv("glyph=🔥 legend=\"hot path\" color=red"),
            vec![
                ("glyph".to_string(), "🔥".to_string()),
                ("legend".to_string(), "hot path".to_string()),
                ("color".to_string(), "red".to_string())
            ]
        );
        a.run_command("spot 2-3 a1");
        assert!(matches!(a.message, Some((_, false))), "{:?}", a.message);
        assert!(matches!(
            a.doc.annotations.last().unwrap().shape,
            Shape::Spotlight { .. }
        ));
        a.run_command("spot zz9");
        assert!(
            matches!(a.message, Some((_, true))),
            "a nonexistent id must fail"
        );
        a.run_command("del a5");
        a.run_command("color a2 blue");
        assert_eq!(a.doc.annotations[1].color.as_deref(), Some("blue"));
        a.run_command("color a2 fucsia");
        assert!(
            matches!(a.message, Some((_, true))),
            "an unknown color must be an error"
        );
        a.run_command("color a2 auto");
        assert_eq!(a.doc.annotations[1].color, None);
        a.run_command("del a1");
        assert_eq!(a.doc.annotations.len(), 3);
        a.run_command("clear");
        assert_eq!(a.doc.annotations.len(), 0);
        a.undo();
        assert_eq!(a.doc.annotations.len(), 3);
        a.run_command("step a2 2");
        a.run_command("clear 2");
        assert_eq!(a.doc.annotations.len(), 2);
        a.undo();
        assert_eq!(a.doc.annotations.len(), 3);
        a.run_command("arrow 2:zzz 5:t");
        assert!(
            matches!(a.message, Some((_, true))),
            "nonexistent text must be an error"
        );
        assert_eq!(
            a.doc.annotations.len(),
            3,
            "an error must not touch the document"
        );
        a.undo(); // undoes :step a2 2
        a.undo(); // undoes :del a1
        assert_eq!(a.doc.annotations.len(), 4);
    }

    #[test]
    fn keyboard_selection_feeds_commands() {
        let mut a = app();
        // line 3: "    takes(s);" → cursor on `takes`
        a.cursor_line = 3;
        a.line_start();
        assert_eq!(a.cursor_col, 4);
        a.select_word();
        assert_eq!(
            a.selection(),
            Some(Selection::Span(codedraw_core::TextSpan {
                line: 3,
                start: 4,
                end: 9
            }))
        );
        assert!(a.select_action('h'));
        assert!(
            matches!(a.doc.annotations.last().unwrap().shape, Shape::Highlight { span, .. } if span.start == 4 && span.end == 9)
        );
        // mark via pre-filled command line using the pending selection
        a.cursor_col = 10; // `s` inside takes(s)
        a.start_select(false);
        assert!(a.select_action('m'));
        assert_eq!(a.mode, Mode::Command("mark ".into()));
        a.mode = Mode::Normal;
        a.run_command("mark move moves here");
        match &a.doc.annotations.last().unwrap().shape {
            Shape::Mark {
                kind, span, label, ..
            } => {
                assert_eq!(kind, "move");
                assert_eq!((span.line, span.start, span.end), (3, 10, 11));
                assert_eq!(label.as_deref(), Some("moves here"));
            }
            other => panic!("{other:?}"),
        }
        // arrow: select → a → select → a
        a.cursor_line = 2;
        a.line_start();
        a.start_select(false);
        assert!(a.select_action('a'));
        assert!(a.arrow_from.is_some());
        a.cursor_line = 5;
        a.cursor_col = 3;
        a.start_select(false);
        assert!(a.select_action('a'));
        assert!(
            matches!(a.doc.annotations.last().unwrap().shape, Shape::Arrow { from, to, .. } if from.line == 2 && to.line == 5)
        );
        // linewise selection → box prefill
        a.cursor_line = 5;
        a.start_select(true);
        a.cursor_line = 4;
        assert!(a.select_action('b'));
        assert_eq!(a.mode, Mode::Command("box 4-5 ".into()));
        // explicit anchors still win over a pending selection
        a.pending_sel = Some(Selection::Span(codedraw_core::TextSpan {
            line: 1,
            start: 0,
            end: 2,
        }));
        a.run_command("hl 2@4..5 by coords");
        match &a.doc.annotations.last().unwrap().shape {
            Shape::Highlight { span, label } => {
                assert_eq!((span.line, span.start, span.end), (2, 4, 5));
                assert_eq!(label.as_deref(), Some("by coords"));
            }
            other => panic!("{other:?}"),
        }
        // word motions
        a.cursor_line = 2;
        a.line_start();
        a.word_forward();
        assert_eq!(a.cursor_col, 8, "let → s");
        a.word_forward();
        assert_eq!(a.cursor_col, 10, "s → =");
        a.word_backward();
        assert_eq!(a.cursor_col, 8);
    }

    #[test]
    fn tab_selects_and_edits_annotations() {
        let mut a = app();
        a.run_command("note 5 five");
        a.run_command("note 2 two");
        a.select_annotation(1);
        assert_eq!(
            a.selected.as_deref(),
            Some("a2"),
            "reading order: line 2 first"
        );
        assert_eq!(a.cursor_line, 2);
        a.select_annotation(1);
        assert_eq!(a.selected.as_deref(), Some("a1"));
        a.select_annotation(1);
        assert_eq!(a.selected.as_deref(), Some("a2"), "wraps around");
        assert!(a.selected_action('\n'));
        assert_eq!(a.mode, Mode::Command("label a2 two".into()));
        a.mode = Mode::Normal;
        assert!(a.selected_action('+'));
        assert_eq!(a.doc.find_annotation("a2").unwrap().step, 2);
        assert!(a.selected_action('x'));
        assert!(a.doc.find_annotation("a2").is_none());
        assert_eq!(a.selected, None);
    }

    #[test]
    fn command_completion_and_history() {
        let mut a = app();
        assert_eq!(a.complete("ma").0, "mark ");
        let (text, many) = a.complete("s");
        assert_eq!(text, "s");
        assert!(many.contains(&"spot".to_string()) && many.contains(&"step".to_string()));
        assert_eq!(a.complete("mark wa").0, "mark warn ");
        a.run_command("note 1 x");
        a.run_command("note 2 y");
        assert_eq!(a.complete("del a").1.len(), 2);
        assert_eq!(a.complete("del a1").0, "del a1 ");
        assert_eq!(a.complete("theme d").0, "theme dark ");
        assert_eq!(a.history_step(-1).as_deref(), Some("note 2 y"));
        assert_eq!(a.history_step(-1).as_deref(), Some("note 1 x"));
        assert_eq!(a.history_step(1).as_deref(), Some("note 2 y"));
        assert_eq!(a.history_step(1).as_deref(), Some(""));
    }

    #[test]
    fn callout_command_parses_placement_and_title() {
        let mut a = app();
        a.run_command("callout 2-3 @below title=\"why\" because s is moved");
        assert!(matches!(a.message, Some((_, false))), "{:?}", a.message);
        match &a.doc.annotations.last().unwrap().shape {
            Shape::Callout {
                from_line,
                to_line,
                text,
                title,
                placement,
                ..
            } => {
                assert_eq!((*from_line, *to_line), (2, Some(3)));
                assert_eq!(text, "because s is moved");
                assert_eq!(title.as_deref(), Some("why"));
                assert_eq!(*placement, codedraw_core::model::Placement::Below);
            }
            other => panic!("{other:?}"),
        }
        a.run_command("callout 2");
        assert!(matches!(a.message, Some((_, true))), "empty text must fail");
    }

    #[test]
    fn api_update_moves_without_losing_kind_or_style() {
        let mut a = app();
        let call = |a: &mut App, method: &str, params: Value| {
            a.handle_request(Request {
                id: json!(1),
                method: method.into(),
                params,
            })
        };
        let r = call(
            &mut a,
            "annotation.add",
            json!({"id": "m", "type": "mark", "kind": "move", "at": {"find": "takes"}, "label": "moves", "style": {"glyph": "🔥"}}),
        );
        assert!(r.error.is_none(), "{:?}", r.error);
        // Move by text: kind, style and label survive, the span changes.
        let r = call(
            &mut a,
            "annotation.update",
            json!({"id": "m", "move": {"at": {"find": "let s"}}}),
        );
        assert!(r.error.is_none(), "{:?}", r.error);
        let want = a.doc.find_text("let s", 1, None).unwrap();
        match &a.doc.find_annotation("m").unwrap().shape {
            Shape::Mark {
                kind,
                span,
                label,
                style,
            } => {
                assert_eq!(kind, "move");
                assert_eq!(*span, want);
                assert_eq!(label.as_deref(), Some("moves"));
                assert_eq!(style.glyph.as_deref(), Some("🔥"));
            }
            other => panic!("{other:?}"),
        }
        // A field that does not apply to the type is an error, not a no-op.
        let r = call(
            &mut a,
            "annotation.update",
            json!({"id": "m", "move": {"from_line": 2}}),
        );
        assert_eq!(r.error.unwrap().code, "invalid_argument");
        // Unknown text is still text_not_found.
        let r = call(
            &mut a,
            "annotation.update",
            json!({"id": "m", "move": {"at": {"find": "nope nope"}}}),
        );
        assert_eq!(r.error.unwrap().code, "text_not_found");
        // Boxes move by lines; an out-of-range result is rejected and rolled back.
        call(
            &mut a,
            "annotation.add",
            json!({"id": "b", "type": "box", "from_line": 1, "to_line": 2}),
        );
        let r = call(
            &mut a,
            "annotation.update",
            json!({"id": "b", "move": {"to_line": 3}}),
        );
        assert!(r.error.is_none(), "{:?}", r.error);
        let r = call(
            &mut a,
            "annotation.update",
            json!({"id": "b", "move": {"to_line": 999}}),
        );
        assert_eq!(r.error.unwrap().code, "out_of_range");
        assert!(matches!(
            a.doc.find_annotation("b").unwrap().shape,
            Shape::Box { to_line: 3, .. }
        ));
    }

    #[test]
    fn api_add_and_steps() {
        let mut a = app();
        let req: Request = serde_json::from_value(json!({
            "id": 1, "method": "annotation.add",
            "params": {"type": "arrow", "from": {"find": "let s"}, "to": {"find": "takes(s)"}, "label": "move", "step": 2}
        }))
        .unwrap();
        let resp = a.handle_request(req);
        assert!(resp.error.is_none(), "{:?}", resp.error);
        assert_eq!(a.doc.annotations[0].step, 2);
        let resp = a.handle_request(Request {
            id: json!("x"),
            method: "step.next".into(),
            params: Value::Null,
        });
        assert_eq!(resp.result.unwrap()["step"], 2);
        a.set_step(0);
        assert_eq!(a.doc.step, 0, "step 0 (code only) is valid");
        assert!(
            a.layout(80).rows.len() == a.doc.code.len(),
            "step 0 has no virtual rows"
        );
        a.run_command("note . created at step 1 while on step 0");
        assert_eq!(a.doc.annotations.last().unwrap().step, 1);
        let resp = a.handle_request(Request {
            id: json!("y"),
            method: "annotation.delete".into(),
            params: json!({"id": "nope"}),
        });
        assert_eq!(resp.error.unwrap().code, "not_found");
    }
}
