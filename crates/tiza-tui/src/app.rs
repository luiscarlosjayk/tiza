//! Application state and logic, with no drawing dependencies. Both the
//! keyboard and the local API go through here, so an agent and the presenter
//! do exactly the same thing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_core::history::History;
use tiza_core::layout::{Layout, LayoutOptions, layout};
use tiza_core::model::{
    Annotation, BUILTIN_KINDS, Document, Emphasis, MarkStyle, Shape, ThemeName,
};
use tiza_core::moves::{self, Move};
use tiza_core::protocol::{
    AddParams, AnnotationMutated, IdParams, METHODS, MoveIndexParams, MoveList, MoveListed,
    MoveMutated, Mutated, PatchParams, PutParams, Request, Response, Scene, SceneChanged, Status,
    StepGotoParams, StepParams, StepReply, UpdateParams, Where,
};
use tiza_core::theme::Theme;
use tiza_core::{Error, fs};

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
    Span(tiza_core::TextSpan),
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

/// A typing animation in progress (t-098): the moves of the step just
/// reached whose `typing` is `line`/`char`, revealed unit by unit. The code
/// shown is `full` with the content of the pending items blanked out, so the
/// line count — and every shifted annotation — stays put while it types.
#[derive(Debug, Clone)]
pub struct TypingState {
    pub plan: moves::TypingPlan,
    pub index: usize,
    pub revealed: usize,
    last: std::time::Instant,
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

/// Agent changes the view does not show that arrive within this long of
/// each other share one status-bar message (t-155).
pub const AGENT_NOTICE_GROUP: std::time::Duration = std::time::Duration::from_secs(1);

/// The status-bar message about agent changes the view does not show
/// (t-155): what it names, so a change arriving within
/// [`AGENT_NOTICE_GROUP`] joins it, and the text it put on the bar.
#[derive(Debug, Clone)]
struct AgentNotice {
    /// Annotation id and verb (`added`, `updated`), first time each.
    items: Vec<(String, &'static str)>,
    last: std::time::Instant,
    text: String,
}

/// How long a proposal waits for the presenter before it is rejected.
pub const PROPOSAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// How long the result of a `:` command stays on the last row in
/// presentation mode, where there is no status bar to keep it (t-107).
pub const PRESENT_MESSAGE_TTL: std::time::Duration = std::time::Duration::from_secs(2);

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
    "check",
    "follow.status",
    "move.list",
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
    pub arrow_from: Option<tiza_core::Anchor>,
    /// Annotation selected with Tab (its id), for quick edits.
    pub selected: Option<String>,
    pub scroll: usize,
    pub message: Option<(String, bool)>,
    /// Presentation mode only (t-107): until when `message` — the result of
    /// the command just run — is drawn on the last row. `None`: hidden.
    pub present_message_until: Option<std::time::Instant>,
    pub revision: u64,
    pub dirty: bool,
    pub quit: bool,
    pub size: (u16, u16),
    pub socket_path: Option<PathBuf>,
    pub autosave: bool,
    /// `:redact off` shows the author the real text on their own screen;
    /// presentation mode, exports, recordings and agents keep the masks.
    pub redact_shown: bool,
    /// `:layout stable`: the authoring screen uses the take's geometry
    /// (lanes, box band and margin sized for every step at once, t-111) so
    /// rehearsing looks like presenting. Presentation mode and every
    /// export are always stable.
    pub layout_stable: bool,
    /// A reload or paste widened (fail-closed) or dropped a redact: shown as an error after the
    /// reload's own message, so it is never missed.
    redact_drop_notice: Option<String>,
    /// Loud redact notices still to be read (t-135): a reload or paste that
    /// widened or dropped a redact (t-117), a step whose move did (t-140).
    /// `ui::draw` puts them whole on their own rows above the status bar —
    /// never cut, whatever the bar holds — until the next key; a later
    /// reload or message never replaces them (a new notice is added).
    pub redact_alerts: Vec<String>,
    /// The revision [`App::note_masked_texts`] last looked at (t-174).
    masked_checked: Option<u64>,
    /// The texts repeating a masked value already said (t-174).
    masked_said: Vec<String>,
    /// The last autosave failure; stays on the status line until a save succeeds.
    pub save_error: Option<String>,
    /// The first failed autosave just set `message`; the next `info` of the
    /// action that triggered it must not replace it.
    save_warning_pending: bool,
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
    /// Typing animation of the step just reached (t-098).
    pub typing: Option<TypingState>,
    /// When the current step became ready (reached, its typing finished
    /// and its `wait` moves elapsed): the `auto_advance_ms` countdown runs
    /// from here (t-099). `None` while typing or after it fired.
    pub step_ready_at: Option<std::time::Instant>,
    /// Follow the host terminal's appearance (t-119): light or dark from
    /// OSC 11, re-asked on resize. Off once the document, `--theme`, `t`
    /// or `:theme NAME` fixes a theme; `:theme auto` turns it back on.
    pub theme_auto: bool,
    /// What the terminal said about its background (`None`: never asked
    /// or no answer).
    pub host_dark: Option<bool>,
    /// The `[theme]` table of the config file, applied under the
    /// document's own overrides.
    pub config_overrides: Option<tiza_core::theme::ThemeOverrides>,
    /// What the last theme resolution changed or ignored (pushed tokens,
    /// unknown keys), for `:theme` to report.
    pub theme_notes: Vec<String>,
    /// OSC answers arriving through the key stream after a re-query.
    pub osc: crate::appearance::OscCapture,
    /// The live pointer (t-030): `f` (or `pointer.set`) turns it on; while
    /// on, its lines are drawn normally and the rest of the code dimmed on
    /// this screen only. Never in the document, the undo stack, a save or
    /// an export.
    pub pointer: Option<tiza_core::protocol::PointerAt>,
    /// Cursor line and selection lines when the pointer last looked: a
    /// change since then is a motion of the presenter's, and the pointer
    /// follows it ([`App::sync_pointer`]).
    pointer_seen: ((u32, u32), Option<u32>),
    /// The last message about an agent change the view does not show
    /// (t-155), while it may still group the next one.
    agent_notice: Option<AgentNotice>,
    /// Agent changes not shown that arrived in presentation mode: told on
    /// the bar when it ends, never on the take (t-155).
    agent_deferred: Vec<(String, &'static str)>,
}

impl App {
    pub fn new(doc: Document, save_path: PathBuf) -> Self {
        let theme_auto = doc.theme.is_none();
        let theme = Theme::from_name(doc.theme.unwrap_or(ThemeName::Dark));
        let cursor_line = doc.source.first_line;
        let mut app = Self {
            theme_auto,
            host_dark: None,
            config_overrides: None,
            theme_notes: Vec::new(),
            osc: Default::default(),
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
            present_message_until: None,
            revision: 1,
            dirty: false,
            quit: false,
            size: (80, 24),
            socket_path: None,
            autosave: true,
            redact_shown: true,
            layout_stable: false,
            redact_drop_notice: None,
            redact_alerts: Vec::new(),
            masked_checked: None,
            masked_said: Vec::new(),
            save_error: None,
            save_warning_pending: false,
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
            typing: None,
            step_ready_at: Some(std::time::Instant::now()),
            pointer: None,
            pointer_seen: ((cursor_line, cursor_line), None),
            agent_notice: None,
            agent_deferred: Vec::new(),
        };
        app.rebuild_theme();
        app
    }

    // ---------- themes (t-119) ----------

    /// Builds the current theme name again with the host's appearance and
    /// the override tables (config file, then the document's).
    pub fn rebuild_theme(&mut self) {
        let (theme, notes) = Theme::resolve(
            self.theme.name,
            self.host_dark.unwrap_or(self.theme.host_dark),
            self.config_overrides.as_ref(),
            self.doc.theme_overrides.as_ref(),
        );
        self.theme = theme;
        self.theme_notes = notes;
        self.revision += 1;
    }

    /// Switches to `name`. `fix` records it in the document and stops
    /// following the terminal (`t`, `:theme NAME`, `--theme`, a document
    /// that names one); auto-detection passes `false`.
    pub fn set_theme_name(&mut self, name: ThemeName, fix: bool) {
        self.theme.name = name;
        if fix {
            self.theme_auto = false;
            self.doc.theme = Some(name);
        }
        self.rebuild_theme();
    }

    /// Follow the terminal again: the document stops naming a theme and
    /// the last answer (if any) picks light or dark.
    pub fn theme_follow_host(&mut self) {
        self.theme_auto = true;
        self.doc.theme = None;
        match self.host_dark {
            Some(dark) => self.set_theme_name(
                if dark {
                    ThemeName::Dark
                } else {
                    ThemeName::Light
                },
                false,
            ),
            None => self.rebuild_theme(),
        }
    }

    /// The terminal answered (at start-up or after a resize): remember its
    /// appearance and, when following, pick the matching theme.
    pub fn set_host(&mut self, host: crate::appearance::Host) {
        if let Some(dark) = host.dark() {
            self.host_dark = Some(dark);
            if self.theme_auto {
                self.set_theme_name(
                    if dark {
                        ThemeName::Dark
                    } else {
                        ThemeName::Light
                    },
                    false,
                );
            } else {
                self.rebuild_theme();
            }
        }
    }

    /// Parses what `:theme`, `theme.set` and `--theme` accept.
    pub fn parse_theme_name(text: &str) -> Option<ThemeName> {
        match text.trim().to_ascii_lowercase().as_str() {
            "dark" | "oscuro" => Some(ThemeName::Dark),
            "light" | "claro" => Some(ThemeName::Light),
            "terminal" | "term" | "ansi" => Some(ThemeName::Terminal),
            _ => None,
        }
    }

    /// What the status line says about the theme: the name, `·auto` while
    /// following the terminal.
    pub fn theme_status(&self) -> String {
        format!(
            "{}{}",
            theme_label(self.theme.name),
            if self.theme_auto { "·auto" } else { "" }
        )
    }

    /// Applies a theme file (`--theme-file`, `:theme file PATH`) as the
    /// document's overrides.
    pub fn load_theme_file(&mut self, path: &std::path::Path) -> Result<String, Error> {
        let ov = tiza_core::theme::load_theme_file(path).map_err(Error::Invalid)?;
        self.history.record(&self.doc, "theme file");
        self.doc.theme_overrides = Some(ov);
        self.rebuild_theme();
        self.touch();
        Ok(format!(
            "theme file {}: {} token(s) applied on {}{}",
            path.display(),
            self.doc
                .theme_overrides
                .as_ref()
                .map(|o| o.0.len())
                .unwrap_or(0),
            theme_label(self.theme.name),
            if self.theme_notes.is_empty() {
                String::new()
            } else {
                format!(" — {}", self.theme_notes.join("; "))
            }
        ))
    }

    /// Padding in effect: the session override, else the document's, else the default.
    pub fn effective_padding(&self) -> (u16, u16) {
        self.padding_override
            .or(self.doc.padding)
            .unwrap_or(crate::ui::DEFAULT_PADDING)
    }

    pub fn info(&mut self, msg: impl Into<String>) {
        // The one-time "not saved" warning set by the autosave that ran
        // inside this very action wins over the action's own message.
        if self.save_warning_pending {
            self.save_warning_pending = false;
            return;
        }
        self.message = Some((self.shown(&msg.into()), false));
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        self.save_warning_pending = false;
        self.message = Some((self.shown(&msg.into()), true));
    }

    /// Document-derived text as the bar, the alerts and the exports show
    /// it (t-174): a copy of a masked value is masked
    /// ([`tiza_core::redact::shown`]).
    pub fn shown(&self, text: &str) -> String {
        tiza_core::redact::shown(text, &self.doc)
    }

    /// Says once, on the alert row (t-135), each text of the document that
    /// repeats a masked value (t-174): `a3: its label repeats masked text;
    /// masked on screen — fix the label`. Checked once per revision; a
    /// text fixed and broken again is said again.
    pub fn note_masked_texts(&mut self) {
        if self.masked_checked == Some(self.revision) {
            return;
        }
        self.masked_checked = Some(self.revision);
        let found = tiza_core::redact::masked_texts(&self.doc);
        let keys: Vec<String> = found.iter().map(|m| m.key()).collect();
        self.masked_said.retain(|k| keys.contains(k));
        for (m, k) in found.into_iter().zip(keys) {
            if !self.masked_said.contains(&k) {
                self.masked_said.push(k);
                self.redact_alert(m.notice());
            }
        }
    }

    /// A loud redact notice (t-117, t-140): the error message, as before,
    /// and a pending alert drawn whole above the bar until the next key
    /// (t-135). An alert already pending stays; the same text is not
    /// repeated.
    pub fn redact_alert(&mut self, notice: String) {
        let notice = self.shown(&notice);
        self.error(notice.clone());
        if !self.redact_alerts.contains(&notice) {
            self.redact_alerts.push(notice);
        }
    }

    /// The presenter pressed a key with the alerts on screen: they have been
    /// seen (t-135). `true` when there was something to hide.
    pub fn dismiss_redact_alerts(&mut self) -> bool {
        let had = !self.redact_alerts.is_empty();
        self.redact_alerts.clear();
        had
    }

    /// Whether `msg` is one of the pending redact alerts, already drawn whole
    /// above the bar (the bar and the presentation row then skip it).
    pub fn is_redact_alert(&self, msg: &str) -> bool {
        self.redact_alerts.iter().any(|a| a == msg)
    }

    /// Every annotation id with its step: taken before an action so
    /// [`App::add_step_hint`] can tell which ones it added or moved (t-132).
    fn annotation_steps(&self) -> HashMap<String, u32> {
        self.doc
            .annotations
            .iter()
            .map(|a| (a.id.clone(), a.step))
            .collect()
    }

    /// An action that added (or moved) annotations to a step after the one
    /// on screen — a new one at step 0 goes to step 1 (t-079), `:step ID N`
    /// past the view — ends its confirmation saying where they went and how
    /// to get there: `a17: box 3-5 (step 1, press n)` (t-132). Only a new
    /// info message gets it; errors, an unchanged message and a message that
    /// already carries the hint are left alone.
    fn add_step_hint(
        &mut self,
        before: &HashMap<String, u32>,
        prev_message: Option<&(String, bool)>,
    ) {
        let shown = self.doc.step;
        let Some(target) = self
            .doc
            .annotations
            .iter()
            .filter(|a| before.get(&a.id) != Some(&a.step) && a.step > shown)
            .map(|a| a.step)
            .min()
        else {
            return;
        };
        let Some((msg, false)) = &self.message else {
            return;
        };
        if prev_message.is_some_and(|p| p.0 == *msg) || step_hint_start(msg).is_some() {
            return;
        }
        let hint = step_hint(msg, target, shown);
        if let Some((m, _)) = &mut self.message {
            m.push_str(&hint);
        }
    }

    /// The layout every reader gets (render.text, screen captures): masks on.
    pub fn layout(&self, width: usize) -> Layout {
        self.layout_with_masks(width, true)
    }

    /// The author's own screen: masks off only after `:redact off`, and
    /// never in presentation mode.
    pub fn screen_layout(&self, width: usize) -> Layout {
        self.layout_with_masks(width, self.redact_shown || self.present)
    }

    /// [`App::screen_layout`] with the live pointer (t-030). Only the
    /// presenter's frame (`ui::draw`) calls it; `layout` (`render.text`,
    /// screen captures) and every export never draw the pointer.
    pub fn screen_layout_with_pointer(&self, width: usize) -> Layout {
        let mut opts = self.layout_options(width, self.redact_shown || self.present);
        opts.pointer = self.pointer_shown();
        layout(&self.doc, &self.theme, &opts)
    }

    fn layout_with_masks(&self, width: usize, redact: bool) -> Layout {
        layout(&self.doc, &self.theme, &self.layout_options(width, redact))
    }

    fn layout_options(&self, width: usize, redact: bool) -> LayoutOptions {
        LayoutOptions {
            width,
            step: self.doc.step,
            focus: self.focus,
            pointer: None,
            line_numbers: self.doc.numbers,
            tab_width: 4,
            typing: self.typing.is_some(),
            redact,
            // Presentation mode is a take: one geometry for every step
            // (t-111). Authoring grows as it goes unless `:layout stable`.
            stable: self.present || self.layout_stable,
            margin_cap: self.doc.margin.map(usize::from),
        }
    }

    // ---------- live pointer (t-030) ----------

    /// The lines the presenter's cursor names: the selection's lines while
    /// one is open (`v`/`V`), else the cursor line.
    fn cursor_lines(&self) -> (u32, u32) {
        match self.selection() {
            Some(Selection::Lines(a, b)) => (a, b),
            Some(Selection::Span(sp)) => (sp.line, sp.line),
            None => (self.cursor_line, self.cursor_line),
        }
    }

    fn pointer_snapshot(&self) -> ((u32, u32), Option<u32>) {
        let anchor = match self.mode {
            Mode::Select { anchor_line, .. } => Some(anchor_line),
            _ => None,
        };
        (self.cursor_lines(), anchor)
    }

    /// The pointer lines to draw now: none while the command line is open
    /// (an agent's pointer shows when it closes) or when it is off.
    pub fn pointer_shown(&self) -> Option<(u32, u32)> {
        if matches!(self.mode, Mode::Command(_)) {
            return None;
        }
        self.pointer.map(|p| (p.line, p.to))
    }

    /// `f`: the pointer on the cursor line (or the selection), or off.
    pub fn toggle_pointer(&mut self) {
        if self.pointer.take().is_some() {
            self.info("pointer off");
        } else {
            self.point_at_cursor();
            self.info("pointer on (f or Esc turns it off; never saved nor exported)");
        }
    }

    fn point_at_cursor(&mut self) {
        let (line, to) = self.cursor_lines();
        self.pointer = Some(tiza_core::protocol::PointerAt {
            line,
            to,
            by: tiza_core::protocol::PointerBy::Presenter,
        });
        self.pointer_seen = self.pointer_snapshot();
    }

    /// The pointer follows the presenter's motions (keys, mouse): when the
    /// cursor or the selection moved since it last looked, it moves there.
    /// A pointer whose lines no longer exist (a reload, a paste) turns
    /// off. Called after every key and before every frame.
    pub fn sync_pointer(&mut self) {
        let now = self.pointer_snapshot();
        if now != self.pointer_seen {
            if self.pointer.is_some() {
                self.point_at_cursor();
            }
            self.pointer_seen = now;
        }
        if let Some(p) = self.pointer
            && (p.line < self.doc.source.first_line || p.to > self.doc.last_line())
        {
            self.pointer = None;
        }
    }

    /// `pointer.set` (t-030): resolves the lines on the code shown — a
    /// `find` on the masked code, so it never confirms text under a redact
    /// — moves the cursor there (the view scrolls to it, `j`/`k` go on
    /// from it) and points. Nothing is recorded: no revision, no undo, no
    /// save.
    pub fn set_pointer(
        &mut self,
        p: &tiza_core::protocol::PointerParams,
    ) -> Result<tiza_core::protocol::PointerAt, Error> {
        let first = self.doc.source.first_line;
        let last = self.doc.last_line();
        let check = |line: u32| {
            if line < first || line > last {
                Err(Error::LineOutOfRange { line, first, last })
            } else {
                Ok(line)
            }
        };
        let (a, b) = match (p.line, &p.find, p.from, p.to) {
            (_, Some(_), Some(_), _)
            | (_, Some(_), _, Some(_))
            | (Some(_), _, Some(_), _)
            | (Some(_), _, _, Some(_)) => {
                return Err(Error::Invalid(
                    "pointer.set: give `line` (with an optional `find`), `find`, or `from`/`to`, not both"
                        .into(),
                ));
            }
            (None, None, None, Some(_)) => {
                return Err(Error::Invalid("pointer.set: `to` needs `from`".into()));
            }
            (None, None, None, None) => {
                return Err(Error::Invalid(
                    "pointer.set needs `line`, `line` + `find`, `find` or `from`/`to` (pointer.clear turns it off)"
                        .into(),
                ));
            }
            (None, None, Some(f), t) => {
                let t = t.unwrap_or(f);
                (check(f.min(t))?, check(f.max(t))?)
            }
            (line, Some(text), None, None) => {
                if text.is_empty() {
                    return Err(Error::Invalid("pointer.set: empty `find`".into()));
                }
                let view = self.doc.view_at(self.doc.step);
                let code = tiza_core::redact::masked_code(&view.doc, self.doc.step);
                let hits = moves::matches_of(&code, first, text);
                let l = match line {
                    Some(l) => {
                        let l = check(l)?;
                        if !hits.contains(&l) {
                            return Err(Error::TextNotFound(format!("«{text}» on line {l}")));
                        }
                        l
                    }
                    None => {
                        let mut lines = hits.clone();
                        lines.dedup();
                        match lines.as_slice() {
                            [] => return Err(Error::TextNotFound(format!("«{text}»"))),
                            [l] => *l,
                            many => {
                                return Err(Error::Invalid(format!(
                                    "pointer.set: «{text}» is on {} lines ({}); add `line`",
                                    many.len(),
                                    many.iter()
                                        .map(|l| format!("L{l}"))
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                )));
                            }
                        }
                    }
                };
                (l, l)
            }
            (Some(l), None, None, None) => {
                let l = check(l)?;
                (l, l)
            }
        };
        // The presenter's selection gives way: the last one wins.
        if matches!(self.mode, Mode::Select { .. }) {
            self.mode = Mode::Normal;
        }
        self.cursor_line = a;
        self.clamp_col();
        let at = tiza_core::protocol::PointerAt {
            line: a,
            to: b,
            by: tiza_core::protocol::PointerBy::Agent,
        };
        self.pointer = Some(at);
        self.pointer_seen = self.pointer_snapshot();
        Ok(at)
    }

    /// `pointer.clear`, `Esc`, a second `f`: off.
    pub fn clear_pointer(&mut self) {
        self.pointer = None;
    }

    /// A step change keeps the pointer on its line, carried through the
    /// moves of the steps in between (t-030); off when the line is gone.
    /// The cursor goes with it, so the view and `j`/`k` stay with it.
    fn carry_pointer(&mut self, from_step: u32) {
        let Some(p) = self.pointer else { return };
        let to_step = self.doc.step;
        let carried = moves::carry_line(&self.doc, from_step, to_step, p.line)
            .zip(moves::carry_line(&self.doc, from_step, to_step, p.to));
        let first = self.doc.source.first_line;
        let last = self.doc.last_line();
        match carried {
            Some((a, b)) if a >= first && b <= last && a <= b => {
                self.pointer = Some(tiza_core::protocol::PointerAt {
                    line: a,
                    to: b,
                    by: p.by,
                });
                self.cursor_line = a;
            }
            _ => self.pointer = None,
        }
    }

    /// Whether the document has redact masks (for the status bar).
    pub fn has_redacts(&self) -> bool {
        tiza_core::redact::has_redacts(&self.doc)
    }

    // ---------- mutations ----------

    fn touch(&mut self) {
        self.revision += 1;
        self.dirty = true;
        self.note_masked_texts();
        if self.autosave {
            // `save` records the failure in `save_error`, apart from `message`,
            // so a later info message cannot hide it.
            let _ = self.save();
        }
    }

    /// Writes the sidecar. A sidecar that cannot be written (read-only
    /// file or folder) puts the instance in the same state as `:autosave
    /// off` — one rule for `--no-save`, `:autosave off`, a foreign `scene
    /// put` and read-only (t-110, t-039): ONE notice on the first failed
    /// autosave, then silence on the message line and the persistent
    /// `read-only • unsaved (:w)` on the bar; `:q` refuses while there are
    /// unsaved changes; the first autosave that works again says so.
    pub fn save(&mut self) -> Result<(), Error> {
        // A document that follows the terminal keeps following it: only a
        // fixed theme is written (t-119).
        if !self.theme_auto {
            self.doc.theme = Some(self.theme.name);
        }
        match fs::save_document(&self.doc, &self.save_path) {
            Ok(()) => {
                self.dirty = false;
                if self.save_error.take().is_some() {
                    // Writable again: say it once, over the action's own
                    // confirmation, and the bar indicator goes away.
                    self.message = Some((format!("saved to {}", self.save_path.display()), false));
                    self.save_warning_pending = true;
                }
                Ok(())
            }
            Err(e) => {
                if self.save_error.is_none() {
                    self.error(format!(
                        "read-only: could not save {} — :w PATH to save elsewhere",
                        self.save_path.display()
                    ));
                    self.save_warning_pending = true;
                }
                self.save_error = Some(e.to_string());
                Err(e)
            }
        }
    }

    /// Quits unless there are unsaved changes: ones that could not be
    /// written, or ones made with autosave off (`force` ignores both).
    pub fn request_quit(&mut self, force: bool) {
        if !force && self.dirty {
            if self.save_error.is_some() {
                self.error(format!(
                    "unsaved (read-only): :q! discards, :w PATH saves elsewhere — {}",
                    self.save_path.display()
                ));
                return;
            }
            if !self.autosave {
                self.error(format!(
                    "unsaved changes (autosave off): :w saves to {}, :w PATH saves elsewhere, :q! discards",
                    self.save_path.display()
                ));
                return;
            }
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
        self.add_annotation_full(id, step, color, None, None, shape)
    }

    /// `add_annotation_colored` plus the range markers a box/callout/spotlight
    /// was created from (t-097).
    pub fn add_annotation_full(
        &mut self,
        id: Option<String>,
        step: Option<u32>,
        color: Option<String>,
        markers: Option<tiza_core::model::RangeMarkers>,
        file: Option<String>,
        shape: Shape,
    ) -> Result<String, Error> {
        if let Some(c) = &color
            && self.theme.named_color(c).is_none()
        {
            return Err(Error::Invalid(format!(
                "unknown color: {c} (use {} or #rrggbb)",
                tiza_core::theme::PALETTE.join(", ")
            )));
        }
        self.history.record(&self.doc, "add");
        let step = step.unwrap_or(self.doc.step).max(1);
        let kind = shape.kind_name();
        // Marking the value again over a widened redact (t-117) replaces
        // it: the precise mask takes over, the widened state goes away.
        let replaced: Vec<String> = match &shape {
            Shape::Redact {
                span,
                widened: false,
                ..
            } if file.is_none() => self
                .doc
                .annotations
                .iter()
                .filter(|a| {
                    a.file.is_none()
                        && matches!(&a.shape, Shape::Redact { span: w, widened: true, .. }
                            if w.line == span.line && w.start < span.end && span.start < w.end)
                })
                .map(|a| a.id.clone())
                .collect(),
            _ => Vec::new(),
        };
        let res = self.doc.add(Annotation {
            id: id.unwrap_or_default(),
            step,
            color,
            markers,
            file,
            shape,
        });
        if res.is_ok() {
            for id in &replaced {
                let _ = self.doc.delete(id);
            }
        }
        match res {
            Ok(id) => {
                self.history.label_last(format!("{kind} {id}"));
                self.touch();
                Ok(id)
            }
            Err(e) => {
                self.history.discard_last();
                Err(e)
            }
        }
    }

    pub fn delete_annotation(&mut self, id: &str) -> Result<(), Error> {
        self.history.record(&self.doc, format!("delete {id}"));
        match self.doc.delete(id) {
            Ok(_) => {
                self.touch();
                Ok(())
            }
            Err(e) => {
                self.history.discard_last();
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
        self.history.record(&self.doc, "clear");
        match step {
            Some(st) => self.doc.annotations.retain(|a| a.step != st),
            None => self.doc.annotations.clear(),
        }
        let removed = before - self.doc.annotations.len();
        if removed == 0 {
            self.history.discard_last();
            return 0;
        }
        self.history.label_last(format!(
            "{removed} annotation{} cleared",
            if removed == 1 { "" } else { "s" }
        ));
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
                    tiza_core::theme::PALETTE.join(", ")
                )));
            }
        }
        if let Some(spec) = &p.shape {
            let (shape, markers) = spec.resolve_with_markers(&self.doc)?;
            ann.shape = shape;
            ann.markers = markers;
        }
        if let Some(mv) = &p.mv {
            mv.apply(&mut ann.shape, &self.doc)?;
            // Moved by lines: the markers no longer describe the range.
            if mv.from_line.is_some() || mv.to_line.is_some() {
                ann.markers = None;
            }
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
                | Shape::Arrow { label, .. }
                | Shape::Redact { label, .. } => *label = l,
                Shape::Note { text, .. } => {
                    if let Some(l) = l {
                        *text = l;
                    }
                }
                Shape::Callout { title, .. } => *title = l,
                Shape::Spotlight { .. } => {}
            }
        }
        self.history.record(&self.doc, format!("update {}", p.id));
        match self.doc.update(ann) {
            Ok(()) => {
                self.touch();
                Ok(())
            }
            Err(e) => {
                self.history.discard_last();
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
                tiza_core::model::CodeSource::Embedded(lines) => doc.code = lines.clone(),
                _ => {
                    return Err(Error::Invalid(
                        "scene.put needs the code embedded (`\"code\": [lines…]`); resolve file references before sending".into(),
                    ));
                }
            }
        }
        // Linked files (t-105): embedded in `views`, else read next to the
        // open document.
        let base = self.save_path.parent().map(Path::to_path_buf);
        fs::load_views(&mut doc, base.as_deref())?;
        doc.normalize_loaded()?;
        for a in &doc.annotations {
            doc.validate_annotation(a)
                .map_err(|e| tiza_core::redact::Echo::new(&doc).error(e))?;
        }
        self.finish_typing();
        // Saving rule (t-039/t-110): a document from another source must
        // never be autosaved over the file that was open. Autosave stops;
        // `:w` writes it over that file on purpose, `:w PATH` elsewhere,
        // `:autosave on` resumes. The same source (an agent re-putting an
        // edited copy of the open document, `scene get` → `scene put`)
        // keeps saving as usual.
        let other = doc.source.path != self.doc.source.path;
        self.history.record_with_step(&self.doc, "scene.put");
        self.doc = doc;
        self.step_ready_at = Some(std::time::Instant::now());
        match self.doc.theme {
            Some(t) => self.set_theme_name(t, true),
            None => self.rebuild_theme(),
        }
        self.selected = None;
        self.focus = None;
        self.scroll = 0;
        self.clamp_cursor();
        if other && self.autosave {
            self.autosave = false;
            self.info(format!(
                "another document loaded: autosave off (:w saves over {}, :w PATH saves elsewhere, :autosave on resumes)",
                self.save_path.display()
            ));
        }
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
        if let Some(mut steps) = p.steps {
            tiza_core::steps::normalize(&mut steps)?;
            d.steps = steps;
        }
        if let Some(l) = p.looping {
            d.looping = l;
        }
        if let Some(pad) = p.padding {
            d.padding = pad;
        }
        if let Some(n) = p.numbers {
            d.numbers = n;
        }
        if let Some(m) = p.margin {
            d.margin = match m {
                Some(cap) => Some(tiza_core::layout::check_margin_cap(usize::from(cap))? as u16),
                None => None,
            };
        }
        if let Some(theme) = p.theme {
            d.theme = Some(theme);
        }
        if let Some(step) = p.step {
            d.step = step.min(d.max_step().max(1));
        } else if d.step > d.max_step() {
            d.step = d.max_step();
        }
        self.history.record_with_step(&self.doc, "scene.patch");
        // A new `auto_advance_ms` counts from now, not from the arrival.
        if d.steps != self.doc.steps && self.typing.is_none() {
            self.step_ready_at = Some(std::time::Instant::now());
        }
        self.doc = d;
        match self.doc.theme {
            Some(t) => self.set_theme_name(t, true),
            None => self.rebuild_theme(),
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

    /// `:next`, `:prev`, `:step N`: goes to `step` and says where it is,
    /// unless the step raised a redact notice (t-154): then the notice
    /// stays on the bar, as with the `n` key, and nothing replaces it.
    fn step_message(&mut self, step: u32) -> String {
        if self.step_to(step) {
            return String::new();
        }
        self.doc.step_label(self.doc.step)
    }

    pub fn set_step(&mut self, step: u32) {
        self.step_to(step);
    }

    /// `n`, `→`, `:next`, `step.next` and the auto-advance: one step on, or
    /// back to step 0 past the last one when the document loops (t-099).
    pub fn next_step(&mut self) -> bool {
        match self.doc.next_step_from(self.doc.step) {
            Some(0) => self.step_to_with(0, true),
            Some(n) => self.step_to(n),
            // At the last step: as before, nothing moves.
            None => self.step_to(self.doc.step + 1),
        }
    }

    /// `step.goto`, MCP `goto_step`, `tiza goto ID`, `:goto ID` (t-099):
    /// the step a named id (or a number) names, landed on exactly as `n`
    /// would after a full replay — the document is recomputed from step 0
    /// (moves, views, redacts), the typing of that step complete, and the
    /// redact notices of every step on the way said once. Returns the step.
    pub fn goto_step(&mut self, id: &str) -> Result<u32, Error> {
        let s = self.doc.resolve_step(id)?;
        self.step_to_with(s, true);
        Ok(self.doc.step)
    }

    /// The `auto_advance_ms` countdown (t-099): when the current step has
    /// one and has been ready that long, go to the next step. Deferred while
    /// the command line, a `v`/`V` selection or an overlay is open (it fires
    /// once they close, so a selection is never lost under the presenter),
    /// never while a typed move or a `wait` is still running. `true` when
    /// the screen changed.
    pub fn auto_advance_tick(&mut self, now: std::time::Instant) -> bool {
        if self.typing.is_some()
            || matches!(self.mode, Mode::Command(_) | Mode::Select { .. })
            || self.overlay != Overlay::None
        {
            return false;
        }
        let (Some(ms), Some(ready)) = (self.doc.auto_advance_ms(self.doc.step), self.step_ready_at)
        else {
            return false;
        };
        if now.saturating_duration_since(ready) < std::time::Duration::from_millis(ms) {
            return false;
        }
        // Fires once per arrival: a last step without `loop` stays put.
        self.step_ready_at = None;
        let before = (self.doc.step, self.revision);
        self.next_step();
        (self.doc.step, self.revision) != before
    }

    /// Goes to `step` (moves applied, typing started going forward one).
    /// `true` when it said something loud on the bar: a move that does not
    /// apply, or a redact notice (t-140).
    fn step_to(&mut self, step: u32) -> bool {
        self.step_to_with(step, false)
    }

    /// [`App::step_to`]; `jump` (goto, the loop back to step 0) lands with
    /// the step's typing complete and says the redact notices of every step
    /// up to it, as walking there would have.
    fn step_to_with(&mut self, step: u32, jump: bool) -> bool {
        let mut loud = false;
        self.finish_typing();
        let max = self.doc.max_step();
        let s = step.min(max.max(1));
        if s != self.doc.step {
            let forward_one = s == self.doc.step + 1 && !jump;
            let from_step = self.doc.step;
            self.doc.step = s;
            if let Some(e) = self.doc.sync_code_to_step() {
                self.error(format!("moves: {e}"));
                self.pointer = None;
                loud = true;
            } else {
                // A `cursor` move left at this step moves the TUI cursor
                // too; a `code.link` puts it on the first shown line (t-105).
                if !self.doc.moves.is_empty() {
                    let v = self.doc.view_at(s);
                    if let Some(c) = v.cursor {
                        self.cursor_line = c.line;
                        self.cursor_col = c.col;
                    } else if v.view.file.is_some() || v.view.window.is_some() {
                        self.cursor_line = self.doc.source.first_line;
                    }
                }
                // Before the typing blanks what it is about to type.
                self.carry_pointer(from_step);
                if forward_one {
                    self.start_typing();
                }
                // A move of this step could not carry a redact with its
                // text (t-140): loud, before anything is recorded.
                let notices = self.doc.redact_notices_at(s, jump);
                if !notices.is_empty() {
                    self.redact_alert(notices.join(" · "));
                    loud = true;
                }
            }
            self.clamp_cursor();
            // The step moved the cursor (to the pointer, a `cursor` move):
            // not a motion of the presenter's for the pointer to follow.
            self.pointer_seen = self.pointer_snapshot();
            self.revision += 1;
            // The auto-advance countdown starts now, or when the typing ends.
            self.step_ready_at = self.typing.is_none().then(std::time::Instant::now);
        }
        loud
    }

    // ---------- moves (t-098) ----------

    /// Starts the typing animation of the current step, when any of its
    /// moves asks for one. The content of those moves is blanked out and
    /// revealed by `typing_tick`.
    fn start_typing(&mut self) {
        let Some(plan) = moves::typing_plan(&self.doc, self.doc.step) else {
            return;
        };
        self.typing = Some(TypingState {
            plan,
            index: 0,
            revealed: 0,
            last: std::time::Instant::now(),
        });
        self.refresh_typing_code();
    }

    /// Writes the partially typed code into the document.
    fn refresh_typing_code(&mut self) {
        let Some(t) = &self.typing else { return };
        self.doc.code = t
            .plan
            .code_at(self.doc.source.first_line, t.index, t.revealed);
    }

    /// Advances the typing animation by the time elapsed. Returns true when
    /// the screen changed. Call it every loop iteration.
    pub fn typing_tick(&mut self) -> bool {
        let Some(t) = &mut self.typing else {
            return false;
        };
        let elapsed = t.last.elapsed();
        let item = &t.plan.items[t.index];
        let speed = std::time::Duration::from_millis(item.speed_ms);
        let units = (elapsed.as_micros() / speed.as_micros().max(1)) as usize;
        if units == 0 {
            return false;
        }
        t.last += speed * units as u32;
        t.revealed += units;
        while t.revealed >= t.plan.items[t.index].units {
            t.revealed -= t.plan.items[t.index].units;
            t.index += 1;
            if t.index >= t.plan.items.len() {
                self.finish_typing();
                self.revision += 1;
                return true;
            }
        }
        self.refresh_typing_code();
        self.revision += 1;
        true
    }

    /// Ends the typing animation at once (any key, `p`, a socket step change).
    pub fn finish_typing(&mut self) {
        if let Some(t) = self.typing.take() {
            self.doc.code = t.plan.full;
            // The step is ready: its auto-advance countdown starts (t-099).
            self.step_ready_at = Some(std::time::Instant::now());
        }
    }

    pub fn is_typing(&self) -> bool {
        self.typing.is_some()
    }

    /// Adds a move (validated against the code at its step). Undoable. A
    /// `code.link` reads its file next to the document first (t-105).
    pub fn add_move(&mut self, m: Move) -> Result<usize, Error> {
        self.add_move_noting(m).map(|(i, _)| i)
    }

    /// [`App::add_move`] plus the moves it retargets (t-157): a `position`
    /// move of a later step that now hits another line. Said loudly, on
    /// rows of their own (t-135); `u` undoes the move and clears them.
    pub fn add_move_noting(&mut self, m: Move) -> Result<(usize, Vec<String>), Error> {
        self.finish_typing();
        if let tiza_core::moves::MoveAction::CodeLink(l) = &m.action {
            let base = self.save_path.parent().map(Path::to_path_buf);
            fs::load_view(&mut self.doc, &l.file, base.as_deref())?;
        }
        self.history.record(&self.doc, "move added");
        match self.doc.add_move_noting(m) {
            Ok((i, notices)) => {
                self.history.label_last(format!("move #{} added", i + 1));
                self.clamp_cursor();
                self.touch();
                if !notices.is_empty() {
                    self.redact_alert(notices.join(" · "));
                }
                Ok((i, notices))
            }
            Err(e) => {
                self.history.undo(&mut self.doc);
                Err(e)
            }
        }
    }

    /// Deletes a move by 1-based index. Undoable.
    pub fn delete_move(&mut self, index: usize) -> Result<Move, Error> {
        self.delete_move_noting(index).map(|(m, _)| m)
    }

    /// [`App::delete_move`] plus the redacts it dropped (t-154): the move
    /// wrote their line. Said loudly; `u` brings both back.
    pub fn delete_move_noting(&mut self, index: usize) -> Result<(Move, Vec<String>), Error> {
        self.finish_typing();
        let i = index
            .checked_sub(1)
            .filter(|i| *i < self.doc.moves.len())
            .ok_or_else(|| Error::NotFound(format!("move #{index}")))?;
        self.history
            .record(&self.doc, format!("move #{index} deleted"));
        let (m, notices) = self.doc.delete_move_noting(i)?;
        if self.doc.step > self.doc.max_step() {
            self.doc.step = self.doc.max_step();
            self.doc.sync_code_to_step();
        }
        self.clamp_cursor();
        self.touch();
        if !notices.is_empty() {
            self.redact_alert(notices.join(" · "));
        }
        Ok((m, notices))
    }

    /// `scene.list` / `:list`: one line per annotation, in document order.
    /// With moves, the coordinates are where the annotation is drawn at the
    /// current step (shifted through the patches); one that lost its anchor
    /// keeps its authoring coordinates and is marked `(unanchored)`.
    pub fn list_lines(&self) -> Vec<String> {
        let echo = tiza_core::redact::Echo::quoting(&self.doc);
        let describe = |a: &Annotation| tiza_core::protocol::describe_with(a, &echo);
        if self.doc.moves.is_empty() {
            return self.doc.annotations.iter().map(describe).collect();
        }
        let view = self.doc.view_at(self.doc.step);
        self.doc
            .annotations
            .iter()
            .map(|a| {
                if view.unanchored.contains(&a.id) {
                    format!("{} (unanchored)", describe(a))
                } else {
                    let shown = view.doc.find_annotation(&a.id).unwrap_or(a);
                    describe(shown)
                }
            })
            .collect()
    }

    /// `move.list` payload.
    pub fn move_list(&self) -> MoveList {
        // Content masked where it lands under a redact (t-140).
        let shown = tiza_core::moves::masked_moves(&self.doc);
        MoveList {
            moves: shown
                .into_iter()
                .enumerate()
                .map(|(i, m)| MoveListed {
                    index: i + 1,
                    summary: tiza_core::protocol::describe_move(i + 1, &m),
                    r#move: m,
                })
                .collect(),
            unanchored: self.doc.unanchored(),
            step: self.doc.step,
            max_step: self.doc.max_step(),
            view: self.doc.shown_view_at(self.doc.step).label(),
        }
    }

    /// The file shown at the current step: the one a `code.link` left up
    /// (as the move names it), else the source file.
    pub fn shown_file(&self) -> Option<String> {
        let v = self.doc.shown_view_at(self.doc.step);
        v.file.or_else(|| self.doc.source.path.clone())
    }

    /// Why the code cannot be replaced right now (paste, follow): moves are
    /// applied at the current step. `None` when it can.
    fn moves_block_reason(&self, what: &str) -> Option<Error> {
        self.doc.moves_applied().then(|| {
            Error::Invalid(format!(
                "cannot {what} while moves are applied (step {}): go back to step 0 first",
                self.doc.step
            ))
        })
    }

    pub fn undo(&mut self) {
        self.finish_typing();
        if let Some(what) = self.history.undo(&mut self.doc) {
            self.rebuild_theme();
            self.touch();
            self.info(format!("undone: {what}"));
        } else {
            self.info("nothing to undo");
        }
    }

    /// Redoes the last undone edit. Redoing a move added or deleted shifts
    /// the code again under every later `position` move it retargeted, so
    /// the presenter is told again, as when it was first made (t-157).
    pub fn redo(&mut self) {
        self.finish_typing();
        let before = self.doc.clone();
        if let Some(what) = self.history.redo(&mut self.doc) {
            self.rebuild_theme();
            self.touch();
            self.info(format!("redone: {what}"));
            let notices = tiza_core::moves::retarget_notices_between(&before, &self.doc);
            if !notices.is_empty() {
                self.redact_alert(notices.join(" · "));
            }
        } else {
            self.info("nothing to redo");
        }
    }

    /// Enters or leaves presentation mode. Proposals that arrived while
    /// presenting were only a chip on screen; coming back opens the list.
    /// Agent changes the view did not show while presenting are told on
    /// the bar now (t-155).
    pub fn set_present(&mut self, on: bool) {
        self.present = on;
        self.present_message_until = None;
        if !on && !self.proposals.is_empty() && self.overlay == Overlay::None {
            self.overlay = Overlay::Review;
            self.overlay_scroll = 0;
        }
        if !on && !self.agent_deferred.is_empty() {
            let items = std::mem::take(&mut self.agent_deferred);
            self.agent_notice = None;
            if let Some(text) = self.agent_notice_text(&items) {
                self.info(text);
            }
        }
    }

    /// A `:` command just ran (Enter): in presentation mode its message
    /// stays on the last row for [`PRESENT_MESSAGE_TTL`] (t-107).
    pub fn command_finished(&mut self, now: std::time::Instant) {
        self.present_message_until = if self.present && self.message.is_some() {
            Some(now + PRESENT_MESSAGE_TTL)
        } else {
            None
        };
    }

    /// Hides the presentation-mode message once its time is up; `true` when
    /// the frame must be drawn again.
    pub fn present_message_tick(&mut self, now: std::time::Instant) -> bool {
        match self.present_message_until {
            Some(until) if now >= until => {
                self.present_message_until = None;
                if self.present {
                    // Seen and gone: it must not come back on the status
                    // bar when presentation mode ends.
                    self.message = None;
                }
                true
            }
            _ => false,
        }
    }

    /// Whether the last row shows a command result in presentation mode.
    pub fn present_message_shown(&self) -> bool {
        self.present
            && self.present_message_until.is_some()
            && self.message.is_some()
            && !matches!(self.mode, Mode::Command(_))
    }

    pub fn toggle_theme(&mut self) {
        let next = self.theme.toggled().name;
        self.set_theme_name(next, true);
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
        if let Some(e) = self.moves_block_reason("paste") {
            return Err(e);
        }
        let lang = self.doc.source.language.clone();
        let d = Document::from_code(text, lang, None);
        // Code that was never given a language: a shebang or modeline in
        // the pasted text picks one. An explicit `:lang`/`--lang` sticks.
        let detected = (self.doc.source.language == "txt" || self.doc.source.language.is_empty())
            .then(|| tiza_core::highlight::language_from_content(&d.code))
            .flatten();
        let out = self.apply_new_code(
            d.code,
            d.code_src,
            d.source.first_line,
            Some("paste".into()),
        );
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
        if let Some(n) = self.redact_drop_notice.take() {
            self.redact_alert(n);
        }
        Ok(())
    }

    /// Replaces the code lines (and where they come from), re-anchoring the
    /// annotations by text. One undoable step labelled `always` — or, with
    /// `None` (a follow reload), only when the reload touched an annotation
    /// (something dropped or shifted): a harmless reload leaves no step, so
    /// that after an editor undo that restores dropped text, one `u` reaches
    /// the reload that dropped it. Returns (kept, dropped, recorded).
    fn apply_new_code(
        &mut self,
        code: Vec<String>,
        code_src: tiza_core::model::CodeSource,
        first_line: u32,
        always: Option<String>,
    ) -> crate::reanchor::Outcome {
        let snapshot = self.doc.clone();
        let mut d = self.doc.clone();
        d.annotations.clear();
        if !d.moves.is_empty() {
            // Only reachable at step 0 (see `moves_block_reason`): the new
            // text is the step-0 code.
            d.base_code = code.clone();
            d.base_first_line = first_line;
        }
        d.code = code;
        d.code_src = code_src;
        d.source.first_line = first_line;
        let before = self.doc.annotations.len();
        let old = self.doc.clone();
        // Each annotation is authored against the code at its own step
        // (ADR 0007 §3d), so it is re-anchored between the old and the new
        // code *at that step*: the moves are re-applied on the new text and
        // a step-3 arrow onto a line a move typed in is matched on the
        // step-3 code, not on the step-0 code where that line does not
        // exist (AC9). A step whose moves no longer apply on the new text
        // falls back to the step-0 code (those moves are reported when
        // the presenter steps onto them).
        // A redact is re-anchored on the code its columns refer to (t-154):
        // the file, or the code right after the move it is pinned to.
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        enum Frame {
            Step(u32),
            AfterMove(usize),
        }
        let frame_of = |a: &Annotation| -> Frame {
            if old.moves.is_empty() {
                return Frame::Step(a.step);
            }
            match tiza_core::moves::redact_frame(&old, a) {
                Some(tiza_core::moves::RedactFrame::File) => Frame::Step(0),
                Some(tiza_core::moves::RedactFrame::AfterMove(i)) => Frame::AfterMove(i),
                _ => Frame::Step(a.step),
            }
        };
        let mut steps: Vec<Frame> = old.annotations.iter().map(frame_of).collect();
        steps.sort_unstable();
        steps.dedup();
        // Only the main file changed: its annotations are re-anchored on
        // its own code at their step; those of a linked file (t-105) keep
        // their coordinates.
        let at_step = |doc: &Document, frame: Frame| -> Document {
            let mut s = doc.clone();
            let code = match frame {
                _ if doc.moves.is_empty() => None,
                Frame::Step(step) => doc.view_code_at(&None, step).ok(),
                Frame::AfterMove(i) => tiza_core::moves::code_after_move(doc, i)
                    .filter(|(key, _, _)| key.is_none())
                    .map(|(_, c, f)| (c, f))
                    .or_else(|| doc.view_code_at(&None, 0).ok()),
            };
            if let Some((code, first)) = code {
                s.code = code;
                s.source.first_line = first;
            }
            s.moves.clear();
            s.base_code.clear();
            s.step = match frame {
                Frame::Step(step) => step,
                Frame::AfterMove(i) => doc.moves.get(i).map_or(0, |m| m.at_step),
            };
            s
        };
        let mut shapes: Vec<Option<Shape>> = old
            .annotations
            .iter()
            .map(|a| a.file.is_some().then(|| a.shape.clone()))
            .collect();
        // Redacts that could not follow their text, fail-closed (t-117):
        // the spans their mask widens to.
        let mut widened: Vec<Option<Vec<tiza_core::TextSpan>>> = vec![None; old.annotations.len()];
        for step in steps {
            let old_s = at_step(&old, step);
            let new_s = at_step(&d, step);
            let mut remap = crate::reanchor::Remap::new(&old_s, &new_s);
            for (i, a) in old.annotations.iter().enumerate() {
                if frame_of(a) != step || a.file.is_some() {
                    continue;
                }
                if let Shape::Redact { span, .. } = &a.shape
                    && let crate::reanchor::RedactFate::Widened(spans) = remap.redact(span)
                {
                    widened[i] = Some(spans);
                    continue;
                }
                shapes[i] = remap.shape(&a.shape);
            }
        }
        let mut dropped_redacts: Vec<String> = Vec::new();
        let mut widened_redacts: Vec<(String, String)> = Vec::new();
        // Lines of a hunk a widened redact also covers: added after every
        // other annotation, so their new ids never take an old one's.
        let mut extra: Vec<Annotation> = Vec::new();
        for (i, (a, shape)) in old.annotations.iter().zip(shapes).enumerate() {
            let is_redact = matches!(a.shape, Shape::Redact { .. });
            if let (Some(spans), Shape::Redact { label, pin, .. }) = (widened[i].take(), &a.shape) {
                // Still on the code it is pinned to (t-154).
                let mask = |span: tiza_core::TextSpan, id: String| Annotation {
                    id,
                    shape: Shape::Redact {
                        span,
                        keep: 0,
                        label: label.clone(),
                        widened: true,
                        pin: *pin,
                    },
                    ..a.clone()
                };
                let place = match spans.as_slice() {
                    [one] => format!("the rest of L{}", one.line),
                    [first, .., last] => format!("L{}-{} whole", first.line, last.line),
                    [] => String::new(),
                };
                let mut spans = spans.into_iter();
                let first = spans.next();
                let kept_one = first.is_some_and(|sp| d.add(mask(sp, a.id.clone())).is_ok());
                if kept_one {
                    widened_redacts.push((a.id.clone(), place));
                    extra.extend(spans.map(|sp| mask(sp, String::new())));
                } else {
                    dropped_redacts.push(a.id.clone());
                }
                continue;
            }
            // `add` validates against the code at the annotation's step.
            let kept_one = match shape {
                Some(shape) => d.add(Annotation { shape, ..a.clone() }).is_ok(),
                None => false,
            };
            if !kept_one && is_redact {
                dropped_redacts.push(a.id.clone());
            }
        }
        for ann in extra {
            // Two widened redacts of one hunk cover the same lines once.
            let covered = d.annotations.iter().any(|b| b.shape == ann.shape);
            if !covered {
                let _ = d.add(ann);
            }
        }
        let kept = d.annotations.len();
        let harmless = kept == before && d.annotations == snapshot.annotations;
        let recorded = always.is_some() || !harmless;
        if recorded {
            self.history
                .record(&snapshot, always.unwrap_or_else(|| "reload".into()));
        }
        self.doc = d;
        self.selected = None;
        self.clamp_cursor();
        self.touch();
        // Loud, and it must outlive the reload's own message (t-117 AC5).
        self.redact_drop_notice = redact_notice(&widened_redacts, &dropped_redacts);
        crate::reanchor::Outcome {
            kept,
            dropped: before.saturating_sub(kept),
            recorded,
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
        if let Some(e) = self.moves_block_reason("follow") {
            return Err(e);
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
        if self.doc.moves_applied() {
            // ADR 0007 §7: follow is paused while moves are applied (t-106
            // reconciles the edits); until then, say so instead of reloading.
            let step = self.doc.step;
            self.error(format!(
                "{name} changed on disk but moves are applied at step {step}: go back to step 0 to reload"
            ));
            return true;
        }
        f.mark_reloaded(new.clone());
        let reloads = f.reloads;
        let code_src = self.doc.code_src.clone();
        let first = self.doc.source.first_line;
        let out = self.apply_new_code(new, code_src, first, None);
        if out.recorded {
            self.history.label_last(format!("reload #{reloads}"));
        }
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
        if let Some(n) = self.redact_drop_notice.take() {
            self.redact_alert(n);
        }
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
        Some(Selection::Span(tiza_core::TextSpan {
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
        let before = self.annotation_steps();
        let prev_message = self.message.clone();
        let handled = self.select_action_inner(key);
        self.add_step_hint(&before, prev_message.as_ref());
        handled
    }

    fn select_action_inner(&mut self, key: char) -> bool {
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
            ('r', Selection::Span(span)) => {
                // Redact the selection (t-117): columns only, never the text.
                self.mode = Mode::Normal;
                match self.add_annotation(
                    None,
                    None,
                    Shape::Redact {
                        span,
                        keep: 0,
                        label: None,
                        widened: false,
                        pin: None,
                    },
                ) {
                    Ok(id) => self.info(format!(
                        "{id}: redacted L{} (:set {id} keep=4 shows the last 4 characters)",
                        span.line
                    )),
                    Err(e) => self.error(e.to_string()),
                }
            }
            ('r', Selection::Lines(a, b)) => {
                self.mode = Mode::Normal;
                let mut ids = Vec::new();
                for line in a..=b {
                    match self.redact_whole_line(line) {
                        Ok(Some(id)) => ids.push(id),
                        Ok(None) => {}
                        Err(e) => {
                            self.error(e.to_string());
                            return true;
                        }
                    }
                }
                self.info(format!(
                    "{} line{} redacted",
                    ids.len(),
                    if ids.len() == 1 { "" } else { "s" }
                ));
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
                let anchor = tiza_core::Anchor {
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
        Some(tiza_core::protocol::describe_with(
            a,
            &tiza_core::redact::Echo::quoting(&self.doc),
        ))
    }

    /// Quick edits on the selected annotation. Returns false if `key` is not one.
    pub fn selected_action(&mut self, key: char) -> bool {
        let before = self.annotation_steps();
        let prev_message = self.message.clone();
        let handled = self.selected_action_inner(key);
        self.add_step_hint(&before, prev_message.as_ref());
        handled
    }

    fn selected_action_inner(&mut self, key: char) -> bool {
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
                    | Shape::Arrow { label, .. }
                    | Shape::Redact { label, .. } => label.clone().unwrap_or_default(),
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
        "layout",
        "box",
        "callout",
        "clear",
        "redact",
        "ref",
        "follow",
        "color",
        "del",
        "embed",
        "focus",
        "export",
        "goto",
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
        "numbers",
        "margin",
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
        let before = self.annotation_steps();
        let prev_message = self.message.clone();
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
                    // The code no longer comes from the file: there is
                    // nothing to follow (t-117; `:follow on` says the same).
                    if self.follow.is_some() {
                        self.set_follow(false)?;
                        return Ok("code frozen inside the .tiza; follow off: edits on disk no longer reach the screen (:link and :follow on to resume)".into());
                    }
                    Ok("code frozen inside the .tiza".into())
                }
                "link" if !rest.is_empty() && self.doc.step >= 1 => {
                    // At a step: a `code.link` move that shows `path` (with
                    // `A-B` as a window) from this step on (t-105). At step 0,
                    // or as `:ref`, the one-shot form: reference the file.
                    self.cmd_link_move(rest)
                }
                "link" | "ref" => {
                    let (file, range) = if rest.is_empty() {
                        let Some(f) = self.doc.source.path.clone() else {
                            return Err(Error::Invalid(
                                "usage: :link path[:A-B]   (without a path, uses the source file)"
                                    .into(),
                            ));
                        };
                        let r = (self.doc.base_first() > 1).then(|| {
                            (
                                self.doc.base_first(),
                                self.doc.base_first()
                                    + self.doc.base().len().saturating_sub(1) as u32,
                            )
                        });
                        (f, r)
                    } else {
                        let (p, r) = fs::parse_target(rest);
                        (p.to_string_lossy().into_owned(), r)
                    };
                    self.doc.reference_file(file.clone(), range);
                    self.touch();
                    Ok(format!("the .tiza now references {file}"))
                }
                "spot" | "spotlight" => self.cmd_spot(rest),
                "redact" => self.cmd_redact(rest),
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
                    if n < ann.step {
                        return Err(Error::Invalid(format!(
                            "{id} starts at step {}: :until {id} N needs N ≥ {}",
                            ann.step, ann.step
                        )));
                    }
                    *until = Some(n);
                    self.history.record(&self.doc, format!("{id} until {n}"));
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
                            tiza_core::theme::PALETTE.join(", ")
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
                            Ok(self.step_message(n))
                        }
                        [id, n] => {
                            let n: u32 = n
                                .parse()
                                .map_err(|_| Error::Invalid("usage: :step ID N".into()))?;
                            let max = self.doc.max_step();
                            if n > max + 1 {
                                return Err(Error::Invalid(format!(
                                    "step {n} is beyond the last step ({max}): an annotation can go up to step {} (the next empty one)",
                                    max + 1
                                )));
                            }
                            if n == 0 {
                                return Err(Error::Invalid(
                                    "annotations live at steps ≥ 1 (step 0 is the code alone)"
                                        .into(),
                                ));
                            }
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
                    if self.next_step() {
                        Ok(String::new())
                    } else {
                        Ok(self.doc.step_label(self.doc.step))
                    }
                }
                "goto" => {
                    let id = rest.trim();
                    if id.is_empty() {
                        return Err(Error::Invalid(
                            "usage: :goto ID  (a named step, or a step number)".into(),
                        ));
                    }
                    self.goto_step(id)?;
                    Ok(self.doc.step_label(self.doc.step))
                }
                "prev" => Ok(self.step_message(self.doc.step.saturating_sub(1))),
                "focus" | "f" => {
                    if rest.is_empty() {
                        self.focus = None;
                        Ok("focus cleared".into())
                    } else {
                        let (a, b) = parse_range(rest).map_err(|_| {
                            Error::Invalid("usage: :focus A-B  |  :focus   (clear)".into())
                        })?;
                        let (first, last) = (self.doc.source.first_line, self.doc.last_line());
                        if a < first || a > last {
                            return Err(Error::LineOutOfRange {
                                line: a,
                                first,
                                last,
                            });
                        }
                        let clamped = b.min(last);
                        self.focus = Some((a, clamped));
                        Ok(if clamped != b {
                            format!("focus on {a}-{clamped} ({b} is past the last line, {last})")
                        } else {
                            format!("focus on {a}-{b}")
                        })
                    }
                }
                "theme" | "t" => {
                    let arg = rest.trim();
                    let lower = arg.to_ascii_lowercase();
                    if let Some(path) = lower
                        .strip_prefix("file ")
                        .map(|_| arg[5..].trim())
                        .filter(|p| !p.is_empty())
                    {
                        return self.load_theme_file(std::path::Path::new(path));
                    }
                    match lower.as_str() {
                        "" | "toggle" => {
                            let next = self.theme.toggled().name;
                            self.set_theme_name(next, true);
                        }
                        "auto" | "host" => {
                            self.theme_follow_host();
                            self.touch();
                            return Ok(match self.host_dark {
                                Some(dark) => format!(
                                    "theme auto: following the terminal ({} now)",
                                    if dark { "dark" } else { "light" }
                                ),
                                None => "theme auto: following the terminal (no answer yet; a resize asks again)"
                                    .into(),
                            });
                        }
                        other => match Self::parse_theme_name(other) {
                            Some(name) => self.set_theme_name(name, true),
                            None => {
                                return Err(Error::Invalid(format!(
                                    "unknown theme: {other} (dark, light, terminal, auto, file PATH; :theme alone toggles)"
                                )));
                            }
                        },
                    }
                    self.touch();
                    Ok(format!(
                        "theme {}{}",
                        theme_label(self.theme.name),
                        if self.theme_notes.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", self.theme_notes.join("; "))
                        }
                    ))
                }
                "present" | "p" => {
                    self.set_present(!self.present);
                    Ok(if self.present {
                        "presentation mode (P to exit)".into()
                    } else {
                        String::new()
                    })
                }
                "lang" => {
                    use tiza_core::highlight::{detect_language, has_grammar, normalize_language};
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
                    if rest.split_whitespace().count() != 1 {
                        return Err(Error::Invalid(
                            "usage: :lang ID  |  :lang auto  |  :lang   (one language id, e.g. rs, ts, py)".into(),
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
                    self.history.record(&self.doc, "title");
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
                    self.history.record(&self.doc, "padding");
                    self.doc.padding = None;
                    self.padding_override = None;
                    self.touch();
                    let (v, h) = crate::ui::DEFAULT_PADDING;
                    Ok(format!("padding {v} {h} (default)"))
                }
                "margin" => match rest.trim() {
                    "" => Ok(match self.doc.margin {
                        Some(n) => format!(
                            "margin {n} (notes wrap at {n} cells; :margin auto removes the cap)"
                        ),
                        None => "margin auto (the right margin takes what the width leaves)".into(),
                    }),
                    "auto" | "off" | "none" => {
                        if self.doc.margin.is_none() {
                            return Ok("margin already auto".into());
                        }
                        self.history.record(&self.doc, "margin");
                        self.doc.margin = None;
                        self.touch();
                        Ok("margin auto".into())
                    }
                    n => {
                        let cap = n.parse::<usize>().map_err(|_| {
                            Error::Invalid(format!(
                                "margin {n}: use `:margin N` ({}-{} cells) or `:margin auto`",
                                tiza_core::layout::MIN_MARGIN_CAP,
                                tiza_core::layout::MAX_MARGIN_CAP
                            ))
                        })?;
                        let cap = tiza_core::layout::check_margin_cap(cap)?;
                        self.history.record(&self.doc, "margin");
                        self.doc.margin = Some(cap as u16);
                        self.touch();
                        Ok(format!(
                            "margin {cap}: notes, labels and legends wrap at {cap} cells (saved in the document)"
                        ))
                    }
                },
                "numbers" | "nums" => match rest.trim() {
                    "" => Ok(format!(
                        "numbers {}",
                        if self.doc.numbers { "on" } else { "off" }
                    )),
                    "on" | "yes" | "off" | "no" => {
                        let on = matches!(rest.trim(), "on" | "yes");
                        if on == self.doc.numbers {
                            return Ok(format!(
                                "numbers already {}",
                                if on { "on" } else { "off" }
                            ));
                        }
                        self.history.record(&self.doc, "numbers");
                        self.doc.numbers = on;
                        self.touch();
                        Ok(if on {
                            "numbers on (saved in the document)".into()
                        } else {
                            "numbers off: the gutter is hidden here, in exports and for agents (saved in the document; :numbers on restores it)".into()
                        })
                    }
                    other => Err(Error::Invalid(format!(
                        "numbers {other}: use `:numbers on` or `:numbers off`"
                    ))),
                },
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
                    self.history.record(&self.doc, "padding");
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
                "layout" => match rest.trim() {
                    "" => Ok(format!(
                        "layout {}{}",
                        if self.layout_stable {
                            "stable"
                        } else {
                            "dynamic"
                        },
                        if self.present {
                            " (presentation mode is always stable)"
                        } else {
                            ""
                        }
                    )),
                    "stable" | "take" => {
                        self.layout_stable = true;
                        Ok("layout stable: lanes and margin sized for every step, as in presentation mode and exports".into())
                    }
                    "dynamic" | "auto" => {
                        self.layout_stable = false;
                        Ok("layout dynamic: the authoring screen grows lanes as steps add arrows (exports stay stable)".into())
                    }
                    other => Err(Error::Invalid(format!(
                        "layout {other}: use `:layout stable` or `:layout dynamic`"
                    ))),
                },
                "autosave" => match rest.trim() {
                    "" => Ok(format!(
                        "autosave {}{}",
                        if self.autosave { "on" } else { "off" },
                        if self.dirty && !self.autosave {
                            " (unsaved changes: :w)"
                        } else {
                            ""
                        }
                    )),
                    "on" | "yes" => {
                        self.autosave = true;
                        if self.dirty {
                            self.save()?;
                            Ok(format!(
                                "autosave on (saved to {})",
                                self.save_path.display()
                            ))
                        } else {
                            Ok("autosave on".into())
                        }
                    }
                    "off" | "no" => {
                        self.autosave = false;
                        Ok("autosave off (:w saves explicitly)".into())
                    }
                    _ => Err(Error::Invalid("usage: :autosave [on|off]".into())),
                },
                // Only with TIZA_TEST_PANIC set: lets the crash hook be
                // exercised from a test (t-115 AC2).
                "panic" if std::env::var_os("TIZA_TEST_PANIC").is_some() => {
                    panic!("forced by :panic (TIZA_TEST_PANIC)")
                }
                other => Err(Error::Invalid(match nearest_commands(other) {
                    v if v.is_empty() => format!("unknown command: {other} (try :help)"),
                    v if v.len() == 1 => {
                        format!("unknown command: {other} — did you mean :{}?", v[0])
                    }
                    v => format!(
                        "unknown command: {other} — did you mean {}?",
                        v.iter()
                            .map(|c| format!(":{c}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                })),
            }
        };
        let result = exec();
        match result {
            Ok(msg) if !msg.is_empty() => self.info(msg),
            Ok(_) => {}
            // The presenter's own targets are not repeated either (t-167).
            Err(e) => {
                let e = self.echo().written(command_words(input)).error(e);
                self.error(e.to_string())
            }
        }
        // Undo/redo bring annotations back as they were: no hint there.
        if !matches!(cmd, "undo" | "u" | "redo") {
            self.add_step_hint(&before, prev_message.as_ref());
        }
    }

    /// `:link path[:A-B]` at step N ≥ 1: records `code.link {file, lines}`
    /// at the current step. The file is read next to the document.
    fn cmd_link_move(&mut self, rest: &str) -> Result<String, Error> {
        let (p, r) = fs::parse_target_checked(rest.trim())?;
        let file = p.to_string_lossy().into_owned();
        let lines = r.map(|(a, b)| {
            if a == b {
                a.to_string()
            } else {
                format!("{a}-{b}")
            }
        });
        let m = Move {
            at_step: self.doc.step,
            action: tiza_core::moves::MoveAction::CodeLink(tiza_core::moves::Link {
                file: file.clone(),
                lines: lines.clone(),
            }),
            rebind: Default::default(),
        };
        let i = self.add_move(m)?;
        Ok(format!(
            "move #{} added: step {} shows {}{} (:ref path to change the document's own file)",
            i + 1,
            self.doc.step,
            file,
            lines.map(|l| format!(":{l}")).unwrap_or_default()
        ))
    }

    fn cmd_box(&mut self, rest: &str) -> Result<String, Error> {
        let (a, b, markers, rest) = self.parse_range_arg(rest).map_err(|e| match e {
            Error::Invalid(m) if m.starts_with("invalid range") => Error::Invalid(format!(
                "usage: :box A-B [label] [border=double] [corners=sharp] [fill=no]   or   :box @'start'..@'end' [label]   ({m})"
            )),
            other => other,
        })?;
        let (label, kv) = split_trailing_kv(rest);
        let mut frame = tiza_core::model::BorderStyle::default();
        for (k, v) in &kv {
            apply_border_kv(&mut frame, k, v)?;
        }
        let id = self.add_annotation_full(
            None,
            None,
            None,
            markers,
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

    /// The first argument of a range command: `A-B`, `.`, or two text
    /// markers `@'start'..@'end'` (quotes optional when the text has no
    /// spaces; `@start..@end`). Returns the lines, the markers, and the rest.
    fn parse_range_arg<'a>(
        &self,
        s: &'a str,
    ) -> Result<(u32, u32, Option<tiza_core::model::RangeMarkers>, &'a str), Error> {
        let s = s.trim_start();
        if let Some(after_at) = s.strip_prefix('@') {
            let (start, rest) = take_marker(after_at)?;
            let rest = rest.strip_prefix("..").ok_or_else(|| {
                Error::Invalid("markers: write @'start'..@'end' (two markers joined by ..)".into())
            })?;
            let rest = rest.strip_prefix('@').ok_or_else(|| {
                Error::Invalid("markers: the end marker also starts with @".into())
            })?;
            let (end, rest) = take_marker(rest)?;
            let (a, b) = self.doc.resolve_markers(&start, &end)?;
            return Ok((
                a,
                b,
                Some(tiza_core::model::RangeMarkers { start, end }),
                rest.trim_start(),
            ));
        }
        let (range, rest) = split_first(s);
        if range == "." {
            return Ok((self.cursor_line, self.cursor_line, None, rest));
        }
        let (a, b) = parse_range(range)?;
        // `3 - 5` or `3 -5`: the dash got spaces around it; the rest would
        // become a label starting with `-`.
        if rest.starts_with('-')
            && rest[1..]
                .trim_start()
                .starts_with(|c: char| c.is_ascii_digit())
        {
            return Err(Error::Invalid(format!(
                "invalid range: {range} {}: write A-B without spaces",
                rest.split_whitespace()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ")
            )));
        }
        Ok((a, b, None, rest))
    }

    /// `resolve_span_strict` for a point anchor (arrows).
    fn resolve_anchor_strict(
        &self,
        w: &tiza_core::protocol::Where,
    ) -> Result<tiza_core::model::Anchor, Error> {
        if w.find.is_some() {
            let s = self.resolve_span_strict(w)?;
            return Ok(tiza_core::model::Anchor {
                line: s.line,
                col: s.start,
            });
        }
        if let Some(line) = w.line
            && !self.doc.has_line(line)
        {
            return Err(Error::LineOutOfRange {
                line,
                first: self.doc.source.first_line,
                last: self.doc.last_line(),
            });
        }
        w.resolve_anchor(&self.doc)
    }

    /// Resolves an anchor typed in a command, strictly: `L:text` must find
    /// the text on line L (not elsewhere), and L must exist. A miss says
    /// where the text is, so the typo is one edit away from right.
    fn resolve_span_strict(
        &self,
        w: &tiza_core::protocol::Where,
    ) -> Result<tiza_core::model::TextSpan, Error> {
        if let Some(line) = w.line
            && !self.doc.has_line(line)
        {
            return Err(Error::LineOutOfRange {
                line,
                first: self.doc.source.first_line,
                last: self.doc.last_line(),
            });
        }
        let span = w.resolve_span(&self.doc)?;
        if let (Some(line), Some(text)) = (w.line, &w.find)
            && span.line != line
        {
            return Err(Error::Invalid(format!(
                "«{text}» not on L{line}; found on L{} — use {}:{text}",
                span.line, span.line
            )));
        }
        Ok(span)
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
        let mut line = tiza_core::model::LineStyle::default();
        for (k, v) in &kv {
            apply_line_kv(&mut line, k, v)?;
        }
        let f = self.resolve_anchor_strict(&parse_where(from, self.cursor_line)?)?;
        let t = self.resolve_anchor_strict(&parse_where(to, self.cursor_line)?)?;
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
                    width = tiza_core::layout::check_width(v.parse::<usize>().map_err(|_| {
                        Error::Invalid(format!(
                            "width={v}: use a number between {} and {}",
                            tiza_core::layout::MIN_WIDTH,
                            tiza_core::layout::MAX_WIDTH
                        ))
                    })?)?;
                }
                "theme" => {
                    let name = Self::parse_theme_name(&v)
                        .filter(|n| *n != ThemeName::Terminal)
                        .ok_or_else(|| Error::Invalid(format!("theme={v}: dark or light")))?;
                    theme = Theme::resolve(
                        name,
                        self.host_dark.unwrap_or(true),
                        self.config_overrides.as_ref(),
                        self.doc.theme_overrides.as_ref(),
                    )
                    .0;
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
                .unwrap_or_else(|| "tiza".into());
            let stem = stem.strip_suffix(".tiza").unwrap_or(&stem).to_string();
            self.save_path
                .with_file_name(format!("{stem}-step{step}.txt"))
        } else {
            PathBuf::from(path)
        };
        let format = ExportFormat::from_path(&path).ok_or_else(|| {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            if matches!(ext.as_deref(), Some("gif" | "mp4" | "webm" | "cast" | "tape")) {
                Error::Invalid(format!(
                    "{}: a recording of every step is made from the shell — `tiza render --out {}` records this instance (no FILE needed)",
                    path.display(),
                    path.display()
                ))
            } else {
                Error::Invalid(format!(
                    "{}: the extension picks the format (.txt .ansi .md .svg .png)",
                    path.display()
                ))
            }
        })?;
        // A terminal theme (or `reset` tokens) has no palette in a file:
        // the export uses the dark/light theme and the message says so.
        let (theme, theme_notice) = theme.for_export();
        let title = self.doc.title.as_deref().map(|t| self.shown(t));
        let l = layout(
            &self.doc,
            &theme,
            &LayoutOptions {
                width,
                step,
                focus: self.focus,
                // `:export` never draws the live pointer (t-030).
                pointer: None,
                line_numbers: self.doc.numbers,
                tab_width: 4,
                typing: false,
                redact: true,
                stable: true,
                margin_cap: self.doc.margin.map(usize::from),
            },
        );
        let bytes = match format {
            ExportFormat::Txt => (l.canvas.to_plain_text() + "\n").into_bytes(),
            ExportFormat::Ansi => l.canvas.to_ansi().into_bytes(),
            // The title is shown text (t-174).
            ExportFormat::Md => l.canvas.to_markdown(title.as_deref()).into_bytes(),
            ExportFormat::Svg | ExportFormat::Png => {
                let f = if format == ExportFormat::Svg {
                    ImageFormat::Svg
                } else {
                    ImageFormat::Png
                };
                let exporter = self.image_exporter.as_ref().ok_or_else(|| {
                    Error::Invalid(
                        "SVG/PNG export is not available in this build; use `tiza render`".into(),
                    )
                })?;
                exporter(&l.canvas, &theme, f, title.as_deref()).map_err(Error::Invalid)?
            }
        };
        std::fs::write(&path, &bytes)
            .map_err(|e| Error::Invalid(format!("could not write {}: {e}", path.display())))?;
        Ok(format!(
            "exported step {step} to {} ({} bytes){}",
            path.display(),
            bytes.len(),
            theme_notice.map(|n| format!(" — {n}")).unwrap_or_default()
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
        check_kv_shape(&kv, rest)?;
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
        let mut heads = tiza_core::protocol::HeadsPatch::default();
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
                    let mut bs = tiza_core::model::BorderStyle::default();
                    apply_border_kv(&mut bs, k, v)?;
                    match k.as_str() {
                        "border" => p.border = Some(bs.border),
                        "corners" => p.corners = Some(bs.corners),
                        _ => p.fill = bs.fill,
                    }
                }
                _ => {
                    let mut ls = tiza_core::model::LineStyle::default();
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
                if !typed_anchor_has_line_and_text(at) {
                    return Err(Error::Invalid(
                        "usage: :hl L:text [label]   (L:text = the text on line L; L: alone = the whole line; .:text = the cursor line)"
                            .into(),
                    ));
                }
                let w = parse_where(at, self.cursor_line)?;
                (self.resolve_span_strict(&w)?, label)
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
            || tiza_core::model::builtin_kind(kind_tok).is_some();
        let (kind, mut style) = if known {
            (kind_tok.to_string(), MarkStyle::default())
        } else {
            // Not a known kind: treat it as a glyph for an inline style.
            let w = tiza_core::canvas::display_width(kind_tok, 4);
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
            _ => {
                if !typed_anchor_has_line_and_text(at) {
                    return Err(Error::Invalid(format!(
                        "usage: :mark KIND[@COLOR] L:text [label]   (L:text = the text on line L; L: alone = the whole line)   KIND: {} | a :style name | a glyph",
                        self.doc.kind_names().join(" ")
                    )));
                }
                let w = parse_where(at, self.cursor_line)?;
                (self.resolve_span_strict(&w)?, label)
            }
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
        let kv = parse_kv(rest);
        check_kv_shape(&kv, rest)?;
        for (k, v) in kv {
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
        if st.glyph.is_none() && tiza_core::model::builtin_style(name).is_none() {
            return Err(Error::Invalid("a style needs at least a glyph".into()));
        }
        self.doc.styles.insert(name.to_string(), st);
        self.touch();
        Ok(format!("style {name} defined (use it with :mark {name} …)"))
    }

    /// Masks the content of `line` (from its first non-blank character).
    /// `Ok(None)` for a blank line.
    fn redact_whole_line(&mut self, line: u32) -> Result<Option<String>, Error> {
        let text = self
            .doc
            .line_text(line)
            .ok_or(Error::LineOutOfRange {
                line,
                first: self.doc.source.first_line,
                last: self.doc.last_line(),
            })?
            .to_string();
        use unicode_segmentation::UnicodeSegmentation;
        let start = text
            .graphemes(true)
            .take_while(|g| g.trim().is_empty())
            .count() as u32;
        let end = self.doc.line_len(line);
        if start >= end {
            return Ok(None);
        }
        let id = self.add_annotation(
            None,
            None,
            Shape::Redact {
                span: tiza_core::TextSpan { line, start, end },
                keep: 0,
                label: None,
                widened: false,
                pin: None,
            },
        )?;
        Ok(Some(id))
    }

    /// `:redact L` (the whole line), `:redact L:anchor [keep=N] [label]` (what
    /// follows the anchor text to the end of the line), `:redact L@S..E`
    /// (columns), `:redact on|off` (the author's view), `:redact auto
    /// [--dry]` (built-in patterns → ordinary redact annotations), `:redact`
    /// (state). The anchor is the text BEFORE the secret, never the secret.
    fn cmd_redact(&mut self, rest: &str) -> Result<String, Error> {
        let rest = rest.trim();
        let usage = "usage: :redact L  |  :redact L:ANCHOR [keep=N] [label]  |  :redact L@S..E  |  :redact on|off  |  :redact auto [--dry]   (select the value and press r is the safest)";
        match rest {
            "" => {
                let n = self
                    .doc
                    .annotations
                    .iter()
                    .filter(|a| matches!(a.shape, Shape::Redact { .. }))
                    .count();
                return Ok(format!(
                    "redact: {n} mask{} ({}; presentation mode and every export always mask)",
                    if n == 1 { "" } else { "s" },
                    if self.redact_shown {
                        "shown"
                    } else {
                        "OFF for the author: :redact on"
                    }
                ));
            }
            "on" => {
                self.redact_shown = true;
                self.revision += 1;
                return Ok("redact on: masks shown".into());
            }
            "off" => {
                self.redact_shown = false;
                self.revision += 1;
                return Ok(
                    "redact off for the author only: the screen shows the real text until :redact on (presentation mode, exports, recordings and agents keep the masks)"
                        .into(),
                );
            }
            _ => {}
        }
        if let Some(flags) = rest.strip_prefix("auto") {
            let dry = flags.trim() == "--dry";
            if !flags.trim().is_empty() && !dry {
                return Err(Error::Invalid("usage: :redact auto [--dry]".into()));
            }
            return self.redact_auto(dry);
        }
        let (at, tail) = split_first(rest);
        let (label, kv) = split_trailing_kv(tail);
        let mut keep: u8 = 0;
        for (k, v) in &kv {
            match k.as_str() {
                "keep" => {
                    keep = v
                        .parse::<u8>()
                        .ok()
                        .filter(|n| *n <= 8)
                        .ok_or_else(|| Error::Invalid(format!("keep={v}: use 0 to 8")))?;
                }
                other => {
                    return Err(Error::Invalid(format!(
                        "unknown key `{other}` for :redact (keep=N)"
                    )));
                }
            }
        }
        if !looks_like_anchor(at) {
            return Err(Error::Invalid(usage.into()));
        }
        // `L` alone: the whole line.
        if at.chars().all(|c| c.is_ascii_digit()) {
            let line: u32 = at.parse().map_err(|_| Error::Invalid(usage.into()))?;
            return match self.redact_whole_line(line)? {
                Some(id) => {
                    if keep > 0 || !label.trim().is_empty() {
                        self.apply_redact_options(&id, keep, label)?;
                    }
                    Ok(format!("{id}: L{line} redacted"))
                }
                None => Err(Error::Invalid(format!("L{line} is blank: nothing to mask"))),
            };
        }
        let w = parse_where(at, self.cursor_line)?;
        if let Some(line) = w.line
            && !self.doc.has_line(line)
        {
            return Err(Error::LineOutOfRange {
                line,
                first: self.doc.source.first_line,
                last: self.doc.last_line(),
            });
        }
        let span = tiza_core::protocol::resolve_redact_span(&self.doc, &w)?;
        if let (Some(line), Some(_)) = (w.line, &w.find)
            && span.line != line
        {
            return Err(Error::Invalid(format!(
                "the anchor is not on L{line}; found on L{} — use {}:…",
                span.line, span.line
            )));
        }
        let id = self.add_annotation(
            None,
            None,
            Shape::Redact {
                span,
                keep,
                label: opt(label),
                widened: false,
                pin: None,
            },
        )?;
        Ok(format!(
            "{id}: redacted L{} ({} cell{} masked{})",
            span.line,
            span.end - span.start,
            if span.end - span.start == 1 { "" } else { "s" },
            if keep > 0 {
                format!(", last {keep} shown")
            } else {
                String::new()
            }
        ))
    }

    fn apply_redact_options(&mut self, id: &str, keep: u8, label: &str) -> Result<(), Error> {
        let mut ann = self
            .doc
            .find_annotation(id)
            .cloned()
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        if let Shape::Redact {
            keep: k, label: l, ..
        } = &mut ann.shape
        {
            *k = keep;
            *l = opt(label);
        }
        self.doc.update(ann)?;
        self.touch();
        Ok(())
    }

    /// `:redact auto`: the built-in patterns on the shown code, each match an
    /// ordinary redact annotation the presenter can review and delete.
    fn redact_auto(&mut self, dry: bool) -> Result<String, Error> {
        // Scan the code as shown (masks already applied are bullets, so a
        // second run does not stack masks on masks).
        let code = tiza_core::redact::masked_code(&self.doc, self.doc.step);
        let matches = tiza_core::redact::auto_matches(&code, self.doc.source.first_line);
        if matches.is_empty() {
            return Ok("redact auto: 0 matches (it only knows JWTs, Bearer/Basic tokens, password/secret/token/api_key values, URL credentials, AWS/GitHub/Stripe/Slack/Google keys and e-mail addresses)".into());
        }
        let places: Vec<String> = matches
            .iter()
            .map(|m| format!("L{} {}", m.line, m.what))
            .collect();
        if dry {
            return Ok(format!(
                "redact auto --dry: {} match{} ({}); run :redact auto to mask them",
                matches.len(),
                if matches.len() == 1 { "" } else { "es" },
                places.join(", ")
            ));
        }
        let mut masked = 0;
        for m in &matches {
            // Skip a place that is already a mask (bullets) or already redacted.
            let already = self.doc.annotations.iter().any(|a| {
                matches!(&a.shape, Shape::Redact { span, .. } if span.line == m.line && span.start < m.end && span.end > m.start)
            });
            if already {
                continue;
            }
            self.add_annotation(
                None,
                None,
                Shape::Redact {
                    span: tiza_core::TextSpan {
                        line: m.line,
                        start: m.start,
                        end: m.end,
                    },
                    keep: 0,
                    label: None,
                    widened: false,
                    pin: None,
                },
            )?;
            masked += 1;
        }
        Ok(format!(
            "redact auto: {masked} masked ({}); :list shows them, :del ID removes one, u undoes",
            places.join(", ")
        ))
    }

    /// `:spot 7-12`, `:spot a2 a3`, `:spot 7-12 a2 a3`. Active only in the current step
    /// (`:until ID N` extends it).
    fn cmd_spot(&mut self, rest: &str) -> Result<String, Error> {
        let mut range: Option<(u32, u32)> = None;
        let mut markers = None;
        let mut ids = Vec::new();
        let mut rest = rest.trim_start();
        if rest.starts_with('@') {
            let (a, b, m, after) = self.parse_range_arg(rest)?;
            range = Some((a, b));
            markers = m;
            rest = after;
        }
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
        let id = self.add_annotation_full(
            None,
            None,
            None,
            markers,
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
        let (a, b, markers, mut rest) = self.parse_range_arg(rest).map_err(|e| match e {
            Error::Invalid(m) if m.starts_with("invalid range") => Error::Invalid(format!(
                "usage: :callout A[-B] [@below|@margin|@auto] [title=\"…\"] text   (or @'start'..@'end')   ({m})"
            )),
            other => other,
        })?;
        let mut placement = tiza_core::model::Placement::Margin;
        let mut title = None;
        loop {
            let (tok, after) = split_first(rest);
            if let Some(name) = tok.strip_prefix('@') {
                placement = tiza_core::model::Placement::parse(name).ok_or_else(|| {
                    Error::Invalid(format!(
                        "unknown placement @{name}: use @below, @margin or @auto"
                    ))
                })?;
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
        let mut frame = tiza_core::model::BorderStyle::default();
        for (k, v) in &kv {
            apply_border_kv(&mut frame, k, v)?;
        }
        if text.trim().is_empty() {
            return Err(Error::Invalid("the callout text is empty".into()));
        }
        let id = self.add_annotation_full(
            None,
            None,
            None,
            markers,
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
        // What the agent wrote, for the error's echo filter (t-167).
        let brings_redact = tiza_core::redact::json_has_redact(&req.params);
        let written = if brings_redact || tiza_core::redact::has_redacts(&self.doc) {
            let mut w = tiza_core::redact::json_strings(&req.params);
            w.push(req.method.clone());
            w
        } else {
            Vec::new()
        };
        let result = self.dispatch(&req.method, req.params);
        // An agent's text repeating a masked value is said on the alert
        // row and in `status.redact_alerts` right away (t-174).
        self.note_masked_texts();
        match result {
            Ok(v) => Response::ok(id, v),
            Err(e) => {
                let e = self
                    .echo()
                    .redacted(brings_redact)
                    .written(written)
                    .error(e);
                Response::err(id, e.code(), e.to_string())
            }
        }
    }

    /// Entry point of the event loop: answers now, or (supervised mode and a
    /// mutating method) parks the request as a proposal until the presenter
    /// decides. `supervise.*` and read-only methods are never parked.
    pub fn handle_ipc(&mut self, req: Request, reply: std::sync::mpsc::Sender<Response>) {
        self.handle_ipc_at(req, reply, std::time::Instant::now());
    }

    /// [`App::handle_ipc`] at `now`, the clock that groups the messages
    /// about agent changes the view does not show (t-155); tests pass
    /// their own.
    pub fn handle_ipc_at(
        &mut self,
        req: Request,
        reply: std::sync::mpsc::Sender<Response>,
        now: std::time::Instant,
    ) {
        let parked = self.supervised
            && !READ_ONLY_METHODS.contains(&req.method.as_str())
            && !req.method.starts_with("supervise.");
        if !parked {
            let verb = annotation_verb(&req.method);
            let before = self.message.clone();
            let resp = self.handle_request(req);
            if let (Some(verb), Some(r)) = (verb, &resp.result) {
                self.agent_changed(r, verb, before.as_ref(), now);
            }
            let _ = reply.send(resp);
            return;
        }
        // The review list, the y/n/expiry messages and
        // `supervise.status` all show this summary: on a document with
        // redacts it quotes no target (t-167).
        let summary = proposal_summary(&self.echo(), &req);
        let id = self.next_proposal;
        self.next_proposal += 1;
        self.proposals.push(Proposal {
            id,
            req,
            reply,
            since: now,
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

    /// An agent's `annotation.add` / `annotation.update` (socket or MCP,
    /// never the TUI) left `result`: when the view does not show it, the bar
    /// says so — `a17: box 3-5 added by agent (step 1, press n)` — and
    /// changes within [`AGENT_NOTICE_GROUP`] of the last one join it
    /// (`3 agent annotations not shown (step 1, press n)`). In presentation
    /// mode nothing is drawn: it waits for the bar (t-155). An error the
    /// request itself raised (a redact alert, a failed save) keeps the bar.
    fn agent_changed(
        &mut self,
        result: &Value,
        verb: &'static str,
        before: Option<&(String, bool)>,
        now: std::time::Instant,
    ) {
        if result.get("visible").and_then(Value::as_bool) != Some(false) {
            return;
        }
        let Some(id) = mutated_id(result) else {
            return;
        };
        if self.present {
            if !self.agent_deferred.iter().any(|(i, _)| *i == id) {
                self.agent_deferred.push((id, verb));
            }
            return;
        }
        if self
            .message
            .as_ref()
            .is_some_and(|m| m.1 && Some(m) != before)
        {
            return;
        }
        let mut items = match self.agent_notice.take() {
            Some(n)
                if now.saturating_duration_since(n.last) < AGENT_NOTICE_GROUP
                    && self.message.as_ref() == Some(&(n.text.clone(), false)) =>
            {
                n.items
            }
            _ => Vec::new(),
        };
        if !items.iter().any(|(i, _)| *i == id) {
            items.push((id, verb));
        }
        let Some(text) = self.agent_notice_text(&items) else {
            return;
        };
        self.info(text.clone());
        self.agent_notice = Some(AgentNotice {
            items,
            last: now,
            text,
        });
    }

    /// The bar's text for agent changes (t-155), as the view stands now:
    /// those it shows meanwhile are left out; `None` when it shows them all.
    fn agent_notice_text(&self, items: &[(String, &'static str)]) -> Option<String> {
        let step = self.doc.step;
        let hidden: Vec<_> = items
            .iter()
            .filter_map(|(id, verb)| {
                let a = self.doc.find_annotation(id)?;
                let why = self.doc.hidden_reason(id, step)?;
                Some((a, *verb, why))
            })
            .collect();
        match hidden.as_slice() {
            [] => None,
            [(a, verb, why)] => {
                let msg = format!("{}: {} {verb} by agent", a.id, shape_summary(a));
                let tail = self.hidden_tail(why, &msg);
                Some(msg + &tail)
            }
            many => {
                let msg = format!("{} agent annotations not shown", many.len());
                let first_later = many
                    .iter()
                    .filter_map(|(_, _, w)| match w {
                        tiza_core::model::Hidden::Later(s) => Some(*s),
                        _ => None,
                    })
                    .min();
                let tail = match first_later {
                    Some(s) => self.hidden_tail(&tiza_core::model::Hidden::Later(s), &msg),
                    None => {
                        let tails: Vec<String> = many
                            .iter()
                            .map(|(_, _, w)| self.hidden_tail(w, &msg))
                            .collect();
                        if tails.iter().all(|t| *t == tails[0]) {
                            tails[0].clone()
                        } else {
                            String::new()
                        }
                    }
                };
                Some(msg + &tail)
            }
        }
    }

    /// The tail of a message about something the view does not show: the
    /// t-132 hint when it appears later (` (step 1, press n)`), the file
    /// when a link shows another one (` (src/queue.rs, not shown at step
    /// 2)`), else the reason in words (t-155).
    fn hidden_tail(&self, why: &tiza_core::model::Hidden, msg: &str) -> String {
        use tiza_core::model::Hidden;
        match why {
            Hidden::Later(s) => step_hint(msg, *s, self.doc.step),
            Hidden::OtherFile { file, step, .. } => format!(" ({file}, not shown at step {step})"),
            other => format!(" (not shown: {other})"),
        }
    }

    /// Applies the first pending proposal (`y`).
    pub fn accept_proposal(&mut self) -> Option<String> {
        if self.proposals.is_empty() {
            return None;
        }
        let p = self.proposals.remove(0);
        let mutates_annotation = annotation_verb(&p.req.method).is_some();
        let resp = self.handle_request(p.req);
        let ok = resp.error.is_none();
        // Accepted but not on screen: the acceptance says where (t-155).
        let tail = match (&resp.result, mutates_annotation) {
            (Some(r), true) => mutated_id(r)
                .and_then(|id| self.doc.hidden_reason(&id, self.doc.step))
                .map(|why| self.hidden_tail(&why, &p.summary))
                .unwrap_or_default(),
            _ => String::new(),
        };
        let _ = p.reply.send(resp);
        self.decided.0 += 1;
        self.close_review_if_empty();
        Some(if ok {
            format!("accepted p{}: {}{tail}", p.id, p.summary)
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
        self.expire_proposals_at(std::time::Instant::now())
    }

    /// [`App::expire_proposals`] at `now` (tests pass their own clock).
    pub fn expire_proposals_at(&mut self, now: std::time::Instant) -> bool {
        let mut expired = Vec::new();
        self.proposals.retain(|p| {
            if now.saturating_duration_since(p.since) >= PROPOSAL_TIMEOUT {
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

    /// The filter for every error and echo about the open document
    /// (t-167): on a document with redacts no target is quoted.
    pub fn echo(&self) -> tiza_core::redact::Echo {
        tiza_core::redact::Echo::new(&self.doc)
    }

    pub fn supervise_status(&self) -> Value {
        // A method the API does not have is the agent's text (t-167).
        let quoting = tiza_core::redact::Echo::quoting(&self.doc).active();
        let method = |m: &str| -> String {
            if quoting && !tiza_core::protocol::METHODS.iter().any(|(k, _)| *k == m) {
                tiza_core::redact::ELIDED.into()
            } else {
                m.to_string()
            }
        };
        json!({
            "supervised": self.supervised,
            "pending": self.proposals.iter().map(|p| json!({"id": p.id, "method": method(&p.req.method), "summary": p.summary, "age_ms": p.since.elapsed().as_millis() as u64})).collect::<Vec<_>>(),
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
            // Plain "pong": `status` already carries `protocol`.
            "ping" => json!("pong"),
            "status" => {
                self.sync_pointer();
                serde_json::to_value(self.status())?
            }
            "pointer.set" => {
                let p: tiza_core::protocol::PointerParams = parse(params)?;
                let at = self.set_pointer(&p)?;
                serde_json::to_value(tiza_core::protocol::PointerReply {
                    pointer: Some(at),
                    shown: self.pointer_shown().is_some(),
                })?
            }
            "pointer.clear" => {
                self.clear_pointer();
                serde_json::to_value(tiza_core::protocol::PointerReply {
                    pointer: None,
                    shown: false,
                })?
            }
            "api.schema" => serde_json::to_value(tiza_core::schema::api_schema())?,
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
                // What an agent reads is what the screen shows: masked (t-117).
                let view = self.doc.view_at(self.doc.step);
                let code = tiza_core::redact::masked_code(&view.doc, self.doc.step);
                let mut out = String::new();
                for (i, text) in code.iter().enumerate() {
                    let line = self.doc.source.first_line + i as u32;
                    out.push_str(&format!("{line:>4}  {text}\n"));
                }
                json!({"language": self.doc.source.language, "file": self.shown_file(), "view": self.doc.shown_view_at(self.doc.step).label(), "first_line": self.doc.source.first_line, "last_line": self.doc.last_line(), "text": out})
            }
            "scene.get" => {
                let p: tiza_core::protocol::SceneGetParams = if params.is_null() {
                    Default::default()
                } else {
                    parse(params)?
                };
                // On the wire the code travels embedded: a client in another
                // directory could not resolve a relative file reference.
                // Masked (t-117): the embedded code an agent gets never holds
                // a secret; linked files travel embedded too (t-105).
                let mut document = self.doc.embedded_copy();
                // A reader's copy (`tiza://scene`, the exports): every text
                // repeating a masked value masked too (t-174). Without it
                // the author's text, as written: the editing call.
                if p.as_shown {
                    tiza_core::redact::Echo::quoting(&self.doc).shown_doc(&mut document);
                }
                // A redact a move widened at this step says so (t-140).
                if !self.doc.moves.is_empty() {
                    let view = self.doc.view_at(self.doc.step);
                    for a in &mut document.annotations {
                        let now = view.doc.find_annotation(&a.id).map(|v| &v.shape);
                        if let (
                            Shape::Redact { widened, .. },
                            Some(Shape::Redact { widened: true, .. }),
                        ) = (&mut a.shape, now)
                        {
                            *widened = true;
                        }
                    }
                }
                serde_json::to_value(Scene {
                    revision: self.revision,
                    step: self.doc.step,
                    max_step: self.doc.max_step(),
                    document,
                })?
            }
            "scene.list" => json!(self.list_lines()),
            "move.add" => {
                let m: Move = parse(params)?;
                let (i, notices) = self.add_move_noting(m)?;
                serde_json::to_value(MoveMutated {
                    index: i + 1,
                    moves: self.doc.moves.len(),
                    max_step: self.doc.max_step(),
                    revision: self.revision,
                    notices,
                })?
            }
            "move.delete" => {
                let p: MoveIndexParams = parse(params)?;
                let (_, notices) = self.delete_move_noting(p.index)?;
                serde_json::to_value(MoveMutated {
                    index: p.index,
                    moves: self.doc.moves.len(),
                    max_step: self.doc.max_step(),
                    revision: self.revision,
                    notices,
                })?
            }
            "move.list" => serde_json::to_value(self.move_list())?,
            "annotation.add" => {
                let p: AddParams = parse(params)?;
                let (shape, markers) = p.shape.resolve_with_markers(&self.doc)?;
                let id = self.add_annotation_full(p.id, p.step, p.color, markers, p.file, shape)?;
                // Where it stands against the view right now (t-155).
                serde_json::to_value(AnnotationMutated::of(&self.doc, id, self.revision))?
            }
            "annotation.update" => {
                let p: UpdateParams = parse(params)?;
                self.update_annotation(&p)?;
                serde_json::to_value(AnnotationMutated::of(&self.doc, p.id, self.revision))?
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
                if st.glyph.is_none() && tiza_core::model::builtin_style(name).is_none() {
                    return Err(Error::Invalid("a style needs at least a glyph".into()));
                }
                self.doc.styles.insert(name.to_string(), st.clone());
                self.touch();
                json!({"name": name, "style": st, "revision": self.revision})
            }
            "style.list" => {
                // Style names and legends are shown text (t-174).
                let mut shown = self.doc.clone();
                tiza_core::redact::Echo::quoting(&self.doc).shown_doc(&mut shown);
                json!({
                "document": shown.styles,
                "builtin": BUILTIN_KINDS.iter().map(|(k, g, l, c)| json!({"kind": k, "glyph": g, "legend": l, "color": c})).collect::<Vec<_>>(),
                "aliases": tiza_core::model::KIND_ALIASES.iter().map(|(a, k)| json!({"alias": a, "kind": k})).collect::<Vec<_>>(),
                })
            }
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
                serde_json::to_value(StepReply::of(&self.doc, self.revision))?
            }
            "step.next" => {
                self.next_step();
                serde_json::to_value(StepReply::of(&self.doc, self.revision))?
            }
            "step.prev" => {
                self.set_step(self.doc.step.saturating_sub(1));
                serde_json::to_value(StepReply::of(&self.doc, self.revision))?
            }
            "step.goto" => {
                let p: StepGotoParams = parse(params)?;
                let to = p.id.resolve(&self.doc)?;
                self.goto_step(&to.to_string())?;
                serde_json::to_value(StepReply::of(&self.doc, self.revision))?
            }
            "check" => {
                let p: tiza_core::schema::CheckParams = parse(params)?;
                let width = p.width.map(tiza_core::layout::check_width).transpose()?;
                // On the document the instance holds, never on the masked
                // copy an agent reads: masks change the columns. Only the
                // report leaves, and it never quotes a secret.
                let base = self.save_path.parent().map(Path::to_path_buf);
                serde_json::to_value(tiza_core::check::check_document(
                    &self.doc,
                    base.as_deref(),
                    &tiza_core::check::CheckOptions { width },
                ))?
            }
            "render.text" => {
                let width = match params.get("width").and_then(|w| w.as_u64()) {
                    Some(w) => tiza_core::layout::check_width(w.min(u64::from(u32::MAX)) as usize)?,
                    None => self.size.0 as usize,
                };
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
                    None | Some("") | Some("toggle") => {
                        let next = self.theme.toggled().name;
                        self.set_theme_name(next, true);
                    }
                    Some("auto") => self.theme_follow_host(),
                    Some(other) => match Self::parse_theme_name(other) {
                        Some(name) => self.set_theme_name(name, true),
                        None => {
                            return Err(Error::Invalid(format!(
                                "unknown theme: {other} (dark, light, terminal, auto, toggle)"
                            )));
                        }
                    },
                }
                self.touch();
                json!({"theme": self.theme.name, "auto": self.theme_auto, "revision": self.revision})
            }
            other => return Err(Error::Invalid(format!("unknown method: {other}"))),
        })
    }

    pub fn status(&self) -> Status {
        Status {
            app: tiza_core::APP_NAME.into(),
            version: tiza_core::VERSION.into(),
            build: tiza_core::GIT_SHA.into(),
            protocol: tiza_core::protocol::PROTOCOL_VERSION,
            revision: self.revision,
            file: self.shown_file(),
            lines: self.doc.code.len(),
            first_line: self.doc.source.first_line,
            last_line: self.doc.last_line(),
            view: self.doc.shown_view_at(self.doc.step).label(),
            annotations: self.doc.annotations.len(),
            step: self.doc.step,
            max_step: self.doc.max_step(),
            // Shown text (t-174): a title or id repeating a masked value.
            step_id: self
                .doc
                .named_step(self.doc.step)
                .and_then(|n| n.id.as_deref().map(|t| self.shown(t))),
            step_title: self
                .doc
                .named_step(self.doc.step)
                .and_then(|n| n.title.as_deref().map(|t| self.shown(t))),
            step_label: self.doc.step_label(self.doc.step),
            auto_advance_ms: self.doc.auto_advance_ms(self.doc.step),
            looping: self.doc.looping,
            cols: self.size.0,
            rows: self.size.1,
            warnings: {
                let mut w = self.viewport_warnings();
                w.extend(self.doc.step_warnings());
                w
            },
            redact_alerts: self.redact_alerts.clone(),
            supervised: self.supervised,
            autosave: self.autosave,
            pending: self.proposals.len(),
            numbers: self.doc.numbers,
            margin: self.doc.margin,
            pointer: self.pointer,
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

/// The tail [`App::add_step_hint`] appends: ` (step 1, press n)` when the
/// annotation is one step ahead, ` (step 3, :step 3 shows it)` further on;
/// just ` (press n)` / ` (:step 3 shows it)` when `msg` already names the
/// step (`a3 now appears at step 3`).
pub fn step_hint(msg: &str, target: u32, shown: u32) -> String {
    let how = if target == shown + 1 {
        "press n".to_string()
    } else {
        format!(":step {target} shows it")
    };
    if names_step(msg, target) {
        format!(" ({how})")
    } else {
        format!(" (step {target}, {how})")
    }
}

/// Whether `msg` contains `step N` as whole words.
fn names_step(msg: &str, n: u32) -> bool {
    let needle = format!("step {n}");
    msg.match_indices(&needle).any(|(i, _)| {
        let before_ok = msg[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric() && c != ':');
        let after_ok = msg[i + needle.len()..]
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_digit());
        before_ok && after_ok
    })
}

/// Byte index where a [`step_hint`] tail starts in `msg`, if it ends with
/// one: the status bar and the presentation line keep that tail when they
/// cut a long message (t-132).
pub fn step_hint_start(msg: &str) -> Option<usize> {
    let i = msg.rfind(" (")?;
    let inner = msg[i + 2..].strip_suffix(')')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    // ` (src/queue.rs, not shown at step 2)`: an agent change whose file a
    // link hides (t-155).
    if let Some((file, n)) = inner.rsplit_once(", not shown at step ") {
        return (!file.is_empty() && digits(n)).then_some(i);
    }
    let how = match inner.strip_prefix("step ") {
        Some(rest) => {
            let (n, how) = rest.split_once(", ")?;
            if !digits(n) {
                return None;
            }
            how
        }
        None => inner,
    };
    let ok = how == "press n"
        || how
            .strip_prefix(":step ")
            .and_then(|r| r.strip_suffix(" shows it"))
            .is_some_and(digits);
    ok.then_some(i)
}

pub fn theme_label(t: ThemeName) -> &'static str {
    match t {
        ThemeName::Dark => "dark",
        ThemeName::Light => "light",
        ThemeName::Terminal => "terminal",
    }
}

/// The loud notice after a reload or paste touched a redact it could not
/// follow (t-117): widened (fail-closed) and/or dropped. `None` when every
/// redact followed its text. Never the text under a mask: ids and lines.
fn redact_notice(widened: &[(String, String)], dropped: &[String]) -> Option<String> {
    let mut parts = Vec::new();
    if !widened.is_empty() {
        let ids: Vec<&str> = widened.iter().map(|(id, _)| id.as_str()).collect();
        let places: Vec<&str> = widened.iter().map(|(_, p)| p.as_str()).collect();
        parts.push(format!(
            "redact {} could not follow {} text: masking {} — fix it before recording",
            ids.join(", "),
            if ids.len() == 1 { "its" } else { "their" },
            places.join(", ")
        ));
    }
    if !dropped.is_empty() {
        parts.push(format!(
            "redact {} dropped: the line changed — check the screen before recording",
            dropped.join(", ")
        ));
    }
    (!parts.is_empty()).then(|| format!("{} (u undoes the reload)", parts.join("; ")))
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
        // Presentation mode hides the status bar, the cursor and the
        // selection, so annotating with the mouse there would be blind: the
        // capture stays on (back in normal mode it works again) but every
        // event is ignored (t-033 AC6).
        if !self.mouse || self.present || self.overlay != Overlay::None {
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

/// `added` / `updated` for the methods that add or change one annotation
/// (t-155), `None` for the rest.
fn annotation_verb(method: &str) -> Option<&'static str> {
    match method {
        "annotation.add" => Some("added"),
        "annotation.update" => Some("updated"),
        _ => None,
    }
}

/// The `id` of an `annotation.add` / `annotation.update` result.
fn mutated_id(result: &Value) -> Option<String> {
    result.get("id").and_then(Value::as_str).map(String::from)
}

/// An annotation in a few words, as the `:` command that makes it
/// confirms it: `box 3-5`, `note L13`, `arrow L2 → L6` (t-155).
pub fn shape_summary(a: &Annotation) -> String {
    match &a.shape {
        Shape::Box {
            from_line, to_line, ..
        } => format!("box {from_line}-{to_line}"),
        Shape::Callout {
            from_line, to_line, ..
        } => match to_line {
            Some(t) if t != from_line => format!("callout L{from_line}-{t}"),
            _ => format!("callout L{from_line}"),
        },
        Shape::Note { line, .. } => format!("note L{line}"),
        Shape::Arrow { from, to, .. } => format!("arrow L{} → L{}", from.line, to.line),
        Shape::Highlight { span, .. } => format!("highlight L{}", span.line),
        Shape::Mark { kind, span, .. } => format!("mark {kind} L{}", span.line),
        Shape::Redact { span, .. } => format!("redact L{}", span.line),
        Shape::Spotlight {
            from_line,
            to_line,
            ids,
            ..
        } => match (from_line, to_line) {
            (Some(f), Some(t)) if f != t => format!("spotlight L{f}-{t}"),
            (Some(f), _) => format!("spotlight L{f}"),
            _ if !ids.is_empty() => format!("spotlight {}", ids.join(", ")),
            _ => "spotlight".into(),
        },
    }
}

/// One line describing an agent request for the review overlay. It goes to
/// the list, the y/n/expiry messages and `supervise.status` (t-167): on a
/// document with redacts a `find` and a step id are quoted `«…»` and every
/// other text the agent wrote (a label, an id) passes the copy rule of
/// t-157 ([`tiza_core::redact::Echo`]).
pub fn proposal_summary(echo: &tiza_core::redact::Echo, req: &Request) -> String {
    let p = &req.params;
    let raw = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let s = |k: &str| echo.copies(&raw(k));
    let n = |k: &str| p.get(k).and_then(|v| v.as_u64());
    let place = |k: &str| -> String {
        match p.get(k) {
            Some(w) if w.is_object() => {
                if let Some(f) = w.get("find").and_then(|v| v.as_str()) {
                    echo.quote(f)
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
            let kind = raw("type");
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
                // A type no shape has is the agent's text (t-167).
                k if echo.active() && !SHAPE_TYPES.contains(&k) => tiza_core::redact::ELIDED.into(),
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
        "move.add" => format!(
            "add a {} move at step {}",
            s("op"),
            n("at_step").unwrap_or(0)
        ),
        "move.delete" => format!("delete move #{}", n("index").unwrap_or(0)),
        "step.set" => format!("go to step {}", n("step").unwrap_or(0)),
        "step.next" => "next step".into(),
        "step.goto" => format!(
            "go to step {}",
            p.get("id")
                .map(|v| match v.as_str() {
                    // A step id the agent wrote is quoted like a target.
                    Some(id) if echo.active() && !id.trim().chars().all(|c| c.is_ascii_digit()) => {
                        echo.quote(id)
                    }
                    Some(id) => id.to_string(),
                    None => v.to_string(),
                })
                .unwrap_or_default()
        ),
        "step.prev" => "previous step".into(),
        "focus.set" => "change the focus".into(),
        "pointer.set" => match (n("line"), n("from"), n("to")) {
            (Some(l), _, _) => format!("point at L{l}"),
            (None, Some(a), Some(b)) => format!("point at L{a}-{b}"),
            (None, Some(a), None) => format!("point at L{a}"),
            _ => format!("point at {}", echo.quote(&s("find"))),
        },
        "pointer.clear" => "turn the pointer off".into(),
        "theme.set" => format!("theme {}", s("theme")),
        "follow.set" => "toggle follow mode".into(),
        // A method the API does not have is the agent's text (t-167).
        other
            if echo.active()
                && !tiza_core::protocol::METHODS
                    .iter()
                    .any(|(m, _)| *m == other) =>
        {
            tiza_core::redact::ELIDED.into()
        }
        other => other.to_string(),
    }
}

/// The words of a `:` command line, as the redact filter reads what was
/// written (t-167): the line, and its pieces split at blanks, quotes,
/// `@`, `:` and `..` (`:box @'a'..@'b'`, `:hl 2:text`).
fn command_words(input: &str) -> Vec<String> {
    let mut out = vec![input.to_string()];
    out.extend(
        input
            .replace("..", " ")
            .split(|c: char| c.is_whitespace() || "'\"`@:".contains(c))
            .filter(|w| !w.is_empty())
            .map(String::from),
    );
    out
}

/// The `type`s of `annotation.add`.
const SHAPE_TYPES: &[&str] = &[
    "highlight",
    "mark",
    "box",
    "arrow",
    "note",
    "callout",
    "spotlight",
    "redact",
];

/// Image formats `:export` can ask the binary for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Svg,
    Png,
}

/// Renders a canvas to SVG or PNG bytes (`title` goes into the SVG metadata).
pub type ImageExporter = Box<
    dyn Fn(&tiza_core::canvas::Canvas, &Theme, ImageFormat, Option<&str>) -> Result<Vec<u8>, String>
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
    bs: &mut tiza_core::model::BorderStyle,
    k: &str,
    v: &str,
) -> Result<(), Error> {
    use tiza_core::model::{BorderStyle, Route, Stroke};
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
pub fn apply_line_kv(ls: &mut tiza_core::model::LineStyle, k: &str, v: &str) -> Result<(), Error> {
    use tiza_core::model::{Head, Heads, Route, Stroke};
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

/// Known commands within edit distance 2 of `typed` (up to three, nearest
/// first), so a typo can be fixed in one keystroke.
fn nearest_commands(typed: &str) -> Vec<&'static str> {
    let typed = typed.to_ascii_lowercase();
    let mut scored: Vec<(usize, &str)> = App::COMMANDS
        .iter()
        .filter(|c| c.len() > 1)
        .map(|c| (edit_distance(&typed, c), *c))
        .filter(|(d, _)| *d <= 2)
        .collect();
    scored.sort_by_key(|(d, c)| (*d, c.len(), *c));
    scored.dedup_by_key(|(_, c)| *c);
    if let Some((best, _)) = scored.first() {
        let best = *best;
        scored.retain(|(d, _)| *d == best);
    }
    scored.into_iter().take(3).map(|(_, c)| c).collect()
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// Every `key=value` must have both halves: `=`, `=red` or `glyph=` are
/// typos, not an empty key or an empty value.
fn check_kv_shape(kv: &[(String, String)], raw: &str) -> Result<(), Error> {
    for (k, v) in kv {
        if k.is_empty() || v.is_empty() {
            let shown = raw
                .split_whitespace()
                .find(|t| t.contains('='))
                .unwrap_or(raw.trim());
            return Err(Error::Invalid(format!("expected key=value, got «{shown}»")));
        }
    }
    Ok(())
}

/// True if a typed `:hl` / `:mark` anchor names a line *and* has the colon
/// (`7:text`, `7:` for the whole line, `.:text`, `7@3..9`): `s` alone or
/// `7` alone (a bare line followed by what was meant as text) is a typo.
fn typed_anchor_has_line_and_text(tok: &str) -> bool {
    let t = tok.trim();
    if !looks_like_anchor(t) {
        return false;
    }
    t.contains(':') || t.contains('@')
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

/// Reads one marker after `@`: `'text with spaces'`, `"text"` or a bare word
/// (up to `..` or whitespace). Returns the text and what follows.
fn take_marker(s: &str) -> Result<(String, &str), Error> {
    if let Some(q) = s.chars().next().filter(|c| *c == '\'' || *c == '"') {
        let body = &s[1..];
        let end = body
            .find(q)
            .ok_or_else(|| Error::Invalid(format!("markers: missing closing {q}")))?;
        let text = &body[..end];
        if text.is_empty() {
            return Err(Error::Invalid("markers: empty marker".into()));
        }
        return Ok((text.to_string(), &body[end + 1..]));
    }
    let end = s
        .find("..")
        .into_iter()
        .chain(s.find(char::is_whitespace))
        .min()
        .unwrap_or(s.len());
    if end == 0 {
        return Err(Error::Invalid("markers: empty marker".into()));
    }
    Ok((s[..end].to_string(), &s[end..]))
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
    // `-1-3`, `3-` or `-5`: a sign or a dangling dash is a typo, never
    // silently a valid range.
    if s.starts_with(['-', ':']) || s.ends_with(['-', ':']) {
        return Err(Error::Invalid(format!("invalid range: {s}")));
    }
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
            Some(Selection::Span(tiza_core::TextSpan {
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
        a.pending_sel = Some(Selection::Span(tiza_core::TextSpan {
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
        // `ma` is ambiguous since `:margin` (t-113): both are offered.
        let (text, many) = a.complete("ma");
        assert_eq!(text, "mar");
        assert!(many.contains(&"mark".to_string()) && many.contains(&"margin".to_string()));
        assert_eq!(a.complete("mark").0, "mark ");
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
                assert_eq!(*placement, tiza_core::model::Placement::Below);
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
