//! Application state and logic, with no drawing dependencies. Both the
//! keyboard and the local API go through here, so an agent and the presenter
//! do exactly the same thing.

use std::path::{Path, PathBuf};

use codedraw_core::history::History;
use codedraw_core::layout::{Layout, LayoutOptions, layout};
use codedraw_core::model::{
    Annotation, BUILTIN_KINDS, Document, Emphasis, MarkStyle, Shape, ThemeName,
};
use codedraw_core::protocol::{
    AddParams, IdParams, METHODS, Mutated, PatchParams, PutParams, Request, Response, Scene,
    SceneChanged, Status, StepParams, UpdateParams, Where,
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
    /// Pending agent proposals (supervised mode, t-045).
    Review,
}

/// A mutation an agent asked for while supervised mode is on: it waits for
/// the presenter's `y`/`n` (or expires) before the socket gets its reply.
pub struct Proposal {
    pub id: u64,
    pub req: Request,
    pub reply: std::sync::mpsc::Sender<Response>,
    pub since: std::time::Instant,
    /// Human description shown in the review overlay.
    pub summary: String,
}

/// How long a proposal waits for the presenter before it is rejected.
pub const PROPOSAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Methods that never change the document or the view: they bypass supervision.
pub const READ_ONLY_METHODS: &[&str] = &[
    "ping",
    "status",
    "api.methods",
    "api.schema",
    "code.get",
    "scene.get",
    "scene.list",
    "style.list",
    "render.text",
    "follow.status",
];

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
    /// The last autosave failure; stays on the status line until a save succeeds.
    pub save_error: Option<String>,
    /// `--padding` for this session only: shown instead of the document's
    /// value but never written to it (`:pad` sets the document and clears this).
    pub padding_override: Option<(u16, u16)>,
    /// SVG/PNG renderer for `:export`, installed by the binary.
    pub image_exporter: Option<ImageExporter>,
    /// Mouse capture (t-033): opt-in, because capturing the mouse takes the
    /// terminal's native text selection away (Shift/Option-drag gets it back).
    pub mouse: bool,
    /// Where the last frame put things, so a click can be mapped to a line
    /// and column (filled by `ui::draw`).
    pub hit: HitMap,
    /// A drag in progress: where the button went down.
    pub drag_from: Option<(u32, u32)>,
    /// Supervised mode: agent mutations wait for the presenter's approval.
    pub supervised: bool,
    pub proposals: Vec<Proposal>,
    next_proposal: u64,
    /// Decisions taken (accepted, rejected) since the app started, for `status`.
    pub decided: (u64, u64),
    /// Scroll offset (wrapped rows) of the help / list overlay.
    pub overlay_scroll: usize,
    /// Follow mode: the watched source file, when enabled.
    pub follow: Option<crate::follow::Follow>,
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
            save_error: None,
            padding_override: None,
            image_exporter: None,
            mouse: false,
            hit: HitMap::default(),
            drag_from: None,
            supervised: false,
            proposals: Vec::new(),
            next_proposal: 1,
            decided: (0, 0),
            overlay_scroll: 0,
            follow: None,
        }
    }

    /// Padding in effect: the session override, else the document's, else the default.
    pub fn effective_padding(&self) -> (u16, u16) {
        self.padding_override
            .or(self.doc.padding)
            .unwrap_or(crate::ui::DEFAULT_PADDING)
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
        if self.autosave {
            // `save` records the failure in `save_error`, apart from `message`,
            // so a later info message cannot hide it.
            let _ = self.save();
        }
    }

    pub fn save(&mut self) -> Result<(), Error> {
        self.doc.theme = Some(self.theme.name);
        match fs::save_document(&self.doc, &self.save_path) {
            Ok(()) => {
                self.dirty = false;
                self.save_error = None;
                Ok(())
            }
            Err(e) => {
                self.save_error = Some(format!("could not save {}: {e}", self.save_path.display()));
                Err(e)
            }
        }
    }

    /// Quits unless unsaved changes could not be written (`force` ignores that).
    pub fn request_quit(&mut self, force: bool) {
        if !force && self.dirty && self.save_error.is_some() {
            self.error(format!(
                ":q! quits anyway, :w PATH saves elsewhere ({})",
                self.save_error.clone().unwrap_or_default()
            ));
            return;
        }
        self.quit = true;
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
        if p.has_border_style() {
            match &mut ann.shape {
                Shape::Box { frame, .. } | Shape::Callout { frame, .. } => {
                    if let Some(b) = p.border {
                        frame.border = b;
                    }
                    if let Some(c) = p.corners {
                        frame.corners = c;
                    }
                    if let Some(f) = p.fill {
                        frame.fill = Some(f);
                    }
                }
                other => {
                    return Err(Error::Invalid(format!(
                        "border/corners/fill apply to boxes and callouts only; {} is a {}",
                        p.id,
                        other.kind_name()
                    )));
                }
            }
        }
        if let Some(e) = p.emphasis {
            match &mut ann.shape {
                Shape::Highlight { emphasis, .. } | Shape::Mark { emphasis, .. } => *emphasis = e,
                other => {
                    return Err(Error::Invalid(format!(
                        "emphasis applies to highlights and marks only; {} is a {}",
                        p.id,
                        other.kind_name()
                    )));
                }
            }
        }
        if p.has_line_style() {
            match &mut ann.shape {
                Shape::Arrow { line, .. } => {
                    if let Some(s) = p.stroke {
                        line.stroke = s;
                    }
                    if let Some(r) = p.route {
                        line.route = r;
                    }
                    if let Some(h) = &p.heads {
                        if let Some(st) = h.start {
                            line.heads.start = st;
                        }
                        if let Some(en) = h.end {
                            line.heads.end = en;
                        }
                    }
                }
                other => {
                    return Err(Error::Invalid(format!(
                        "stroke/route/heads apply to arrows only; {} is a {}",
                        p.id,
                        other.kind_name()
                    )));
                }
            }
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
    /// `scene.put`: replaces the whole document. The code must come embedded
    /// (the instance cannot resolve a file reference meant for another
    /// directory); the theme follows the document; one undo step.
    pub fn put_scene(&mut self, mut doc: Document) -> Result<SceneChanged, Error> {
        if doc.code.is_empty() {
            match &doc.code_src {
                codedraw_core::model::CodeSource::Embedded(lines) => doc.code = lines.clone(),
                _ => {
                    return Err(Error::Invalid(
                        "scene.put needs the code embedded (`\"code\": [lines…]`); resolve file references before sending".into(),
                    ));
                }
            }
        }
        doc.normalize_loaded()?;
        for a in &doc.annotations {
            doc.validate_shape(&a.shape)?;
        }
        self.history.record(&self.doc);
        self.doc = doc;
        if let Some(t) = self.doc.theme {
            self.theme = Theme::from_name(t);
        }
        self.selected = None;
        self.focus = None;
        self.scroll = 0;
        self.clamp_cursor();
        self.touch();
        Ok(self.scene_changed(Vec::new(), Vec::new(), Vec::new()))
    }

    /// `scene.patch`: partial changes; annotations merge by id (last write
    /// wins), `{"id", "deleted": true}` removes. All or nothing: an invalid
    /// annotation leaves the document untouched.
    pub fn patch_scene(&mut self, p: PatchParams) -> Result<SceneChanged, Error> {
        let mut d = self.doc.clone();
        let (mut added, mut replaced, mut deleted) = (Vec::new(), Vec::new(), Vec::new());
        // Styles first: a patched mark may use a style defined in the same patch.
        if let Some(styles) = p.styles {
            for (name, st) in styles {
                d.styles.insert(name, st);
            }
        }
        if let Some(anns) = p.annotations {
            for v in anns {
                let id = v
                    .get("id")
                    .and_then(|i| i.as_str())
                    .map(str::trim)
                    .filter(|i| !i.is_empty())
                    .ok_or_else(|| Error::Invalid("every patched annotation needs an `id`".into()))?
                    .to_string();
                let is_delete = v.get("deleted").and_then(|b| b.as_bool()).unwrap_or(false);
                let existed = d.find_annotation(&id).is_some();
                if is_delete {
                    if !existed {
                        return Err(Error::Invalid(format!(
                            "cannot delete {id}: no such annotation"
                        )));
                    }
                    d.annotations.retain(|a| a.id != id);
                    deleted.push(id);
                    continue;
                }
                let ann: Annotation = serde_json::from_value(v)
                    .map_err(|e| Error::Invalid(format!("annotation {id}: {e}")))?;
                d.annotations.retain(|a| a.id != id);
                d.add(ann)?;
                if existed {
                    replaced.push(id);
                } else {
                    added.push(id);
                }
            }
        }
        if let Some(t) = p.title {
            d.title = t.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
        }
        if let Some(pad) = p.padding {
            d.padding = pad;
        }
        if let Some(theme) = p.theme {
            d.theme = Some(theme);
        }
        if let Some(step) = p.step {
            d.step = step.min(d.max_step().max(1));
        } else if d.step > d.max_step() {
            d.step = d.max_step();
        }
        self.history.record(&self.doc);
        self.doc = d;
        if let Some(t) = self.doc.theme {
            self.theme = Theme::from_name(t);
        }
        self.selected = None;
        self.clamp_cursor();
        self.touch();
        Ok(self.scene_changed(added, replaced, deleted))
    }

    fn scene_changed(
        &self,
        added: Vec<String>,
        replaced: Vec<String>,
        deleted: Vec<String>,
    ) -> SceneChanged {
        SceneChanged {
            revision: self.revision,
            lines: self.doc.code.len(),
            annotations: self.doc.annotations.len(),
            step: self.doc.step,
            max_step: self.doc.max_step(),
            added,
            replaced,
            deleted,
        }
    }

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

    /// Replaces the code (paste, stdin). Annotations are re-anchored by the
    /// text they pointed at: the old text of each anchor is searched in the
    /// new code (same line first, then anywhere); what cannot be found is
    /// dropped with a message, and `u` brings everything back.
    pub fn replace_code(&mut self, text: &str) -> Result<(), Error> {
        if text.trim().is_empty() {
            return Err(Error::Invalid(
                "nothing to paste: the clipboard text is empty".into(),
            ));
        }
        let lang = self.doc.source.language.clone();
        let d = Document::from_code(text, lang, None);
        // Code that was never given a language: a shebang or modeline in
        // the pasted text picks one. An explicit `:lang`/`--lang` sticks.
        let detected = (self.doc.source.language == "txt" || self.doc.source.language.is_empty())
            .then(|| codedraw_core::highlight::language_from_content(&d.code))
            .flatten();
        let out = self.apply_new_code(d.code, d.code_src, d.source.first_line);
        if let Some(det) = detected {
            self.doc.source.language = det.language;
        }
        self.cursor_line = self.doc.source.first_line;
        self.scroll = 0;
        if out.dropped > 0 {
            self.info(format!(
                "{} lines pasted; {} annotation{} dropped (their text is gone; u undoes)",
                self.doc.code.len(),
                out.dropped,
                if out.dropped == 1 { "" } else { "s" }
            ));
        } else {
            self.info(format!(
                "{} lines pasted ({} annotations kept)",
                self.doc.code.len(),
                out.kept
            ));
        }
        Ok(())
    }

    /// Replaces the code lines (and where they come from), re-anchoring the
    /// annotations by text. One undoable step. Returns (kept, dropped).
    fn apply_new_code(
        &mut self,
        code: Vec<String>,
        code_src: codedraw_core::model::CodeSource,
        first_line: u32,
    ) -> crate::reanchor::Outcome {
        self.history.record(&self.doc);
        let mut d = self.doc.clone();
        d.annotations.clear();
        d.code = code;
        d.code_src = code_src;
        d.source.first_line = first_line;
        let before = self.doc.annotations.len();
        let old = self.doc.clone();
        let mut remap = crate::reanchor::Remap::new(&old, &d);
        let shapes: Vec<Option<Shape>> = old
            .annotations
            .iter()
            .map(|a| remap.shape(&a.shape))
            .collect();
        for (a, shape) in old.annotations.iter().zip(shapes) {
            if let Some(shape) = shape
                && d.validate_shape(&shape).is_ok()
            {
                let _ = d.add(Annotation { shape, ..a.clone() });
            }
        }
        let kept = d.annotations.len();
        self.doc = d;
        self.selected = None;
        self.clamp_cursor();
        self.touch();
        crate::reanchor::Outcome {
            kept,
            dropped: before - kept,
        }
    }

    fn clamp_cursor(&mut self) {
        let first = self.doc.source.first_line;
        let last = self.doc.last_line();
        self.cursor_line = self.cursor_line.clamp(first, last);
        if matches!(self.mode, Mode::Select { .. }) {
            self.mode = Mode::Normal;
        }
    }

    // ---------- follow mode ----------

    /// Absolute path of the followed (or followable) source file.
    pub fn source_file(&self) -> Option<PathBuf> {
        fs::source_file(&self.doc, self.save_path.parent())
    }

    /// Enables or disables follow mode. Enabling needs a file reference
    /// (`:link` first when the code is embedded).
    pub fn set_follow(&mut self, enabled: bool) -> Result<String, Error> {
        if !enabled {
            if let Some(f) = &mut self.follow {
                f.stop();
            }
            self.follow = None;
            return Ok("follow off".into());
        }
        if let Some(f) = &self.follow {
            return Ok(format!("already following {}", f.file_name()));
        }
        let path = self.source_file().ok_or_else(|| {
            Error::Invalid(
                "nothing to follow: the code is embedded (use :link path to reference a file)"
                    .into(),
            )
        })?;
        let f =
            crate::follow::Follow::start(path, self.doc.code.clone()).map_err(Error::Invalid)?;
        let name = f.file_name();
        self.follow = Some(f);
        // Pick up anything that changed before we started watching.
        let _ = self.reload_from_disk();
        Ok(format!("following {name}"))
    }

    /// Called every loop iteration: reloads when the watcher says so.
    /// Returns true when something changed on screen.
    pub fn follow_tick(&mut self) -> bool {
        let Some(f) = &mut self.follow else {
            return false;
        };
        if !f.poll() {
            return false;
        }
        self.reload_from_disk()
    }

    /// Re-reads the followed file and applies it. Returns true when the
    /// screen changed (content, or a message).
    fn reload_from_disk(&mut self) -> bool {
        let Some(f) = &mut self.follow else {
            return false;
        };
        let base = self.save_path.parent().map(Path::to_path_buf);
        let read = fs::reread_code(&self.doc, base.as_deref());
        let name = f.file_name();
        let new = match read {
            Ok(lines) => lines,
            Err(e) => {
                let msg = e.to_string();
                if !f.path.exists() {
                    if !f.missing {
                        f.missing = true;
                        self.error(format!("{name} removed; waiting for it to come back"));
                        return true;
                    }
                    return false;
                }
                self.error(
                    if msg.contains("stream did not contain valid UTF-8")
                        || msg.contains("invalid utf-8")
                    {
                        format!(
                            "cannot follow {name}: not valid UTF-8 (keeping the previous content)"
                        )
                    } else {
                        format!("cannot reload {name}: {msg}")
                    },
                );
                return true;
            }
        };
        let was_missing = std::mem::replace(&mut f.missing, false);
        // An editor that truncates and then writes: give the second write a chance.
        let empty = new.iter().all(|l| l.trim().is_empty());
        if empty && !f.last_content.iter().all(|l| l.trim().is_empty()) && !f.take_deferred() {
            f.defer();
            return false;
        }
        if new == f.last_content && !was_missing {
            return false;
        }
        f.mark_reloaded(new.clone());
        let reloads = f.reloads;
        let code_src = self.doc.code_src.clone();
        let first = self.doc.source.first_line;
        let out = self.apply_new_code(new, code_src, first);
        self.info(format!(
            "{name} reloaded (#{reloads}): {} annotation{} kept{}",
            out.kept,
            if out.kept == 1 { "" } else { "s" },
            if out.dropped > 0 {
                format!(", {} dropped (text gone; u undoes)", out.dropped)
            } else {
                String::new()
            }
        ));
        true
    }

    /// `follow.status` payload.
    pub fn follow_status(&self) -> Value {
        match &self.follow {
            Some(f) => json!({
                "enabled": true,
                "path": f.path.display().to_string(),
                "reloads": f.reloads,
                "last_reload": f.last_reload_clock,
                "missing": f.missing,
            }),
            None => {
                json!({"enabled": false, "path": self.source_file().map(|p| p.display().to_string())})
            }
        }
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
    pub(crate) fn word_class(g: &str) -> u8 {
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
                match self.add_annotation(
                    None,
                    None,
                    Shape::Highlight {
                        span,
                        label: None,
                        emphasis: Emphasis::default(),
                    },
                ) {
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
                                line: Default::default(),
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
            .filter(|a| a.step <= self.doc.step)
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
            self.info(if self.doc.step == 0 {
                "step 0 shows only the code: press n to see the annotations"
            } else {
                "no annotations in this step"
            });
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
            'e' => self.mode = Mode::Command(format!("set {id} ")),
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
                    stroke: None,
                    route: None,
                    heads: None,
                    border: None,
                    corners: None,
                    fill: None,
                    emphasis: None,
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
        "arrow",
        "autosave",
        "box",
        "callout",
        "clear",
        "follow",
        "color",
        "del",
        "embed",
        "focus",
        "export",
        "help",
        "hl",
        "label",
        "lang",
        "link",
        "list",
        "mark",
        "mouse",
        "next",
        "note",
        "pad",
        "present",
        "prev",
        "q",
        "redo",
        "review",
        "set",
        "spot",
        "step",
        "style",
        "supervise",
        "theme",
        "title",
        "undo",
        "until",
        "w",
        "wq",
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
            // ↑ stops at the oldest entry; ↓ past the newest clears the input.
            Some(p) => (p as i64 + dir as i64).clamp(0, n),
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
                    self.request_quit(false);
                    Ok(String::new())
                }
                "q!" | "quit!" => {
                    self.request_quit(true);
                    Ok(String::new())
                }
                "w" | "write" => {
                    if !rest.is_empty() {
                        self.save_path = PathBuf::from(rest);
                    }
                    self.save()
                        .map(|_| format!("saved to {}", self.save_path.display()))
                }
                "export" => self.export_command(rest),
                "supervise" | "supervised" => match rest.trim() {
                    "" => Ok(format!(
                        "supervised mode {} ({} pending)",
                        if self.supervised { "on" } else { "off" },
                        self.proposals.len()
                    )),
                    "on" => Ok(self.set_supervised(true)),
                    "off" => Ok(self.set_supervised(false)),
                    _ => Err(Error::Invalid("usage: :supervise [on|off]".into())),
                },
                "mouse" => match rest.trim() {
                    "" => Ok(format!("mouse {}", if self.mouse { "on" } else { "off" })),
                    "on" => Ok(self.set_mouse(true)),
                    "off" => Ok(self.set_mouse(false)),
                    _ => Err(Error::Invalid("usage: :mouse [on|off]".into())),
                },
                "review" => {
                    if self.proposals.is_empty() {
                        Ok("no pending proposals".into())
                    } else {
                        self.overlay = Overlay::Review;
                        self.overlay_scroll = 0;
                        Ok(String::new())
                    }
                }
                "wq" => {
                    // Quit only when the write succeeded (`:q!` leaves anyway).
                    self.save()?;
                    self.quit = true;
                    Ok(String::new())
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
                "set" => self.cmd_set(rest),
                "follow" => match rest.trim() {
                    "" => Ok(match &self.follow {
                        Some(f) => format!(
                            "following {} ({} reload{}{})",
                            f.file_name(),
                            f.reloads,
                            if f.reloads == 1 { "" } else { "s" },
                            f.last_reload_clock
                                .as_ref()
                                .map(|t| format!(", last at {t}"))
                                .unwrap_or_default()
                        ),
                        None => "follow off (:follow on to reload when the file is saved)".into(),
                    }),
                    "on" | "yes" | "start" => self.set_follow(true),
                    "off" | "no" | "stop" => self.set_follow(false),
                    other => Err(Error::Invalid(format!(
                        "usage: :follow [on|off], got {other}"
                    ))),
                },
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
                        stroke: None,
                        route: None,
                        heads: None,
                        border: None,
                        corners: None,
                        fill: None,
                        emphasis: None,
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
                        stroke: None,
                        route: None,
                        heads: None,
                        border: None,
                        corners: None,
                        fill: None,
                        emphasis: None,
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
                            // One past the last step is allowed: it opens an
                            // empty step for the next annotations.
                            let max = self.doc.max_step();
                            if n > max + 1 {
                                return Err(Error::Invalid(format!(
                                    "step {n} is beyond the last step ({max}); :step {} opens a new one",
                                    max + 1
                                )));
                            }
                            if n == max + 1 && n != self.doc.step {
                                self.doc.step = n;
                                self.revision += 1;
                                return Ok(format!(
                                    "step {n}/{max}: empty, new annotations go here"
                                ));
                            }
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
                                stroke: None,
                                route: None,
                                heads: None,
                                border: None,
                                corners: None,
                                fill: None,
                                emphasis: None,
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
                    match rest.trim().to_ascii_lowercase().as_str() {
                        "dark" | "oscuro" => self.theme = Theme::dark(),
                        "light" | "claro" => self.theme = Theme::light(),
                        "" | "toggle" => self.theme = self.theme.toggled(),
                        other => {
                            return Err(Error::Invalid(format!(
                                "unknown theme: {other} (dark, light; :theme alone toggles)"
                            )));
                        }
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
                    use codedraw_core::highlight::{
                        detect_language, has_grammar, normalize_language,
                    };
                    if rest.is_empty() {
                        let l = &self.doc.source.language;
                        return Ok(if has_grammar(l) {
                            format!("language: {l} (:lang auto re-detects, :lang <id> overrides)")
                        } else {
                            format!("language: {l} (plain text; :lang <id> to set one)")
                        });
                    }
                    if rest == "auto" {
                        let d = detect_language(self.doc.source.path.as_deref(), &self.doc.code);
                        self.doc.source.language = d.language;
                        self.touch();
                        return Ok(format!(
                            "language: {} ({})",
                            self.doc.source.language,
                            d.via.label()
                        ));
                    }
                    let lang = normalize_language(rest);
                    self.doc.source.language = lang.clone();
                    self.touch();
                    if lang == "txt" || has_grammar(&lang) {
                        Ok(format!("language: {lang}"))
                    } else {
                        Ok(format!(
                            "language: {lang} (no grammar: shown as plain text; try rs, ts, py, go, toml…)"
                        ))
                    }
                }
                "title" => {
                    self.history.record(&self.doc);
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
                    self.history.record(&self.doc);
                    self.doc.padding = None;
                    self.padding_override = None;
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
                            let (v, h) = self.effective_padding();
                            return Ok(format!(
                                "padding {v} {h}{}",
                                if self.padding_override.is_some() {
                                    " (from --padding, not saved)"
                                } else if self.doc.padding.is_none() {
                                    " (default)"
                                } else {
                                    ""
                                }
                            ));
                        }
                        [v] => (*v, *v * 2),
                        [v, h, ..] => (*v, *h),
                    };
                    // `:pad 0 0` is a real value (no padding); `:pad default` returns to the theme default.
                    self.history.record(&self.doc);
                    self.doc.padding = Some((v, h));
                    self.padding_override = None;
                    self.touch();
                    let (cv, ch) = crate::ui::clamp_padding((v, h), self.size, self.present);
                    Ok(if (cv, ch) != (v, h) {
                        format!(
                            "padding {v} {h} (shown as {cv} {ch}: the terminal is too small for more)"
                        )
                    } else {
                        format!("padding {v} {h}")
                    })
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
        let (range, rest) = split_first(rest);
        let (a, b) = parse_range(range).map_err(|_| {
            Error::Invalid(
                "usage: :box A-B [label] [border=double] [corners=sharp] [fill=no]".into(),
            )
        })?;
        let (label, kv) = split_trailing_kv(rest);
        let mut frame = codedraw_core::model::BorderStyle::default();
        for (k, v) in &kv {
            apply_border_kv(&mut frame, k, v)?;
        }
        let id = self.add_annotation(
            None,
            None,
            Shape::Box {
                from_line: a,
                to_line: b,
                label: opt(label),
                frame,
            },
        )?;
        Ok(format!("{id}: box {a}-{b}"))
    }

    fn cmd_arrow(&mut self, rest: &str) -> Result<String, Error> {
        let (from, rest) = split_first(rest);
        let (to, rest) = split_first(rest);
        if from.is_empty() || to.is_empty() {
            return Err(Error::Invalid(
                "usage: :arrow L:text L:text [label] [stroke=dashed] [heads=none,arrow] [route=sharp]"
                    .into(),
            ));
        }
        let (label, kv) = split_trailing_kv(rest);
        let mut line = codedraw_core::model::LineStyle::default();
        for (k, v) in &kv {
            apply_line_kv(&mut line, k, v)?;
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
                line,
            },
        )?;
        Ok(format!("{id}: arrow L{} → L{}", f.line, t.line))
    }

    /// `:export [path] [step=N] [width=N] [theme=dark|light]` — writes what
    /// the current step shows as text (`.txt`), ANSI (`.ansi`), Markdown
    /// (`.md`, fenced block), SVG or PNG. Without a path: `<doc>-stepN.txt`
    /// next to the document.
    fn export_command(&mut self, rest: &str) -> Result<String, Error> {
        let (path, kv) = split_trailing_kv(rest);
        let mut step = self.doc.step;
        let mut width = (self.size.0 as usize).saturating_sub(2).max(40);
        let mut theme = self.theme.clone();
        for (k, v) in kv {
            match k.as_str() {
                "step" => {
                    step = v
                        .parse()
                        .map_err(|_| Error::Invalid(format!("step={v}: not a number")))?;
                }
                "width" => {
                    width = v
                        .parse::<usize>()
                        .ok()
                        .filter(|w| *w >= 20)
                        .ok_or_else(|| Error::Invalid(format!("width={v}: use a number ≥ 20")))?;
                }
                "theme" => {
                    theme = Theme::from_name(match v.to_ascii_lowercase().as_str() {
                        "dark" | "oscuro" => ThemeName::Dark,
                        "light" | "claro" => ThemeName::Light,
                        _ => return Err(Error::Invalid(format!("theme={v}: dark or light"))),
                    });
                }
                _ => {
                    return Err(Error::Invalid(format!(
                        "unknown option {k}= (step, width, theme)"
                    )));
                }
            }
        }
        let path = if path.is_empty() {
            let stem = self
                .save_path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| "codedraw".into());
            let stem = stem.strip_suffix(".codedraw").unwrap_or(&stem).to_string();
            self.save_path
                .with_file_name(format!("{stem}-step{step}.txt"))
        } else {
            PathBuf::from(path)
        };
        let format = ExportFormat::from_path(&path).ok_or_else(|| {
            Error::Invalid(format!(
                "{}: the extension picks the format (.txt .ansi .md .svg .png)",
                path.display()
            ))
        })?;
        let l = layout(
            &self.doc,
            &theme,
            &LayoutOptions {
                width,
                step,
                focus: self.focus,
                line_numbers: true,
                tab_width: 4,
            },
        );
        let bytes = match format {
            ExportFormat::Txt => (l.canvas.to_plain_text() + "\n").into_bytes(),
            ExportFormat::Ansi => l.canvas.to_ansi().into_bytes(),
            ExportFormat::Md => l.canvas.to_markdown(self.doc.title.as_deref()).into_bytes(),
            ExportFormat::Svg | ExportFormat::Png => {
                let f = if format == ExportFormat::Svg {
                    ImageFormat::Svg
                } else {
                    ImageFormat::Png
                };
                let exporter = self.image_exporter.as_ref().ok_or_else(|| {
                    Error::Invalid(
                        "SVG/PNG export is not available in this build; use `codedraw render`"
                            .into(),
                    )
                })?;
                exporter(&l.canvas, &theme, f, self.doc.title.as_deref()).map_err(Error::Invalid)?
            }
        };
        std::fs::write(&path, &bytes)
            .map_err(|e| Error::Invalid(format!("could not write {}: {e}", path.display())))?;
        Ok(format!(
            "exported step {step} to {} ({} bytes)",
            path.display(),
            bytes.len()
        ))
    }

    /// `:set ID key=value…` — changes properties of an existing annotation:
    /// `label`, `color`, `step` for any; `stroke`, `route`, `heads`, `start`,
    /// `end` for arrows.
    fn cmd_set(&mut self, rest: &str) -> Result<String, Error> {
        let (id, rest) = split_first(rest);
        let kv = parse_kv(rest);
        if id.is_empty() || kv.is_empty() {
            return Err(Error::Invalid(
                "usage: :set ID key=value…   (label, color, step; arrows: stroke, route, heads, start, end; boxes/callouts: border, corners, fill; highlights/marks: emphasis)"
                    .into(),
            ));
        }
        let is_arrow = matches!(
            self.doc.find_annotation(id).map(|a| &a.shape),
            Some(Shape::Arrow { .. })
        );
        let is_framed = matches!(
            self.doc.find_annotation(id).map(|a| &a.shape),
            Some(Shape::Box { .. } | Shape::Callout { .. })
        );
        let is_span = matches!(
            self.doc.find_annotation(id).map(|a| &a.shape),
            Some(Shape::Highlight { .. } | Shape::Mark { .. })
        );
        let is_mark = matches!(
            self.doc.find_annotation(id).map(|a| &a.shape),
            Some(Shape::Mark { .. })
        );
        let mut p = UpdateParams {
            id: id.to_string(),
            step: None,
            color: None,
            label: None,
            shape: None,
            mv: None,
            stroke: None,
            route: None,
            heads: None,
            border: None,
            corners: None,
            fill: None,
            emphasis: None,
        };
        let mut heads = codedraw_core::protocol::HeadsPatch::default();
        for (k, v) in &kv {
            match k.as_str() {
                "label" | "text" | "title" => p.label = Some(v.clone()),
                "color" => p.color = Some(v.clone()),
                "step" => {
                    p.step =
                        Some(v.parse().map_err(|_| {
                            Error::Invalid(format!("step must be a number, got {v}"))
                        })?)
                }
                "stroke" | "route" | "heads" | "start" | "end" if !is_arrow => {
                    return Err(Error::Invalid(format!(
                        "`{k}` applies to arrows only ({id} is not an arrow)"
                    )));
                }
                "emphasis" if !is_span => {
                    return Err(Error::Invalid(format!(
                        "`emphasis` applies to highlights and marks only ({id} is not one)"
                    )));
                }
                "emphasis" => p.emphasis = Some(parse_emphasis(v)?),
                "style" if is_span && !is_mark => p.emphasis = Some(parse_emphasis(v)?),
                "border" | "corners" | "fill" if !is_framed => {
                    return Err(Error::Invalid(format!(
                        "`{k}` applies to boxes and callouts only ({id} is not one)"
                    )));
                }
                "border" | "corners" | "fill" => {
                    let mut bs = codedraw_core::model::BorderStyle::default();
                    apply_border_kv(&mut bs, k, v)?;
                    match k.as_str() {
                        "border" => p.border = Some(bs.border),
                        "corners" => p.corners = Some(bs.corners),
                        _ => p.fill = bs.fill,
                    }
                }
                _ => {
                    let mut ls = codedraw_core::model::LineStyle::default();
                    apply_line_kv(&mut ls, k, v)?;
                    match k.as_str() {
                        "stroke" => p.stroke = Some(ls.stroke),
                        "route" => p.route = Some(ls.route),
                        "heads" => {
                            heads.start = Some(ls.heads.start);
                            heads.end = Some(ls.heads.end);
                        }
                        "start" => heads.start = Some(ls.heads.start),
                        "end" => heads.end = Some(ls.heads.end),
                        _ => unreachable!(),
                    }
                }
            }
        }
        if heads.start.is_some() || heads.end.is_some() {
            p.heads = Some(heads);
        }
        self.update_annotation(&p)?;
        Ok(format!("{id}: updated"))
    }

    fn cmd_highlight(&mut self, rest: &str) -> Result<String, Error> {
        // `emphasis=underline` (or `style=`) may trail the label.
        let (rest, kv) = split_trailing_kv(rest);
        let mut emphasis = Emphasis::default();
        for (k, v) in &kv {
            match k.as_str() {
                "emphasis" | "style" => emphasis = parse_emphasis(v)?,
                other => {
                    return Err(Error::Invalid(format!(
                        "unknown key `{other}` for :hl (emphasis=fill|underline|brackets|strike)"
                    )));
                }
            }
        }
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
                emphasis,
            },
        )?;
        Ok(format!("{id}: highlight at L{}", span.line))
    }

    /// `:mark KIND L:text [label]` where KIND is a built-in kind (`warn`, `read`, `move`…),
    /// an alias (`borrow`), a style defined with `:style`, or a bare glyph/emoji for a
    /// one-off inline style (`:mark 🔥 7:&t hot path`). `KIND@COLOR` sets the color.
    fn cmd_mark(&mut self, rest: &str) -> Result<String, Error> {
        let (rest, kv) = split_trailing_kv(rest);
        let mut emphasis = Emphasis::default();
        for (k, v) in &kv {
            match k.as_str() {
                "emphasis" => emphasis = parse_emphasis(v)?,
                other => {
                    return Err(Error::Invalid(format!(
                        "unknown key `{other}` for :mark (emphasis=fill|underline|brackets|strike)"
                    )));
                }
            }
        }
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
                emphasis,
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
        let (text, kv) = split_trailing_kv(rest);
        let mut frame = codedraw_core::model::BorderStyle::default();
        for (k, v) in &kv {
            apply_border_kv(&mut frame, k, v)?;
        }
        if text.trim().is_empty() {
            return Err(Error::Invalid("the callout text is empty".into()));
        }
        let id = self.add_annotation(
            None,
            None,
            Shape::Callout {
                from_line: a,
                to_line: if b == a { None } else { Some(b) },
                text: text.trim().to_string(),
                title,
                placement,
                width: None,
                frame,
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

    /// Entry point of the event loop: answers now, or (supervised mode and a
    /// mutating method) parks the request as a proposal until the presenter
    /// decides. `supervise.*` and read-only methods are never parked.
    pub fn handle_ipc(&mut self, req: Request, reply: std::sync::mpsc::Sender<Response>) {
        let parked = self.supervised
            && !READ_ONLY_METHODS.contains(&req.method.as_str())
            && !req.method.starts_with("supervise.");
        if !parked {
            let _ = reply.send(self.handle_request(req));
            return;
        }
        let summary = proposal_summary(&req);
        let id = self.next_proposal;
        self.next_proposal += 1;
        self.proposals.push(Proposal {
            id,
            req,
            reply,
            since: std::time::Instant::now(),
            summary,
        });
        if !self.present && self.overlay == Overlay::None {
            self.overlay = Overlay::Review;
            self.overlay_scroll = 0;
        }
        self.info(format!(
            "{} proposal{} waiting for review (y accept, n reject; :review)",
            self.proposals.len(),
            if self.proposals.len() == 1 { "" } else { "s" }
        ));
    }

    /// Applies the first pending proposal (`y`).
    pub fn accept_proposal(&mut self) -> Option<String> {
        if self.proposals.is_empty() {
            return None;
        }
        let p = self.proposals.remove(0);
        let resp = self.handle_request(p.req);
        let ok = resp.error.is_none();
        let _ = p.reply.send(resp);
        self.decided.0 += 1;
        self.close_review_if_empty();
        Some(if ok {
            format!("accepted p{}: {}", p.id, p.summary)
        } else {
            format!("p{} accepted but failed: {}", p.id, p.summary)
        })
    }

    /// Rejects the first pending proposal (`n`): the agent gets a `rejected` error.
    pub fn reject_proposal(&mut self) -> Option<String> {
        if self.proposals.is_empty() {
            return None;
        }
        let p = self.proposals.remove(0);
        let _ = p.reply.send(Response::err(
            p.req.id.clone(),
            "rejected",
            "the presenter rejected this change (supervised mode)",
        ));
        self.decided.1 += 1;
        self.close_review_if_empty();
        Some(format!("rejected p{}: {}", p.id, p.summary))
    }

    pub fn accept_all_proposals(&mut self) -> usize {
        let mut n = 0;
        while self.accept_proposal().is_some() {
            n += 1;
        }
        n
    }

    pub fn reject_all_proposals(&mut self) -> usize {
        let mut n = 0;
        while self.reject_proposal().is_some() {
            n += 1;
        }
        n
    }

    fn close_review_if_empty(&mut self) {
        if self.proposals.is_empty() && self.overlay == Overlay::Review {
            self.overlay = Overlay::None;
            self.overlay_scroll = 0;
        }
    }

    /// Expires proposals older than `PROPOSAL_TIMEOUT` (called every tick).
    /// Returns true when something changed.
    pub fn expire_proposals(&mut self) -> bool {
        let mut expired = Vec::new();
        self.proposals.retain(|p| {
            if p.since.elapsed() >= PROPOSAL_TIMEOUT {
                expired.push((p.id, p.req.id.clone(), p.reply.clone()));
                false
            } else {
                true
            }
        });
        if expired.is_empty() {
            return false;
        }
        for (pid, rid, reply) in &expired {
            let _ = reply.send(Response::err(
                rid.clone(),
                "rejected",
                format!(
                    "proposal p{pid} expired after {}s without a decision (supervised mode)",
                    PROPOSAL_TIMEOUT.as_secs()
                ),
            ));
            self.decided.1 += 1;
        }
        self.close_review_if_empty();
        self.info(format!(
            "{} proposal{} expired without a decision",
            expired.len(),
            if expired.len() == 1 { "" } else { "s" }
        ));
        true
    }

    /// Turns supervised mode on or off. Turning it off applies nothing by
    /// itself: pending proposals are rejected so no change sneaks in.
    pub fn set_supervised(&mut self, on: bool) -> String {
        if self.supervised == on {
            return format!("supervised mode already {}", if on { "on" } else { "off" });
        }
        self.supervised = on;
        self.revision += 1;
        if on {
            "supervised mode on: agent changes wait for y/n (:review lists them)".into()
        } else {
            let n = self.reject_all_proposals();
            if n > 0 {
                format!(
                    "supervised mode off; {n} pending proposal{} rejected",
                    if n == 1 { "" } else { "s" }
                )
            } else {
                "supervised mode off".into()
            }
        }
    }

    pub fn supervise_status(&self) -> Value {
        json!({
            "supervised": self.supervised,
            "pending": self.proposals.iter().map(|p| json!({"id": p.id, "method": p.req.method, "summary": p.summary, "age_ms": p.since.elapsed().as_millis() as u64})).collect::<Vec<_>>(),
            "accepted": self.decided.0,
            "rejected": self.decided.1,
            "timeout_secs": PROPOSAL_TIMEOUT.as_secs(),
        })
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
            "api.schema" => serde_json::to_value(codedraw_core::schema::api_schema())?,
            "follow.status" => self.follow_status(),
            "follow.set" => {
                let enabled = params
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| {
                        Error::Invalid("follow.set needs `enabled`: true|false".into())
                    })?;
                let msg = self.set_follow(enabled)?;
                let mut st = self.follow_status();
                st["message"] = json!(msg);
                st
            }
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
            "supervise.set" => {
                let on = params
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .ok_or_else(|| Error::Invalid("`enabled` (true|false) is required".into()))?;
                let msg = self.set_supervised(on);
                let mut st = self.supervise_status();
                st["message"] = json!(msg);
                st
            }
            "supervise.status" => self.supervise_status(),
            "scene.put" => {
                let p: PutParams = parse(params)?;
                serde_json::to_value(self.put_scene(p.document)?)?
            }
            "scene.patch" => {
                let p: PatchParams = parse(params)?;
                serde_json::to_value(self.patch_scene(p)?)?
            }
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
                if params.get("theme").is_some_and(|t| !t.is_string()) {
                    return Err(Error::Invalid(
                        "`theme` must be a string: dark, light or toggle".into(),
                    ));
                }
                match params
                    .get("theme")
                    .and_then(|t| t.as_str())
                    .map(|t| t.trim().to_ascii_lowercase())
                    .as_deref()
                {
                    Some("dark") | Some("oscuro") => self.theme = Theme::dark(),
                    Some("light") | Some("claro") => self.theme = Theme::light(),
                    None | Some("") | Some("toggle") => self.theme = self.theme.toggled(),
                    Some(other) => {
                        return Err(Error::Invalid(format!(
                            "unknown theme: {other} (dark, light, toggle)"
                        )));
                    }
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
            build: codedraw_core::GIT_SHA.into(),
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
            supervised: self.supervised,
            pending: self.proposals.len(),
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
        let (_, pad_h) = self.effective_padding();
        let width = (cols as usize).saturating_sub(2 * pad_h as usize);
        let l = self.layout(width.max(1));
        self.viewport_warnings_for(&l)
    }

    /// `viewport_warnings` for a layout already computed this frame (the
    /// status bar must not lay the document out a second time).
    pub fn viewport_warnings_for(&self, l: &Layout) -> Vec<String> {
        let (cols, rows) = self.size;
        if cols == 0 || rows == 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
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

/// `fill|underline|brackets|strike` → [`Emphasis`], with the options on error.
fn parse_emphasis(v: &str) -> Result<Emphasis, Error> {
    Emphasis::parse(v).ok_or_else(|| {
        Error::Invalid(format!(
            "unknown emphasis: {v} (fill, underline, brackets, strike)"
        ))
    })
}

/// Screen geometry of the last frame: which code line each visible row shows
/// and where the code column starts, for mouse hit-testing.
#[derive(Debug, Clone, Default)]
pub struct HitMap {
    /// Top-left of the canvas on screen.
    pub origin: (u16, u16),
    /// Code line shown on each visible row (`None` for virtual rows).
    pub rows: Vec<Option<u32>>,
    /// Screen column where the code text starts.
    pub code_x: u16,
    pub tab_width: usize,
}

/// What a mouse event did, for the status line and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MouseOutcome {
    Ignored,
    Moved(u32, u32),
    Selecting,
    Selected,
    Scrolled,
}

impl App {
    /// Maps a screen cell to (line, grapheme column); rows without code snap
    /// to the nearest code row above (a label belongs to its line).
    pub fn hit_test(&self, column: u16, row: u16) -> Option<(u32, u32)> {
        let (ox, oy) = self.hit.origin;
        let r = row.checked_sub(oy)? as usize;
        if r >= self.hit.rows.len() || column < ox {
            return None;
        }
        let line = (0..=r).rev().find_map(|i| self.hit.rows[i])?;
        let x = column.saturating_sub(self.hit.code_x) as usize;
        use unicode_segmentation::UnicodeSegmentation;
        use unicode_width::UnicodeWidthStr;
        let text = self.doc.line_text(line).unwrap_or("");
        let mut w = 0usize;
        let mut col = 0u32;
        for g in text.graphemes(true) {
            let gw = if g == "\t" {
                self.hit.tab_width - (w % self.hit.tab_width)
            } else {
                UnicodeWidthStr::width(g).max(1)
            };
            if x < w + gw {
                return Some((line, col));
            }
            w += gw;
            col += 1;
        }
        Some((line, col.saturating_sub(1)))
    }

    /// Handles a mouse event (only when `mouse` is on): click moves the
    /// cursor, drag selects (a span on one line, lines across several) and
    /// leaves the selection open for `h`/`b`/`a`/`n`…, the wheel moves the
    /// cursor three lines (which scrolls).
    pub fn on_mouse(&mut self, kind: MouseKind, column: u16, row: u16) -> MouseOutcome {
        if !self.mouse || self.overlay != Overlay::None {
            return MouseOutcome::Ignored;
        }
        match kind {
            // The wheel moves the cursor three lines (like `j`/`k`): the frame
            // keeps the cursor visible, so moving the cursor is what scrolls.
            MouseKind::ScrollDown => {
                self.move_cursor(3);
                MouseOutcome::Scrolled
            }
            MouseKind::ScrollUp => {
                self.move_cursor(-3);
                MouseOutcome::Scrolled
            }
            MouseKind::Down => {
                let Some((line, col)) = self.hit_test(column, row) else {
                    return MouseOutcome::Ignored;
                };
                if let Mode::Command(_) = self.mode {
                    return MouseOutcome::Ignored;
                }
                self.mode = Mode::Normal;
                self.cursor_line = line;
                self.cursor_col = col;
                self.drag_from = Some((line, col));
                MouseOutcome::Moved(line, col)
            }
            MouseKind::Drag => {
                let Some((from_line, from_col)) = self.drag_from else {
                    return MouseOutcome::Ignored;
                };
                let Some((line, col)) = self.hit_test(column, row) else {
                    return MouseOutcome::Ignored;
                };
                if (line, col) == (from_line, from_col) {
                    return MouseOutcome::Ignored;
                }
                self.cursor_line = line;
                self.cursor_col = col;
                self.mode = Mode::Select {
                    anchor_line: from_line,
                    anchor_col: from_col,
                    linewise: false,
                };
                MouseOutcome::Selecting
            }
            MouseKind::Up => {
                let was_dragging = self.drag_from.take().is_some();
                if was_dragging && matches!(self.mode, Mode::Select { .. }) {
                    self.info(match self.selection() {
                        Some(Selection::Lines(a, b)) => format!(
                            "lines {a}-{b} selected: b box · s spotlight · C callout · n note · h focus (Esc)"
                        ),
                        _ => "selected: h highlight · m mark · a arrow · n note · b box (Esc)".into(),
                    });
                    MouseOutcome::Selected
                } else {
                    MouseOutcome::Ignored
                }
            }
        }
    }

    /// Turns mouse capture on or off; the caller (the event loop) talks to the terminal.
    pub fn set_mouse(&mut self, on: bool) -> String {
        self.mouse = on;
        self.drag_from = None;
        if on {
            "mouse on: click moves, drag selects, wheel scrolls (Shift/Option-drag keeps the terminal's own selection)".into()
        } else {
            "mouse off".into()
        }
    }
}

/// The mouse events the app understands (crossterm stays out of `app`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseKind {
    Down,
    Drag,
    Up,
    ScrollUp,
    ScrollDown,
}

/// One line describing an agent request for the review overlay.
pub fn proposal_summary(req: &Request) -> String {
    let p = &req.params;
    let s = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let n = |k: &str| p.get(k).and_then(|v| v.as_u64());
    let place = |k: &str| -> String {
        match p.get(k) {
            Some(w) if w.is_object() => {
                if let Some(f) = w.get("find").and_then(|v| v.as_str()) {
                    format!("«{f}»")
                } else if let Some(l) = w.get("line").and_then(|v| v.as_u64()) {
                    format!("L{l}")
                } else {
                    "?".into()
                }
            }
            _ => String::new(),
        }
    };
    let text = |k: &str| {
        let t = s(k);
        if t.is_empty() {
            String::new()
        } else {
            format!(" «{}»", t.chars().take(40).collect::<String>())
        }
    };
    match req.method.as_str() {
        "annotation.add" => {
            let kind = s("type");
            let at = match kind.as_str() {
                "box" | "callout" | "spotlight" => match (n("from_line"), n("to_line")) {
                    (Some(a), Some(b)) => format!("L{a}-{b}"),
                    (Some(a), None) => format!("L{a}"),
                    _ => String::new(),
                },
                "note" => n("line").map(|l| format!("L{l}")).unwrap_or_default(),
                "arrow" => format!("{} → {}", place("from"), place("to")),
                _ => place("at"),
            };
            let what = match kind.as_str() {
                "mark" => format!("mark {}", s("kind")),
                k => k.to_string(),
            };
            format!("add {what} {at}{}", text("label") + &text("text"))
        }
        "annotation.update" => format!("update {}{}", s("id"), text("label")),
        "annotation.delete" => format!("delete {}", s("id")),
        "scene.clear" => match n("step") {
            Some(st) => format!("clear step {st}"),
            None => "clear every annotation".into(),
        },
        "scene.put" => "replace the whole document".into(),
        "scene.patch" => "patch the document".into(),
        "style.set" => format!("define style {}", s("name")),
        "step.set" => format!("go to step {}", n("step").unwrap_or(0)),
        "step.next" => "next step".into(),
        "step.prev" => "previous step".into(),
        "focus.set" => "change the focus".into(),
        "theme.set" => format!("theme {}", s("theme")),
        "follow.set" => "toggle follow mode".into(),
        other => other.to_string(),
    }
}

/// Image formats `:export` can ask the binary for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Svg,
    Png,
}

/// Renders a canvas to SVG or PNG bytes (`title` goes into the SVG metadata).
pub type ImageExporter = Box<
    dyn Fn(
            &codedraw_core::canvas::Canvas,
            &Theme,
            ImageFormat,
            Option<&str>,
        ) -> Result<Vec<u8>, String>
        + Send,
>;

/// Export formats, picked by the file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Txt,
    Ansi,
    Md,
    Svg,
    Png,
}

impl ExportFormat {
    pub fn from_path(path: &std::path::Path) -> Option<ExportFormat> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("txt") => Some(ExportFormat::Txt),
            Some("ansi") | Some("ans") => Some(ExportFormat::Ansi),
            Some("md") | Some("markdown") => Some(ExportFormat::Md),
            Some("svg") => Some(ExportFormat::Svg),
            Some("png") => Some(ExportFormat::Png),
            _ => None,
        }
    }
}

/// Splits trailing `key=value` tokens off a free-text argument:
/// `"move stroke=dashed heads=none,arrow"` → (`"move"`, [(stroke, dashed), (heads, "none,arrow")]).
pub fn split_trailing_kv(s: &str) -> (&str, Vec<(String, String)>) {
    let mut text = s.trim_end();
    let mut kv = Vec::new();
    loop {
        let Some(space) = text.rfind(' ') else {
            if is_kv_token(text) {
                kv.push(split_token(text));
                text = "";
            }
            break;
        };
        let tok = &text[space + 1..];
        if is_kv_token(tok) {
            kv.push(split_token(tok));
            text = text[..space].trim_end();
        } else {
            break;
        }
    }
    kv.reverse();
    (text, kv)
}

fn is_kv_token(t: &str) -> bool {
    t.split_once('=').is_some_and(|(k, v)| {
        !k.is_empty() && !v.is_empty() && k.chars().all(|c| c.is_ascii_alphabetic())
    })
}

fn split_token(t: &str) -> (String, String) {
    let (k, v) = t.split_once('=').unwrap_or((t, ""));
    (k.to_string(), v.to_string())
}

/// Applies one `key=value` to a border style; errors list the valid values.
pub fn apply_border_kv(
    bs: &mut codedraw_core::model::BorderStyle,
    k: &str,
    v: &str,
) -> Result<(), Error> {
    use codedraw_core::model::{BorderStyle, Route, Stroke};
    match k {
        "border" | "stroke" => {
            bs.border = Stroke::parse(v).ok_or_else(|| {
                Error::Invalid(format!(
                    "unknown border: {v} (use {})",
                    Stroke::NAMES.join(", ")
                ))
            })?
        }
        "corners" | "route" => {
            bs.corners = Route::parse(v).ok_or_else(|| {
                Error::Invalid(format!(
                    "unknown corners: {v} (use {})",
                    Route::NAMES.join(", ")
                ))
            })?
        }
        "fill" => {
            bs.fill = Some(BorderStyle::parse_fill(v).ok_or_else(|| {
                Error::Invalid(format!("fill must be yes|tint or no|none, got {v}"))
            })?)
        }
        other => {
            return Err(Error::Invalid(format!(
                "unknown key: {other} (border, corners, fill)"
            )));
        }
    }
    Ok(())
}

/// Applies one `key=value` to a line style; errors list the valid values.
pub fn apply_line_kv(
    ls: &mut codedraw_core::model::LineStyle,
    k: &str,
    v: &str,
) -> Result<(), Error> {
    use codedraw_core::model::{Head, Heads, Route, Stroke};
    match k {
        "stroke" => {
            ls.stroke = Stroke::parse(v).ok_or_else(|| {
                Error::Invalid(format!(
                    "unknown stroke: {v} (use {})",
                    Stroke::NAMES.join(", ")
                ))
            })?
        }
        "route" => {
            ls.route = Route::parse(v).ok_or_else(|| {
                Error::Invalid(format!(
                    "unknown route: {v} (use {})",
                    Route::NAMES.join(", ")
                ))
            })?
        }
        "heads" => {
            ls.heads = Heads::parse(v).ok_or_else(|| {
                Error::Invalid(format!(
                    "heads must be END or START,END (each one of {})",
                    Head::NAMES.join(", ")
                ))
            })?
        }
        "start" => {
            ls.heads.start = Head::parse(v).ok_or_else(|| {
                Error::Invalid(format!(
                    "unknown head: {v} (use {})",
                    Head::NAMES.join(", ")
                ))
            })?
        }
        "end" => {
            ls.heads.end = Head::parse(v).ok_or_else(|| {
                Error::Invalid(format!(
                    "unknown head: {v} (use {})",
                    Head::NAMES.join(", ")
                ))
            })?
        }
        other => {
            return Err(Error::Invalid(format!(
                "unknown key: {other} (stroke, route, heads, start, end)"
            )));
        }
    }
    Ok(())
}

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
            Shape::Highlight { span, label, .. } => {
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
                ..
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
