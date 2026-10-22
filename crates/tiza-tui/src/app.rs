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

/// What the screen shows instead of the followed file, for the reason
/// follow is off.
#[derive(Debug, Clone, Copy)]
enum NotTheFile {
    /// Pasted code: `the code is pasted`, `the code is the paste again`…
    Paste(&'static str),
    /// A version of the file it no longer holds, which an undo or a redo
    /// brought back.
    Version,
}

impl NotTheFile {
    fn why(self, name: &str) -> String {
        match self {
            NotTheFile::Paste(what) => format!("{what}, not {name}; :follow on reloads {name}"),
            NotTheFile::Version => {
                format!("the code on screen is not {name}; :follow on reloads {name}")
            }
        }
    }
}

/// The screen before an undo or a redo, for what follow says after it.
struct BeforeHistory {
    /// The file followed: the reference on screen, or the one embedded code
    /// on screen replaced.
    followed: Option<tiza_core::model::CodeSource>,
    /// The file `:follow on` would reload while follow is off over embedded
    /// code ([`App::paused_file`]).
    was: Option<String>,
    /// [`App::code_now`].
    code: (Vec<String>, u32, bool),
    /// Why follow was off ([`App::off_reason`]).
    reason: Option<String>,
}

/// What a `:` command does with the selection a `v`/`V` left pending
/// (t-203): `:hl` and `:mark` take it as their anchor and keep it when they
/// used it, so ↑ + Enter runs them again on the same text; the commands
/// that only look (`:help`, `:ls`, `:kinds`, `:style`) and `:undo`/`:redo`
/// leave it as it is; every other command lets it go once it has run
/// (`:note`, `:box`, `:callout` opened from a selection carry its lines in
/// their own text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionUse {
    Takes,
    Neutral,
    Ignores,
}

/// The one list of which `:` commands take the pending selection: every
/// name and alias `run_command` knows, `None` for any other (a test walks
/// `App::COMMANDS` so a new command cannot be left out).
fn selection_use(name: &str) -> Option<SelectionUse> {
    use SelectionUse::*;
    Some(match name {
        "hl" | "highlight" | "mark" | "m" => Takes,
        "help" | "h" | "?" | "ls" | "list" | "style" | "kinds" | "undo" | "u" | "redo" => Neutral,
        "q" | "quit" | "q!" | "quit!" | "w" | "write" | "export" | "supervise" | "supervised"
        | "mouse" | "review" | "wq" | "box" | "arrow" | "ar" | "note" | "n" | "embed" | "link"
        | "ref" | "spot" | "spotlight" | "callout" | "co" | "set" | "follow" | "until"
        | "clear" | "del" | "delete" | "rm" | "color" | "col" | "label" | "step" | "s" | "next"
        | "goto" | "prev" | "focus" | "f" | "theme" | "t" | "present" | "p" | "lang" | "title"
        | "margin" | "numbers" | "nums" | "pad" | "padding" | "layout" | "autosave" => Ignores,
        _ => return None,
    })
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

/// How many pasted codes [`App`] remembers to tell a paste on screen from
/// an older version of the followed file.
const PASTES_KEPT: usize = 32;

thread_local! {
    static ANSWERING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// Where the last caught request panic happened and its message, for
    /// `panics.log` (set by the crash hook).
    static LAST_PANIC: std::cell::RefCell<Option<(String, String)>> = const { std::cell::RefCell::new(None) };
}

/// Called by the crash hook for a panic [`App::handle_request`] catches
/// (t-166): keeps its place and message for `panics.log`.
pub fn note_request_panic(info: &std::panic::PanicHookInfo<'_>) {
    let at = info
        .location()
        .map(|l| format!("{}:{}", l.file(), l.line()))
        .unwrap_or_else(|| "unknown place".into());
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_default();
    LAST_PANIC.with(|p| *p.borrow_mut() = Some((at, message)));
}

/// What one socket request may change in the [`App`] (t-166), taken
/// before it runs and put back if it panics, so a dropped request leaves
/// nothing behind: not its document, not a typing animation that would
/// keep drawing the code it wrote, not an undo entry.
struct RequestState {
    doc: Document,
    save_path: PathBuf,
    history: tiza_core::history::Checkpoint,
    theme: Theme,
    theme_auto: bool,
    mode: Mode,
    overlay: Overlay,
    overlay_scroll: usize,
    present: bool,
    focus: Option<(u32, u32)>,
    cursor_line: u32,
    cursor_col: u32,
    pending_sel: Option<Selection>,
    arrow_from: Option<tiza_core::Anchor>,
    selected: Option<String>,
    scroll: usize,
    message: Option<(String, bool)>,
    dirty: bool,
    autosave: bool,
    layout_stable: bool,
    padding_override: Option<(u16, u16)>,
    supervised: bool,
    typing: Option<TypingState>,
    step_ready_at: Option<std::time::Instant>,
    pointer: Option<tiza_core::protocol::PointerAt>,
    pointer_seen: ((u32, u32), Option<u32>),
    agent_notice: Option<AgentNotice>,
    agent_deferred: Vec<(String, &'static str)>,
    following: bool,
}

impl RequestState {
    fn take(app: &App) -> RequestState {
        RequestState {
            doc: app.doc.clone(),
            save_path: app.save_path.clone(),
            history: app.history.checkpoint(),
            theme: app.theme.clone(),
            theme_auto: app.theme_auto,
            mode: app.mode.clone(),
            overlay: app.overlay,
            overlay_scroll: app.overlay_scroll,
            present: app.present,
            focus: app.focus,
            cursor_line: app.cursor_line,
            cursor_col: app.cursor_col,
            pending_sel: app.pending_sel,
            arrow_from: app.arrow_from,
            selected: app.selected.clone(),
            scroll: app.scroll,
            message: app.message.clone(),
            dirty: app.dirty,
            autosave: app.autosave,
            layout_stable: app.layout_stable,
            padding_override: app.padding_override,
            supervised: app.supervised,
            typing: app.typing.clone(),
            step_ready_at: app.step_ready_at,
            pointer: app.pointer,
            pointer_seen: app.pointer_seen,
            agent_notice: app.agent_notice.clone(),
            agent_deferred: app.agent_deferred.clone(),
            following: app.follow.is_some(),
        }
    }

    fn restore(self, app: &mut App) {
        app.doc = self.doc;
        app.save_path = self.save_path;
        app.history.rollback(self.history);
        app.theme = self.theme;
        app.theme_auto = self.theme_auto;
        app.mode = self.mode;
        app.overlay = self.overlay;
        app.overlay_scroll = self.overlay_scroll;
        app.present = self.present;
        app.focus = self.focus;
        app.cursor_line = self.cursor_line;
        app.cursor_col = self.cursor_col;
        app.pending_sel = self.pending_sel;
        app.arrow_from = self.arrow_from;
        app.selected = self.selected;
        app.scroll = self.scroll;
        app.message = self.message;
        app.dirty = self.dirty;
        app.autosave = self.autosave;
        app.layout_stable = self.layout_stable;
        app.padding_override = self.padding_override;
        app.supervised = self.supervised;
        app.typing = self.typing;
        app.step_ready_at = self.step_ready_at;
        app.pointer = self.pointer;
        app.pointer_seen = self.pointer_seen;
        app.agent_notice = self.agent_notice;
        app.agent_deferred = self.agent_deferred;
        // Follow holds a file watcher: stopped, or started again.
        match (self.following, app.follow.is_some()) {
            (false, true) => app.follow = None,
            (true, false) => {
                let _ = app.set_follow(true);
            }
            _ => {}
        }
        app.clamp_cursor();
    }
}

/// Whether this thread is answering a socket request inside
/// [`App::handle_request`]'s guard (t-166): the crash hook then leaves the
/// terminal as it is, since the panic is caught, answered as an error and
/// the TUI keeps running.
pub fn answering_request() -> bool {
    ANSWERING.with(|a| a.get())
}

/// Runs `f` as a socket request is answered, on any thread (t-210: the
/// live `check` runs on one of its own): a panic in it is caught, the crash
/// hook leaves the terminal alone ([`answering_request`]), and its place
/// and message go to `panics.log`, whose path is the error when written.
pub(crate) fn catch_request_panic<R>(
    what: &str,
    f: impl FnOnce() -> R,
) -> Result<R, Option<PathBuf>> {
    let was = ANSWERING.with(|a| a.replace(true));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    ANSWERING.with(|a| a.set(was));
    r.map_err(|_| log_last_request_panic(what))
}

/// Appends one line about the last request panic caught on this thread to
/// `panics.log` (the time, the version, the request, where it panicked and
/// its message). Returns the log's path when it was written.
fn log_last_request_panic(what: &str) -> Option<PathBuf> {
    let (at, message) = LAST_PANIC
        .with(|p| p.borrow_mut().take())
        .unwrap_or_else(|| ("unknown place".into(), String::new()));
    crate::log_line(&format!("{what} panicked at {at}: {message}"))
}

/// Methods that never change the document or the view: they bypass supervision.
/// What the take's geometry was computed for: the document's revision,
/// and, as a check on it, a few of its sizes and the layout options it
/// reads. A typing tick changes the code shown, not the take, and keeps it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GeometryKey {
    revision: u64,
    moves: usize,
    annotations: usize,
    lines: usize,
    line_numbers: bool,
    tab_width: usize,
}

/// What a layout depends on, for the viewport facts of [`App::viewport`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewportKey {
    revision: u64,
    step: u32,
    width: usize,
    stable: bool,
    typing: bool,
}

/// The layout's answers `status` needs for its warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ViewportFacts {
    clipped_code: Option<usize>,
    labels_in_margin: bool,
}

impl ViewportFacts {
    fn of(l: &Layout) -> Self {
        ViewportFacts {
            clipped_code: l.clipped_code,
            labels_in_margin: l.labels_in_margin,
        }
    }
}

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

/// The files a `scene.put` sent different code for ([`App::keep_references`]).
#[derive(Debug, Default)]
struct SentCode {
    /// Changed by the agent, as the bar says it (`the code sent for main.rs
    /// differs from the file on disk`): shown, not saved over the file.
    edited: Vec<String>,
    /// Changed on disk since the document was loaded: still a reference.
    stale: Vec<String>,
    /// Embedded in memory by an earlier put of edited code, and read from
    /// the file again now.
    back: Vec<String>,
}

pub struct App {
    pub doc: Document,
    pub save_path: PathBuf,
    pub history: History,
    /// The newest undo entry when the sidecar was last written: the edits
    /// after it are the unsaved ones.
    pub saved_at: u64,
    /// The undo entries and the sidecar text as last written.
    saved_entries: Vec<(u64, String)>,
    saved_json: Option<String>,
    /// Ctrl-C asked to quit: a failed autosave is tried once more on the
    /// way out.
    pub retry_save_on_exit: bool,
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
    /// The `:` command running now anchored itself on `pending_sel` (t-203).
    selection_used: bool,
    /// Command-line history (most recent last) and the index while browsing it.
    pub history_cmds: Vec<String>,
    pub history_pos: Option<usize>,
    /// First end of an arrow being built with `a` (select → a → select → a).
    pub arrow_from: Option<tiza_core::Anchor>,
    /// Annotation selected with Tab (its id), for quick edits.
    pub selected: Option<String>,
    pub scroll: usize,
    pub message: Option<(String, bool)>,
    /// The last notice about moves that stopped matching the code (or a
    /// `rebind` entry a paste or a reload left ignored): while it is the
    /// message, the bar gives it the room it needs.
    pub stale_message: Option<String>,
    /// Presentation mode only (t-107): until when `message` — the result of
    /// the command just run — is drawn on the last row. `None`: hidden.
    pub present_message_until: Option<std::time::Instant>,
    pub revision: u64,
    /// The take's geometry (arrow lanes, box band, code column), computed
    /// from the whole document once per change instead of once per frame:
    /// presentation mode, `:layout stable` and the exports use it.
    geometry: std::cell::RefCell<Option<(GeometryKey, tiza_core::layout::StableGeometry)>>,
    /// What the last frame's layout said about the viewport, for `status`
    /// (its warnings), so a poll does not lay the document out again.
    viewport: std::cell::RefCell<Option<(ViewportKey, ViewportFacts)>>,
    /// How many layouts this instance has made (every frame is one), and
    /// how many times it computed the take's geometry: what the tests count
    /// to see that a poll lays nothing out and a frame reuses the geometry.
    pub layouts: std::cell::Cell<u64>,
    pub geometries: std::cell::Cell<u64>,
    pub dirty: bool,
    pub quit: bool,
    pub size: (u16, u16),
    pub socket_path: Option<PathBuf>,
    pub autosave: bool,
    /// The file the code was last read from (`code: {file}` when opened or
    /// at the last put): a put that sent other code leaves the document
    /// embedded in memory, and a later put of that file's code makes it a
    /// reference again. `None` once `:embed` froze the code on purpose.
    reference: Option<tiza_core::model::CodeSource>,
    /// Linked files the document itself keeps in `views` (as opened, or
    /// frozen by `:embed`), as opposed to ones a put sent embedded.
    frozen_views: std::collections::BTreeSet<String>,
    /// `:layout stable`: the authoring screen uses the take's geometry
    /// (lanes, box band and margin sized for every step at once, t-111) so
    /// rehearsing looks like presenting. Presentation mode and every
    /// export are always stable.
    pub layout_stable: bool,
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
    /// The live `check` requests, answered off this thread (t-210).
    checks: crate::checks::Checks,
    /// Scroll offset (wrapped rows) of the help / list overlay.
    pub overlay_scroll: usize,
    /// Follow mode: the watched source file, when enabled.
    pub follow: Option<crate::follow::Follow>,
    /// The followed file reference that embedded code on screen replaced
    /// (a paste, or an undo past a reload): what `:follow on` reloads. A
    /// paste with follow off sets it too once one is known (follow was on
    /// before). It counts only while the code is embedded ([`App::paused`]);
    /// `:follow off` keeps it, `:embed` and another document drop it. Why
    /// follow is off is worked out from the screen each time
    /// ([`App::off_reason`]).
    follow_file: Option<tiza_core::model::CodeSource>,
    /// The followed file changed while moves were applied (t-206): its
    /// reload waits for step 0, and the code on screen is embedded meanwhile.
    reload_waits: bool,
    /// Code pasted in this session (the newest last, at most
    /// [`PASTES_KEPT`]): embedded code on screen equal to one of them is
    /// «pasted», anything else a version of the file.
    pasted_codes: Vec<Vec<String>>,
    /// The code on screen was reached by an undo or a redo, not by the edit
    /// itself: a paste is then «the paste again».
    screen_via_history: bool,
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
        let reference = (!doc.is_embedded()).then(|| doc.code_src.clone());
        let frozen_views = doc.views.keys().cloned().collect();
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
            saved_at: 0,
            saved_entries: Vec::new(),
            saved_json: None,
            retry_save_on_exit: false,
            theme,
            mode: Mode::Normal,
            overlay: Overlay::None,
            present: false,
            focus: None,
            cursor_line,
            cursor_col: 0,
            pending_sel: None,
            selection_used: false,
            history_cmds: Vec::new(),
            history_pos: None,
            arrow_from: None,
            selected: None,
            scroll: 0,
            message: None,
            stale_message: None,
            present_message_until: None,
            revision: 1,
            geometry: Default::default(),
            viewport: Default::default(),
            layouts: Default::default(),
            geometries: Default::default(),
            dirty: false,
            quit: false,
            size: (80, 24),
            socket_path: None,
            autosave: true,
            reference,
            frozen_views,
            layout_stable: false,
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
            checks: Default::default(),
            overlay_scroll: 0,
            follow: None,
            follow_file: None,
            reload_waits: false,
            pasted_codes: Vec::new(),
            screen_via_history: false,
            typing: None,
            step_ready_at: Some(std::time::Instant::now()),
            pointer: None,
            pointer_seen: ((cursor_line, cursor_line), None),
            agent_notice: None,
            agent_deferred: Vec::new(),
        };
        app.rebuild_theme();
        app.mark_saved();
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
        self.message = Some((msg.into(), false));
    }

    pub fn error(&mut self, msg: impl Into<String>) {
        self.save_warning_pending = false;
        self.message = Some((msg.into(), true));
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

    /// The layout every reader gets (render.text, screen captures) and the
    /// author's own screen.
    pub fn layout(&self, width: usize) -> Layout {
        self.lay_out(&self.doc, &self.theme, &self.layout_options(width))
    }

    /// The take's geometry computed at revision `from` still holds now.
    fn keep_geometry(&mut self, from: u64) {
        if let Some((k, _)) = self.geometry.get_mut()
            && k.revision == from
        {
            k.revision = self.revision;
        }
    }

    /// Lays `doc` (the open document, maybe with other options or a theme
    /// of its own) out, with the take's geometry computed once per change
    /// ([`App::geometry`]) when `opts` asks for a stable layout.
    fn lay_out(&self, doc: &Document, theme: &Theme, opts: &LayoutOptions) -> Layout {
        self.layouts.set(self.layouts.get() + 1);
        if !opts.stable {
            return layout(doc, theme, opts);
        }
        let key = GeometryKey {
            revision: self.revision,
            moves: doc.moves.len(),
            annotations: doc.annotations.len(),
            lines: doc.base().len(),
            line_numbers: opts.line_numbers,
            tab_width: opts.tab_width,
        };
        let fresh = self
            .geometry
            .borrow()
            .as_ref()
            .is_some_and(|(k, _)| *k == key);
        if !fresh {
            self.geometries.set(self.geometries.get() + 1);
            let g = tiza_core::layout::stable_geometry(doc, opts);
            *self.geometry.borrow_mut() = Some((key, g));
        }
        let cache = self.geometry.borrow();
        tiza_core::layout::layout_with(doc, theme, opts, cache.as_ref().map(|(_, g)| g))
    }

    /// The width the screen lays the code out at: the terminal of the last
    /// frame less its side padding, as `ui::draw` does; `check`'s default
    /// width while no terminal size is known.
    pub fn screen_width(&self) -> usize {
        let (cols, rows) = self.size;
        if cols == 0 || rows == 0 {
            return tiza_core::check::DEFAULT_WIDTH;
        }
        let (_, pad_h) =
            crate::ui::clamp_padding(self.effective_padding(), (cols, rows), self.present);
        usize::from(cols.saturating_sub(2 * pad_h)).max(1)
    }

    /// The labels cut with `…` or left out at the screen's width (within
    /// the widths `check` takes), with that width: the take, the layout
    /// presentation mode, the exports and `check` draw, with the step as it
    /// is once typed. While authoring the screen reserves room for the
    /// steps so far only, so before the last step it may place a label
    /// otherwise; what the audience sees is the take.
    fn screen_cuts(&self) -> (usize, Vec<tiza_core::layout::ClippedLabel>) {
        use tiza_core::layout::{MAX_WIDTH, MIN_WIDTH};
        let width = self.screen_width().clamp(MIN_WIDTH, MAX_WIDTH);
        let mut opts = self.layout_options(width);
        opts.stable = true;
        opts.typing = false;
        (
            width,
            self.lay_out(&self.doc, &self.theme, &opts).clipped_labels,
        )
    }

    /// Why annotation `id` is not on the screen now: the document's reasons
    /// at the current step, or a note or a callout (its text is all it
    /// draws) that the take leaves out. `cuts` holds [`App::screen_cuts`]
    /// once it is needed, so a layout runs only for those.
    fn not_shown(
        &self,
        id: &str,
        cuts: &mut Option<(usize, Vec<tiza_core::layout::ClippedLabel>)>,
    ) -> Option<tiza_core::model::Hidden> {
        if let Some(why) = self.doc.hidden_reason(id, self.doc.step) {
            return Some(why);
        }
        let a = self.doc.find_annotation(id)?;
        if !matches!(a.shape, Shape::Note { .. } | Shape::Callout { .. }) {
            return None;
        }
        let (width, cut) = cuts.get_or_insert_with(|| self.screen_cuts());
        cut.iter()
            .any(|c| c.id == id && c.dropped)
            .then_some(tiza_core::model::Hidden::NoRoom { width: *width })
    }

    /// The reply to `annotation.add` and `annotation.update`: where the
    /// annotation stands against the view, and against the take at the
    /// screen's width at this moment (a note left out is not visible; a
    /// label cut or left out is said in `clipped`).
    fn mutated(&self, id: String) -> AnnotationMutated {
        let mut m = AnnotationMutated::of(&self.doc, id, self.revision);
        if !m.visible {
            return m;
        }
        let mut cuts = None;
        if let Some(why) = self.not_shown(&m.id, &mut cuts) {
            m.visible = false;
            m.hidden = Some(why.to_string());
            return m;
        }
        let (width, cut) = cuts.get_or_insert_with(|| self.screen_cuts());
        let width = *width;
        let mine = || cut.iter().filter(|c| c.id == m.id);
        m.clipped = match (mine().any(|c| c.dropped), mine().next()) {
            (true, _) => Some(format!(
                "its label is not drawn: no room at {width} columns (check at width {width} lists it)"
            )),
            (false, Some(_)) => Some(format!(
                "its label is cut with … at {width} columns (check at width {width} lists it)"
            )),
            (false, None) => None,
        };
        m
    }

    /// [`App::layout`] with the live pointer (t-030). Only the presenter's
    /// frame (`ui::draw`) calls it; `layout` (`render.text`, screen
    /// captures) and every export never draw the pointer.
    pub fn screen_layout_with_pointer(&self, width: usize) -> Layout {
        let mut opts = self.layout_options(width);
        opts.pointer = self.pointer_shown();
        let l = self.lay_out(&self.doc, &self.theme, &opts);
        // `status` reads the viewport from here instead of laying out again.
        *self.viewport.borrow_mut() = Some((self.viewport_key(width), ViewportFacts::of(&l)));
        l
    }

    fn layout_options(&self, width: usize) -> LayoutOptions {
        LayoutOptions {
            width,
            step: self.doc.step,
            focus: self.focus,
            pointer: None,
            line_numbers: self.doc.numbers,
            tab_width: 4,
            typing: self.typing.is_some(),
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

    /// `pointer.set` (t-030): resolves the lines on the code shown, moves
    /// the cursor there (the view scrolls to it, `j`/`k` go on from it) and
    /// points. Nothing is recorded: no revision, no undo, no save.
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
                let hits = moves::matches_of(&view.doc.code, first, text);
                let l = match line {
                    Some(l) => {
                        let l = check(l)?;
                        if !hits.contains(&l) {
                            return Err(Error::NoMatch(format!("find «{text}» on line {l}")));
                        }
                        l
                    }
                    None => {
                        let mut lines = hits.clone();
                        lines.dedup();
                        match lines.as_slice() {
                            [] => return Err(Error::NoMatch(format!("find «{text}»"))),
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
                self.mark_saved();
                if self.save_error.take().is_some() {
                    // Writable again: say it once, over the action's own
                    // confirmation, and the bar indicator goes away.
                    let msg = format!("saved to {}", self.save_path.display());
                    self.message = Some((msg, false));
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

    /// The sidecar as a save would write it now (the theme as `save` sets
    /// it): what quitting compares with what was last written.
    fn pending_json(&self) -> Option<String> {
        let mut d = self.doc.clone();
        if !self.theme_auto {
            d.theme = Some(self.theme.name);
        }
        fs::document_json(&d).ok()
    }

    /// Takes note of the document as it stands as the one on disk (opened
    /// or just saved): what quitting compares with.
    fn mark_saved(&mut self) {
        self.saved_at = self.history.newest();
        self.saved_entries = self.history.entries();
        self.saved_json = self.pending_json();
    }

    /// What quitting now leaves unsaved, for stderr once the terminal is
    /// restored (only a Ctrl-C after a failed autosave writes on the way
    /// out): `quit without saving doc.tiza: 2 changes not saved (note a2,
    /// note a1 undone)`. `None` when the sidecar a save would write is the
    /// one last written.
    pub fn unsaved_summary(&self) -> Option<String> {
        if !self.dirty || self.pending_json() == self.saved_json {
            return None;
        }
        // Undone since the save: the edits it had that undo took back
        // (newest first); new: the ones recorded after it.
        let now = self.history.entries();
        let lowest = now.first().map_or(0, |(id, _)| *id);
        let mut labels: Vec<String> = self
            .saved_entries
            .iter()
            .rev()
            .filter(|(id, _)| *id >= lowest && !now.iter().any(|(n, _)| n == id))
            .map(|(_, l)| format!("{l} undone"))
            .collect();
        labels.extend(self.history.labels_after(self.saved_at));
        let path = self.save_path.display();
        Some(match labels.len() {
            0 => {
                format!("quit without saving {path}: the changes since the last save are not saved")
            }
            n => {
                let shown: Vec<&str> = labels.iter().take(5).map(String::as_str).collect();
                let more = if n > 5 {
                    format!(", and {} more", n - 5)
                } else {
                    String::new()
                };
                format!(
                    "quit without saving {path}: {n} change{} not saved ({}{more})",
                    if n == 1 { "" } else { "s" },
                    shown.join(", ")
                )
            }
        })
    }

    /// Quits unless there are unsaved changes: ones that could not be
    /// written, or ones made with autosave off (`force` ignores both), or
    /// stale moves (their marks are not saved).
    pub fn request_quit(&mut self, force: bool) {
        if !force && self.dirty {
            if self.save_error.is_some() {
                self.error(format!(
                    "unsaved (read-only): :q! quits without saving, :w PATH saves elsewhere — {}",
                    self.save_path.display()
                ));
                return;
            }
            if !self.autosave {
                self.error(format!(
                    "unsaved changes (autosave off): :w saves to {}, :w PATH saves elsewhere, :q! quits without saving",
                    self.save_path.display()
                ));
                return;
            }
        }
        // The marks of stale moves live only here: closing loses them.
        if !force && let Some(msg) = tiza_core::stale::on_close(&self.doc) {
            // As the notice of stale moves: the bar gives it the room.
            self.error(msg.clone());
            self.stale_message = Some(msg);
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
            return Err(Error::Invalid(unknown_color()));
        }
        // A mark's inline style color too (t-167 review): checked like the
        // annotation's, without repeating it.
        if let Shape::Mark { style, .. } = &shape
            && let Some(c) = &style.color
            && self.theme.named_color(c).is_none()
        {
            return Err(Error::Invalid(format!("style: {}", unknown_color())));
        }
        self.history.record(&self.doc, "add");
        let step = step.unwrap_or(self.doc.step).max(1);
        let kind = shape.kind_name();
        let res = self.doc.add(Annotation {
            id: id.unwrap_or_default(),
            step,
            color,
            markers,
            file,
            shape,
        });
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
                    "unknown color (use {} or #rrggbb)",
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
    /// directory); the theme follows the document; one undo step. A put
    /// that changes nothing (the `scene.get` copy sent back as it was)
    /// changes nothing: no undo step, no save.
    pub fn put_scene(&mut self, mut doc: Document) -> Result<SceneChanged, Error> {
        // Its numbers first (t-166): a first line near `u32::MAX` or a step
        // past the cap is refused before anything counts with them.
        doc.check_bounds()?;
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
        // A `rebind` entry naming no annotation is a typo of the caller,
        // unless it is the open document's own: the same code, and the move
        // at the same place, otherwise equal, holding that same entry (a
        // `scene.get` copy sent back after that annotation went away).
        let same_code = doc.code.as_slice() == self.doc.base();
        let bare = |m: &Move| Move {
            rebind: Default::default(),
            stale: None,
            held: Vec::new(),
            ..m.clone()
        };
        for (i, m) in doc.moves.iter().enumerate() {
            let open = self.doc.moves.get(i).filter(|o| bare(o) == bare(m));
            for (id, spec) in &m.rebind {
                // An entry the open document already ignores, sent on a move
                // or with code that changed: the caller did not write it.
                let ignored_here = self.doc.find_annotation(id).is_none()
                    && self
                        .doc
                        .moves
                        .iter()
                        .any(|o| o.rebind.get(id) == Some(spec));
                let kept = same_code && open.is_some_and(|o| o.rebind.get(id) == Some(spec));
                if ignored_here && !kept && doc.find_annotation(id).is_none() {
                    return Err(Error::Invalid(format!(
                        "move #{}, step {}: rebind names {id}, which was dropped earlier: the entry is ignored and stays only on the same move with the same code; remove it from the rebind to change this move or the code",
                        i + 1,
                        m.at_step
                    )));
                }
            }
            tiza_core::moves::check_rebind_ids(m, i, &doc.annotations, |id, spec| {
                same_code && open.is_some_and(|o| o.rebind.get(id) == Some(spec))
            })?;
        }
        let base = self.save_path.parent().map(Path::to_path_buf);
        let other = doc.source.path != self.doc.source.path;
        if !self.doc.is_embedded() {
            self.reference = Some(self.doc.code_src.clone());
        }
        let sent = if other {
            SentCode::default()
        } else {
            self.keep_references(&mut doc, base.as_deref())?
        };
        // Linked files (t-105): embedded in `views`, else read next to the
        // open document.
        fs::load_views(&mut doc, base.as_deref())?;
        // The open document's stale moves come back stale (and the
        // annotations they hide, hidden), as `scene.get` gave them out.
        if !other {
            tiza_core::stale::inherit(&self.doc, &mut doc);
        }
        doc.normalize_loaded()?;
        let stale = tiza_core::stale::refresh(None, &mut doc);
        let held = tiza_core::stale::held(&doc);
        for (a, fit) in doc.annotations.iter().zip(doc.annotation_checks()) {
            if held.contains(&a.id) {
                continue;
            }
            fit.map_err(|e| Error::Within {
                what: format!("annotation {}, step {}", a.id, a.step),
                inner: Box::new(e),
            })?;
        }
        if self.same_document(&doc) {
            return Ok(self.scene_changed(Vec::new(), Vec::new(), Vec::new()));
        }
        self.finish_typing();
        // Saving rule (t-039/t-110): a document from another source must
        // never be autosaved over the file that was open. Autosave stops;
        // `:w` writes it over that file on purpose, `:w PATH` elsewhere,
        // `:autosave on` resumes. The same source (an agent re-putting an
        // edited copy of the open document, `scene get` → `scene put`)
        // keeps saving as usual, and the files it reads stay references
        // ([`App::keep_references`]); code sent different from those
        // files is shown but not saved over them.
        self.history.record_with_step(&self.doc, "scene.put");
        self.doc = doc;
        // Another document follows nothing the open one did; a put of the
        // open one keeps the file its embedded code replaced (`:follow on`
        // still reloads it).
        if other {
            self.follow_file = None;
        }
        self.screen_via_history = false;
        self.step_ready_at = Some(std::time::Instant::now());
        match self.doc.theme {
            Some(t) => self.set_theme_name(t, true),
            None => self.rebuild_theme(),
        }
        self.selected = None;
        self.focus = None;
        self.scroll = 0;
        self.clamp_cursor();
        // Said first: what is not saved, below, says it over this.
        self.say_stale(stale);
        // Another document is not the followed file's code: follow goes off
        // and says so, as after a paste (its file is not this one's).
        let unfollowed = if other && let Some(f) = &self.follow {
            let name = f.file_name();
            let _ = self.set_follow(false);
            Some(format!("follow off: the code on screen is not {name}"))
        } else {
            None
        };
        if other && self.autosave {
            self.autosave = false;
            self.info(format!(
                "another document loaded: autosave off (:w saves over {}, :w PATH saves elsewhere, :autosave on resumes)",
                self.save_path.display()
            ));
            if let Some(note) = unfollowed {
                self.say_first(note);
            }
        } else if other && let Some(note) = &unfollowed {
            self.info(format!("another document loaded; {note}"));
        } else if !sent.edited.is_empty() || !sent.stale.is_empty() {
            // What is not saved first: the bar may cut the end.
            let mut msg = String::new();
            if !sent.edited.is_empty() {
                msg = if self.autosave {
                    self.autosave = false;
                    format!(
                        "not saved — autosave off: {} (:w saves it embedded in {}, :w PATH saves elsewhere, :autosave on resumes)",
                        sent.edited.join("; "),
                        self.save_path.display()
                    )
                } else {
                    format!("not saved: {} (shown embedded)", sent.edited.join("; "))
                };
            }
            if !sent.stale.is_empty() {
                let stale = format!(
                    "{} changed on disk since it was loaded: still read from it; the screen keeps the copy loaded (:follow on reloads it)",
                    sent.stale.join(", ")
                );
                msg = if msg.is_empty() {
                    stale
                } else {
                    format!("{msg} · also {stale}")
                };
            }
            self.info(msg);
        } else if !sent.back.is_empty() && !self.autosave {
            self.info(format!(
                "{} {} a reference again; autosave still off (:autosave on resumes)",
                sent.back.join(", "),
                if sent.back.len() == 1 { "is" } else { "are" }
            ));
        }
        self.touch();
        Ok(self.scene_changed(Vec::new(), Vec::new(), Vec::new()))
    }

    /// Saving rule of a put of the open document: a file the open document
    /// reads from disk (its `code.file`, a linked file it does not keep in
    /// `views`) whose code comes back equal to that file's text **now**
    /// stays a reference — `"code": {"file": …}` (with its `lines`) and the
    /// link, read from disk again; nothing is embedded. Just a comparison of
    /// the code sent with the file, no fingerprint and no state kept
    /// between requests. Code that differs:
    ///
    /// - equal to the copy this instance holds: the file changed on disk
    ///   after it was loaded (the agent sent back what `scene.get` gave
    ///   out). Still a reference, with the copy loaded on screen, as for an
    ///   edit made in the TUI meanwhile; listed in `stale`. With follow on
    ///   the screen follows the file, so a put cannot change its code: an
    ///   error, and so is a file that can no longer be read. When the open
    ///   document holds that copy embedded (a paste, or an undo past a
    ///   reload, over the file), it stays embedded: that code was never
    ///   the file's, and a reference would draw the file's text under it.
    /// - anything else: the agent changed it. Kept as sent (embedded in
    ///   memory) and listed in `edited`: the put is shown, autosave stops.
    ///
    /// A `views` entry no `code.link` names is dropped. An open document
    /// with embedded code (stdin, `:embed`) stays embedded.
    fn keep_references(&self, doc: &mut Document, base: Option<&Path>) -> Result<SentCode, Error> {
        use tiza_core::model::CodeSource;
        let mut out = SentCode::default();
        let unreadable = |file: &str, e: Error| {
            Error::Invalid(format!(
                "scene.put: {file} cannot be read ({e}) and the document reads its code from it: put the file back, or :embed to keep the copy loaded"
            ))
        };
        if let Some(reference @ CodeSource::File { file, .. }) = &self.reference
            && let CodeSource::Embedded(lines) = &doc.code_src
        {
            let mut probe = self.doc.clone();
            probe.code_src = reference.clone();
            let (disk, first) =
                fs::reread_reference(&probe, base).map_err(|e| unreadable(file, e))?;
            let same = disk == *lines && first == doc.source.first_line;
            if !same && self.follow.is_some() {
                return Err(Error::Invalid(format!(
                    "scene.put: the code sent is not what {file} holds now, and follow is on (the screen shows the file as it is): scene.get again; to change the code, edit {file}"
                )));
            }
            let loaded = self.doc.base() == lines.as_slice()
                && self.doc.base_first() == doc.source.first_line;
            // The copy loaded stays a reference only while the open document
            // is one: embedded code on screen (a paste, an undo past a
            // reload) is not what the file held, and saved as `code: {file}`
            // it would put the file's text under annotations made for it.
            // Sent back as it is shown, it stays embedded as it was.
            if same || (loaded && !self.doc.is_embedded()) {
                doc.code_src = reference.clone();
                if !same {
                    out.stale.push(file.clone());
                }
                if self.doc.is_embedded() {
                    out.back.push(file.clone());
                }
            } else if !loaded {
                out.edited.push(format!(
                    "the code sent for {file} differs from the file on disk"
                ));
            }
        }
        let linked = doc.link_files();
        for (file, code) in doc.views.clone() {
            let Some(key) = doc.view_key(&file) else {
                continue;
            };
            if !linked.contains(&key) {
                doc.views.remove(&file);
                continue;
            }
            // Kept in the document itself (as opened, or by `:embed`).
            let frozen =
                |k: &String| self.frozen_views.contains(k) && self.doc.views.contains_key(k);
            if frozen(&file) || frozen(&key) {
                continue;
            }
            let code = if code.is_empty() {
                vec![String::new()]
            } else {
                code
            };
            let known = self.doc.view_code.get(&key);
            let disk = match fs::read_view_file(&file, base) {
                Ok(d) => Some(d),
                Err(e) if known.is_some() => return Err(unreadable(&file, e)),
                Err(_) => None,
            };
            let same = disk.as_ref() == Some(&code);
            if same || known == Some(&code) {
                doc.views.remove(&file);
                doc.view_code.insert(key.clone(), code);
                if self.doc.views.contains_key(&file) || self.doc.views.contains_key(&key) {
                    out.back.push(file.clone());
                }
                if !same {
                    out.stale.push(file);
                }
            } else if disk.is_none() {
                out.edited.push(format!("{file} is not a file on disk"));
            } else {
                out.edited.push(format!(
                    "the code sent for {file} differs from the file on disk"
                ));
            }
        }
        Ok(out)
    }

    /// Whether `doc` is the open document as it stands: what a save would
    /// write, its code and that of its linked files.
    fn same_document(&self, doc: &Document) -> bool {
        self.doc.base() == doc.base()
            && self.doc.base_first() == doc.base_first()
            && self.doc.view_code == doc.view_code
            && matches!(
                (fs::document_json(&self.doc), fs::document_json(doc)),
                (Ok(a), Ok(b)) if a == b
            )
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
                let ann = <Annotation as serde::Deserialize>::deserialize(&v).map_err(|e| {
                    Error::Invalid(format!(
                        "annotation {id}: {}",
                        tiza_core::error::serde_text(&e.to_string())
                    ))
                })?;
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
    /// unless the step said something loud (a move that does not apply):
    /// then that stays on the bar, as with the `n` key.
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
    /// (moves, views), the typing of that step complete. Returns
    /// the step.
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
    /// apply.
    fn step_to(&mut self, step: u32) -> bool {
        self.step_to_with(step, false)
    }

    /// [`App::step_to`]; `jump` (goto, the loop back to step 0) lands with
    /// the step's typing complete.
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
            }
            self.clamp_cursor();
            // The step moved the cursor (to the pointer, a `cursor` move):
            // not a motion of the presenter's for the pointer to follow.
            self.pointer_seen = self.pointer_snapshot();
            self.revision += 1;
            // The take's geometry comes from every step's code, the one on
            // screen as shown: synced to the new step it is the same, so it
            // is kept. A step that starts typing shows other code there.
            if !loud && self.typing.is_none() {
                self.keep_geometry(self.revision - 1);
            }
            // The auto-advance countdown starts now, or when the typing ends.
            self.step_ready_at = self.typing.is_none().then(std::time::Instant::now);
            // Its notice stays on the bar over the step's label.
            if self.reload_waiting() {
                loud = true;
            }
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

    /// [`App::add_move`], with what it says when the new move leaves
    /// annotations unseen at some step (kept, not drawn there): outside a
    /// `code.link` window (`a3, a4: outside the window at step 3
    /// (src/queue.rs:17-23)`) or of a file their own step no longer shows
    /// (`a6: its file src/stack.rs is not shown at step 4 (src/queue.rs
    /// is)`). Only what this move hides is named.
    pub fn add_move_noting(&mut self, mut m: Move) -> Result<(usize, Option<String>), Error> {
        self.finish_typing();
        let from = m.at_step;
        let before = hides(&m).then(|| tiza_core::check::Unseen::of(&self.doc, from));
        // Recorded first: a refused move leaves nothing behind, not even
        // the file a `code.link` read.
        self.history.record(&self.doc, "move added");
        if let tiza_core::moves::MoveAction::CodeLink(l) = &mut m.action {
            let base = self.save_path.parent().map(Path::to_path_buf);
            // An alias of a file the document shows is that file.
            let mut known = self.doc.link_files();
            known.extend(self.doc.view_code.keys().cloned());
            l.file = fs::canonical_link(&self.doc, &l.file, base.as_deref(), &known);
            if let Err(e) = fs::load_view(&mut self.doc, &l.file, base.as_deref()) {
                self.history.discard_last();
                return Err(e);
            }
        }
        match self.doc.add_move(m) {
            Ok(i) => {
                self.history.label_last(format!("move #{} added", i + 1));
                // The new move may make a stale one match again, or not.
                let n = tiza_core::stale::refresh(None, &mut self.doc);
                self.say_stale(n);
                self.clamp_cursor();
                self.touch();
                let warning = before.and_then(|b| b.warning(&self.doc));
                Ok((i, warning))
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

    /// [`App::delete_move`], with what it says when the deletion leaves
    /// annotations unseen at some step, as [`App::add_move_noting`] does:
    /// deleting a `code.link` can leave a step showing another file.
    pub fn delete_move_noting(&mut self, index: usize) -> Result<(Move, Option<String>), Error> {
        self.finish_typing();
        let i = index
            .checked_sub(1)
            .filter(|i| *i < self.doc.moves.len())
            .ok_or_else(|| Error::MoveNotFound(format!("#{index}")))?;
        let before = hides(&self.doc.moves[i])
            .then(|| tiza_core::check::Unseen::of(&self.doc, self.doc.moves[i].at_step));
        self.history
            .record(&self.doc, format!("move #{index} deleted"));
        let m = match self.doc.delete_move(i) {
            Ok(m) => m,
            Err(e) => {
                self.history.discard_last();
                return Err(e);
            }
        };
        if self.doc.step > self.doc.max_step() {
            self.doc.step = self.doc.max_step();
            self.doc.sync_code_to_step();
        }
        let n = tiza_core::stale::refresh(None, &mut self.doc);
        self.say_stale(n);
        self.clamp_cursor();
        self.touch();
        let warning = before.and_then(|b| b.warning(&self.doc));
        Ok((m, warning))
    }

    /// `scene.list` / `:list`: one line per annotation, in document order.
    /// With moves, the coordinates are where the annotation is drawn at the
    /// current step (shifted through the patches); one that lost its anchor
    /// keeps its authoring coordinates and is marked `(unanchored)`.
    pub fn list_lines(&self) -> Vec<String> {
        let describe = |a: &Annotation| tiza_core::protocol::describe(a);
        let doc = &self.doc;
        if doc.moves.is_empty() {
            return doc.annotations.iter().map(describe).collect();
        }
        let view = doc.view_at(doc.step);
        let mut lines: Vec<String> = doc
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
            .collect();
        // A move the code changed under is listed, with why it is skipped.
        lines.extend(
            doc.moves
                .iter()
                .enumerate()
                .filter(|(_, m)| m.stale.is_some())
                .map(|(i, m)| format!("move {}", tiza_core::protocol::describe_move(i + 1, m))),
        );
        lines
    }

    /// `move.list` payload.
    pub fn move_list(&self) -> MoveList {
        MoveList {
            moves: self
                .doc
                .moves
                .iter()
                .enumerate()
                .map(|(i, m)| MoveListed {
                    index: i + 1,
                    summary: tiza_core::protocol::describe_move(i + 1, m),
                    stale: m.stale.is_some(),
                    stale_reason: m.stale.as_ref().map(|s| s.reason.clone()),
                    r#move: m.clone(),
                })
                .collect(),
            unanchored: self.doc.unanchored(),
            step: self.doc.step,
            max_step: self.doc.max_step(),
            view: self.shown_view(),
        }
    }

    /// The file shown at the current step: the one a `code.link` left up
    /// (as the move names it), else the source file.
    pub fn shown_file(&self) -> Option<String> {
        let v = self.doc.shown_view_at(self.doc.step);
        v.file.or_else(|| self.doc.source.path.clone())
    }

    /// The `code.link` view shown at the current step (`lib.rs:2-9`):
    /// `status.view`, `code.get`, the bar.
    pub fn shown_view(&self) -> Option<String> {
        self.doc.shown_view_at(self.doc.step).label()
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
        let before = self.before_history();
        match self.history.undo(&mut self.doc) {
            Some(what) => self.after_history(before, format!("undone: {what}")),
            None => self.info("nothing to undo"),
        }
    }

    /// Redoes the last undone edit.
    pub fn redo(&mut self) {
        self.finish_typing();
        let before = self.before_history();
        match self.history.redo(&mut self.doc) {
            Some(what) => self.after_history(before, format!("redone: {what}")),
            None => self.info("nothing to redo"),
        }
    }

    /// What follow needs to know of the screen before an undo or a redo.
    fn before_history(&self) -> BeforeHistory {
        BeforeHistory {
            // The file followed now: the reference on screen, or the one
            // embedded code on screen replaced (never the embedded code).
            followed: if self.doc.is_embedded() {
                self.followed_source()
            } else {
                Some(self.doc.code_src.clone())
            },
            was: self.paused_file(),
            code: self.code_now(),
            reason: self.off_reason(),
        }
    }

    /// The end of an undo or a redo (`said` names it): the code it put on
    /// screen is kept embedded when the file does not hold it, and follow
    /// goes off, or says how to resume, from what the screen shows now.
    fn after_history(&mut self, before: BeforeHistory, said: String) {
        self.screen_via_history = true;
        // Before the save `touch` makes: it must write the code as it is on
        // screen now.
        let replaced = self.embed_if_not_the_file(&before.code);
        if let Some(reference) = &replaced {
            self.follow_file = Some(reference.clone());
        }
        self.rebuild_theme();
        self.touch();
        self.info(said);
        self.follow_after_history(before, replaced);
        if !self.doc.is_embedded()
            && let Some(Err(e)) = self.follow_the_reference()
        {
            self.error(format!("follow off: {e}"));
        }
    }

    /// The step-0 code, its first line and whether it is embedded: what
    /// [`App::embed_if_not_the_file`] compares after an undo or a redo.
    fn code_now(&self) -> (Vec<String>, u32, bool) {
        (
            self.doc.base().to_vec(),
            self.doc.base_first(),
            self.doc.is_embedded(),
        )
    }

    /// After an undo or a redo that put a file reference back with other
    /// code than the screen had (or over embedded code): when that file does
    /// not hold the code now on screen (the undo went back past a follow
    /// reload, or the file changed on disk since), the code is kept
    /// embedded, as after a paste. Saved as `code: {file}`, a fresh load
    /// would show the file's text under annotations and `rebind` entries
    /// made for the code on screen: drawn on other text, or a move that no
    /// longer finds its text. A file that cannot be read counts as not
    /// holding it. An undo that leaves the same reference and the same code
    /// changes nothing here. Returns the reference it replaced.
    fn embed_if_not_the_file(
        &mut self,
        before: &(Vec<String>, u32, bool),
    ) -> Option<tiza_core::model::CodeSource> {
        let (code, first, embedded) = before;
        if self.doc.is_embedded()
            || (!embedded && self.doc.base() == code.as_slice() && self.doc.base_first() == *first)
        {
            return None;
        }
        let base = self.save_path.parent().map(Path::to_path_buf);
        let holds = fs::reread_reference(&self.doc, base.as_deref())
            .is_ok_and(|(code, first)| code == self.doc.base() && first == self.doc.base_first());
        if holds {
            return None;
        }
        let reference = self.doc.code_src.clone();
        self.doc.code_src = tiza_core::model::CodeSource::Embedded(self.doc.base().to_vec());
        Some(reference)
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
        // Code pasted while following is the author's on purpose: a save in
        // the editor must not throw it away, so follow goes off and says so.
        // A paste of exactly what the file holds now has nothing to protect:
        // it stays a reference to the file, numbered as its lines are, and
        // follow stays on (the file is read: what follow loaded last may be
        // another range's, or older).
        let same = if self.doc.is_embedded() {
            None
        } else {
            let base = self.save_path.parent().map(Path::to_path_buf);
            fs::reread_reference(&self.doc, base.as_deref())
                .ok()
                .filter(|(code, _)| *code == d.code)
                .map(|(_, first)| first)
        };
        let followed = self.doc.code_src.clone();
        let (code_src, first) = match same {
            Some(first) => (self.doc.code_src.clone(), first),
            None => (d.code_src, d.source.first_line),
        };
        let out = self.apply_new_code(d.code, code_src, first, Some("paste".into()));
        self.screen_via_history = false;
        if self.doc.is_embedded() {
            let pasted = self.doc.base().to_vec();
            self.pasted_codes.retain(|c| *c != pasted);
            self.pasted_codes.push(pasted);
            if self.pasted_codes.len() > PASTES_KEPT {
                self.pasted_codes.remove(0);
            }
        }
        // With follow off, a paste over a file reference still says which
        // file `:follow on` reloads, once one is known (follow was on).
        if self.follow.is_none()
            && self.follow_file.is_some()
            && self.doc.is_embedded()
            && matches!(followed, tiza_core::model::CodeSource::File { .. })
        {
            self.follow_file = Some(followed.clone());
        }
        let paused = self.pause_follow(Some(followed), NotTheFile::Paste("the code is pasted"));
        if let Some(det) = detected {
            self.doc.source.language = det.language;
        }
        self.cursor_line = self.doc.source.first_line;
        self.scroll = 0;
        let lines = self.doc.code.len();
        match out.stale {
            None if out.ignored.is_none() && out.dropped > 0 => self.info(format!(
                "{lines} lines pasted; {} annotation{} dropped (their text is gone; u undoes)",
                out.dropped,
                if out.dropped == 1 { "" } else { "s" }
            )),
            None if out.ignored.is_none() => self.info(format!(
                "{lines} lines pasted ({} annotations kept)",
                out.kept
            )),
            n => self.stale_notice(
                n,
                out.dropped,
                &out.ignored,
                "their text is gone",
                format!("{lines} lines pasted ({} annotations kept)", out.kept),
            ),
        }
        if let Some(note) = paused {
            self.say_first(note);
        }
        Ok(())
    }

    /// Follow shows the file, so it goes off when the code on screen is not
    /// the file's: pasted over it, or code an undo brought back. `followed`
    /// is the file reference that code replaced (what `:follow on`
    /// reloads). The notice says what the screen shows
    /// ([`App::off_reason`]), or `fallback` when the file holds that code
    /// (embedded code equal to it). Returns the notice; `None` when follow
    /// was off or the code is still a reference.
    fn pause_follow(
        &mut self,
        followed: Option<tiza_core::model::CodeSource>,
        fallback: NotTheFile,
    ) -> Option<String> {
        if !self.doc.is_embedded() {
            return None;
        }
        let mut f = self.follow.take()?;
        f.stop();
        if let Some(src @ tiza_core::model::CodeSource::File { .. }) = followed {
            self.follow_file = Some(src);
        }
        let why = self
            .off_reason()
            .unwrap_or_else(|| fallback.why(&f.file_name()));
        Some(format!("follow off: {why}"))
    }

    /// Why follow is off, from what the screen shows now (`follow.status`
    /// `reason`, `:follow`): embedded code over a followed file that is a
    /// paste (`the code is pasted, not main.rs; :follow on reloads
    /// main.rs`, or `the code is the paste again` after an undo or a redo)
    /// or another version of the file (`the code on screen is not main.rs;
    /// …`). `None` when the code is a reference, when no followed file is
    /// known, or when the file holds the code on screen again (edited back
    /// on disk): there is nothing to say, and `:follow on` still follows it.
    fn off_reason(&self) -> Option<String> {
        let src = self.paused()?;
        let name = self.reference_name(src)?;
        let mut probe = self.doc.clone();
        probe.code_src = src.clone();
        let base = self.save_path.parent().map(Path::to_path_buf);
        if fs::reread_reference(&probe, base.as_deref())
            .is_ok_and(|(code, first)| code == self.doc.base() && first == self.doc.base_first())
        {
            return None;
        }
        let what = if self.pasted_codes.iter().any(|c| c == self.doc.base()) {
            NotTheFile::Paste(if self.screen_via_history {
                "the code is the paste again"
            } else {
                "the code is pasted"
            })
        } else {
            NotTheFile::Version
        };
        Some(what.why(&name))
    }

    /// The file name of a file reference, as follow names it.
    fn reference_name(&self, src: &tiza_core::model::CodeSource) -> Option<String> {
        let path = fs::code_source_file(src, self.save_path.parent())?;
        Some(path.file_name()?.to_string_lossy().into_owned())
    }

    /// The name of the file `:follow on` would reload while follow is off
    /// over code that is not the file's ([`App::paused`]).
    fn paused_file(&self) -> Option<String> {
        self.reference_name(self.paused()?)
    }

    /// After an undo or a redo: follow goes off when the code on screen is
    /// no longer the followed file's; and when it brought the file back
    /// while follow was off over embedded code (`before.was` names that
    /// file), the message says how to follow it again. `replaced` is the
    /// file reference [`App::embed_if_not_the_file`] embedded: the code on
    /// screen is a version of that file it no longer holds, so follow is off
    /// whether it was on or not. With follow already off over embedded code
    /// the message says why, unless the reason is the one the screen
    /// already had.
    fn follow_after_history(
        &mut self,
        before: BeforeHistory,
        replaced: Option<tiza_core::model::CodeSource>,
    ) {
        // Follow on over a file reference again (an undo of the paste a wait
        // was for): the file holds the code on screen
        // ([`App::embed_if_not_the_file`] checked it), so nothing waits.
        if !self.doc.is_embedded()
            && let Some(f) = &mut self.follow
        {
            self.reload_waits = false;
            f.last_content = self.doc.base().to_vec();
        }
        // A change on disk waits for step 0 (t-206) and the undo left the
        // step-0 code as it was (a note, a move): follow stays on, the code
        // embedded, and step 0 still reloads.
        if self.reload_waits
            && self.follow.is_some()
            && self.doc.is_embedded()
            && self.doc.base() == before.code.0.as_slice()
            && self.doc.base_first() == before.code.1
        {
            return;
        }
        // Embedded code the file holds (a reference undone back to the
        // embedded code it was made from).
        let fallback = NotTheFile::Paste("the code is embedded again");
        let followed = replaced.clone().or(before.followed);
        if let Some(note) = self.pause_follow(followed, fallback) {
            self.say_first(note);
            return;
        }
        // Follow off over embedded code that is not the file's: say why
        // whenever the reason is not the one the screen had before (the
        // file a moment ago, a paste, another version), once.
        if self.follow.is_none()
            && let Some(why) = self.off_reason()
        {
            if Some(&why) != before.reason.as_ref() {
                let (said, _) = self.message.take().unwrap_or_default();
                self.info(format!("{said}; follow is off: {why}"));
            }
            return;
        }
        if let Some(name) = before.was
            && self.follow.is_none()
            && self.paused().is_none()
        {
            // The file on screen now (a redo past `:ref` names another).
            let name = self.reference_name(&self.doc.code_src).unwrap_or(name);
            let (said, _) = self.message.take().unwrap_or_default();
            self.info(format!("{said}; follow is off: :follow on reloads {name}"));
        }
    }

    /// Puts `note` before the message on the bar, with the room it needs
    /// (the bar no longer shows `⟳ following`, and it says why).
    fn say_first(&mut self, note: String) {
        let (said, loud) = self.message.take().unwrap_or_default();
        let text = format!("{note} · {said}");
        if loud {
            self.error(text.clone());
        } else {
            self.info(text.clone());
        }
        self.stale_message = Some(text);
    }

    /// The status line after a paste or a reload that changed a stale mark
    /// (`n`) or left a `rebind` entry ignored, in the order the bar can cut
    /// it: annotations dropped (and the entries that leaves `ignored`), the
    /// stale moves, then what happened (`what`). Loud when a move stopped
    /// matching. The bar gives it the room it needs (`stale_message`).
    fn stale_notice(
        &mut self,
        n: Option<tiza_core::stale::Notice>,
        dropped: usize,
        ignored: &Option<String>,
        why: &str,
        what: String,
    ) {
        let mut parts = Vec::new();
        if dropped > 0 {
            parts.push(format!(
                "{dropped} annotation{} dropped ({why}; u undoes){}",
                if dropped == 1 { "" } else { "s" },
                ignored
                    .as_ref()
                    .map(|t| format!("; {t}"))
                    .unwrap_or_default()
            ));
        }
        let loud = n.as_ref().is_some_and(|n| n.loud);
        if let Some(n) = n {
            parts.push(n.text);
        }
        parts.push(what);
        let text = parts.join(" · ");
        if loud {
            self.error(text.clone());
            self.stale_message = Some(text);
        } else if ignored.is_some() {
            self.info(text.clone());
            self.stale_message = Some(text);
        } else {
            self.info(text);
        }
    }

    /// The notice of a stale mark a document change made or cleared
    /// (`scene.put`, a move added or deleted), when there is one.
    fn say_stale(&mut self, n: Option<tiza_core::stale::Notice>) {
        match n {
            Some(n) if n.loud => {
                self.error(n.text.clone());
                self.stale_message = Some(n.text);
            }
            Some(n) => self.info(n.text),
            None => {}
        }
    }

    /// Whether the message on the bar is a notice about stale moves.
    pub fn stale_message_shown(&self) -> bool {
        self.message
            .as_ref()
            .is_some_and(|(m, _)| Some(m) == self.stale_message.as_ref())
    }

    /// Replaces the code lines (and where they come from), re-anchoring the
    /// annotations by text. One undoable step labelled `always` — or, with
    /// `None` (a follow reload), only when the reload touched an annotation
    /// (something dropped or shifted) or a move's stale mark: a harmless
    /// reload leaves no step, so that after an editor undo that restores
    /// dropped text, one `u` reaches the reload that dropped it. Returns
    /// (kept, dropped, recorded) and the notice about stale moves.
    fn apply_new_code(
        &mut self,
        code: Vec<String>,
        code_src: tiza_core::model::CodeSource,
        first_line: u32,
        always: Option<String>,
    ) -> crate::reanchor::Outcome {
        let snapshot = self.doc.clone();
        // A dropped annotation that a `rebind` names leaves that entry
        // ignored, as a fresh load of the saved file will ignore it. The
        // moves were checked with the entry still counting (its `find` may
        // be the text that is gone): rebuild without those annotations, so
        // the moves are checked as that load checks them. That rebuild can
        // drop another one a `rebind` names; it ends when none is left, since
        // each round only adds to `gone`.
        let mut gone: Vec<String> = Vec::new();
        let (d, stale) = loop {
            let (d, stale) = self.rebuilt(&code, &code_src, first_line, &gone);
            let more: Vec<String> = tiza_core::moves::ignored_rebinds(&d)
                .into_iter()
                .map(|r| r.id)
                .filter(|id| snapshot.find_annotation(id).is_some() && !gone.contains(id))
                .collect();
            if more.is_empty() {
                break (d, stale);
            }
            gone.extend(more);
        };
        let before = snapshot.annotations.len();
        let kept = d.annotations.len();
        let harmless =
            kept == before && d.annotations == snapshot.annotations && d.moves == snapshot.moves;
        let recorded = always.is_some() || !harmless;
        if recorded {
            self.history
                .record(&snapshot, always.unwrap_or_else(|| "reload".into()));
        }
        let ignored = tiza_core::moves::newly_ignored(&snapshot, &d);
        self.doc = d;
        self.selected = None;
        self.clamp_cursor();
        self.touch();
        crate::reanchor::Outcome {
            kept,
            dropped: before.saturating_sub(kept),
            recorded,
            stale,
            ignored,
        }
    }

    /// The open document with `code` as its step-0 code and its annotations
    /// re-anchored on it ([`App::apply_new_code`]), with the notice about
    /// stale moves. The annotations in `gone` are dropped: the moves are
    /// checked without their `rebind` entries, and they are not re-added
    /// even where their text would be found.
    fn rebuilt(
        &self,
        code: &[String],
        code_src: &tiza_core::model::CodeSource,
        first_line: u32,
        gone: &[String],
    ) -> (Document, Option<tiza_core::stale::Notice>) {
        let code = code.to_vec();
        let code_src = code_src.clone();
        let mut d = self.doc.clone();
        // `revalidate` reads from these which `rebind` entries count; they
        // are re-added below, re-anchored.
        d.annotations.retain(|a| !gone.contains(&a.id));
        if !d.moves.is_empty() {
            // Only reachable at step 0 (see `moves_block_reason`): the new
            // text is the step-0 code.
            d.base_code = code.clone();
            d.base_first_line = first_line;
        }
        d.code = code;
        d.code_src = code_src;
        d.source.first_line = first_line;
        let old = self.doc.clone();
        // The demo did not write this change: a move that no longer matches
        // the new code is skipped, never moved (ADR 0007 §7).
        let stale = tiza_core::stale::revalidate(Some(&old), &mut d);
        d.annotations.clear();
        // An annotation on a line a now stale patch wrote is hidden and
        // kept as it is, never re-anchored onto other text.
        let mut held = tiza_core::stale::held(&old);
        held.extend(tiza_core::stale::hold_new(&old, &mut d));
        // Each annotation is authored against the code at its own step
        // (ADR 0007 §3d), so it is re-anchored between the old and the new
        // code *at that step*: the moves are re-applied on the new text and
        // a step-3 arrow onto a line a move typed in is matched on the
        // step-3 code, not on the step-0 code where that line does not
        // exist (AC9). A step whose moves no longer apply on the new text
        // falls back to the step-0 code (those moves are reported when
        // the presenter steps onto them).
        let mut steps: Vec<u32> = old.annotations.iter().map(|a| a.step).collect();
        steps.sort_unstable();
        steps.dedup();
        // Only the main file changed: its annotations are re-anchored on
        // its own code at their step; those of a linked file (t-105) keep
        // their coordinates.
        let at_step = |doc: &Document, step: u32| -> Document {
            let mut s = doc.clone();
            if !doc.moves.is_empty()
                && let Ok((code, first)) = doc.view_code_at(&None, step)
            {
                s.code = code;
                s.source.first_line = first;
            }
            s.moves.clear();
            s.base_code.clear();
            s.step = step;
            s
        };
        // One a stale move hides keeps its place: the text it was on is
        // the move's, not in the new code.
        let mut shapes: Vec<Option<Shape>> = old
            .annotations
            .iter()
            .map(|a| (a.file.is_some() || held.contains(&a.id)).then(|| a.shape.clone()))
            .collect();
        for step in steps {
            let old_s = at_step(&old, step);
            let new_s = at_step(&d, step);
            let mut remap = crate::reanchor::Remap::new(&old_s, &new_s);
            for (a, slot) in old.annotations.iter().zip(shapes.iter_mut()) {
                if a.step == step && a.file.is_none() && !held.contains(&a.id) {
                    *slot = remap.shape(&a.shape);
                }
            }
        }
        for (a, shape) in old.annotations.iter().zip(shapes) {
            if gone.contains(&a.id) {
                continue;
            }
            match shape {
                // `add` validates against the code at the annotation's step.
                Some(shape) if !held.contains(&a.id) => {
                    let _ = d.add(Annotation { shape, ..a.clone() });
                }
                Some(shape) => d.annotations.push(Annotation { shape, ..a.clone() }),
                None => {}
            }
        }
        // Held annotations of moves that match again go where they write.
        tiza_core::stale::release(&mut d);
        (d, stale)
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
        let src = self.followed_source()?;
        fs::code_source_file(&src, self.save_path.parent())
    }

    /// Where follow reads the code: the document's file reference, or,
    /// while code pasted over a followed file is on screen, that file.
    /// Why follow is off and what it would reload, while the code a paste
    /// (or an undo) put over the followed file is on screen; `None` once the
    /// screen shows a file reference again.
    fn paused(&self) -> Option<&tiza_core::model::CodeSource> {
        self.follow_file.as_ref().filter(|_| self.doc.is_embedded())
    }

    fn followed_source(&self) -> Option<tiza_core::model::CodeSource> {
        if self.doc.is_embedded() {
            self.paused().cloned()
        } else {
            Some(self.doc.code_src.clone())
        }
    }

    /// Following while moves are applied: why a save does not load now (it
    /// waits for a step with no moves applied), as the reply, the bar and
    /// `follow.status` say it. `None` when a save loads at once.
    pub fn follow_waits(&self) -> Option<String> {
        if self.follow.is_none() || !self.doc.moves_applied() {
            return None;
        }
        let step = self.doc.step;
        // The last step before the first move: a save loads there or earlier.
        let back = self
            .doc
            .moves
            .iter()
            .map(|m| m.at_step)
            .min()
            .unwrap_or(1)
            .saturating_sub(1);
        let when = if back == 0 {
            "at step 0".to_string()
        } else {
            format!("at step {back} or earlier")
        };
        Some(format!(
            "waiting: moves applied at step {step}; loads {when}"
        ))
    }

    /// Enables or disables follow mode. Enabling needs a file reference
    /// (`:link` first when the code is embedded). At a step with moves
    /// applied it starts waiting: a save, or a change made while follow was
    /// off, loads when a step with no moves is reached.
    pub fn set_follow(&mut self, enabled: bool) -> Result<String, Error> {
        if !enabled {
            self.reload_waits = false;
            if let Some(f) = &mut self.follow {
                f.stop();
            }
            self.follow = None;
            // The file embedded code on screen replaced stays known, so the
            // `:follow on reloads main.rs` the bar said still holds.
            return Ok("follow off".into());
        }
        if let Some(f) = &self.follow {
            let name = f.file_name();
            return Ok(match self.follow_waits() {
                Some(w) => format!("already following {name} ({w})"),
                None => format!("already following {name}"),
            });
        }
        let path = self.source_file().ok_or_else(|| {
            Error::Invalid(
                "nothing to follow: the code is embedded (use :link path to reference a file)"
                    .into(),
            )
        })?;
        // The step-0 code: with moves applied, `doc.code` is what they made.
        let f =
            crate::follow::Follow::start(path, self.doc.base().to_vec()).map_err(Error::Invalid)?;
        let name = f.file_name();
        self.follow = Some(f);
        self.reload_waits = false;
        // Pick up anything that changed before we started watching. Code
        // pasted over the file is replaced on purpose: always one undo step,
        // so `u` gives the paste back.
        let pasted = self.doc.is_embedded();
        let said = self.message.clone();
        let _ = self.reload_from_disk(pasted);
        if self.reload_waits {
            // With moves applied what differs waits, and the reply says so
            // (`waiting: …`): over pasted code the file did not change on
            // disk, as the deferral's own notice would say.
            self.message = said;
        }
        // Pasted code that is what the file holds, numbered as the file's
        // lines are: nothing was reloaded, and the document references the
        // file again.
        if pasted
            && self.doc.is_embedded()
            && let Some(src) = self.followed_source()
        {
            let mut probe = self.doc.clone();
            probe.code_src = src.clone();
            let base = self.save_path.parent().map(Path::to_path_buf);
            if fs::reread_reference(&probe, base.as_deref()).is_ok_and(|(code, first)| {
                code == self.doc.base() && first == self.doc.base_first()
            }) {
                self.doc.code_src = src;
                self.touch();
            }
        }
        // `follow_file` stays: while the code on screen is still embedded
        // (the file is missing, or unreadable for now) it names the file the
        // next reload reads, so the file coming back reloads it.
        Ok(match self.follow_waits() {
            Some(w) => format!("following {name} ({w})"),
            None => format!("following {name}"),
        })
    }

    /// Follow on while the document references a file (after `:ref`, or
    /// an undo or a redo past one): the watch moves to that file, its
    /// counts kept, and what it holds now is read, so a save made meanwhile
    /// is not missed. The same file (another range of it, or another path
    /// to it) keeps the watch, with the code on screen as what was loaded.
    /// `Some(Ok(name))` when the watch moved, `Some(Err(why))` when it could
    /// not and follow went off.
    fn follow_the_reference(&mut self) -> Option<Result<String, String>> {
        let path = self.source_file()?;
        let base = self.doc.base().to_vec();
        let old = self.follow.as_mut()?;
        let same = |a: &Path, b: &Path| {
            a == b
                || matches!(
                    (std::fs::canonicalize(a), std::fs::canonicalize(b)),
                    (Ok(x), Ok(y)) if x == y
                )
        };
        if same(&old.path, &path) {
            old.last_content = base;
            return None;
        }
        old.stop();
        let (reloads, last, clock) = (old.reloads, old.last_reload, old.last_reload_clock.clone());
        self.reload_waits = false;
        match crate::follow::Follow::start(path, base) {
            Ok(mut f) => {
                (f.reloads, f.last_reload, f.last_reload_clock) = (reloads, last, clock);
                let name = f.file_name();
                self.follow = Some(f);
                let _ = self.reload_from_disk(false);
                Some(Ok(name))
            }
            Err(e) => {
                self.follow = None;
                Some(Err(e))
            }
        }
    }

    /// Called every loop iteration: reloads when the watcher says so.
    /// Returns true when something changed on screen.
    pub fn follow_tick(&mut self) -> bool {
        let Some(f) = &mut self.follow else {
            return false;
        };
        if !f.poll() {
            // A step change that is not `step_to` (a step cleared away)
            // can end the wait too.
            return self.reload_waiting();
        }
        self.reload_from_disk(false)
    }

    /// Back where no move is applied with a change on disk that waited for
    /// it (t-206): reloaded now, as a normal reload — one undo step, since
    /// the code on screen is embedded meanwhile. A file saved back as the
    /// screen has it is no reload: the document references it again.
    /// `true` when the screen changed.
    fn reload_waiting(&mut self) -> bool {
        if !self.reload_waits || self.doc.moves_applied() || self.follow.is_none() {
            return false;
        }
        self.reload_waits = false;
        if self.reload_from_disk(true) {
            return true;
        }
        if self.back_to_reference() {
            if let Some(f) = &self.follow {
                self.info(format!("following {}", f.file_name()));
            }
            return true;
        }
        false
    }

    /// Following, with no move applied, over embedded code that the file
    /// holds again (saved back as the screen has it): the document
    /// references the file again. `true` when it did.
    fn back_to_reference(&mut self) -> bool {
        if self.follow.is_none() || self.doc.moves_applied() {
            return false;
        }
        let Some(src) = self.paused().cloned() else {
            return false;
        };
        let mut probe = self.doc.clone();
        probe.code_src = src.clone();
        let base = self.save_path.parent().map(Path::to_path_buf);
        if !fs::reread_reference(&probe, base.as_deref())
            .is_ok_and(|(code, first)| code == self.doc.base() && first == self.doc.base_first())
        {
            return false;
        }
        self.doc.code_src = src;
        self.touch();
        true
    }

    /// Re-reads the followed file and applies it; `always` records the
    /// reload as an undo step even when no annotation changed. Returns true
    /// when the screen changed (content, or a message).
    fn reload_from_disk(&mut self, always: bool) -> bool {
        let Some(code_src) = self.followed_source() else {
            return false;
        };
        let mut probe = self.doc.clone();
        probe.code_src = code_src.clone();
        let Some(f) = &mut self.follow else {
            return false;
        };
        let base = self.save_path.parent().map(Path::to_path_buf);
        let read = fs::reread_reference(&probe, base.as_deref());
        let name = f.file_name();
        let (new, read_first) = match read {
            Ok(read) => read,
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
        // Embedded code numbered from another line than the file's (a paste
        // over a range of the file): the same text is still a change to
        // load, or saved as `code: {file}` it would load numbered otherwise.
        let renumbered = self.doc.is_embedded() && read_first != self.doc.base_first();
        if new == f.last_content && !was_missing && !renumbered {
            // Nothing new to load; embedded code the file holds again (a
            // change that waited for step 0, then saved back) is the
            // file's once more.
            return self.back_to_reference();
        }
        if self.doc.moves_applied() && new == f.last_content && !renumbered {
            // Back as it was before it went missing: nothing waits for it.
            self.info(format!("{name} is back; following it"));
            return true;
        }
        if self.doc.moves_applied() {
            // ADR 0007 §7: follow is paused while moves are applied (t-106
            // reconciles the edits); until then, say so instead of reloading.
            // The code on screen is no longer the file's: embedded meanwhile
            // (t-206), as after a paste, so the autosave and a fresh load
            // show it — saved as `code: {file}` they would read the new text
            // under the annotations and `rebind` entries made for this one.
            if !self.doc.is_embedded() {
                self.follow_file = Some(self.doc.code_src.clone());
                self.doc.code_src =
                    tiza_core::model::CodeSource::Embedded(self.doc.base().to_vec());
                self.touch();
            }
            self.reload_waits = true;
            let step = self.doc.step;
            self.error(format!(
                "{name} changed on disk but moves are applied at step {step}: go back to step 0 to reload"
            ));
            return true;
        }
        f.mark_reloaded(new.clone());
        let reloads = f.reloads;
        let first = if self.doc.is_embedded() {
            read_first
        } else {
            self.doc.source.first_line
        };
        // Code that is not the file's (a paste, an older version) replaced
        // by the file's is always one undo step, so `u` gives it back, even
        // when no annotation moved: the reload `:follow on` asked for, or
        // the one of a file that came back after it.
        let always = always || self.doc.is_embedded();
        let out = self.apply_new_code(new, code_src, first, always.then(|| "reload".into()));
        if out.recorded {
            self.history.label_last(format!("reload #{reloads}"));
        }
        match out.stale {
            None if out.ignored.is_none() => self.info(format!(
                "{name} reloaded (#{reloads}): {} annotation{} kept{}",
                out.kept,
                if out.kept == 1 { "" } else { "s" },
                if out.dropped > 0 {
                    format!(", {} dropped (text gone; u undoes)", out.dropped)
                } else {
                    String::new()
                }
            )),
            n => self.stale_notice(
                n,
                out.dropped,
                &out.ignored,
                "text gone",
                format!(
                    "{name} reloaded (#{reloads}): {} annotation{} kept",
                    out.kept,
                    if out.kept == 1 { "" } else { "s" }
                ),
            ),
        }
        true
    }

    /// `follow.status` payload.
    pub fn follow_status(&self) -> Value {
        match &self.follow {
            Some(f) => {
                let mut st = json!({
                    "enabled": true,
                    "path": f.path.display().to_string(),
                    "reloads": f.reloads,
                    "last_reload": f.last_reload_clock,
                    "missing": f.missing,
                });
                if let Some(why) = self.follow_waits() {
                    st["reason"] = json!(why);
                }
                st
            }
            None => {
                let mut st = json!({"enabled": false, "path": self.source_file().map(|p| p.display().to_string())});
                if let Some(why) = self.off_reason() {
                    st["reason"] = json!(why);
                }
                st
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
        Some(tiza_core::protocol::describe(a))
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
                    a.step.saturating_add(1)
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

    /// Whether the `:` command `name` (or one of its aliases) takes the
    /// pending selection as its anchor ([`SelectionUse`], t-203).
    pub fn selection_use(name: &str) -> Option<SelectionUse> {
        selection_use(name)
    }

    /// Every `:` command name, for completion and help.
    pub const COMMANDS: &'static [&'static str] = &[
        "arrow",
        "autosave",
        "layout",
        "box",
        "callout",
        "clear",
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
        self.selection_used = false;
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
                    // Quit only when the write succeeded (`:q!` leaves anyway),
                    // and not over stale moves without a word.
                    self.save()?;
                    self.request_quit(false);
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
                    self.reference = None;
                    self.follow_file = None;
                    self.frozen_views = self.doc.views.keys().cloned().collect();
                    self.touch();
                    // The code no longer comes from the file: there is
                    // nothing to follow (t-117; `:follow on` says the same).
                    if self.follow.is_some() {
                        self.set_follow(false)?;
                        return Ok(
                            "code frozen inside the .tiza; follow off: edits on disk no longer reach the screen (:link and :follow on to resume)"
                                .into(),
                        );
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
                    // Already a reference: there is nothing to link again.
                    if rest.is_empty()
                        && let tiza_core::model::CodeSource::File { file, lines } =
                            &self.doc.code_src
                    {
                        let range = lines.as_ref().map(|l| format!(":{l}")).unwrap_or_default();
                        return Ok(format!("the .tiza already references {file}{range}"));
                    }
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
                    // The file must be readable. Code on screen that is not
                    // the file's (pasted, never from a file, or another
                    // file's): the screen shows the file's code, as the saved
                    // document will, through the usual reload (one undo
                    // step), which needs step 0 like a paste.
                    let base = self.save_path.parent().map(Path::to_path_buf);
                    // A file a `code.link` shows has annotations of its own
                    // view: as the main code they would be another file's.
                    let links = self.doc.link_files();
                    let mut known = links.clone();
                    known.extend(self.doc.view_code.keys().cloned());
                    let name = fs::canonical_link(&self.doc, &file, base.as_deref(), &known);
                    if links.contains(&name) {
                        let step = self
                            .doc
                            .moves
                            .iter()
                            .find_map(|m| match &m.action {
                                moves::MoveAction::CodeLink(l)
                                    if self.doc.view_key(&l.file).as_deref()
                                        == Some(name.as_str()) =>
                                {
                                    Some(m.at_step)
                                }
                                _ => None,
                            })
                            .unwrap_or_default();
                        return Err(Error::Invalid(format!(
                            "{file} is shown by a code.link at step {step}, with annotations of its own: delete that link first to reference it"
                        )));
                    }
                    let mut probe = self.doc.clone();
                    probe.reference_file(file.clone(), range);
                    let (code, first) = fs::reread_reference(&probe, base.as_deref())?;
                    let mut said = if code.as_slice() != self.doc.base()
                        || first != self.doc.base_first()
                    {
                        if let Some(e) = self.moves_block_reason("show another file's code") {
                            return Err(e);
                        }
                        let out = self.apply_new_code(
                            code,
                            probe.code_src,
                            first,
                            Some(format!("link {file}")),
                        );
                        let what = format!(
                            "the .tiza now references {file} and shows its code ({} annotation{} kept; u undoes)",
                            out.kept,
                            if out.kept == 1 { "" } else { "s" }
                        );
                        if out.dropped > 0 || out.stale.is_some() {
                            self.stale_notice(
                                out.stale,
                                out.dropped,
                                &out.ignored,
                                "their text is gone",
                                what,
                            );
                            String::new()
                        } else {
                            what
                        }
                    } else {
                        self.doc.reference_file(file.clone(), range);
                        self.touch();
                        format!("the .tiza now references {file}")
                    };
                    // Follow watches the file the document references now.
                    match self.follow_the_reference() {
                        Some(Ok(name)) if !said.is_empty() => {
                            said.push_str(&format!("; following {name}"));
                        }
                        Some(Err(e)) => {
                            let what = if said.is_empty() {
                                self.message.take().map(|m| m.0).unwrap_or_default()
                            } else {
                                std::mem::take(&mut said)
                            };
                            self.error(format!("{what}; follow off: {e}"));
                        }
                        _ => {}
                    }
                    Ok(said)
                }
                "spot" | "spotlight" => self.cmd_spot(rest),
                "callout" | "co" => self.cmd_callout(rest),
                "set" => self.cmd_set(rest),
                "follow" => match rest.trim() {
                    "" => Ok(match &self.follow {
                        Some(f) => format!(
                            "following {} ({} reload{}{}{})",
                            f.file_name(),
                            f.reloads,
                            if f.reloads == 1 { "" } else { "s" },
                            f.last_reload_clock
                                .as_ref()
                                .map(|t| format!(", last at {t}"))
                                .unwrap_or_default(),
                            self.follow_waits()
                                .map(|w| format!("; {w}"))
                                .unwrap_or_default()
                        ),
                        None => match self.off_reason() {
                            Some(why) => format!("follow off: {why}"),
                            None => {
                                "follow off (:follow on to reload when the file is saved)".into()
                            }
                        },
                    }),
                    "on" | "yes" | "start" => self.set_follow(true),
                    "off" | "no" | "stop" => self.set_follow(false),
                    _ => Err(Error::Invalid("usage: :follow [on|off]".into())),
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
                                return Err(Error::Invalid("unknown theme (dark, light, terminal, auto, file PATH; :theme alone toggles)".to_string()));
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
                    v if v.is_empty() => "unknown command (try :help)".to_string(),
                    v if v.len() == 1 => {
                        format!("unknown command — did you mean :{}?", v[0])
                    }
                    v => format!(
                        "unknown command — did you mean {}?",
                        v.iter()
                            .map(|c| format!(":{c}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                })),
            }
        };
        let result = exec();
        // A command that ran lets the selection go (t-203): a later `:mark`
        // must not land on it. One that anchored itself on it keeps it, so
        // ↑ + Enter runs it again on the same text, and one that only looks
        // (or undoes) leaves it; a command that failed leaves it as it was,
        // to fix the line and retry.
        let keep = match selection_use(cmd) {
            Some(SelectionUse::Takes) => self.selection_used,
            Some(SelectionUse::Neutral) => true,
            _ => false,
        };
        if result.is_ok() && !keep {
            self.pending_sel = None;
        }
        match result {
            Ok(msg) if !msg.is_empty() => self.info(msg),
            Ok(_) => {}
            Err(e) => self.error(e.to_string()),
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
            stale: None,
            held: Vec::new(),
        };
        let (i, warning) = self.add_move_noting(m)?;
        let added = format!(
            "move #{} added: step {} shows {}{} (:ref path to change the document's own file)",
            i + 1,
            self.doc.step,
            file,
            lines.map(|l| format!(":{l}")).unwrap_or_default()
        );
        Ok(match warning {
            Some(w) => format!("{w} · {added}"),
            None => added,
        })
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
                        .map_err(|_| Error::Invalid("step= must be a number".into()))?;
                }
                "width" => {
                    width = tiza_core::layout::check_width(v.parse::<usize>().map_err(|_| {
                        Error::Invalid(format!(
                            "width= must be a number between {} and {}",
                            tiza_core::layout::MIN_WIDTH,
                            tiza_core::layout::MAX_WIDTH
                        ))
                    })?)?;
                }
                "theme" => {
                    let name = Self::parse_theme_name(&v)
                        .filter(|n| *n != ThemeName::Terminal)
                        .ok_or_else(|| Error::Invalid("theme= must be dark or light".into()))?;
                    theme = Theme::resolve(
                        name,
                        self.host_dark.unwrap_or(true),
                        self.config_overrides.as_ref(),
                        self.doc.theme_overrides.as_ref(),
                    )
                    .0;
                }
                _ => {
                    return Err(Error::Invalid(
                        "unknown option (step, width, theme)".to_string(),
                    ));
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
                Error::Invalid(
                    "the path's extension picks the format (.txt .ansi .md .svg .png)".into(),
                )
            }
        })?;
        // A terminal theme (or `reset` tokens) has no palette in a file:
        // the export uses the dark/light theme and the message says so.
        let (theme, theme_notice) = theme.for_export();
        let title = self.doc.title.clone();
        let l = self.lay_out(
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
                stable: true,
                margin_cap: self.doc.margin.map(usize::from),
            },
        );
        let bytes = match format {
            ExportFormat::Txt => (l.canvas.to_plain_text() + "\n").into_bytes(),
            ExportFormat::Ansi => l.canvas.to_ansi().into_bytes(),
            // The title goes in the md heading.
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
                    p.step = Some(
                        v.parse()
                            .map_err(|_| Error::Invalid("step must be a number".into()))?,
                    )
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
                _ => {
                    return Err(Error::Invalid(
                        "unknown key for :hl (emphasis=fill|underline|brackets|strike)".to_string(),
                    ));
                }
            }
        }
        let (at, label) = split_first(rest);
        let (span, label) = match (self.pending_sel, looks_like_anchor(at)) {
            (Some(Selection::Span(sp)), false) => {
                self.selection_used = true;
                (sp, rest.trim())
            }
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
                _ => {
                    return Err(Error::Invalid(
                        "unknown key for :mark (emphasis=fill|underline|brackets|strike)"
                            .to_string(),
                    ));
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
                    "unknown mark kind (kinds: {}; or define one with :style, or use a glyph/emoji)",
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
                return Err(Error::Invalid(unknown_color()));
            }
            style.color = Some(c);
        }
        let inline = !known;
        let (span, label) = match (self.pending_sel, looks_like_anchor(at)) {
            (Some(Selection::Span(sp)), false) => {
                self.selection_used = true;
                (sp, rest.trim())
            }
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
                        return Err(Error::Invalid(unknown_color()));
                    }
                    st.color = Some(v);
                }
                _ => {
                    return Err(Error::Invalid(
                        "unknown style field (glyph, legend, color)".to_string(),
                    ));
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
                    Error::Invalid("unknown placement: use @below, @margin or @auto".into())
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

    /// Answers one socket request (t-166): a panic while it runs (a bug)
    /// answers an `internal` error instead of closing the presenter's TUI,
    /// as the release profile's `panic = "unwind"` promises. The request is
    /// dropped whole: everything it could change in the instance — the
    /// document, the view (step, cursor, selection, scroll, focus, pointer,
    /// a typing animation it started), the mode and overlays, theme, save
    /// path and autosave, follow, supervision, the alerts — goes back to
    /// what it was ([`RequestState`]), the undo entries it recorded are
    /// forgotten, and the document is saved again when the request had
    /// saved a half-done change. The crash hook leaves the terminal alone
    /// meanwhile ([`answering_request`]); the panic's place and message go
    /// to `panics.log` in Tiza's runtime directory, and nothing of it is
    /// shown or returned (it may quote the document, t-167).
    pub fn handle_request(&mut self, req: Request) -> Response {
        let id = req.id.clone();
        let method = req.method.clone();
        let mutating = !READ_ONLY_METHODS.contains(&method.as_str());
        // A read-only method changes nothing to put back (and `status` is
        // polled often: no copy of the document for it).
        let before = mutating.then(|| RequestState::take(self));
        let revision = self.revision;
        let was = ANSWERING.with(|a| a.replace(true));
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.answer(req)));
        ANSWERING.with(|a| a.set(was));
        if let Ok(resp) = r {
            return resp;
        }
        if let Some(before) = before {
            before.restore(self);
        }
        if self.revision != revision {
            // The request saved or announced a change that is gone now:
            // a new revision, and the file as the document is again.
            self.touch();
        }
        let known = tiza_core::protocol::METHODS.iter().any(|m| m.0 == method);
        let what = if known { method.as_str() } else { "request" };
        let log = self.log_request_panic(what);
        let msg = format!(
            "{what}: a bug in Tiza stopped this request (a panic){}; please report it{}",
            if mutating {
                ", and nothing of it was kept"
            } else {
                ""
            },
            match &log {
                Some(p) => format!(" (details in {})", p.display()),
                None => String::new(),
            }
        );
        self.error(msg.clone());
        Response::err(id, "internal", msg)
    }

    /// Appends one line about a request's panic to `panics.log` in Tiza's
    /// runtime directory (owner only, like the sockets): the time, the
    /// version, the request, where it panicked and its message. Returns the
    /// log's path when it was written.
    fn log_request_panic(&self, what: &str) -> Option<PathBuf> {
        log_last_request_panic(what)
    }

    fn answer(&mut self, req: Request) -> Response {
        let id = req.id.clone();
        let result = self.dispatch(&req.method, req.params);
        // Test hook (t-166), only with the `test-hooks` feature: a panic on
        // purpose after the request did its work, for the tests that a
        // caught panic leaves the instance as it was.
        #[cfg(feature = "test-hooks")]
        if std::env::var("TIZA_TEST_HOOK").is_ok_and(|v| v == format!("request:{}", req.method)) {
            panic!("TIZA_TEST_HOOK at {}", req.method);
        }
        match result {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::err(id, e.code(), e.to_string()),
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
        if req.method == "check" {
            self.ask_check(req, reply);
            return;
        }
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
        // `supervise.status` all show this summary.
        let summary = proposal_summary(&req);
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

    /// `check` (t-210): answered from a thread of its own on a copy of the
    /// document as it is now, so the screen, the keys and the other
    /// requests do not wait for it ([`crate::checks`]). Its parameters are
    /// read here: a bad width is answered at once.
    fn ask_check(&mut self, req: Request, reply: std::sync::mpsc::Sender<Response>) {
        let width = match check_width(req.params) {
            // The width the check uses: `{}` and `{"width": 100}` are one
            // check and share one run.
            Ok(w) => w.unwrap_or(tiza_core::check::DEFAULT_WIDTH),
            Err(e) => {
                let _ = reply.send(Response::err(req.id, e.code(), e.to_string()));
                return;
            }
        };
        let base = self.save_path.parent().map(Path::to_path_buf);
        let doc = &self.doc;
        self.checks
            .ask(req.id, reply, width, self.revision, || doc.clone(), base);
    }

    /// An agent's `annotation.add` / `annotation.update` (socket or MCP,
    /// never the TUI) left `result`: when the view does not show it, the bar
    /// says so — `a17: box 3-5 added by agent (step 1, press n)` — and
    /// changes within [`AGENT_NOTICE_GROUP`] of the last one join it
    /// (`3 agent annotations not shown (step 1, press n)`). In presentation
    /// mode nothing is drawn: it waits for the bar (t-155). An error the
    /// request itself raised (a failed save) keeps the bar.
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
        let mut cuts = None;
        let hidden: Vec<_> = items
            .iter()
            .filter_map(|(id, verb)| {
                let a = self.doc.find_annotation(id)?;
                let why = self.not_shown(id, &mut cuts)?;
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
                // `press n` never shows one that has no room: no step hint
                // then, unless every one of them has that same tail.
                let no_room = many
                    .iter()
                    .any(|(_, _, w)| matches!(w, tiza_core::model::Hidden::NoRoom { .. }));
                let first_later = many
                    .iter()
                    .filter_map(|(_, _, w)| match w {
                        tiza_core::model::Hidden::Later(s) => Some(*s),
                        _ => None,
                    })
                    .min()
                    .filter(|_| !no_room);
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
            Hidden::NoRoom { width } => format!(" (not drawn: no room at {width} columns)"),
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
                .and_then(|id| self.not_shown(&id, &mut None))
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

    pub fn supervise_status(&self) -> Value {
        // A method the API does not have is never repeated (t-167 R1).
        let method = |m: &str| -> String {
            if !tiza_core::protocol::METHODS.iter().any(|(k, _)| *k == m) {
                tiza_core::error::ELIDED.into()
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
                let was = self.follow.is_some();
                let said = self.message.clone();
                let msg = self.set_follow(enabled)?;
                // Turned on where a save waits for step 0: the presenter is
                // told too, or the next save would seem lost; unless turning
                // it on said something of its own (a file that cannot be
                // read).
                if !was && self.follow_waits().is_some() && self.message == said {
                    self.info(msg.clone());
                }
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
                let view = self.doc.view_at(self.doc.step);
                let mut out = String::new();
                for (i, text) in view.doc.code.iter().enumerate() {
                    let line = self.doc.source.first_line + i as u32;
                    out.push_str(&format!("{line:>4}  {text}\n"));
                }
                json!({"language": self.doc.source.language, "file": self.shown_file(), "view": self.shown_view(), "first_line": self.doc.source.first_line, "last_line": self.doc.last_line(), "text": out})
            }
            "scene.get" => {
                if !params.is_null() {
                    let _: tiza_core::protocol::SceneGetParams = parse(params)?;
                }
                // On the wire the code travels embedded: a client in another
                // directory could not resolve a relative file reference;
                // linked files travel embedded too (t-105).
                let document = self.doc.embedded_copy();
                let (stale, held) = tiza_core::stale::marks(&self.doc);
                serde_json::to_value(Scene {
                    revision: self.revision,
                    step: self.doc.step,
                    max_step: self.doc.max_step(),
                    document,
                    stale,
                    held,
                })?
            }
            "scene.list" => json!(self.list_lines()),
            "move.add" => {
                let m: Move = parse(params)?;
                let (i, warning) = self.add_move_noting(m)?;
                serde_json::to_value(MoveMutated {
                    index: i + 1,
                    moves: self.doc.moves.len(),
                    max_step: self.doc.max_step(),
                    revision: self.revision,
                    warning,
                })?
            }
            "move.delete" => {
                let p: MoveIndexParams = parse(params)?;
                let (_, warning) = self.delete_move_noting(p.index)?;
                serde_json::to_value(MoveMutated {
                    index: p.index,
                    moves: self.doc.moves.len(),
                    max_step: self.doc.max_step(),
                    revision: self.revision,
                    warning,
                })?
            }
            "move.list" => serde_json::to_value(self.move_list())?,
            "annotation.add" => {
                let mut p: AddParams = parse(params)?;
                // An alias of a linked file is that file, as on load (t-183).
                if let Some(f) = &p.file {
                    let base = self.save_path.parent().map(Path::to_path_buf);
                    let mut known = self.doc.link_files();
                    known.extend(self.doc.view_code.keys().cloned());
                    p.file = Some(fs::canonical_link(&self.doc, f, base.as_deref(), &known));
                }
                let (shape, markers) = p.shape.resolve_with_markers(&self.doc)?;
                let id = self.add_annotation_full(p.id, p.step, p.color, markers, p.file, shape)?;
                // Where it stands against the view right now (t-155).
                serde_json::to_value(self.mutated(id))?
            }
            "annotation.update" => {
                let p: UpdateParams = parse(params)?;
                self.update_annotation(&p)?;
                serde_json::to_value(self.mutated(p.id))?
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
                        return Err(Error::Invalid(unknown_color()));
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
                json!({
                "document": self.doc.styles,
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
                // The event loop sends `check` to [`App::ask_check`]; this
                // answers it in place, for a caller of `handle_request`.
                let width = check_width(params)?;
                let base = self.save_path.parent().map(Path::to_path_buf);
                serde_json::to_value(tiza_core::protocol::LiveCheck {
                    report: tiza_core::check::check_document(
                        &self.doc,
                        base.as_deref(),
                        &tiza_core::check::CheckOptions { width },
                    ),
                    revision: self.revision,
                })?
            }
            "render.text" => {
                let width = match params.get("width").and_then(|w| w.as_u64()) {
                    Some(w) => tiza_core::layout::check_width(w.min(u64::from(u32::MAX)) as usize)?,
                    None => self.size.0 as usize,
                };
                json!({"text": self.layout(width).canvas.to_plain_text()})
            }
            "focus.set" => {
                // A line given is a line number (t-167 review 2: a negative
                // one cleared or cut the focus without a word); `null` or
                // absent is no line. The value is never repeated.
                let line = |k: &str| -> Result<Option<u32>, Error> {
                    match params.get(k) {
                        None | Some(Value::Null) => Ok(None),
                        Some(v) => v
                            .as_u64()
                            .and_then(|n| u32::try_from(n).ok())
                            .map(Some)
                            .ok_or_else(|| {
                                Error::Invalid(format!(
                                    "focus.set: `{k}` must be a line number (1 or more)"
                                ))
                            }),
                    }
                };
                let (a, b) = (line("from_line")?, line("to_line")?);
                if a.is_none() && b.is_some() {
                    return Err(Error::Invalid(
                        "focus.set: `to_line` needs `from_line` (omit both to clear the focus)"
                            .into(),
                    ));
                }
                let focus = match (a, b) {
                    (Some(a), Some(b)) => Some((a.min(b), a.max(b))),
                    (Some(a), None) => Some((a, a)),
                    _ => None,
                };
                // Like `:focus`: the first line must exist, the last one is
                // clamped (t-167 review: the reply repeated any number sent).
                let (first, last) = (self.doc.source.first_line, self.doc.last_line());
                if let Some((a, _)) = focus
                    && (a < first || a > last)
                {
                    return Err(Error::LineOutOfRange {
                        line: a,
                        first,
                        last,
                    });
                }
                self.focus = focus.map(|(a, b)| (a, b.min(last)));
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
                            return Err(Error::Invalid(
                                "unknown theme (dark, light, terminal, auto, toggle)".to_string(),
                            ));
                        }
                    },
                }
                self.touch();
                json!({"theme": self.theme.name, "auto": self.theme_auto, "revision": self.revision})
            }
            _ => {
                return Err(Error::Invalid(
                    "unknown method (`api.methods` lists them)".into(),
                ));
            }
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
            view: self.shown_view(),
            annotations: self.doc.annotations.len(),
            step: self.doc.step,
            max_step: self.doc.max_step(),
            step_id: self
                .doc
                .named_step(self.doc.step)
                .and_then(|n| n.id.clone()),
            step_title: self
                .doc
                .named_step(self.doc.step)
                .and_then(|n| n.title.clone()),
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
            supervised: self.supervised,
            autosave: self.autosave,
            document: self.save_path.is_file().then(|| {
                let p =
                    std::path::absolute(&self.save_path).unwrap_or_else(|_| self.save_path.clone());
                p.to_string_lossy().into_owned()
            }),
            pending: self.proposals.len(),
            numbers: self.doc.numbers,
            margin: self.doc.margin,
            pointer: self.pointer,
            stale_moves: tiza_core::stale::count(&self.doc),
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
        let width = (cols as usize).saturating_sub(2 * pad_h as usize).max(1);
        // What the last frame laid out at this width, when nothing it
        // depends on changed since; else laid out once and kept.
        let key = self.viewport_key(width);
        let known = self
            .viewport
            .borrow()
            .and_then(|(k, facts)| (k == key).then_some(facts));
        let facts = known.unwrap_or_else(|| {
            let facts = ViewportFacts::of(&self.layout(width));
            *self.viewport.borrow_mut() = Some((key, facts));
            facts
        });
        self.viewport_warnings_from(facts)
    }

    /// What the viewport facts depend on at `width`.
    fn viewport_key(&self, width: usize) -> ViewportKey {
        ViewportKey {
            revision: self.revision,
            step: self.doc.step,
            width,
            stable: self.present || self.layout_stable,
            typing: self.typing.is_some(),
        }
    }

    /// `viewport_warnings` for a layout already computed this frame (the
    /// status bar must not lay the document out a second time).
    pub fn viewport_warnings_for(&self, l: &Layout) -> Vec<String> {
        self.viewport_warnings_from(ViewportFacts::of(l))
    }

    fn viewport_warnings_from(&self, l: ViewportFacts) -> Vec<String> {
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
        // A file may have ` (` in its name (`src/stack (v2).rs`): the tail
        // starts at the first ` (` after which the file's parentheses
        // balance (t-173).
        let end = i + 2 + file.len();
        let balanced = |f: &str| {
            let mut depth = 0i32;
            f.chars().all(|c| {
                depth += match c {
                    '(' => 1,
                    ')' => -1,
                    _ => 0,
                };
                depth >= 0
            }) && depth == 0
        };
        let start = msg[..i]
            .match_indices(" (")
            .map(|(j, _)| j)
            .find(|&j| balanced(&msg[j + 2..end]))
            .unwrap_or(i);
        return (start + 2 < end && digits(n)).then_some(start);
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

fn opt(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// The error for a color that is neither a palette name nor `#rrggbb`:
/// it lists the valid names and never repeats the one given (t-167
/// rework: a closed value's error quotes nothing the caller wrote).
fn unknown_color() -> String {
    format!(
        "unknown color (use {} or #rrggbb)",
        tiza_core::theme::PALETTE.join(", ")
    )
}

/// `fill|underline|brackets|strike` → [`Emphasis`], with the options on error.
fn parse_emphasis(v: &str) -> Result<Emphasis, Error> {
    Emphasis::parse(v).ok_or_else(|| {
        Error::Invalid("unknown emphasis (fill, underline, brackets, strike)".to_string())
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
                // A new place on the code: the selection a `:` command could
                // still take is not the one meant any more (t-203).
                self.pending_sel = None;
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
/// the list, the y/n/expiry messages and `supervise.status`. A closed value
/// that is none of its names (a `type`, a method) is never repeated (t-167
/// R1): it reads [`tiza_core::error::ELIDED`].
pub fn proposal_summary(req: &Request) -> String {
    let p = &req.params;
    let s = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let quote = |t: &str| format!("«{t}»");
    let n = |k: &str| p.get(k).and_then(|v| v.as_u64());
    let place = |k: &str| -> String {
        match p.get(k) {
            Some(w) if w.is_object() => {
                if let Some(f) = w.get("find").and_then(|v| v.as_str()) {
                    quote(f)
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
                k if !SHAPE_TYPES.contains(&k) => tiza_core::error::ELIDED.into(),
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
            _ => format!("point at {}", quote(&s("find"))),
        },
        "pointer.clear" => "turn the pointer off".into(),
        "theme.set" => format!("theme {}", s("theme")),
        "follow.set" => "toggle follow mode".into(),
        other
            if !tiza_core::protocol::METHODS
                .iter()
                .any(|(m, _)| *m == other) =>
        {
            tiza_core::error::ELIDED.into()
        }
        other => other.to_string(),
    }
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
                Error::Invalid(format!("unknown border (use {})", Stroke::NAMES.join(", ")))
            })?
        }
        "corners" | "route" => {
            bs.corners = Route::parse(v).ok_or_else(|| {
                Error::Invalid(format!("unknown corners (use {})", Route::NAMES.join(", ")))
            })?
        }
        "fill" => {
            bs.fill = Some(
                BorderStyle::parse_fill(v)
                    .ok_or_else(|| Error::Invalid("fill must be yes|tint or no|none".into()))?,
            )
        }
        _ => {
            return Err(Error::Invalid(
                "unknown key (border, corners, fill)".to_string(),
            ));
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
                Error::Invalid(format!("unknown stroke (use {})", Stroke::NAMES.join(", ")))
            })?
        }
        "route" => {
            ls.route = Route::parse(v).ok_or_else(|| {
                Error::Invalid(format!("unknown route (use {})", Route::NAMES.join(", ")))
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
                Error::Invalid(format!("unknown {k} head (use {})", Head::NAMES.join(", ")))
            })?
        }
        "end" => {
            ls.heads.end = Head::parse(v).ok_or_else(|| {
                Error::Invalid(format!("unknown {k} head (use {})", Head::NAMES.join(", ")))
            })?
        }
        _ => {
            return Err(Error::Invalid(
                "unknown key (stroke, route, heads, start, end)".to_string(),
            ));
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

/// Parameters of a socket request. Ones that do not deserialize: serde
/// quotes the value it could not read (`unknown variant `x``, `invalid
/// type: string "x"`); that value is taken out, like the MCP's own
/// deserialization errors, and serde's list of valid names stays (t-167
/// rework).
fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, Error> {
    T::deserialize(&v).map_err(|e| {
        Error::Invalid(format!(
            "{}{}",
            tiza_core::error::INVALID_PARAMETERS,
            tiza_core::error::serde_text(&e.to_string())
        ))
    })
}

/// The width a `check` request asks for (`None`: the default).
fn check_width(params: Value) -> Result<Option<usize>, Error> {
    let p: tiza_core::schema::CheckParams = parse(params)?;
    p.width.map(tiza_core::layout::check_width).transpose()
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

/// Whether adding or deleting `m` can hide an annotation: only a patch or
/// a link moves lines in or out of a window or changes the file a step
/// shows, and only from its own step on.
fn hides(m: &Move) -> bool {
    matches!(
        m.action,
        tiza_core::moves::MoveAction::CodePatch(_) | tiza_core::moves::MoveAction::CodeLink(_)
    )
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
