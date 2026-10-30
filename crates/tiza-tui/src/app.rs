//! Application state and logic, with no drawing dependencies. Both the
//! keyboard and the local API go through here, so an agent and the presenter
//! do exactly the same thing.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use tiza_core::moves::Camera;

use serde_json::{Value, json};
use tiza_core::effects::Effect;
use tiza_core::history::History;
use tiza_core::layout::{Layout, LayoutOptions};
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

/// What a `:` command does with the selection a `v`/`V` left pending:
/// `:hl` and `:mark` take it as their anchor and keep it when they
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
        "q" | "quit" | "q!" | "quit!" | "w" | "write" | "w!" | "write!" | "saveas" | "sav"
        | "saveas!" | "sav!" | "export" | "supervise" | "supervised" | "mouse" | "review"
        | "wq" | "wq!" | "box" | "arrow" | "ar" | "note" | "n" | "embed" | "link" | "ref"
        | "spot" | "spotlight" | "callout" | "co" | "set" | "follow" | "until" | "clear"
        | "del" | "delete" | "rm" | "color" | "col" | "label" | "step" | "s" | "next" | "goto"
        | "prev" | "focus" | "f" | "theme" | "t" | "present" | "p" | "lang" | "title"
        | "margin" | "numbers" | "nums" | "pad" | "padding" | "layout" | "autosave" | "scroll" => {
            Ignores
        }
        _ => return None,
    })
}

/// What the last selection was, so a command typed right after can use it as its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    Span(tiza_core::TextSpan),
    Lines(u32, u32),
}

/// Follow at a step with a `code.patch` applied (ADR 0007 §7): a save
/// does not load there. It loads at the resume step, the last step before
/// the first patch, or earlier.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Pause {
    /// A save came in this pause: the file is read again at the resume
    /// step (a change to load, or the file back as the screen has it).
    waits: bool,
    /// The file holds other code than the screen: a change waits.
    pending: bool,
    /// This pause gave its notice.
    noticed: bool,
    /// The text of that notice, while the change it is about waits.
    notice: Option<String>,
    /// The file cannot be read now (not valid UTF-8), and the bar said so.
    unreadable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    List,
    /// Pending agent proposals (supervised mode).
    Review,
}

/// A typing animation in progress: the moves of the step just
/// reached whose `typing` is `line`/`char`, revealed unit by unit. The code
/// shown is `full` with the content of the pending items blanked out, so the
/// line count — and every shifted annotation — stays put while it types.
#[derive(Debug, Clone)]
pub struct TypingState {
    pub plan: moves::TypingPlan,
    pub index: usize,
    pub revealed: usize,
    /// The next cue of the plan to apply.
    cue: usize,
    /// The enter effects of the step play when the typing ends: a step
    /// forward, not the step the TUI opens at.
    pub then_effect: bool,
    last: std::time::Instant,
}

/// An enter effect in progress (experimental): the new annotations of the
/// step just reached come in as [`tiza_core::effects`] plans them. A key
/// (in presentation mode, not one that only moves the view), a change of
/// the document or a change of the live pointer ends it.
#[derive(Debug)]
pub struct EffectState {
    pub started: std::time::Instant,
    /// How long it lasts at the width it started at.
    pub duration_ms: u64,
    /// Its clock runs this many times slower (1; more only with the
    /// `test-hooks` feature and `TIZA_TEST_HOOK=effect:slow` or
    /// `effect:late-frames`, so a test on a pty sees it play).
    slow: u64,
    /// The moment every frame shows, whatever the clock says
    /// ([`App::hold_effect_at`], only for tests).
    hold_ms: Option<u64>,
    /// The revision it plays at: at another one, it is over.
    revision: u64,
    /// The live pointer shown when it started: when the pointer changes
    /// (an agent points, the presenter turns it off), it is over.
    pointer: Option<(u32, u32)>,
    /// The work layout and the plan at a width and with a pointer,
    /// computed when the effect starts, and again at another width or
    /// pointer. A frame takes it ([`App::screen_layout_with_pointer`]),
    /// paints the plan on the rows in view and gives it back
    /// ([`App::put_back_screen`]): no frame copies the layout.
    cache: std::cell::RefCell<Option<EffectCache>>,
}

impl EffectState {
    /// The same effect with no cache: a frame computes it again.
    fn copy(&self) -> EffectState {
        EffectState {
            started: self.started,
            duration_ms: self.duration_ms,
            slow: self.slow,
            hold_ms: self.hold_ms,
            revision: self.revision,
            pointer: self.pointer,
            cache: std::cell::RefCell::new(None),
        }
    }
}

#[derive(Debug)]
struct EffectCache {
    width: usize,
    pointer: Option<(u32, u32)>,
    /// The final layout of the step, with the rows that frames painted at
    /// the moment of their frame.
    layout: Layout,
    plan: Option<tiza_core::effects::Plan>,
}

/// The layout of a frame on screen ([`App::screen_layout_with_pointer`]).
#[derive(Debug)]
pub enum ScreenLayout {
    /// A layout of its own.
    Plain(Layout),
    /// While an enter effect plays: the work layout of the effect, lent to
    /// the frame. [`ScreenLayout::paint_rows`] paints the rows in view, and
    /// [`App::put_back_screen`] gives it back at the end of the frame.
    Effect(EffectFrame),
}

/// The work layout of an enter effect, lent to one frame.
#[derive(Debug)]
pub struct EffectFrame {
    /// The effect it belongs to (its start).
    started: std::time::Instant,
    /// The moment of the effect that the frame shows, in ms.
    t_ms: u64,
    cache: EffectCache,
}

impl std::ops::Deref for ScreenLayout {
    type Target = Layout;

    fn deref(&self) -> &Layout {
        match self {
            ScreenLayout::Plain(l) => l,
            ScreenLayout::Effect(f) => &f.cache.layout,
        }
    }
}

impl ScreenLayout {
    /// Paints the enter effect at the moment of the frame on the rows
    /// `rows`, the rows in view: the other rows can show another moment,
    /// so only these rows are drawn. Nothing changes without an effect.
    pub fn paint_rows(&mut self, rows: std::ops::Range<usize>) {
        if let ScreenLayout::Effect(f) = self
            && let Some(plan) = &f.cache.plan
        {
            plan.paint_rows(&mut f.cache.layout.canvas, f.t_ms, rows);
        }
    }
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
/// each other share one status-bar message.
pub const AGENT_NOTICE_GROUP: std::time::Duration = std::time::Duration::from_secs(1);

/// The status-bar message about agent changes the view does not show:
/// what it names, so a change arriving within
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
/// presentation mode, where there is no status bar to keep it.
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

/// Called by the crash hook for a panic [`App::handle_request`] catches:
/// keeps its place and message for `panics.log`.
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

/// What one socket request may change in the [`App`], taken
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
    scroll_applied: Option<(u32, usize)>,
    scroll_step: u32,
    scroll_held: Option<ScrollHeld>,
    held_unresolved: bool,
    view_anchor: Option<(u32, isize)>,
    view_cursor_line: u32,
    view_request: Option<ViewRequest>,
    agent_notice: Option<AgentNotice>,
    agent_deferred: Vec<(String, &'static str)>,
    following: bool,
    /// What a follow pause waits for: a request at the resume step loads
    /// the change and clears it.
    pause: Pause,
    /// What a load changes in the watcher, while follow stays on: with
    /// `last_content` of a load that is undone, the next read of the file
    /// would find nothing new, and the change would never load.
    loaded: Option<FollowLoaded>,
    follow_failed: Option<(PathBuf, String)>,
    screen_via_history: bool,
    pause_embedded: Option<Vec<String>>,
    /// The enter effect that plays: a request that ends it sets
    /// `step_ready_at`, so with the effect gone and `step_ready_at` back the
    /// auto-advance would never start.
    effect: Option<EffectState>,
}

/// The fields of [`crate::follow::Follow`] that a load writes, and the
/// counts of its reads, for a watcher started again.
struct FollowLoaded {
    last_content: Vec<String>,
    reloads: u64,
    last_reload: Option<std::time::Instant>,
    last_reload_clock: Option<String>,
    missing: bool,
    reads: u64,
    last_read: Option<crate::follow::Stamp>,
    hash: Option<String>,
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
            scroll_applied: app.scroll_applied,
            scroll_step: app.scroll_step,
            scroll_held: app.scroll_held.clone(),
            held_unresolved: app.held_unresolved,
            view_anchor: app.view_anchor,
            view_cursor_line: app.view_cursor_line,
            view_request: app.view_request,
            agent_notice: app.agent_notice.clone(),
            agent_deferred: app.agent_deferred.clone(),
            following: app.follow.is_some(),
            pause: app.pause.clone(),
            loaded: app.follow.as_ref().map(|f| FollowLoaded {
                last_content: f.last_content.clone(),
                reloads: f.reloads,
                last_reload: f.last_reload,
                last_reload_clock: f.last_reload_clock.clone(),
                missing: f.missing,
                reads: f.reads,
                last_read: f.last_read,
                hash: f.hash.clone(),
            }),
            follow_failed: app.follow_failed.clone(),
            screen_via_history: app.screen_via_history,
            pause_embedded: app.pause_embedded.clone(),
            effect: app.effect.as_ref().map(EffectState::copy),
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
        app.scroll_applied = self.scroll_applied;
        app.scroll_step = self.scroll_step;
        app.scroll_held = self.scroll_held;
        app.held_unresolved = self.held_unresolved;
        app.view_anchor = self.view_anchor;
        app.view_cursor_line = self.view_cursor_line;
        app.view_request = self.view_request;
        app.agent_notice = self.agent_notice;
        app.agent_deferred = self.agent_deferred;
        app.screen_via_history = self.screen_via_history;
        app.pause_embedded = self.pause_embedded;
        app.effect = self.effect;
        // Follow holds a file watcher: stopped, or started again. The
        // watcher gets back what a load wrote; one started again, its counts
        // of reads too (the same watcher keeps them: Tiza did read the file).
        let restarted = match (self.following, app.follow.is_some()) {
            (false, true) => {
                app.follow = None;
                false
            }
            (true, false) => app.set_follow(true).is_ok(),
            _ => false,
        };
        if let (Some(f), Some(l)) = (app.follow.as_mut(), self.loaded) {
            f.last_content = l.last_content;
            f.reloads = l.reloads;
            f.last_reload = l.last_reload;
            f.last_reload_clock = l.last_reload_clock;
            f.missing = l.missing;
            if restarted {
                (f.reads, f.last_read, f.hash) = (l.reads, l.last_read, l.hash);
            }
        }
        // After follow: starting it again sets a pause of its own.
        app.pause = self.pause;
        app.follow_failed = self.follow_failed;
        app.clamp_cursor();
    }
}

/// Whether this thread is answering a socket request inside
/// [`App::handle_request`]'s guard: the crash hook then leaves the
/// terminal as it is, since the panic is caught, answered as an error and
/// the TUI keeps running.
pub fn answering_request() -> bool {
    ANSWERING.with(|a| a.get())
}

/// Runs `f` as a socket request is answered, on any thread (the
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

/// What the take's geometry was computed for: the document's revision,
/// and, as a check on it, a few of its sizes and the layout options it
/// reads. The code typed so far at the step on screen counts too (the
/// take reads the code shown there): every typing tick is a revision, and
/// a geometry computed while typing is never kept past it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GeometryKey {
    revision: u64,
    moves: usize,
    annotations: usize,
    lines: usize,
    line_numbers: bool,
    tab_width: usize,
    typing: bool,
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
#[derive(Debug, Clone, PartialEq, Eq)]
struct ViewportFacts {
    code_needs: Option<usize>,
    labels_in_margin: bool,
    /// The labels and notes drawn cut with `…` or not drawn: (id, not
    /// drawn at all).
    clipped_labels: Vec<(String, bool)>,
}

impl ViewportFacts {
    fn of(l: &Layout) -> Self {
        ViewportFacts {
            code_needs: l.code_needs,
            labels_in_margin: l.labels_in_margin,
            clipped_labels: l
                .clipped_labels
                .iter()
                .map(|c| (c.id.clone(), c.dropped))
                .collect(),
        }
    }
}

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
    /// The `:` command running now anchored itself on `pending_sel`.
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
    /// Presentation mode only: until when `present_message` — the result
    /// of the command just run — is drawn on the last row. `None`: hidden.
    pub present_message_until: Option<std::time::Instant>,
    /// Presentation mode only: the result of the `:` command (or the notice
    /// at the start) that the last row shows, apart from `message`, so that
    /// the message of a key never shows there.
    pub present_message: Option<(String, bool)>,
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
    /// Opened with `--no-save` (an example annotated without touching it):
    /// `:w` would write over that file, so the bar and `:q` point at
    /// `:saveas PATH` and `:q!`. A `:saveas` or `:autosave on` ends it.
    pub no_save: bool,
    /// The file the code was last read from (`code: {file}` when opened or
    /// at the last put): a put that sent other code leaves the document
    /// embedded in memory, and a later put of that file's code makes it a
    /// reference again. `None` once `:embed` froze the code on purpose.
    reference: Option<tiza_core::model::CodeSource>,
    /// Linked files the document itself keeps in `views` (as opened, or
    /// frozen by `:embed`), as opposed to ones a put sent embedded.
    frozen_views: std::collections::BTreeSet<String>,
    /// `:layout stable`: the authoring screen uses the take's geometry
    /// (lanes, box band and margin sized for every step at once) so
    /// rehearsing looks like presenting. Presentation mode and every
    /// export are always stable.
    pub layout_stable: bool,
    /// The last autosave failure; stays on the status line until a save succeeds.
    pub save_error: Option<String>,
    /// What the file of the document holds that this version does not know
    /// ([`tiza_core::fs::not_known_notice`]): `status.warnings` says it to
    /// an agent until a save writes the file without it.
    pub not_known: Option<String>,
    /// The first failed autosave just set `message`; the next `info` of the
    /// action that triggered it must not replace it.
    save_warning_pending: bool,
    /// `--padding` for this session only: shown instead of the document's
    /// value but never written to it (`:pad` sets the document and clears this).
    pub padding_override: Option<(u16, u16)>,
    /// SVG/PNG/WebP renderer for `:export`, installed by the binary.
    pub image_exporter: Option<ImageExporter>,
    /// Mouse capture: opt-in, because capturing the mouse takes the
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
    /// The live `check` requests, answered off this thread.
    checks: crate::checks::Checks,
    /// Scroll offset (wrapped rows) of the help / list overlay.
    pub overlay_scroll: usize,
    /// The filter of the panel that is open, and whether the user types
    /// it now (`/`).
    pub overlay_filter: String,
    pub overlay_typing: bool,
    /// The selected row of the list or the review, among the rows shown,
    /// and the key of that row, to find it again when the rows change.
    pub overlay_sel: usize,
    pub overlay_pick: Option<String>,
    /// How the panel was drawn last: the rows of one page and the largest
    /// scroll, for the keys that scroll it.
    pub overlay_drawn: std::cell::Cell<crate::panel::Drawn>,
    /// The bar's text of the selected annotation where the moves of the
    /// step put it, for (id, revision, step): drawn each frame, projected
    /// once.
    selected_shown: std::cell::RefCell<Option<(String, u64, u32, String)>>,
    /// Follow mode: the watched source file, when enabled. Only this
    /// watcher reads the file again: with `None`, no save reaches the
    /// screen.
    pub follow: Option<crate::follow::Follow>,
    /// Why follow went off by itself: the watcher of that file could not
    /// start. Said while that file is still the one to follow and nothing
    /// else explains the code on screen, until follow is turned on or off.
    follow_failed: Option<(PathBuf, String)>,
    /// Follow paused at a step with a patch applied: what waits for the
    /// resume step. While a change waits, the code on screen is embedded.
    pause: Pause,
    /// A save came while the presenter was busy ([`App::presenter_busy`]):
    /// it is read when they are done.
    save_held: bool,
    /// Code pasted in this session (the newest last, at most
    /// [`PASTES_KEPT`]): embedded code on screen equal to one of them is
    /// «pasted», anything else a version of the file.
    pasted_codes: Vec<Vec<String>>,
    /// The code a follow pause embedded over its file when a change came
    /// (the code on screen then): quitting names it while it is on screen
    /// and not saved, also when nothing waits any more.
    pause_embedded: Option<Vec<String>>,
    /// The code on screen was reached by an undo or a redo, not by the edit
    /// itself: a paste is then «the paste again».
    screen_via_history: bool,
    /// Typing animation of the step just reached.
    pub typing: Option<TypingState>,
    /// Enter effect of the step just reached, after its typing.
    pub effect: Option<EffectState>,
    /// When the current step became ready (reached, its typing finished
    /// and its `wait` moves elapsed): the `auto_advance_ms` countdown runs
    /// from here. `None` while typing or after it fired.
    pub step_ready_at: Option<std::time::Instant>,
    /// Follow the host terminal's appearance: light or dark from
    /// OSC 11, re-asked on resize. Off once the document, `--theme`, `t`
    /// or `:theme NAME` fixes a theme; `:theme auto` turns it back on.
    pub theme_auto: bool,
    /// What the terminal said about its background (`None`: never asked
    /// or no answer).
    pub host_dark: Option<bool>,
    /// The `[theme]` table of the config file, applied under the
    /// document's own overrides.
    pub config_overrides: Option<tiza_core::theme::ThemeOverrides>,
    /// The `[defaults]` of the config file: `pad` and `numbers` apply
    /// when the document does not set them (the rest is read at start).
    pub config_defaults: tiza_core::config::Defaults,
    /// The `[styles]` of the config file: a mark that uses one copies it
    /// into the document first (a document style of that name wins).
    pub personal_styles: BTreeMap<String, MarkStyle>,
    /// What the last theme resolution changed or ignored (pushed tokens,
    /// unknown keys), for `:theme` to report.
    pub theme_notes: Vec<String>,
    /// OSC answers arriving through the key stream after a re-query.
    pub osc: crate::appearance::OscCapture,
    /// The live pointer: `f` (or `pointer.set`) turns it on; while
    /// on, its lines are drawn normally and the rest of the code dimmed on
    /// this screen only. Never in the document, the undo stack, a save or
    /// an export.
    pub pointer: Option<tiza_core::protocol::PointerAt>,
    /// Cursor line and selection lines when the pointer last looked: a
    /// change since then is a motion of the presenter's, and the pointer
    /// follows it ([`App::sync_pointer`]).
    pointer_seen: ((u32, u32), Option<u32>),
    /// The `scroll` move the view last jumped to, by its id: a step change
    /// jumps only when the camera of the new step is another one.
    scroll_applied: Option<(u32, usize)>,
    /// The step on screen when the view last jumped to a `scroll`: an undo
    /// at that step does not send the view to the top.
    scroll_step: u32,
    /// After a jump, the view keeps the target where `align` puts it, frame
    /// after frame, until the cursor or the pointer moves.
    pub scroll_held: Option<ScrollHeld>,
    /// A step change left a held view that no frame has put on screen yet:
    /// a key or a mouse event before that frame resolves it first
    /// ([`App::resolve_pending_view`]). The frame clears it.
    pub held_unresolved: bool,
    /// Where the last frame showed the code ([`view_anchor_of`]): in
    /// presentation mode the next frame keeps that line at that row while
    /// the rows above it change. `None` after [`App::set_scroll`]: then
    /// `scroll` is the view.
    pub(crate) view_anchor: Option<(u32, isize)>,
    /// The cursor line the last frame saw: in presentation mode the view
    /// follows the cursor only in a frame where its line changed.
    pub(crate) view_cursor_line: u32,
    /// A key that moves the view in presentation mode, for the next frame.
    pub(crate) view_request: Option<ViewRequest>,
    /// The last message about an agent change the view does not show,
    /// while it may still group the next one.
    agent_notice: Option<AgentNotice>,
    /// Agent changes not shown that arrived in presentation mode: told on
    /// the bar when it ends, never on the take.
    agent_deferred: Vec<(String, &'static str)>,
}

impl App {
    pub fn new(mut doc: Document, save_path: PathBuf) -> Self {
        // A message names a move as `:moves` and `move.list` do. The notice
        // of the moves that Tiza skips names them as the file does.
        doc.number_moves_live();
        let not_known = tiza_core::fs::not_known_notice(&doc);
        // The file the embedded code replaced while it was followed (what
        // `:follow on` reloads) means nothing beside a file reference.
        if !doc.is_embedded() {
            doc.follows = None;
        }
        let theme_auto = doc.theme.is_none();
        let reference = (!doc.is_embedded()).then(|| doc.code_src.clone());
        let frozen_views = doc.views.keys().cloned().collect();
        let theme = Theme::from_name(doc.theme.unwrap_or(ThemeName::Dark));
        let cursor_line = doc.source.first_line;
        let mut app = Self {
            theme_auto,
            host_dark: None,
            config_overrides: None,
            config_defaults: Default::default(),
            personal_styles: BTreeMap::new(),
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
            present_message: None,
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
            no_save: false,
            reference,
            frozen_views,
            layout_stable: false,
            save_error: None,
            not_known,
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
            overlay_filter: String::new(),
            overlay_typing: false,
            overlay_sel: 0,
            overlay_pick: None,
            overlay_drawn: Default::default(),
            selected_shown: Default::default(),
            follow: None,
            follow_failed: None,
            pause: Pause::default(),
            save_held: false,
            pasted_codes: Vec::new(),
            pause_embedded: None,
            screen_via_history: false,
            typing: None,
            effect: None,
            step_ready_at: Some(std::time::Instant::now()),
            pointer: None,
            pointer_seen: ((cursor_line, cursor_line), None),
            scroll_applied: None,
            scroll_step: 0,
            scroll_held: None,
            held_unresolved: false,
            view_anchor: None,
            view_cursor_line: cursor_line,
            view_request: None,
            agent_notice: None,
            agent_deferred: Vec::new(),
        };
        app.rebuild_theme();
        app.mark_saved();
        // Opened at a step whose camera is a `scroll`: the first frame
        // shows its target.
        app.camera_changed();
        app
    }

    // ---------- themes ----------

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
    /// document's overrides. `undoable`: `u` takes it back (`:theme file`);
    /// `--theme-file` is not, as `--theme` is not.
    pub fn load_theme_file(
        &mut self,
        path: &std::path::Path,
        undoable: bool,
    ) -> Result<String, Error> {
        let (ov, file_notes) = tiza_core::theme::load_theme_file(path).map_err(Error::Invalid)?;
        if undoable {
            self.history.record(&self.doc, "theme file");
        }
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
            {
                // What the file left out, then what the tokens did.
                let notes: Vec<&String> = file_notes.iter().chain(&self.theme_notes).collect();
                if notes.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; {}",
                        notes
                            .iter()
                            .map(|n| n.as_str())
                            .collect::<Vec<_>>()
                            .join("; ")
                    )
                }
            }
        ))
    }

    /// Padding in effect: the session override, else the document's, else
    /// the config file's, else the default.
    pub fn effective_padding(&self) -> (u16, u16) {
        self.padding_override
            .or(self.doc.padding)
            .or(self.config_defaults.pad)
            .unwrap_or(crate::ui::DEFAULT_PADDING)
    }

    /// Whether the gutter is shown: the document's `numbers`, else the
    /// config file's, else on.
    pub fn numbers(&self) -> bool {
        self.doc
            .numbers_or(self.config_defaults.numbers.unwrap_or(true))
    }

    /// Copies the personal style `kind` into the document when a mark is
    /// about to use it and the document has no style of that name (the
    /// copy keeps the `.tiza` the same on any machine). Returns the name
    /// of the style it copied.
    fn adopt_personal_style(
        doc: &mut Document,
        personal: &BTreeMap<String, MarkStyle>,
        kind: &str,
    ) -> Option<String> {
        let kind = kind.trim();
        if doc.styles.contains_key(kind) || tiza_core::model::builtin_kind(kind).is_some() {
            return None;
        }
        let st = personal.get(kind)?;
        doc.styles.insert(kind.to_string(), st.clone());
        Some(kind.to_string())
    }

    /// [`Self::adopt_personal_style`] for every mark of `doc`.
    /// What `:style NAME field=…` and `style.set` change: the style of the
    /// document, else a copy of the personal style of that name.
    fn style_to_change(&self, name: &str) -> MarkStyle {
        self.doc
            .styles
            .get(name)
            .or_else(|| self.personal_styles.get(name))
            .cloned()
            .unwrap_or_default()
    }

    fn adopt_personal_styles(doc: &mut Document, personal: &BTreeMap<String, MarkStyle>) {
        if personal.is_empty() {
            return;
        }
        let kinds: Vec<String> = doc
            .annotations
            .iter()
            .filter_map(|a| match &a.shape {
                Shape::Mark { kind, .. } => Some(kind.clone()),
                _ => None,
            })
            .collect();
        for k in kinds {
            Self::adopt_personal_style(doc, personal, &k);
        }
    }

    /// The names `:mark` takes: the document's styles, the personal
    /// styles it does not have, then the built-in kinds.
    fn kind_names(&self) -> Vec<String> {
        let mut v = self.doc.kind_names();
        let personal = self
            .personal_styles
            .keys()
            .filter(|k| !self.doc.styles.contains_key(*k))
            .cloned();
        let at = self.doc.styles.len();
        v.splice(at..at, personal);
        v
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

    /// Why the terminal is too small for the panel `which`, if it is (a
    /// size of 0, not known yet, fits). A bar narrower than 60 cells has
    /// room for the size only.
    fn panel_too_small(&self, which: Overlay) -> Option<String> {
        let (w, h) = self.size;
        let (mw, mh) = crate::panel::MIN_SIZE;
        let what = match which {
            Overlay::Help => "the help needs",
            Overlay::List => "the list needs",
            Overlay::Review => "the agent changes need",
            Overlay::None => return None,
        };
        (w > 0 && h > 0 && (w < mw || h < mh)).then(|| {
            if w < 60 {
                format!("needs {mw}×{mh}")
            } else {
                format!("{what} {mw}×{mh} or more")
            }
        })
    }

    /// Opens the panel `which`, its filter and scroll empty. A terminal
    /// smaller than a panel needs refuses it and says how large it must be.
    pub fn open_overlay(&mut self, which: Overlay) -> Result<(), Error> {
        if let Some(why) = self.panel_too_small(which) {
            return Err(Error::Invalid(why));
        }
        self.close_overlay();
        self.overlay = which;
        // The list starts at the first row of the step on screen.
        if which == Overlay::List {
            let items = crate::list::list(self).1;
            if let Some(i) = items.iter().position(|it| it.step() == self.doc.step) {
                self.overlay_sel = i;
                self.overlay_pick = Some(items[i].key());
            }
        }
        Ok(())
    }

    /// A panel the terminal became too small for closes and says why, so
    /// that it does not take keys while it cannot be seen.
    pub fn close_panel_too_small(&mut self) {
        if let Some(why) = self.panel_too_small(self.overlay) {
            self.close_overlay();
            self.error(why);
        }
    }

    /// Closes the panel that is open, its filter and scroll with it.
    pub fn close_overlay(&mut self) {
        self.overlay = Overlay::None;
        self.overlay_scroll = 0;
        self.overlay_filter.clear();
        self.overlay_typing = false;
        self.overlay_sel = 0;
        self.overlay_pick = None;
    }

    /// Every annotation id with its step: taken before an action so
    /// [`App::add_step_hint`] can tell which ones it added or moved.
    fn annotation_steps(&self) -> HashMap<String, u32> {
        self.doc
            .annotations
            .iter()
            .map(|a| (a.id.clone(), a.step))
            .collect()
    }

    /// An action that added (or moved) annotations to a step after the one
    /// on screen — a new one at step 0 goes to step 1, `:step ID N`
    /// past the view — ends its confirmation saying where they went and how
    /// to get there: `a17: box 3-5 (step 1, press n)`. Only a new
    /// info message gets it; errors, an unchanged message and a message that
    /// already carries the hint are left alone. One that its own step does
    /// not draw (a link shows another file there) gets that reason instead,
    /// as an agent's change does: `m2 → step 3 (main.rs, not shown at step
    /// 3)`.
    fn add_step_hint(
        &mut self,
        before: &HashMap<String, u32>,
        prev_message: Option<&(String, bool)>,
    ) {
        let shown = self.doc.step;
        let changed = || {
            self.doc
                .annotations
                .iter()
                .filter(|a| before.get(&a.id) != Some(&a.step))
        };
        let not_drawn = changed()
            .filter_map(|a| self.doc.hidden_reason(&a.id, a.step).map(|w| (a.step, w)))
            .min_by_key(|(s, _)| *s);
        let target = changed().filter(|a| a.step > shown).map(|a| a.step).min();
        if not_drawn.is_none() && target.is_none() {
            return;
        }
        let Some((msg, false)) = &self.message else {
            return;
        };
        if prev_message.is_some_and(|p| p.0 == *msg) || step_hint_start(msg).is_some() {
            return;
        }
        let hint = match (not_drawn, target) {
            (Some((_, why)), _) => self.hidden_tail(&why, msg),
            (None, Some(target)) => step_hint(msg, target, shown),
            (None, None) => return,
        };
        if let Some((m, _)) = &mut self.message {
            m.push_str(&hint);
        }
    }

    /// The author's own screen at `width` (and the `status` warnings about
    /// it). Readers get the take instead ([`App::take_at`]).
    pub fn layout(&self, width: usize) -> Layout {
        self.lay_out(&self.doc, &self.theme, &self.layout_options(width))
    }

    /// The take's geometry computed at revision `from` still holds now
    /// (not one sized from code typed halfway).
    fn keep_geometry(&mut self, from: u64) {
        if let Some((k, _)) = self.geometry.get_mut()
            && k.revision == from
            && !k.typing
        {
            k.revision = self.revision;
        }
    }

    /// Lays `doc` (the open document, maybe with other options or a theme
    /// of its own) out, with the take's geometry computed once per change
    /// ([`App::geometry`]) when `opts` asks for a stable layout.
    fn lay_out(&self, doc: &Document, theme: &Theme, opts: &LayoutOptions) -> Layout {
        self.lay_out_traced(doc, theme, opts, false).0
    }

    /// [`App::lay_out`], with the record of the enter effects when `traced`.
    fn lay_out_traced(
        &self,
        doc: &Document,
        theme: &Theme,
        opts: &LayoutOptions,
        traced: bool,
    ) -> (Layout, Option<tiza_core::canvas::Trace>) {
        self.layouts.set(self.layouts.get() + 1);
        let run = |g: Option<&tiza_core::layout::StableGeometry>| {
            if traced {
                let (l, t) = tiza_core::layout::layout_traced(doc, theme, opts, g);
                (l, Some(t))
            } else {
                (tiza_core::layout::layout_with(doc, theme, opts, g), None)
            }
        };
        if !opts.stable {
            return run(None);
        }
        let key = GeometryKey {
            revision: self.revision,
            moves: doc.moves.len(),
            annotations: doc.annotations.len(),
            lines: doc.base().len(),
            line_numbers: opts.line_numbers,
            tab_width: opts.tab_width,
            typing: self.typing.is_some(),
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
        run(cache.as_ref().map(|(_, g)| g))
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

    /// The terminal width from which the screen lays the code out at
    /// `width` cells or more: the first number of columns that
    /// [`App::screen_width`] turns into `width`, its side padding taken
    /// off. What `code needs N cols` names.
    pub fn columns_for(&self, width: usize) -> usize {
        let rows = self.size.1;
        (width..=usize::from(u16::MAX))
            .find(|&cols| {
                let c = cols as u16;
                let (_, pad_h) =
                    crate::ui::clamp_padding(self.effective_padding(), (c, rows), self.present);
                usize::from(c.saturating_sub(2 * pad_h)) >= width
            })
            .unwrap_or(width)
    }

    /// The labels cut with `…` or left out at the screen's width (within
    /// the widths `check` takes), with that width: the take, the layout
    /// presentation mode, the exports and `check` draw, with the step as it
    /// is once typed. While authoring the screen reserves room for the
    /// steps so far only, so before the last step it may place a label
    /// otherwise; what the audience sees is the take.
    fn screen_cuts(&self) -> (usize, Vec<tiza_core::layout::ClippedLabel>) {
        let (width, layout) = self.take_at_screen();
        (width, layout.clipped_labels)
    }

    /// The take at the screen's width (within the widths `check` takes):
    /// what `visible` and `clipped` go by, and `render.text` without a width.
    fn take_at_screen(&self) -> (usize, Layout) {
        let width = self.take_width();
        (width, self.take_at(width))
    }

    /// The width of the take of the screen: [`App::screen_width`] within the
    /// widths `check` takes. The width of `render.text` and of `:export`
    /// without a width, and the `width` of `status`, which `tiza render` of
    /// this instance and `export_image` use.
    pub fn take_width(&self) -> usize {
        use tiza_core::layout::{MAX_WIDTH, MIN_WIDTH};
        self.screen_width().clamp(MIN_WIDTH, MAX_WIDTH)
    }

    /// The take at `width`: the layout of presentation mode, the exports and
    /// `check` (one geometry for every step), what `render.text` returns.
    /// During a typing animation, the step as it is once typed: the code on
    /// screen is half typed, and the step's annotations wait for its end.
    fn take_at(&self, width: usize) -> Layout {
        let mut opts = self.layout_options(width);
        opts.stable = true;
        opts.typing = false;
        match &self.typing {
            Some(t) => {
                let mut typed = self.doc.clone();
                typed.code = t.plan.full.clone();
                self.lay_out(&typed, &self.theme, &opts)
            }
            None => self.lay_out(&self.doc, &self.theme, &opts),
        }
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

    /// [`App::layout`] with the live pointer. Only the presenter's
    /// frame (`ui::draw`) calls it; `layout`, the take `render.text` returns
    /// and every export never draw the pointer. While an enter effect
    /// plays, it is the work layout of the effect: the frame paints the
    /// rows in view on it ([`ScreenLayout::paint_rows`]) and gives it back
    /// ([`App::put_back_screen`]). Its rows out of view can show other
    /// moments of the effect, and if a caller does not give it back, the
    /// next frame lays the document out again.
    pub fn screen_layout_with_pointer(&self, width: usize) -> ScreenLayout {
        let mut opts = self.layout_options(width);
        opts.pointer = self.pointer_shown();
        let l = match self.effect_frame(&opts) {
            Some(f) => ScreenLayout::Effect(f),
            None => ScreenLayout::Plain(self.lay_out(&self.doc, &self.theme, &opts)),
        };
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
            line_numbers: self.numbers(),
            tab_width: 4,
            typing: self.typing.is_some(),
            // Presentation mode is a take: one geometry for every step.
            // Authoring grows as it goes unless `:layout stable`.
            stable: self.present || self.layout_stable,
            margin_cap: self.doc.margin.map(usize::from),
        }
    }

    // ---------- live pointer ----------

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
            self.info(if matches!(self.mode, Mode::Select { .. }) {
                // Esc closes the selection here, not the pointer.
                "pointer on (f turns it off; never saved nor exported)"
            } else {
                "pointer on (f or Esc turns it off; never saved nor exported)"
            });
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

    /// `pointer.set`: resolves the lines on the code shown, moves
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

    /// What comes in at step `s` in the file shown there: the lines of its
    /// annotations that are drawn at `s`, and the lines that its moves write
    /// there (D (a)), in the window of a `code.link`. The first and the last
    /// line, or `None` when nothing comes in or a move of the step does not
    /// apply. One projection of the step for all of them.
    fn reveal_range(&self, s: u32) -> Option<(u32, u32)> {
        // A move of the step that does not apply: nothing comes in, as at
        // a step change.
        if !self.doc.moves.is_empty() && moves::project(&self.doc, &self.doc.moves, s).is_err() {
            return None;
        }
        let view = (!self.doc.moves.is_empty()).then(|| self.doc.view_at(s));
        // The projection keeps the annotations of the file shown, on their
        // lines at `s`; with no move, the main file is shown.
        let doc = view.as_ref().map_or(&self.doc, |v| &v.doc);
        let main = tiza_core::moves::ViewRef::default().file;
        let mut span: Option<(u32, u32)> = None;
        let mut add = |lo: u32, hi: u32| {
            span = Some(span.map_or((lo, hi), |(a, b)| (a.min(lo), b.max(hi))));
        };
        for a in doc.annotations.iter().filter(|a| a.step == s) {
            if view.is_none() && a.file != main {
                continue;
            }
            let ranges = a.shape.line_ranges();
            if ranges
                .iter()
                .any(|(f, t)| !doc.has_line(*f) || !doc.has_line(*t))
            {
                continue;
            }
            if let (Some(lo), Some(hi)) = (
                ranges.iter().map(|r| r.0).min(),
                ranges.iter().map(|r| r.1).max(),
            ) {
                add(lo, hi);
            }
        }
        if let Some((lo, hi)) = moves::written_lines(&self.doc, s) {
            let (lo, hi) = (lo.max(doc.source.first_line), hi.min(doc.last_line()));
            if lo <= hi {
                add(lo, hi);
            }
        }
        span
    }

    /// AC5 of presentation mode: what comes in at step `s` comes into the
    /// view with the least move (`show`), from the view that a `scroll` of
    /// before holds, or from where the presenter left it. When it does not
    /// fit, its start (C (a)). The hidden cursor goes to its first line, so
    /// `f` points at what came in; its column stays, as a step change keeps
    /// it. `true` when something comes in: then the caller decides the live
    /// pointer ([`App::settle_pointer`], F (a)).
    fn reveal(&mut self, s: u32) -> bool {
        let Some(range) = self.reveal_range(s) else {
            return false;
        };
        self.cursor_line = range.0;
        self.clamp_col();
        let at = (self.cursor_line, self.cursor_col);
        match self.scroll_held.as_mut() {
            Some(h) if h.from.is_some() => {
                h.reveal = Some(range);
                h.step = s;
                h.to_come = None;
                h.cursor = at;
            }
            _ => {
                self.scroll_held = Some(ScrollHeld {
                    lines: range,
                    align: tiza_core::moves::Align::Show,
                    annotation: None,
                    from: None,
                    reveal: None,
                    step: s,
                    cursor: at,
                    pointer: self.pointer,
                    to_come: None,
                    show_row: None,
                });
            }
        }
        true
    }

    /// Presentation mode when the TUI opens: it opens at its step as a step
    /// change reaches it, so what comes in at the step comes into the view
    /// (A (d)), unless a camera of the step places the view (B (a)).
    /// Presentation mode that starts later (`P`) is not a step change.
    pub fn reveal_at_open(&mut self) {
        let s = self.doc.step;
        if !self.present || s == 0 {
            return;
        }
        let own = !self.doc.moves.is_empty() && own_camera(&self.doc.view_at(s).camera, s);
        if !own {
            self.reveal(s);
        }
    }

    /// A step change: the view that a reveal holds is for its own step
    /// only. It goes, and the view stays where that step left it (the line
    /// on top of the last frame); the new step then places it.
    fn drop_stale_reveal(&mut self) {
        if self
            .scroll_held
            .as_ref()
            .is_some_and(|h| h.reveals() && h.step != self.doc.step)
        {
            self.scroll_held = None;
        }
    }

    /// Whether the rows of lines `a` to `b` are in a view whose first row is
    /// `top`, on the layout of the screen now.
    fn lines_in_view_from(&self, top: usize, a: u32, b: u32) -> bool {
        let layout = self.screen_layout_with_pointer(self.screen_width());
        let visible = self.screen_height();
        let shown = |l: u32| {
            layout
                .row_of_line
                .get(&l)
                .is_some_and(|&r| r >= top && r < top + visible)
        };
        shown(a) && shown(b)
    }

    /// The camera of the step is `cam`, a `scroll`: a new one (or any one
    /// with `jump`) holds the view on its target and puts the cursor on the
    /// first line of the target, or keeps it on a live pointer that stays;
    /// the same one keeps holding a held view on its target, which a patch
    /// may have moved. Before the effects of the step start, so that the
    /// pointer it turns off does not end them.
    fn scroll_camera(&mut self, cam: tiza_core::moves::ScrollCam, jump: bool) {
        let before = (self.cursor_line, self.cursor_col);
        if self.place_camera(cam, jump) {
            self.settle_pointer(before);
        }
    }

    /// [`App::scroll_camera`] without the decision on the live pointer:
    /// `true` when it holds the view on a new target, and the pointer is
    /// then to decide. A step change decides it once, after its reveal.
    fn place_camera(&mut self, cam: tiza_core::moves::ScrollCam, jump: bool) -> bool {
        if self.scroll_applied == Some(cam.id) && !jump {
            // Only the held view of this `scroll`: a reveal alone stays.
            match (&mut self.scroll_held, cam.lines) {
                (Some(h), Some(lines)) if h.from == Some(cam.id) => {
                    h.lines = lines;
                    h.align = cam.align;
                    h.annotation = cam.annotation.clone();
                    h.to_come = None;
                    h.pointer = self.pointer;
                    // A pointer that stayed keeps the cursor with it, and a
                    // reveal keeps it on what came in.
                    if self.pointer.is_none() && h.reveal.is_none() {
                        self.cursor_line = lines.0;
                        self.cursor_col = 0;
                    }
                    h.cursor = (self.cursor_line, self.cursor_col);
                }
                (Some(h), None) if h.from == Some(cam.id) => self.scroll_held = None,
                _ => {}
            }
            return false;
        }
        self.scroll_applied = Some(cam.id);
        self.scroll_step = self.doc.step;
        let Some(lines) = cam.lines else {
            self.scroll_held = None;
            return false;
        };
        self.cursor_line = lines.0;
        self.cursor_col = 0;
        self.scroll_held = Some(ScrollHeld {
            lines,
            align: cam.align,
            annotation: cam.annotation.clone(),
            from: Some(cam.id),
            reveal: None,
            step: self.doc.step,
            cursor: (lines.0, 0),
            pointer: self.pointer,
            to_come: None,
            show_row: None,
        });
        true
    }

    /// A key or a mouse event before the first frame of a step with a held
    /// view: the view of that step first ([`App::held_unresolved`]).
    pub fn resolve_pending_view(&mut self) {
        if std::mem::take(&mut self.held_unresolved) {
            self.resolve_held_view();
        }
    }

    /// The view that the held view (a `scroll` or a reveal) puts on screen,
    /// at once, as the next frame would: a key that comes before that frame
    /// starts from it, and a key that moves the cursor does not let the
    /// held view go before it is applied. One layout, only with a held view.
    fn resolve_held_view(&mut self) {
        // Before the first frame the size is not known: that frame does it.
        if self.scroll_held.is_none() || self.screen_height() == 0 {
            return;
        }
        let width = self.screen_width();
        let layout = self.screen_layout_with_pointer(width);
        let visible = self.screen_height();
        // From the view the next frame would show, as `ui::draw` does.
        let current = self
            .view_anchor
            .filter(|_| self.present)
            .and_then(|a| view_row_of(&layout, a))
            .unwrap_or(self.scroll);
        let to_come = self.held_rows_to_come(&layout, width);
        let Some(h) = self.scroll_held.as_mut() else {
            return;
        };
        if let Some(top) = h.scroll_on(&layout, (visible, width), current, to_come) {
            self.set_scroll(top);
        }
    }

    /// The live pointer at a jump or a reveal (D14): it stays only when the
    /// held view shows its lines, also once the rows to come are in; then
    /// the cursor stays with it (`before` is the cursor of before, for its
    /// column), so `j` and `k` go on from the pointer. If not, the pointer
    /// goes off and the cursor stays on the target. One layout, only with
    /// the pointer on.
    fn settle_pointer(&mut self, before: (u32, u32)) {
        let Some(p) = self.pointer else {
            return;
        };
        let width = self.screen_width();
        let layout = self.screen_layout_with_pointer(width);
        let visible = self.screen_height();
        // From the view the next frame would show, as `ui::draw` does.
        let current = self
            .view_anchor
            .filter(|_| self.present)
            .and_then(|a| view_row_of(&layout, a))
            .unwrap_or(self.scroll);
        let to_come = self.held_rows_to_come(&layout, width);
        let Some(h) = self.scroll_held.as_mut() else {
            return;
        };
        let Some(top) = h.scroll_on(&layout, (visible, width), current, to_come) else {
            return;
        };
        // Where each line will be once the rows to come are in: those above
        // the target or the reveal push the code above it up, those under
        // it push the code under it down.
        let ranges = [(Some(h.lines), to_come.target), (h.reveal, to_come.reveal)];
        let shown = |l: u32| {
            let Some(&r) = layout.row_of_line.get(&l) else {
                return false;
            };
            let mut r = r as isize;
            for (range, (above, under)) in ranges {
                match range {
                    Some((a, _)) if l < a => r -= above as isize,
                    Some((_, b)) if l > b => r += under as isize,
                    _ => {}
                }
            }
            usize::try_from(r).is_ok_and(|r| r >= top && r < top + visible)
        };
        let stays = shown(p.line) && shown(p.to);
        self.set_scroll(top);
        if stays {
            // The pointer stays, and the cursor with it.
            self.cursor_line = p.line;
            self.cursor_col = if (p.line..=p.to).contains(&before.0) {
                before.1
            } else {
                0
            };
        } else {
            self.pointer = None;
        }
        if let Some(h) = self.scroll_held.as_mut() {
            h.pointer = self.pointer;
            h.cursor = (self.cursor_line, self.cursor_col);
        }
    }

    /// The rows that a target (`lines`, showing `annotation` if any) does not
    /// have yet on `layout` and will have when all of the step is drawn:
    /// above its first line (the title of a box) and under its last one (a
    /// border, a label). They come with the annotations of the step that
    /// wait for its typing or its `wait`, and with an annotation of a later
    /// step that a `scroll` shows before its step. `(0, 0)` when nothing is
    /// to come, with no layout more. Rows that come in inside the target
    /// make it taller: those cannot keep their place.
    pub(crate) fn rows_to_come(
        &self,
        lines: (u32, u32),
        annotation: Option<&str>,
        layout: &tiza_core::layout::Layout,
        width: usize,
    ) -> (usize, usize) {
        let later = annotation.and_then(|id| {
            self.doc
                .annotations
                .iter()
                .position(|a| a.id == id && a.step > self.doc.step)
        });
        if self.typing.is_none() && later.is_none() {
            return (0, 0);
        }
        let mut opts = self.layout_options(width);
        opts.typing = false;
        let then = match later {
            None => self.lay_out(&self.doc, &self.theme, &opts),
            Some(k) => {
                // The annotation at this step, on the lines where it will
                // come in: its own lines are those of its step.
                let mut doc = self.doc.clone();
                let a = &mut doc.annotations[k];
                let Some(lo) = a.shape.line_ranges().iter().map(|r| r.0).min() else {
                    return (0, 0);
                };
                let delta = i64::from(lines.0) - i64::from(lo);
                let moved = |l: u32| -> Option<u32> {
                    let to = (i64::from(l) + delta).clamp(i64::from(lines.0), i64::from(lines.1));
                    u32::try_from(to).ok()
                };
                let Some(shape) = moves::shift_shape_by(&a.shape, &moved, &|l, _| moved(l)) else {
                    return (0, 0);
                };
                a.shape = shape;
                a.step = doc.step;
                self.lay_out(&doc, &self.theme, &opts)
            }
        };
        // As `ScrollHeld::scroll_on` measures a target ([`span_rows`]): the
        // rows over its first line and under its last one.
        let measure = |l: &tiza_core::layout::Layout| -> Option<(usize, usize)> {
            let (top, bottom) = span_rows(l, lines)?;
            let first = *l.row_of_line.get(&lines.0)?;
            let last = *l.row_of_line.get(&lines.1)?;
            Some((first - top, bottom - last))
        };
        match (measure(layout), measure(&then)) {
            (Some(now), Some(then)) => (then.0.saturating_sub(now.0), then.1.saturating_sub(now.1)),
            _ => (0, 0),
        }
    }

    /// What [`App::rows_to_come`] depends on, for [`ScrollHeld::to_come`].
    pub(crate) fn to_come_key(&self, width: usize) -> ToComeKey {
        (
            self.doc.step,
            width,
            self.typing.is_some(),
            self.history.newest(),
            self.present || self.layout_stable,
        )
    }

    /// [`App::rows_to_come`] for the target and the reveal of the held view,
    /// from its cache when what it depends on did not change.
    pub(crate) fn held_rows_to_come(
        &mut self,
        layout: &tiza_core::layout::Layout,
        width: usize,
    ) -> ToCome {
        let key = self.to_come_key(width);
        let Some(h) = &self.scroll_held else {
            return ToCome::default();
        };
        if let Some((k, v)) = h.to_come
            && k == key
        {
            return v;
        }
        let (lines, annotation, reveal) = (h.lines, h.annotation.clone(), h.reveal);
        let v = ToCome {
            target: self.rows_to_come(lines, annotation.as_deref(), layout, width),
            reveal: reveal.map_or((0, 0), |r| self.rows_to_come(r, None, layout, width)),
        };
        if let Some(h) = &mut self.scroll_held {
            h.to_come = Some((key, v));
        }
        v
    }

    /// Whether a `scroll` move runs at `step` or before it: then a `cursor`
    /// or a `code.link` that is newer places the view as before `scroll`,
    /// and the view does not go back to the top.
    fn scroll_runs_by(&self, step: u32) -> bool {
        self.doc
            .moves
            .iter()
            .any(|m| m.at_step <= step && matches!(m.action, moves::MoveAction::Scroll(_)))
    }

    /// The document or its step changed other than by a step change (a
    /// move added or deleted, an undo, the start): a `scroll` that is now
    /// the camera of the step and is not the last one applied jumps; the
    /// one applied keeps a held view on its target. Without a `scroll`
    /// camera nothing moves.
    fn camera_changed(&mut self) {
        self.camera_changed_with(true);
    }

    /// [`App::camera_changed`]; without `jump` (a move deleted), a camera
    /// that is now another `scroll` does not move the view: it counts as
    /// applied, and the view is not held.
    fn camera_changed_with(&mut self, jump: bool) {
        // An undo or an agent that changed the step.
        self.drop_stale_reveal();
        let any = |m: &moves::Move| matches!(m.action, moves::MoveAction::Scroll(_));
        if !self.doc.moves.iter().any(any)
            && self.scroll_applied.is_none()
            && self.scroll_held.is_none()
        {
            return;
        }
        // During a typing its cues move the view, at their place in the
        // list: a change now (an agent edits an annotation) does not.
        if self.typing.is_some() {
            return;
        }
        let cam = self.doc.view_at(self.doc.step).camera;
        match cam {
            Camera::Scroll(cam) if !jump && self.scroll_applied != Some(cam.id) => {
                self.scroll_applied = Some(cam.id);
                self.drop_camera_view();
            }
            Camera::Scroll(cam) => {
                self.scroll_camera(cam, false);
                self.clamp_cursor();
                self.pointer_seen = self.pointer_snapshot();
            }
            _ => {
                // Back to a step that no `scroll` reaches (no camera, or a
                // `cursor` or a `code.link`): the top, as a step change goes
                // there (not for a deleted move).
                if jump
                    && self.scroll_applied.is_some()
                    && self.scroll_step != self.doc.step
                    && !self.scroll_runs_by(self.doc.step)
                {
                    self.set_scroll(0);
                    self.cursor_line = self.doc.source.first_line;
                    self.cursor_col = 0;
                    // The pointer stays only where the top shows its lines,
                    // and the cursor with it (D14).
                    if let Some(p) = self.pointer {
                        if self.lines_in_view_from(0, p.line, p.to) {
                            self.cursor_line = p.line;
                        } else {
                            self.pointer = None;
                        }
                    }
                    self.pointer_seen = self.pointer_snapshot();
                }
                self.scroll_applied = None;
                self.drop_camera_view();
            }
        }
    }

    /// Lets the held view of a `scroll` go; the view of a reveal alone
    /// stays (an agent edit, a reload).
    fn drop_camera_view(&mut self) {
        if self.scroll_held.as_ref().is_some_and(|h| h.from.is_some()) {
            self.scroll_held = None;
        }
    }

    /// Puts the view at row `row`: in presentation mode too, the next frame
    /// shows it as it is, not where the code of the last frame was.
    fn set_scroll(&mut self, row: usize) {
        self.scroll = row;
        self.view_anchor = None;
    }

    /// A key of the view in presentation mode (the live pointer off): `rows`
    /// rows down, up when negative. The next frame moves the view, and
    /// the hidden cursor stays inside it.
    pub(crate) fn view_by(&mut self, rows: i64) {
        let r = self.view_request.get_or_insert_default();
        r.rows = r.rows.saturating_add(rows);
    }

    /// `g`/Home or `G`/End in presentation mode: the view goes to an end of
    /// the code in the next frame.
    pub(crate) fn view_to(&mut self, edge: ViewEdge) {
        self.view_request = Some(ViewRequest {
            edge: Some(edge),
            rows: 0,
        });
    }

    /// The move of the view that the next frame makes, if a key asked for one.
    pub fn view_request(&self) -> Option<ViewRequest> {
        self.view_request
    }

    /// The height the screen shows the code in: the terminal of the last
    /// frame less the status bar and the padding, as `ui::draw` does.
    pub fn screen_height(&self) -> usize {
        let (cols, rows) = self.size;
        if cols == 0 || rows == 0 {
            return 0;
        }
        let status_h = if self.present { 0 } else { 1 };
        let (pad_v, _) =
            crate::ui::clamp_padding(self.effective_padding(), (cols, rows), self.present);
        usize::from(rows.saturating_sub(status_h).saturating_sub(pad_v * 2))
    }

    /// A step change in presentation mode, before the moves of the new
    /// step place the view: the line on top of the view and the hidden
    /// cursor go to the new step through the patches in between (to the
    /// nearest line above when a patch removed theirs), so the view stays
    /// on its code. A `scroll`, a `cursor` or a `code.link` of the step, the
    /// pointer and the way back to the top then place the view and the
    /// cursor as before. `false` when the code of before is not on screen.
    fn carry_view(&mut self, from_step: u32) -> bool {
        let anchor = self.view_anchor;
        let mut lines = vec![self.cursor_line];
        lines.extend(anchor.map(|a| a.0));
        // Another file, or out of the window of a `code.link`: the code is
        // not on screen, and the view of before stays as it was.
        let Some(carried) = moves::carry_lines_near(&self.doc, from_step, self.doc.step, &lines)
            .filter(|c| c.iter().all(|l| self.doc.has_line(*l)))
        else {
            self.view_anchor = None;
            return false;
        };
        // Not a motion for the view to follow.
        self.cursor_line = carried[0];
        self.view_cursor_line = carried[0];
        if let (Some((_, rows)), Some(line)) = (anchor, carried.get(1)) {
            self.view_anchor = Some((*line, rows));
        }
        true
    }

    /// Presentation mode: the hidden cursor stays on a line of code in the
    /// rows `top..top + visible` of `layout`, so `f` points at a line the
    /// audience sees. From above it goes to the first line in view, from
    /// below to the last one. With no row of code in view (a callout taller
    /// than the view), to the line above, as a click there.
    pub(crate) fn keep_cursor_in_view(
        &mut self,
        layout: &tiza_core::layout::Layout,
        top: usize,
        visible: usize,
    ) {
        use tiza_core::layout::Row;
        let end = (top + visible).min(layout.rows.len());
        let code = |i: usize| match layout.rows[i] {
            Row::Code(l) => Some(l),
            _ => None,
        };
        let line = match layout.row_of_line.get(&self.cursor_line) {
            Some(&r) if r >= top && r < end => return,
            Some(&r) if r >= end => (top..end).rev().find_map(code),
            _ => (top..end).find_map(code),
        };
        let Some(line) = line.or_else(|| (0..top.min(layout.rows.len())).rev().find_map(code))
        else {
            return;
        };
        self.cursor_line = line;
        self.clamp_col();
        self.pointer_seen = self.pointer_snapshot();
    }

    /// A step change keeps the pointer on its line, carried through the
    /// moves of the steps in between; off when the line is gone.
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
    /// put` and read-only: ONE notice on the first failed
    /// autosave, then silence on the message line and the persistent
    /// `read-only • unsaved (:w)` on the bar; `:q` refuses while there are
    /// unsaved changes; the first autosave that works again says so.
    pub fn save(&mut self) -> Result<(), Error> {
        // A document that follows the terminal keeps following it: only a
        // fixed theme is written.
        if !self.theme_auto {
            self.doc.theme = Some(self.theme.name);
        }
        match fs::save_document(&self.doc, &self.save_path) {
            Ok(()) => {
                self.dirty = false;
                // The file holds only what this version knows now.
                self.not_known = None;
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
                        "read-only: could not save {}; :saveas PATH saves elsewhere",
                        self.save_path.display()
                    ));
                    self.save_warning_pending = true;
                }
                self.save_error = Some(e.to_string());
                Err(e)
            }
        }
    }

    /// The document as a `.tiza` at `path` holds it: the theme as `save`
    /// sets it, and every path it names relative to its folder rewritten
    /// to name the same file from `path`'s folder.
    fn rebased_copy(&self, path: &Path) -> Document {
        let mut d = self.doc.clone();
        if !self.theme_auto {
            d.theme = Some(self.theme.name);
        }
        let (from, to) = (fs::document_dir(&self.save_path), fs::document_dir(path));
        if from != to {
            d.map_files(&fs::rebase_fn(&self.doc, &from, &to));
        }
        d
    }

    /// Whether `path` names the file the document is saved to.
    fn is_save_path(&self, path: &Path) -> bool {
        let real = |p: &Path| p.canonicalize().or_else(|_| std::path::absolute(p)).ok();
        real(path).is_some_and(|p| Some(p) == real(&self.save_path))
    }

    /// A path typed in `:w`, `:saveas`, `:wq` or `:export`: as typed, which
    /// starts at the folder where Tiza started when it is relative; a
    /// leading `~` is the home folder, as in Vim. Without `HOME` (not set or
    /// empty) a `~` is an error, so that the file never goes to the current
    /// folder instead.
    fn typed_path(rest: &str) -> Result<PathBuf, Error> {
        let home = || {
            std::env::var_os("HOME")
                .filter(|h| !h.is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| {
                    Error::Invalid(format!(
                        "{rest}: ~ needs HOME, and HOME is empty or not set. Give the full path"
                    ))
                })
        };
        match rest.strip_prefix('~') {
            Some("") => home(),
            Some(tail) if tail.starts_with('/') => home().map(|h| h.join(&tail[1..])),
            _ => Ok(PathBuf::from(rest)),
        }
    }

    /// `path` in full, for a message that says where a file went.
    fn full_path(path: &Path) -> String {
        std::path::absolute(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .display()
            .to_string()
    }

    /// Creates the folders `path` needs, naming the one that cannot be
    /// created.
    fn make_parent(path: &Path) -> Result<(), Error> {
        match path.parent() {
            Some(dir) if !dir.as_os_str().is_empty() => std::fs::create_dir_all(dir).map_err(|e| {
                Error::Invalid(format!(
                    "could not create the folder {}: {e}",
                    Self::full_path(dir)
                ))
            }),
            _ => Ok(()),
        }
    }

    fn refuse_existing(path: &Path, force: bool) -> Result<(), Error> {
        if path.is_dir() || path.as_os_str().to_string_lossy().ends_with('/') {
            return Err(Error::Invalid(format!(
                "{} is a folder: name the file (…/copy.tiza)",
                path.display()
            )));
        }
        if path.exists() && !force {
            return Err(Error::Invalid(format!(
                "{} exists (add ! to overwrite)",
                path.display()
            )));
        }
        Ok(())
    }

    /// `:w PATH`: a copy of the document at `path`, its paths rewritten for
    /// that folder; the document stays saved where it was. `path` naming
    /// the open file is a plain save.
    fn write_copy(&mut self, path: &Path, force: bool) -> Result<String, Error> {
        if self.is_save_path(path) {
            self.save()?;
            return Ok(format!("saved to {}", self.save_path.display()));
        }
        Self::refuse_existing(path, force)?;
        Self::make_parent(path)?;
        fs::save_document(&self.rebased_copy(path), path)?;
        Ok(format!(
            "wrote a copy to {} ({} is still the file open)",
            Self::full_path(path),
            self.save_path.display()
        ))
    }

    /// `:saveas PATH`: the document is written to `path` and saved there
    /// from then on; the paths it names, in memory and in the undo history,
    /// are rewritten for that folder once the file is written.
    fn save_as(&mut self, rest: &str, force: bool) -> Result<String, Error> {
        if rest.is_empty() {
            return Err(Error::Invalid(
                "usage: :saveas PATH   (saves there from now on; ! overwrites an existing file)"
                    .into(),
            ));
        }
        let path = Self::typed_path(rest)?;
        if self.is_save_path(&path) {
            self.save()?;
            return Ok(format!("saved to {}", self.save_path.display()));
        }
        Self::refuse_existing(&path, force)?;
        Self::make_parent(&path)?;
        fs::save_document(&self.rebased_copy(&path), &path)?;
        let (from, to) = (fs::document_dir(&self.save_path), fs::document_dir(&path));
        if from != to {
            let f = fs::rebase_fn(&self.doc, &from, &to);
            self.doc.map_files(&f);
            self.history.map_files(&f);
            if let Some(tiza_core::model::CodeSource::File { file, .. }) = &mut self.reference {
                *file = f(file);
            }
            self.frozen_views = self.frozen_views.iter().map(|v| f(v)).collect();
            self.revision += 1;
        }
        self.save_path = path;
        crate::note_save_path(&self.save_path);
        // The document has a file of its own now. With `--no-save` what
        // stayed safe was the file opened, not this copy: every change
        // saves here from now on.
        if self.no_save {
            self.no_save = false;
            self.autosave = true;
        }
        // The same text again, through the one door: `dirty`, what quitting
        // compares with, and a read-only state left behind.
        self.save()?;
        // Short, with the words of `:autosave`: at 100 columns it fits
        // whole next to a warning badge.
        Ok(format!(
            "saved as {} (autosave {})",
            Self::full_path(&self.save_path),
            if self.autosave { "on" } else { "off" }
        ))
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
        // The code that a follow pause embedded is no entry of the history:
        // it is named first (a long list ends with `and N more`) while it is
        // on screen and the sidecar does not hold it.
        if let Some(label) = self.pause_code_unsaved() {
            labels.insert(0, label);
        }
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

    /// `the code on screen in place of main.rs`: the code that a follow
    /// pause embedded over its file is still on screen (a change waits, or
    /// the file went missing, could not be read, or follow is off), and the
    /// sidecar on disk does not hold it.
    fn pause_code_unsaved(&self) -> Option<String> {
        let embedded = self.pause_embedded.as_ref()?;
        if !self.doc.is_embedded() || self.doc.base() != embedded.as_slice() {
            return None;
        }
        let file = &self.doc.follows.as_ref()?.file;
        let name = Path::new(file)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| file.clone());
        let code = |json: &str| {
            serde_json::from_str::<serde_json::Value>(json)
                .map(|v| v["code"].clone())
                .ok()
        };
        let pending = code(self.pending_json()?.as_str())?;
        (self.saved_json.as_deref().and_then(code) != Some(pending))
            .then(|| format!("the code on screen in place of {name}"))
    }

    /// Quits unless there are unsaved changes: ones that could not be
    /// written, or ones made with autosave off (`force` ignores both), or
    /// stale moves (their marks are not saved).
    pub fn request_quit(&mut self, force: bool) {
        if !force && self.dirty {
            if self.save_error.is_some() {
                self.error(format!(
                    "unsaved (read-only): :q! quits without saving, :saveas PATH saves elsewhere ({})",
                    self.save_path.display()
                ));
                return;
            }
            if !self.autosave && self.no_save {
                self.error(
                    "unsaved changes (--no-save): :saveas PATH saves them elsewhere, :q! discards them",
                );
                return;
            }
            if !self.autosave {
                self.error(format!(
                    "unsaved changes (autosave off): :w saves to {}, :saveas PATH saves elsewhere, :q! quits without saving",
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
        self.add_annotation_full(id, step, color, None, None, None, shape)
    }

    /// `add_annotation_colored` plus the range markers a box/callout/spotlight
    /// was created from.
    #[allow(clippy::too_many_arguments)]
    pub fn add_annotation_full(
        &mut self,
        id: Option<String>,
        step: Option<u32>,
        color: Option<String>,
        markers: Option<tiza_core::model::RangeMarkers>,
        file: Option<String>,
        effect: Option<Effect>,
        shape: Shape,
    ) -> Result<String, Error> {
        if let Some(c) = &color
            && self.theme.named_color(c).is_none()
        {
            return Err(Error::Invalid(unknown_color()));
        }
        if effect == Some(Effect::Draw) && !shape.has_stroke() {
            return Err(Error::Invalid(DRAW_NEEDS_A_STROKE.into()));
        }
        // A mark's inline style color too: checked like the
        // annotation's, without repeating it.
        if let Shape::Mark { style, .. } = &shape
            && let Some(c) = &style.color
            && self.theme.named_color(c).is_none()
        {
            return Err(Error::Invalid(format!("style: {}", unknown_color())));
        }
        self.history.record(&self.doc, "add");
        // A personal style comes into the document with its first mark, in
        // the same undo step.
        let adopted = match &shape {
            Shape::Mark { kind, .. } => {
                Self::adopt_personal_style(&mut self.doc, &self.personal_styles, kind)
            }
            _ => None,
        };
        let step = step.unwrap_or(self.doc.step).max(1);
        let kind = shape.kind_name();
        let res = self.doc.add(Annotation {
            id: id.unwrap_or_default(),
            step,
            color,
            effect,
            markers,
            file,
            shape,
        });
        match res {
            Ok(id) => {
                self.history.label_last(format!("{kind} {id}"));
                // A `scroll` can name the annotation that comes in now.
                self.camera_changed();
                self.touch();
                Ok(id)
            }
            Err(e) => {
                self.history.discard_last();
                if let Some(name) = adopted {
                    self.doc.styles.remove(&name);
                }
                Err(e)
            }
        }
    }

    pub fn delete_annotation(&mut self, id: &str) -> Result<(), Error> {
        self.history.record(&self.doc, format!("delete {id}"));
        match self.doc.delete(id) {
            Ok(_) => {
                self.camera_changed();
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
        self.camera_changed();
        self.touch();
        removed
    }

    /// A file an annotation names, as a request gives it: an alias of a
    /// linked file is that file, as on load.
    fn canonical_file(&self, f: &str) -> String {
        let base = self.save_path.parent().map(Path::to_path_buf);
        let mut known = self.doc.link_files();
        known.extend(self.doc.view_code.keys().cloned());
        fs::canonical_link(&self.doc, f, base.as_deref(), &known)
    }

    /// Changes what the request gives and keeps the rest. Without `file`
    /// the annotation keeps its file, and without `step` its step.
    pub fn update_annotation(&mut self, p: &UpdateParams) -> Result<(), Error> {
        self.update_annotation_and_text(p, None)
    }

    /// [`App::update_annotation`], and with `callout_text` (`:set ID
    /// text=…` on a callout) the paragraph of that callout too, in the same
    /// change: one undo step. `label` is the title of a callout.
    fn update_annotation_and_text(
        &mut self,
        p: &UpdateParams,
        callout_text: Option<&str>,
    ) -> Result<(), Error> {
        let mut ann = self
            .doc
            .find_annotation(&p.id)
            .cloned()
            .ok_or_else(|| Error::NotFound(p.id.clone()))?;
        if let Some(s) = p.step {
            ann.step = s.max(1);
        }
        if let Some(f) = &p.file {
            ann.file = f.as_deref().map(|f| self.canonical_file(f));
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
        if let Some(t) = callout_text {
            // `:set` gives it only for a callout, and refuses an empty value.
            let Shape::Callout { text, .. } = &mut ann.shape else {
                unreachable!("a paragraph for {}, which is not a callout", p.id);
            };
            // Trimmed, as `:callout` takes it.
            *text = t.trim().to_string();
        }
        if let Some(e) = p.effect {
            ann.effect = e;
        }
        // `draw` needs a stroke: checked when the update sets the effect or
        // changes the shape. An annotation from a file keeps its effect
        // through the other updates.
        if (p.effect.is_some() || p.shape.is_some() || p.emphasis.is_some())
            && ann.effect == Some(Effect::Draw)
            && !ann.shape.has_stroke()
        {
            return Err(Error::Invalid(DRAW_NEEDS_A_STROKE.into()));
        }
        // Nothing to change: no undo entry, no new revision, no save.
        if self.doc.find_annotation(&p.id) == Some(&ann) {
            return Ok(());
        }
        self.history.record(&self.doc, format!("update {}", p.id));
        let adopted = match &ann.shape {
            Shape::Mark { kind, .. } => {
                Self::adopt_personal_style(&mut self.doc, &self.personal_styles, kind)
            }
            _ => None,
        };
        match self.doc.update(ann) {
            Ok(()) => {
                self.camera_changed();
                self.touch();
                Ok(())
            }
            Err(e) => {
                self.history.discard_last();
                if let Some(name) = adopted {
                    self.doc.styles.remove(&name);
                }
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
        // Its numbers first: a first line near `u32::MAX` or a step
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
        // Following over embedded code (a change that waits in a pause, or
        // a file that went missing or cannot be read meanwhile), `follows`
        // names the file that follow reads next ([`App::paused`]): no put
        // points it at another file. The same file written another way
        // (`./main.rs`, its absolute path) is the same file, as for follow.
        if !other
            && self.follow.is_some()
            && let Some(tiza_core::model::CodeSource::File { file, .. }) = self.paused()
            && let Some(sent) = &doc.follows
            && !self.follows_the_followed_file(sent)
        {
            return Err(Error::Invalid(format!(
                "scene.put: follow reads {file} over the code on screen: a put cannot change `follows` while follow is on. Send `follows` as scene.get gives it, or leave it out"
            )));
        }
        if !self.doc.is_embedded() {
            self.reference = Some(self.doc.code_src.clone());
        }
        let sent = if other {
            SentCode::default()
        } else {
            self.keep_references(&mut doc, base.as_deref())?
        };
        // Linked files: embedded in `views`, else read next to the
        // open document.
        fs::load_views(&mut doc, base.as_deref())?;
        // Marks that use a personal style bring it into the document.
        Self::adopt_personal_styles(&mut doc, &self.personal_styles);
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
        // What `:follow on` reloads is the document's own `follows`; a put of
        // the open document without one keeps the file it knows (`:follow
        // on` still reloads it), another document follows nothing the open
        // one did.
        if doc.follows.is_none() && !other {
            doc.follows = self.doc.follows.clone();
        }
        if self.same_document(&doc) {
            return Ok(self.scene_changed(Vec::new(), Vec::new(), Vec::new()));
        }
        self.finish_typing();
        // Saving rule: a document from another source must
        // never be autosaved over the file that was open. Autosave stops;
        // `:w` writes it over that file on purpose, `:saveas PATH` elsewhere,
        // `:autosave on` resumes. The same source (an agent re-putting an
        // edited copy of the open document, `scene get` → `scene put`)
        // keeps saving as usual, and the files it reads stay references
        // ([`App::keep_references`]); code sent different from those
        // files is shown but not saved over them.
        self.history.record_with_step(&self.doc, "scene.put");
        self.doc = doc;
        self.screen_via_history = false;
        self.step_ready_at = Some(std::time::Instant::now());
        match self.doc.theme {
            Some(t) => self.set_theme_name(t, true),
            None => self.rebuild_theme(),
        }
        self.selected = None;
        self.focus = None;
        self.set_scroll(0);
        self.clamp_cursor();
        // Another document: its camera, as when it opens.
        self.scroll_applied = None;
        self.scroll_held = None;
        self.camera_changed();
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
                "another document loaded: autosave off (:w saves over {}, :saveas PATH saves elsewhere, :autosave on resumes)",
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
                        "not saved, and autosave is off: {} (:w saves it embedded in {}, :saveas PATH saves elsewhere, :autosave on resumes)",
                        sent.edited.join("; "),
                        self.save_path.display()
                    )
                } else {
                    format!("not saved: {} (shown embedded)", sent.edited.join("; "))
                };
            }
            if !sent.stale.is_empty() {
                let stale = if sent.stale.len() == 1 {
                    format!(
                        "{} changed on disk since it was loaded: still read from it; the screen keeps the copy loaded (:follow on reloads it)",
                        sent.stale[0]
                    )
                } else {
                    format!(
                        "{} changed on disk since they were loaded: still read from them; the screen keeps the copies loaded (:follow on reloads them)",
                        sent.stale.join(", ")
                    )
                };
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
        self.settle_pause();
        Ok(self.scene_changed(Vec::new(), Vec::new(), Vec::new()))
    }

    /// Whether `sent` names the file that follow reads, in any spelling
    /// (resolved next to the document, as [`crate::follow::Follow::same_file`]
    /// matches it), with the same `lines`.
    fn follows_the_followed_file(&self, sent: &tiza_core::model::FileRef) -> bool {
        let Some(f) = &self.follow else {
            return false;
        };
        let same_lines = self
            .doc
            .follows
            .as_ref()
            .is_some_and(|o| o.lines == sent.lines);
        same_lines
            && fs::code_source_file(&sent.clone().into(), self.save_path.parent())
                .is_some_and(|p| f.same_file(&p))
    }

    /// After a change of the moves or the step that did not go through a
    /// step change (a put, a move added or deleted) under a follow pause:
    /// with no patch applied any more, the change that waits loads now, as
    /// a step change loads it before its reply; still paused, the notice
    /// names the step where it loads now.
    fn settle_pause(&mut self) {
        if self.reload_waiting() {
            return;
        }
        let (Some(old), Some(at), Some(f)) =
            (self.pause.notice.clone(), self.resume_step(), &self.follow)
        else {
            return;
        };
        let notice = format!("step {at} loads the change to {}", f.file_name());
        if notice == old {
            return;
        }
        if self
            .message
            .as_ref()
            .is_some_and(|(m, error)| !error && *m == old)
        {
            self.info(notice.clone());
        }
        self.pause.notice = Some(notice);
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
        // While follow is paused, the screen keeps its code and a change of
        // the file waits for the resume step. The code on screen comes back
        // as the open document holds it: a reference, or embedded over the
        // change that waits (which still loads there). Other code would load
        // a change at a step with a patch: refused, and while a change waits
        // the reason is that the file is not the code on screen (a save, or
        // a paste before `:follow on`). A document opened embedded over the
        // file it follows has no `reference`: its `follows` names the file.
        let mut kept = false;
        if self.follow_paused()
            && let Some(CodeSource::File { file, .. }) =
                self.reference.clone().or_else(|| self.paused())
            && let CodeSource::Embedded(lines) = &doc.code_src
        {
            let loaded = self.doc.base() == lines.as_slice()
                && self.doc.base_first() == doc.source.first_line;
            if loaded {
                doc.code_src = self.doc.code_src.clone();
                kept = true;
            } else if self.follow_pending() {
                let at = self.resume_step().unwrap_or(0);
                let when = if at == 0 {
                    "at step 0".to_string()
                } else {
                    format!("at step {at} or earlier")
                };
                return Err(Error::Invalid(format!(
                    "scene.put: follow is paused, and the code on screen is not what {file} holds now: {file} loads {when}. Send the code as scene.get gives it; to change the code, edit {file}"
                )));
            }
        }
        if let Some(reference @ CodeSource::File { file, .. }) = &self.reference
            && !kept
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
                let mut ann = <Annotation as serde::Deserialize>::deserialize(&v).map_err(|e| {
                    let msg = e.to_string();
                    let hint = tiza_core::error::unknown_field_hint(&msg)
                        .map(|h| format!(". {h}"))
                        .unwrap_or_default();
                    Error::Invalid(format!(
                        "annotation {id}: {}{hint}",
                        tiza_core::error::serde_text(&msg)
                    ))
                })?;
                // An alias of a linked file is that file, as in
                // `annotation.add` and `annotation.update`.
                if let Some(f) = &ann.file {
                    ann.file = Some(self.canonical_file(f));
                }
                let old = d.find_annotation(&id).map(|a| (a.step, a.file.clone()));
                d.annotations.retain(|a| a.id != id);
                if let Shape::Mark { kind, .. } = &ann.shape {
                    Self::adopt_personal_style(&mut d, &self.personal_styles, kind);
                }
                // An id the document has keeps the step it is sent with
                // and, without a `step` or a `file` key, its step or its file
                // (`file: null` is the main one). Only a new id gets the
                // defaults of a new annotation.
                if let Some((old_step, old_file)) = old {
                    if v.get("step").is_none() {
                        ann.step = old_step;
                    }
                    if v.get("file").is_none() {
                        ann.file = old_file;
                    }
                    d.settle_file(&mut ann)?;
                    d.add_settled(ann)?;
                } else {
                    d.add(ann)?;
                }
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
            d.numbers = Some(n);
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
        // Nothing to change: no undo entry, no new revision, no save.
        if self.same_document(&d) {
            return Ok(self.scene_changed(added, replaced, deleted));
        }
        self.history.record_with_step(&self.doc, "scene.patch");
        // A new `auto_advance_ms` counts from now, not from the arrival.
        if d.steps != self.doc.steps && self.typing.is_none() {
            self.step_ready_at = Some(std::time::Instant::now());
        }
        let stepped = d.step != self.doc.step;
        let from_step = self.doc.step;
        self.doc = d;
        // Another step: its code, as a step change shows it (a typing in
        // progress ends; the code shown is the new step's).
        if stepped {
            self.finish_typing();
            if let Some(e) = self.doc.sync_code_to_step() {
                self.error(format!("moves: {e}"));
                self.view_anchor = None;
            } else if self.present {
                self.carry_view(from_step);
            }
        }
        match self.doc.theme {
            Some(t) => self.set_theme_name(t, true),
            None => self.rebuild_theme(),
        }
        self.selected = None;
        self.clamp_cursor();
        self.camera_changed();
        self.touch();
        self.settle_pause();
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
        self.step_label_line()
    }

    /// [`App::step_message`] for `:step N`: to the step on screen, as a
    /// goto there ([`App::step_to_asked`]).
    fn step_message_asked(&mut self, step: u32) -> String {
        if self.step_to_asked(step) {
            return String::new();
        }
        self.step_label_line()
    }

    /// `:step N` and `step.set`: [`App::step_to`]; in presentation mode, to
    /// the step on screen, as a goto there: its camera again, and what
    /// comes in at it. Normal mode does not change.
    fn step_to_asked(&mut self, step: u32) -> bool {
        // On the empty step after the last one, that step or a later one:
        // it stays there (a step change would go back to the last step).
        if self.doc.step > self.doc.max_step() && step >= self.doc.step {
            return false;
        }
        let here = self.present && step.min(self.doc.max_step().max(1)) == self.doc.step;
        self.step_to_with(step, here)
    }

    /// The step label for a message: the title's control characters as
    /// their pictures, as the status bar draws them.
    fn step_label_line(&self) -> String {
        tiza_core::printable::printable_line(&self.doc.step_label(self.doc.step)).into_owned()
    }

    pub fn set_step(&mut self, step: u32) {
        self.step_to(step);
    }

    /// `n`, PageDown (`→` in presentation mode), `:next`, `step.next` and
    /// the auto-advance: one step on, or
    /// back to step 0 past the last one when the document loops.
    pub fn next_step(&mut self) -> bool {
        match self.doc.next_step_from(self.doc.step) {
            Some(0) => self.step_to_with(0, true),
            Some(n) => self.step_to(n),
            // At the empty step after the last one: nothing moves (a step
            // change would go back to the last step).
            None if self.doc.step > self.doc.max_step() => false,
            // At the last step: as before, nothing moves.
            None => self.step_to(self.doc.step + 1),
        }
    }

    /// `step.goto`, MCP `goto_step`, `tiza goto ID`, `:goto ID`:
    /// the step a named id (or a number) names, landed on exactly as `n`
    /// would after a full replay — the document is recomputed from step 0
    /// (moves, views), the typing of that step complete. Returns
    /// the step.
    pub fn goto_step(&mut self, id: &str) -> Result<u32, Error> {
        let s = self.doc.resolve_step(id)?;
        self.step_to_with(s, true);
        Ok(self.doc.step)
    }

    /// The `auto_advance_ms` countdown: when the current step has
    /// one and has been ready that long, go to the next step. Deferred while
    /// the command line, a `v`/`V` selection or an overlay is open (it fires
    /// once they close, so a selection is never lost under the presenter),
    /// never while a typed move or a `wait` is still running. `true` when
    /// the screen changed.
    pub fn auto_advance_tick(&mut self, now: std::time::Instant) -> bool {
        if self.presenter_busy() {
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
        // Leaving the step: its `scroll` moves left do not move the view.
        let to = step.min(self.doc.max_step().max(1));
        self.finish_typing_with(to == self.doc.step);
        // An effect that plays ends here, and the auto-advance countdown
        // starts again, also when the step does not change.
        self.finish_effect();
        let max = self.doc.max_step();
        let s = step.min(max.max(1));
        // A goto to the step on screen puts the view back on its target,
        // and in presentation mode shows what comes in at the step, as a
        // full replay ends.
        if s == self.doc.step && jump {
            let v = (!self.doc.moves.is_empty()).then(|| self.doc.view_at(s));
            let camera = v.as_ref().map_or(Camera::None, |v| v.camera.clone());
            let own = own_camera(&camera, s);
            // The live pointer is decided once, on the view of the camera
            // and the reveal together (D14).
            let before = (self.cursor_line, self.cursor_col);
            let mut settle = false;
            match (camera, &v) {
                (Camera::Scroll(cam), _) => settle |= self.place_camera(cam, true),
                // A `cursor` or a `code.link`: the cursor goes there, and
                // the view with it, as when the presenter comes to the step
                // (the view of the step on screen stays on its code). A
                // live pointer keeps the cursor on its line, as then.
                (camera @ Camera::Cursor { .. }, Some(v))
                    if self.cursor_camera_places(&camera, s, true) =>
                {
                    self.scroll_held = None;
                    self.place_step_cursor(v);
                    self.carry_pointer(s);
                }
                _ => {}
            }
            if self.present && s != 0 && !own {
                settle |= self.reveal(s);
            }
            if settle {
                self.settle_pointer(before);
            }
            self.clamp_cursor();
            self.pointer_seen = self.pointer_snapshot();
        }
        if s != self.doc.step {
            let forward_one = s == self.doc.step + 1 && !jump;
            let from_step = self.doc.step;
            self.doc.step = s;
            // The reveal of the step of before lets go.
            self.drop_stale_reveal();
            if let Some(e) = self.doc.sync_code_to_step() {
                self.error(format!("moves: {e}"));
                self.pointer = None;
                // The view of before, as it was.
                self.view_anchor = None;
                loud = true;
            } else {
                // In presentation mode the view and the hidden cursor go
                // with their code first; the moves of the step can then
                // place them.
                let carried = self.present && self.carry_view(from_step);
                // A `cursor` move left at this step moves the TUI cursor
                // too; a `code.link` puts it on the first shown line. A
                // `scroll` newer than both places the view instead.
                let mut scroll = None;
                // A camera of the step itself: it places the view, and
                // nothing is revealed (B (a)).
                let mut own = false;
                let mut went_top = false;
                if !self.doc.moves.is_empty() {
                    let v = self.doc.view_at(s);
                    own = own_camera(&v.camera, s);
                    if let Camera::Scroll(cam) = v.camera {
                        scroll = Some(cam);
                    } else {
                        let places = self.cursor_camera_places(&v.camera, s, carried);
                        if self.scroll_applied.is_some() && !self.scroll_runs_by(s) {
                            went_top = true;
                            // Back to a step that no `scroll` reaches: the
                            // top. A `cursor` or a `code.link` of before then
                            // moves the view only when its line is not in view.
                            self.set_scroll(0);
                            self.cursor_line = self.doc.source.first_line;
                            self.cursor_col = 0;
                        }
                        self.scroll_applied = None;
                        self.scroll_held = None;
                        if places {
                            self.place_step_cursor(&v);
                        }
                    }
                }
                // The cursor that the moves left, for when the pointer goes.
                let placed = (self.cursor_line, self.cursor_col);
                // Before the typing blanks what it is about to type.
                self.carry_pointer(from_step);
                // A step that types moves the view at its cues, in the order
                // of its list ([`App::apply_cues`]).
                let plan = forward_one
                    .then(|| moves::typing_plan(&self.doc, s))
                    .flatten();
                // After the pointer: the cursor goes to the target, and then
                // to what comes in.
                let before = (self.cursor_line, self.cursor_col);
                let mut settle = false;
                if let Some(cam) = scroll
                    && !plan.as_ref().is_some_and(|p| !p.cues.is_empty())
                {
                    settle |= self.place_camera(cam, jump);
                }
                if let Some(plan) = plan {
                    self.start_typing_with(plan);
                }
                // AC5 (A (d)): in presentation mode a step change shows what
                // comes in at the step, except one step back and the way
                // back to step 0.
                if self.present && s != 0 && s + 1 != from_step && !own {
                    settle |= self.reveal(s);
                }
                // The live pointer stays only where the new view shows its
                // lines, and the cursor with it (D14): decided once, on the
                // view of the camera and the reveal together. Back at the
                // top, on the top.
                let pointer_on = self.pointer.is_some();
                if settle {
                    self.settle_pointer(before);
                } else if went_top
                    && let Some(p) = self.pointer
                    && !self.lines_in_view_from(0, p.line, p.to)
                {
                    self.pointer = None;
                    (self.cursor_line, self.cursor_col) = placed;
                }
                // A key in the same turn of the loop, before the next frame,
                // starts from the view of the new step: it resolves the held
                // view first (`settle_pointer` already did, with the
                // pointer). The frame does it at no cost otherwise.
                self.held_unresolved = self.scroll_held.is_some() && !(settle && pointer_on);
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
            // Forward one step, also with a goto: the enter effects of the
            // step, after its typing. A goto of more than one step, a step
            // back and the loop show the final drawing.
            if !loud && s == from_step + 1 && self.typing.is_none() {
                self.start_effect();
            }
            // Its notice stays on the bar over the step's label.
            if self.reload_waiting() {
                loud = true;
            }
        }
        loud
    }

    /// Whether the `cursor` or `code.link` camera of step `s` places the
    /// cursor. N (a): in presentation mode an older `cursor` or `code.link`
    /// places the view only at its own step, or when the view cannot stay on
    /// its code (`carried` is `false`: another file, out of the window).
    /// Normal mode places it at each step, as before.
    fn cursor_camera_places(&self, camera: &Camera, s: u32, carried: bool) -> bool {
        match camera {
            Camera::Cursor { at_step, .. } => !self.present || *at_step == s || !carried,
            _ => true,
        }
    }

    /// The cursor that the `cursor` or `code.link` camera of a step puts:
    /// the line of the `cursor` move, or the first line that the
    /// `code.link` shows, also when it goes back to the main file with no
    /// window. A `cursor` whose line a later patch removed puts nothing.
    fn place_step_cursor(&mut self, v: &tiza_core::model::View) {
        let link = matches!(v.camera, Camera::Cursor { link: true, .. });
        if let Some(c) = v.cursor {
            self.cursor_line = c.line;
            self.cursor_col = c.col;
        } else if link || v.view.file.is_some() || v.view.window.is_some() {
            self.cursor_line = self.doc.source.first_line;
        }
    }

    // ---------- moves ----------

    /// Starts the typing animation of the current step, when any of its
    /// moves asks for one. The content of those moves is blanked out and
    /// revealed by `typing_tick`.
    fn start_typing_with(&mut self, plan: moves::TypingPlan) {
        self.typing = Some(TypingState {
            plan,
            index: 0,
            revealed: 0,
            cue: 0,
            then_effect: true,
            last: std::time::Instant::now(),
        });
        self.refresh_typing_code();
        self.apply_cues(0);
    }

    /// Moves the view to each `scroll` of the typing that comes before
    /// item `upto` and has not moved it yet; they take no time.
    fn apply_cues(&mut self, upto: usize) {
        let Some(t) = &mut self.typing else { return };
        let mut cameras = Vec::new();
        while let Some(cue) = t.plan.cues.get(t.cue)
            && cue.before <= upto
        {
            cameras.push(cue.camera.clone());
            t.cue += 1;
        }
        for cam in cameras {
            self.scroll_camera(cam, false);
            self.clamp_cursor();
            self.pointer_seen = self.pointer_snapshot();
        }
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
        self.typing_tick_at(std::time::Instant::now())
    }

    /// [`App::typing_tick`] at `now`: a test gives the time, so that a slow
    /// machine does not end the typing between two ticks.
    pub fn typing_tick_at(&mut self, now: std::time::Instant) -> bool {
        let Some(t) = &mut self.typing else {
            return false;
        };
        let elapsed = now.saturating_duration_since(t.last);
        let item = &t.plan.items[t.index];
        let speed = std::time::Duration::from_millis(item.speed_ms);
        let units = (elapsed.as_micros() / speed.as_micros().max(1)) as usize;
        if units == 0 {
            return false;
        }
        t.last += speed * units as u32;
        t.revealed += units;
        let mut done = false;
        while t.revealed >= t.plan.items[t.index].units {
            t.revealed -= t.plan.items[t.index].units;
            t.index += 1;
            if t.index >= t.plan.items.len() {
                done = true;
                break;
            }
        }
        let (index, then_effect) = (t.index, t.then_effect);
        // The `scroll` moves that come before the item typed now, or after
        // the last one: before the effects start.
        self.apply_cues(index);
        if done {
            self.finish_typing();
            self.revision += 1;
            // The typing is over: the effects of the step come next.
            if then_effect {
                self.start_effect();
            }
            return true;
        }
        self.refresh_typing_code();
        self.revision += 1;
        true
    }

    /// Ends the typing animation at once (any key, `p`, a socket step change).
    pub fn finish_typing(&mut self) {
        self.finish_typing_with(true);
    }

    /// [`App::finish_typing`]; without `cues` (the step changes), the
    /// `scroll` moves left do not move a view that would not be seen.
    fn finish_typing_with(&mut self, cues: bool) {
        // The view goes where the `scroll` moves left would put it.
        if cues {
            self.apply_cues(usize::MAX);
        }
        if let Some(t) = self.typing.take() {
            self.doc.code = t.plan.full;
            // The take was sized from the code typed so far.
            *self.geometry.get_mut() = None;
            // The step is ready: its auto-advance countdown starts.
            self.step_ready_at = Some(std::time::Instant::now());
        }
    }

    pub fn is_typing(&self) -> bool {
        self.typing.is_some()
    }

    // ---------- enter effects (experimental) ----------

    /// Starts the enter effect of the step on screen, when something of the
    /// step comes in with one ([`Document::has_effect_at`]). Its plan is
    /// made here, once, at the screen's width.
    fn start_effect(&mut self) {
        self.effect = None;
        if !self.doc.has_effect_at(self.doc.step) {
            return;
        }
        let mut opts = self.layout_options(self.screen_width());
        opts.pointer = self.pointer_shown();
        let cache = self.effect_cache(&opts);
        let Some(plan) = &cache.plan else {
            return;
        };
        #[allow(unused_mut)]
        let mut slow = 1;
        #[cfg(feature = "test-hooks")]
        if crate::test_hook("effect:slow") || crate::test_hook("effect:late-frames") {
            slow = 100;
        }
        self.effect = Some(EffectState {
            started: std::time::Instant::now(),
            duration_ms: plan.duration_ms() * slow,
            slow,
            hold_ms: None,
            revision: self.revision,
            pointer: opts.pointer,
            cache: std::cell::RefCell::new(Some(cache)),
        });
        // The auto-advance countdown starts when the effect ends.
        self.step_ready_at = None;
    }

    /// The final layout of the step with `opts` and the plan of its effect.
    fn effect_cache(&self, opts: &LayoutOptions) -> EffectCache {
        let (layout, trace) = self.lay_out_traced(&self.doc, &self.theme, opts, true);
        let plan = trace.and_then(|t| tiza_core::effects::Plan::new(&layout.canvas, &t, &self.doc));
        EffectCache {
            width: opts.width,
            pointer: opts.pointer,
            layout,
            plan,
        }
    }

    /// The work layout of the effect for a frame at this moment, while one
    /// plays: taken from the effect until [`App::put_back_screen`] gives it
    /// back. A new one is laid out when the width or the pointer changed,
    /// or when the last frame did not give it back.
    fn effect_frame(&self, opts: &LayoutOptions) -> Option<EffectFrame> {
        let e = self
            .effect
            .as_ref()
            .filter(|e| e.revision == self.revision && e.pointer == opts.pointer)?;
        let t_ms = e
            .hold_ms
            .unwrap_or_else(|| e.started.elapsed().as_millis() as u64 / e.slow);
        // Test hook, only with the `test-hooks` feature: each frame of the
        // effect takes 50 ms more, longer than the time between two frames,
        // so a test on a pty sees how the loop paces late frames.
        #[cfg(feature = "test-hooks")]
        if crate::test_hook("effect:late-frames") {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let cache = e
            .cache
            .borrow_mut()
            .take()
            .filter(|c| c.width == opts.width && c.pointer == opts.pointer)
            .unwrap_or_else(|| self.effect_cache(opts));
        Some(EffectFrame {
            started: e.started,
            t_ms,
            cache,
        })
    }

    /// Gives the work layout of the effect back at the end of a frame
    /// (`ui::draw`), so that the next frame paints on it again. It is
    /// dropped when its effect is over or another one plays.
    pub fn put_back_screen(&self, screen: ScreenLayout) {
        if let ScreenLayout::Effect(f) = screen
            && let Some(e) = &self.effect
            && e.started == f.started
            && e.revision == self.revision
        {
            let mut slot = e.cache.borrow_mut();
            if slot.is_none() {
                *slot = Some(f.cache);
            }
        }
    }

    /// Only for tests: every frame from now on shows the enter effect that
    /// plays as it is at `ms` from its start, whatever the clock says. The
    /// effect still ends by its clock or by a key. Nothing in Tiza calls it:
    /// no key, request, tool or setting.
    #[doc(hidden)]
    pub fn hold_effect_at(&mut self, ms: u64) {
        if let Some(e) = &mut self.effect {
            e.hold_ms = Some(ms);
        }
    }

    /// Advances the enter effect: `true` while one plays or when it just
    /// ended (the screen changes). Call it every loop iteration.
    pub fn effect_tick(&mut self) -> bool {
        let Some(e) = &self.effect else {
            return false;
        };
        if e.revision != self.revision
            || e.pointer != self.pointer_shown()
            || e.started.elapsed().as_millis() as u64 >= e.duration_ms
        {
            self.finish_effect();
        }
        true
    }

    /// Ends the enter effect at once: the final drawing shows (any key).
    pub fn finish_effect(&mut self) {
        if self.effect.take().is_some() {
            // The step is ready: its auto-advance countdown starts.
            self.step_ready_at = Some(std::time::Instant::now());
        }
    }

    /// The time between two frames of an enter effect on the screen: from
    /// `effect_fps` in `[defaults]` of config.toml, else
    /// [`tiza_core::effects::SCREEN_FPS`].
    pub fn effect_frame_period(&self) -> std::time::Duration {
        tiza_core::effects::screen_frame_period(
            self.config_defaults
                .effect_fps
                .unwrap_or(tiza_core::effects::SCREEN_FPS),
        )
    }

    pub fn is_playing_effect(&self) -> bool {
        self.effect.is_some()
    }

    /// Adds a move (validated against the code at its step). Undoable. A
    /// `code.link` reads its file next to the document first.
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
                self.camera_changed();
                self.touch();
                let warning = before.and_then(|b| b.warning(&self.doc));
                self.settle_pause();
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
        self.camera_changed_with(false);
        self.touch();
        let warning = before.and_then(|b| b.warning(&self.doc));
        self.settle_pause();
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
            self.doc.follows = reference.file_ref();
        }
        self.rebuild_theme();
        self.camera_changed();
        self.touch();
        self.info(said);
        self.follow_after_history(before, replaced);
        self.save_follows();
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

    /// Enters or leaves presentation mode. Agent changes that wait stay a
    /// chip (`:review` shows them); the ones the view did not show while
    /// presenting are told on the bar now.
    pub fn set_present(&mut self, on: bool) {
        self.present = on;
        self.present_message_until = None;
        self.present_message = None;
        // A key of the view and a reveal are for presentation mode only.
        self.view_request = None;
        // A selection of normal mode cannot be seen there: it goes.
        if on {
            self.selected = None;
        }
        if !on && self.scroll_held.as_ref().is_some_and(ScrollHeld::reveals) {
            self.scroll_held = None;
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
    /// stays on the last row for [`PRESENT_MESSAGE_TTL`].
    pub fn command_finished(&mut self, now: std::time::Instant) {
        self.present_message = self.message.clone().filter(|_| self.present);
        self.present_message_until = self
            .present_message
            .is_some()
            .then_some(now + PRESENT_MESSAGE_TTL);
    }

    /// Presentation mode hides the bar: when the TUI opens there, the effects
    /// and the moves that this version does not know show on the last row
    /// for [`PRESENT_MESSAGE_TTL`] from `now`, as the result of a `:`
    /// command does. Normal mode shows them on the bar, as before.
    pub fn show_unknown_effects(&mut self, now: std::time::Instant) {
        if !self.present {
            return;
        }
        let Some(short) = tiza_core::fs::not_known_short(&self.doc) else {
            return;
        };
        if !self
            .message
            .as_ref()
            .is_some_and(|(m, _)| m.contains(&short))
        {
            self.info(short);
        }
        self.present_message = self.message.clone();
        self.present_message_until = Some(now + PRESENT_MESSAGE_TTL);
    }

    /// Hides the presentation-mode message once its time is up; `true` when
    /// the frame must be drawn again.
    pub fn present_message_tick(&mut self, now: std::time::Instant) -> bool {
        match self.present_message_until {
            Some(until) if now >= until => {
                self.present_message_until = None;
                self.present_message = None;
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
            && self.present_message.is_some()
            && !matches!(self.mode, Mode::Command(_))
    }

    pub fn toggle_theme(&mut self) {
        let next = self.theme.toggled().name;
        self.set_theme_name(next, true);
    }

    /// A paste from the terminal. On the command line it adds the first line
    /// of the text: a terminal can end a pasted line with CR alone. With a
    /// panel open or in presentation mode it changes nothing and says how to
    /// paste. Elsewhere the text replaces the code.
    pub fn paste(&mut self, text: &str) {
        if let Mode::Command(input) = &mut self.mode {
            let first = text.split(['\r', '\n']).next().unwrap_or("");
            input.push_str(first.trim_end());
            return;
        }
        let panel = match self.overlay {
            Overlay::None => None,
            Overlay::Help => Some("help"),
            Overlay::List => Some("list"),
            Overlay::Review => Some("review"),
        };
        if let Some(panel) = panel {
            // With a filter, the first Esc only clears it.
            let esc = if self.overlay_typing || !self.overlay_filter.is_empty() {
                "Esc twice"
            } else {
                "Esc"
            };
            self.error(format!(
                "paste ignored: close the {panel} with {esc}, then paste again"
            ));
        } else if self.present {
            self.error("paste ignored in presentation mode: press P to leave it, then paste again");
        } else if let Err(e) = self.replace_code(text) {
            self.error(e.to_string());
        }
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
            && self.doc.follows.is_some()
            && self.doc.is_embedded()
            && matches!(followed, tiza_core::model::CodeSource::File { .. })
        {
            self.doc.follows = followed.file_ref();
        }
        let paused = self.pause_follow(Some(followed), NotTheFile::Paste("the code is pasted"));
        self.save_follows();
        if let Some(det) = detected {
            self.doc.source.language = det.language;
        }
        self.cursor_line = self.doc.source.first_line;
        self.set_scroll(0);
        self.scroll_applied = None;
        self.scroll_held = None;
        self.camera_changed();
        let lines = self.doc.code.len();
        let pasted = format!("{lines} line{} pasted", if lines == 1 { "" } else { "s" });
        let kept = format!(
            "{} annotation{} kept",
            out.kept,
            if out.kept == 1 { "" } else { "s" }
        );
        match out.stale {
            None if out.ignored.is_none() && out.dropped > 0 => self.info(format!(
                "{pasted}; {} annotation{} dropped (their text is gone; u undoes)",
                out.dropped,
                if out.dropped == 1 { "" } else { "s" }
            )),
            None if out.ignored.is_none() => self.info(format!("{pasted} ({kept})")),
            n => self.stale_notice(
                n,
                out.dropped,
                &out.ignored,
                "their text is gone",
                format!("{pasted} ({kept})"),
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
            self.doc.follows = src.file_ref();
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
        let name = self.reference_name(&src)?;
        let mut probe = self.doc.clone();
        probe.code_src = src;
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

    /// Why follow is off: the code on screen ([`App::off_reason`]), else a
    /// watcher that could not start for the file still to follow.
    fn follow_off_why(&self) -> Option<String> {
        self.off_reason().or_else(|| {
            let (path, why) = self.follow_failed.as_ref()?;
            (self.source_file().as_ref() == Some(path)).then(|| why.clone())
        })
    }

    /// The file name of a file reference, as follow names it.
    fn reference_name(&self, src: &tiza_core::model::CodeSource) -> Option<String> {
        let path = fs::code_source_file(src, self.save_path.parent())?;
        Some(path.file_name()?.to_string_lossy().into_owned())
    }

    /// The name of the file `:follow on` would reload while follow is off
    /// over code that is not the file's ([`App::paused`]).
    fn paused_file(&self) -> Option<String> {
        self.reference_name(&self.paused()?)
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
        // was for, or of an edit that kept the code): no change waits to
        // load. The read at the resume step stays: the file may not be
        // readable now.
        if !self.doc.is_embedded()
            && let Some(f) = &mut self.follow
        {
            self.pause.pending = false;
            f.last_content = self.doc.base().to_vec();
        }
        // A change on disk waits for the resume step and the undo left the
        // step-0 code as it was (a note, a move): follow stays on, the code
        // embedded, and the resume step still reloads.
        if self.pause.waits
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
        // A typed move or a `wait` that runs (follow loads at a step with no
        // patch) ends now: at its end it would put back the code it typed
        // over, the code before this one.
        self.finish_typing();
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
            // The new text is the step-0 code: a follow reload comes only at
            // a step with no patch applied, the rest only at step 0 (see
            // `moves_block_reason`). `revalidate` shows the step's view of it.
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
        // its own code at their step; those of a linked file keep
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
                // Validated against the code at the annotation's step; its
                // step and file stay as they are.
                Some(shape) if !held.contains(&a.id) => {
                    let _ = d.add_settled(Annotation { shape, ..a.clone() });
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
    fn paused(&self) -> Option<tiza_core::model::CodeSource> {
        let file = self
            .doc
            .follows
            .as_ref()
            .filter(|_| self.doc.is_embedded())?;
        Some(file.clone().into())
    }

    /// The autosave of a paste or an undo comes before follow goes off and
    /// the file it replaced is known: written again when that is all that
    /// changed.
    fn save_follows(&mut self) {
        if self.autosave && self.save_error.is_none() && self.pending_json() != self.saved_json {
            let _ = self.save();
        }
    }

    fn followed_source(&self) -> Option<tiza_core::model::CodeSource> {
        if self.doc.is_embedded() {
            self.paused()
        } else {
            Some(self.doc.code_src.clone())
        }
    }

    /// Following while a patch is applied: why a save does not load now (it
    /// waits for a step with no patch applied), as the reply, the bar and
    /// `follow.status` say it. `None` when a save loads at once.
    pub fn follow_waits(&self) -> Option<String> {
        let back = self.resume_step()?;
        let when = if back == 0 {
            "at step 0".to_string()
        } else {
            format!("at step {back} or earlier")
        };
        Some(format!("paused; loads {when}"))
    }

    /// Following at a step with a patch applied: a save waits.
    pub fn follow_paused(&self) -> bool {
        self.follow.is_some() && self.doc.patches_applied()
    }

    /// Paused with a change on disk that waits to load.
    pub fn follow_pending(&self) -> bool {
        self.follow_paused() && self.pause.pending
    }

    /// While paused: the step where a change loads, the last step before
    /// the first patch (or an earlier one).
    pub fn resume_step(&self) -> Option<u32> {
        if !self.follow_paused() {
            return None;
        }
        Some(self.doc.first_patch_step()?.saturating_sub(1))
    }

    /// The presenter is in the middle of something that a reload must not
    /// change under them: a typed move, the `:` line, a `v`/`V` selection
    /// or a panel. The auto-advance and a save wait for it.
    fn presenter_busy(&self) -> bool {
        self.typing.is_some()
            || matches!(self.mode, Mode::Command(_) | Mode::Select { .. })
            || self.overlay != Overlay::None
    }

    /// Enables or disables follow mode. Enabling needs a file reference
    /// (`:link` first when the code is embedded). At a step with a patch
    /// applied it starts paused: a save, or a change made while follow was
    /// off, loads at the resume step.
    pub fn set_follow(&mut self, enabled: bool) -> Result<String, Error> {
        if !enabled {
            self.follow_failed = None;
            let dropped = self.follow_pending();
            self.drop_change();
            self.pause = Pause::default();
            self.save_held = false;
            let name = self.follow.as_mut().map(|f| {
                f.stop();
                f.file_name()
            });
            self.follow = None;
            // The file embedded code on screen replaced stays known, so the
            // `:follow on reloads main.rs` the bar said still holds.
            return Ok(match name {
                Some(name) if dropped => format!("follow off: the change to {name} is not loaded"),
                _ => "follow off".into(),
            });
        }
        if let Some(f) = &self.follow {
            let name = f.file_name();
            return Ok(match self.resume_step() {
                Some(at) => format!("already following {name}; step {at} loads a save"),
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
        self.follow_failed = None;
        let f =
            crate::follow::Follow::start(path, self.doc.base().to_vec()).map_err(Error::Invalid)?;
        let name = f.file_name();
        self.follow = Some(f);
        self.pause = Pause::default();
        self.save_held = false;
        // Pick up anything that changed before we started watching. Code
        // pasted over the file is replaced on purpose: always one undo step,
        // so `u` gives the paste back.
        let pasted = self.doc.is_embedded();
        let said = self.message.clone();
        let _ = self.reload_from_disk(pasted);
        if self.pause.pending {
            // Paused, what differs waits, and the reply says so (`paused:
            // …`), the bar too (`⟳ paused · changed`): over pasted code the
            // file did not change on disk, as the notice would say.
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
        // `follows` stays: while the code on screen is still embedded
        // (the file is missing, or unreadable for now) it names the file the
        // next reload reads, so the file coming back reloads it.
        // Paused, it says where a save loads, as the notice does: the bar
        // shows `⟳ paused` beside it.
        Ok(match self.resume_step() {
            Some(at) => format!("following {name}; step {at} loads a save"),
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
        let (reloads, reads, last, clock) = (
            old.reloads,
            old.reads,
            old.last_reload,
            old.last_reload_clock.clone(),
        );
        self.pause = Pause::default();
        self.save_held = false;
        match crate::follow::Follow::start(path, base) {
            Ok(mut f) => {
                (f.reloads, f.reads, f.last_reload, f.last_reload_clock) =
                    (reloads, reads, last, clock);
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
        match f.poll() {
            crate::follow::Poll::Reload => self.read_save(),
            // A save held while the presenter was busy, read now that they
            // are done; and a step change that is not `step_to` (a step
            // cleared away) can end a pause too.
            crate::follow::Poll::Idle => {
                if self.save_held && !self.presenter_busy() {
                    return self.read_save();
                }
                self.reload_waiting()
            }
            // A save made while the watcher started is read as any save.
            crate::follow::Poll::Started => {
                f.touch();
                false
            }
            crate::follow::Poll::Failed(why) => {
                let path = f.path.clone();
                self.follow = None;
                self.pause = Pause::default();
                self.save_held = false;
                self.error(format!("follow off: {why}"));
                self.follow_failed = Some((path, why));
                true
            }
        }
    }

    /// No change waits any more, and none was loaded: the notice that said
    /// one waits leaves the bar, when it is still there. Another message
    /// stays.
    fn drop_change(&mut self) {
        self.pause.pending = false;
        if let Some(notice) = self.pause.notice.take()
            && self
                .message
                .as_ref()
                .is_some_and(|(m, error)| !error && *m == notice)
        {
            self.message = None;
        }
    }

    /// What a save of the followed file does when the watcher has seen it
    /// (after its debounce): the file is read and loaded, or, paused, kept
    /// as the change that waits. While the presenter is busy
    /// ([`App::presenter_busy`]) it is read when they are done. `true` when
    /// the screen changed. The in-process tests call it in place of the
    /// watcher.
    pub fn read_save(&mut self) -> bool {
        self.save_held = self.presenter_busy();
        if self.save_held || self.follow.is_none() {
            return false;
        }
        self.reload_from_disk(false)
    }

    /// At a step with no patch applied, the pause is over: a change on
    /// disk that waited loads now, as a normal reload (one undo step, since
    /// the code on screen is embedded meanwhile). A file saved back as the
    /// screen has it is no reload: the document references it again. A
    /// file that cannot be read gives its error, and the code on screen
    /// stays. `true` when the screen changed.
    fn reload_waiting(&mut self) -> bool {
        if self.follow.is_none() || self.doc.patches_applied() {
            return false;
        }
        // The next pause gives its own notice.
        if !std::mem::take(&mut self.pause).waits {
            return false;
        }
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

    /// Following over embedded code that the file holds again (saved back
    /// as the screen has it): the document references the file again, at
    /// any step, since the file is the step-0 code. `true` when it did.
    fn back_to_reference(&mut self) -> bool {
        if self.follow.is_none() {
            return false;
        }
        let Some(src) = self.paused() else {
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
        // ADR 0007 §7: follow is paused while a patch is applied (the edits
        // are not reconciled yet). The other moves do not change the text: a
        // save there loads at once. Presentation mode shows no notice.
        let paused = self.doc.patches_applied();
        let quiet = paused && self.present;
        let Some(f) = &mut self.follow else {
            return false;
        };
        let base = self.save_path.parent().map(Path::to_path_buf);
        f.last_read = crate::follow::Stamp::of(&f.path);
        let (read, bytes) = fs::reread_reference_with_bytes(&probe, base.as_deref());
        f.reads += 1;
        // Kept for a read that waits for the second write of an editor:
        // the hash stays the one of the code that the screen has.
        let hash_before =
            std::mem::replace(&mut f.hash, bytes.as_deref().map(crate::follow::sha256_hex));
        let name = f.file_name();
        let (new, read_first) = match read {
            Ok(read) => read,
            Err(e) => {
                let msg = e.to_string();
                if !f.path.exists() {
                    let first = !std::mem::replace(&mut f.missing, true);
                    // Nothing to load while it is away.
                    self.drop_change();
                    if first && !quiet {
                        self.error(format!("{name} removed; waiting for it to come back"));
                    }
                    return first;
                }
                if paused {
                    // Nothing to load either: the resume step reads it
                    // again, and says why if it still cannot. Said once while
                    // it stays so.
                    self.pause.waits = true;
                    self.drop_change();
                    if self.pause.unreadable || self.present {
                        return true;
                    }
                    self.pause.unreadable = true;
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
        let was_unreadable = std::mem::replace(&mut self.pause.unreadable, false);
        // An editor that truncates and then writes: give the second write a
        // chance, at each save.
        let empty = new.iter().all(|l| l.trim().is_empty());
        if !empty {
            f.take_deferred();
        }
        if empty && !f.last_content.iter().all(|l| l.trim().is_empty()) && !f.take_deferred() {
            f.hash = hash_before;
            f.defer();
            // The read of the second write is the one that finds the file
            // back.
            f.missing = was_missing;
            self.pause.unreadable = was_unreadable;
            return false;
        }
        // Embedded code numbered from another line than the file's (a paste
        // over a range of the file): the same text is still a change to
        // load, or saved as `code: {file}` it would load numbered otherwise.
        let renumbered = self.doc.is_embedded() && read_first != self.doc.base_first();
        if new == f.last_content && !was_missing && !renumbered {
            // Nothing new to load: a change that waited is dropped, in
            // silence, and embedded code the file holds again is the file's
            // once more (the bar changes: `true`).
            self.drop_change();
            if was_unreadable && !quiet {
                // What the bar said (it could not be read) is no longer true.
                self.info(format!("{name} can be read again; following it"));
                let _ = self.back_to_reference();
                return true;
            }
            return self.back_to_reference();
        }
        if paused && new == f.last_content && !renumbered {
            // Back as it was before it went missing: nothing waits for it.
            self.pause.pending = false;
            if !quiet {
                self.info(format!("{name} is back; following it"));
            }
            return true;
        }
        if paused {
            // The change waits for the resume step; one notice per pause
            // says which step that is. The code on screen is no longer the
            // file's: embedded meanwhile, as after a paste, so the autosave
            // and a fresh load show it — saved as `code: {file}` they would
            // read the new text under the annotations and `rebind` entries
            // made for this one.
            if !self.doc.is_embedded() {
                self.doc.follows = self.doc.code_src.file_ref();
                self.doc.code_src =
                    tiza_core::model::CodeSource::Embedded(self.doc.base().to_vec());
                self.pause_embedded = Some(self.doc.base().to_vec());
                self.touch();
            }
            (self.pause.waits, self.pause.pending) = (true, true);
            // The notice of the pause, once; and again when what the bar
            // said of the file last (it went missing, it could not be
            // read) is no longer true.
            if (!self.pause.noticed || was_missing || was_unreadable) && !quiet {
                self.pause.noticed = true;
                let at = self.resume_step().unwrap_or(0);
                let notice = format!("step {at} loads the change to {name}");
                self.info(notice.clone());
                self.pause.notice = Some(notice);
            }
            return true;
        }
        f.mark_reloaded(new.clone());
        let reloads = f.reloads;
        // The numbers of the step-0 code: the code on screen can be the
        // window of a `code.link`, numbered from its own first line.
        let first = if self.doc.is_embedded() {
            read_first
        } else {
            self.doc.base_first()
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
        // The target of a `scroll` may have moved with the new code.
        self.camera_changed();
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

    /// Whether the follow watcher is still starting on its own thread (the
    /// in-process tests wait for it, outside any request).
    pub fn follow_starting(&self) -> bool {
        self.follow.as_ref().is_some_and(|f| f.starting())
    }

    /// `follow.status` payload.
    pub fn follow_status(&self) -> Value {
        #[allow(unused_mut)]
        let mut st = match &self.follow {
            Some(f) => {
                let mut st = json!({
                    "enabled": true,
                    "path": f.path.display().to_string(),
                    "reloads": f.reloads,
                    "reads": f.reads,
                    "hash": f.hash,
                    "last_reload": f.last_reload_clock,
                    "missing": f.missing,
                    "paused": self.follow_paused(),
                    "pending": self.follow_pending(),
                });
                if let Some(step) = self.resume_step() {
                    st["resume_step"] = json!(step);
                }
                if let Some(why) = self.follow_waits() {
                    st["reason"] = json!(why);
                }
                st
            }
            None => {
                let mut st = json!({
                    "enabled": false,
                    "path": self.source_file().map(|p| p.display().to_string()),
                    "paused": false,
                    "pending": false,
                });
                if let Some(why) = self.follow_off_why() {
                    st["reason"] = json!(why);
                }
                st
            }
        };
        // Only in the tests' build: the watchers in this process and the
        // message on the bar.
        #[cfg(feature = "test-hooks")]
        {
            st["watchers"] = json!(crate::follow::live_watchers());
            st["bar"] = json!(self.message.as_ref().map(|m| m.0.clone()));
        }
        st
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

    /// The annotations that the screen draws at this step, in reading order
    /// (the first line on the screen, then the id), each with that line.
    /// The rules of the layout: the file shown, the step reached, a
    /// spotlight within its `until`, the lines inside the view where the
    /// moves of the step put them, a note or a callout with room, and not
    /// the step's own while its moves type. A spotlight of ids only takes
    /// the first line of what it names; with none of them drawn it has no
    /// line, and comes last.
    fn drawn_annotations(&self) -> Vec<(String, Option<u32>)> {
        let mut v: Vec<(u32, String, Option<u32>)> = self
            .drawn_annotation_lines()
            .into_iter()
            .map(|(id, line, _)| (line.unwrap_or(u32::MAX), id, line))
            .collect();
        v.sort();
        v.into_iter().map(|(_, id, line)| (id, line)).collect()
    }

    /// [`App::drawn_annotations`], each with its first line and its line
    /// ranges on the screen (those of what a spotlight of ids names), in the
    /// order of the document.
    fn drawn_annotation_lines(&self) -> Vec<DrawnLines> {
        let step = self.doc.step;
        let view;
        let doc = if self.doc.moves.is_empty() {
            &self.doc
        } else {
            view = self.doc.view_at(step);
            &view.doc
        };
        let ranges_of = |a: &Annotation| -> (Option<u32>, Vec<(u32, u32)>) {
            match &a.shape {
                Shape::Spotlight {
                    from_line: None,
                    ids,
                    ..
                } => {
                    let ranges: Vec<(u32, u32)> = ids
                        .iter()
                        .filter_map(|id| doc.find_annotation(id))
                        .filter_map(|t| t.shape.line_ranges().first().copied())
                        .collect();
                    (ranges.iter().map(|r| r.0).min(), ranges)
                }
                shape => {
                    let ranges = shape.line_ranges();
                    (ranges.first().map(|r| r.0), ranges)
                }
            }
        };
        let mut cuts = None;
        let mut v = Vec::new();
        for a in &doc.annotations {
            let lines = a.shape.line_ranges();
            let drawn = a.step <= step
                && !(self.typing.is_some() && a.step == step)
                && match &a.shape {
                    Shape::Spotlight { until, .. } => step <= until.unwrap_or(a.step),
                    _ => true,
                }
                && lines
                    .iter()
                    .all(|(f, t)| doc.has_line(*f) && doc.has_line(*t));
            if !drawn {
                continue;
            }
            // A note or a callout that the take has no room for.
            if matches!(a.shape, Shape::Note { .. } | Shape::Callout { .. }) {
                let (_, cut) = cuts.get_or_insert_with(|| self.screen_cuts());
                if cut.iter().any(|c| c.id == a.id && c.dropped) {
                    continue;
                }
            }
            let (line, ranges) = ranges_of(a);
            v.push((a.id.clone(), line, ranges));
        }
        v
    }

    /// Presentation mode: the lines where the annotations that the screen
    /// draws start in the view now (the first line of each one that is in
    /// the view), in the order of the screen.
    fn annotation_lines_in_view(&self) -> Vec<u32> {
        let layout = self.screen_layout_with_pointer(self.screen_width());
        // From the view the next frame shows, as `ui::draw` does: a step
        // change or an agent since the last frame keeps the line on top.
        let top = self
            .view_anchor
            .and_then(|a| view_row_of(&layout, a))
            .unwrap_or(self.scroll);
        let end = top + self.screen_height();
        let in_view = |l: u32| {
            layout
                .row_of_line
                .get(&l)
                .is_some_and(|&r| r >= top && r < end)
        };
        let mut v: Vec<u32> = self
            .drawn_annotation_lines()
            .into_iter()
            .filter_map(|(_, _, ranges)| {
                ranges
                    .iter()
                    .filter_map(|&(f, t)| (f..=t).find(|l| in_view(*l)))
                    .min()
            })
            .collect();
        v.sort_unstable();
        v
    }

    /// `Tab` / `Shift-Tab` in presentation mode: the next line under the
    /// cursor (or the previous one above it) where an annotation that the
    /// view shows starts, and round to the first (or the last). Going from
    /// the cursor line, the annotations of one line are one stop, so each
    /// `Tab` moves the pointer, and it goes from where the presenter put
    /// the cursor. The
    /// hidden cursor goes there, and the live pointer with it; the view
    /// does not move. Nothing is selected, so no key edits an annotation
    /// that the presenter cannot see. With none in the view, nothing
    /// happens.
    fn tab_in_view(&mut self, dir: i32) {
        let lines = self.annotation_lines_in_view();
        let from = self.cursor_line;
        let next = if dir >= 0 {
            lines.iter().find(|l| **l > from).or(lines.first())
        } else {
            lines.iter().rev().find(|l| **l < from).or(lines.last())
        };
        let Some(&line) = next else {
            return;
        };
        self.cursor_line = line;
        self.clamp_col();
        // In the view already: not a motion for the view to follow.
        self.view_cursor_line = line;
        self.selected = None;
    }

    /// Tab / Shift-Tab: select the next or the previous annotation that the
    /// screen draws, and move the cursor to its first line on the screen.
    /// The others (another file's, a spotlight past its `until`) are in the
    /// list (`L`, Enter).
    pub fn select_annotation(&mut self, dir: i32) {
        if self.present {
            self.tab_in_view(dir);
            return;
        }
        let drawn = self.drawn_annotations();
        if drawn.is_empty() {
            self.selected = None;
            let earlier = self.doc.annotations.iter().any(|a| a.step <= self.doc.step);
            self.info(if self.doc.step == 0 && !earlier {
                "step 0 shows only the code: press n to see the annotations"
            } else if earlier {
                "no annotation on the screen at this step: L lists them all"
            } else {
                "no annotations in this step"
            });
            return;
        }
        let cur = self
            .selected
            .as_ref()
            .and_then(|s| drawn.iter().position(|(i, _)| i == s));
        let next = match (cur, dir) {
            (None, d) if d >= 0 => 0,
            (None, _) => drawn.len() - 1,
            (Some(i), d) => ((i as i64 + d as i64).rem_euclid(drawn.len() as i64)) as usize,
        };
        let (id, line) = drawn[next].clone();
        self.move_cursor_to(line);
        self.selected = Some(id);
    }

    /// The cursor on `line` of the screen, when it has one there.
    fn move_cursor_to(&mut self, line: Option<u32>) {
        if let Some(line) = line.filter(|l| self.doc.has_line(*l)) {
            self.cursor_line = line;
        }
    }

    /// Selects annotation `id`, as Tab does. The cursor goes to its first
    /// line on the screen when the step draws it; one that the step does
    /// not draw is selected all the same, and the cursor stays. In
    /// presentation mode nothing is selected (no key edits what the
    /// presenter cannot see): the cursor goes there.
    fn focus_annotation(&mut self, id: String) {
        let line = self
            .drawn_annotations()
            .into_iter()
            .find(|(i, _)| *i == id)
            .and_then(|(_, line)| line);
        self.move_cursor_to(line);
        if !self.present {
            self.selected = Some(id);
        }
    }

    /// Enter on a row of the list: the step of that annotation, with it
    /// selected as Tab does, or the step of that move.
    pub fn go_to_list_item(&mut self, item: &crate::list::ListItem) {
        match item {
            // A jump, as `:goto`: no move types on the way.
            crate::list::ListItem::Annotation { id, step } => {
                self.step_to_with(*step, true);
                self.focus_annotation(id.clone());
            }
            crate::list::ListItem::Move { step, .. } => {
                self.step_to_with(*step, true);
            }
        }
    }

    /// `x` on a row of the list: deletes that annotation or move, as `:del`
    /// and the socket's `move.delete` do; `u` gives it back.
    pub fn delete_list_item(&mut self, item: &crate::list::ListItem) {
        let said = match item {
            crate::list::ListItem::Annotation { id, .. } => {
                if self.selected.as_deref() == Some(id.as_str()) {
                    self.selected = None;
                }
                self.delete_annotation(id)
                    .map(|()| format!("{id} deleted (u to undo)"))
            }
            crate::list::ListItem::Move { index, .. } => {
                self.delete_move_noting(*index)
                    .map(|(_, warning)| match warning {
                        Some(w) => format!("#{index} deleted (u to undo); {w}"),
                        None => format!("#{index} deleted (u to undo)"),
                    })
            }
        };
        match said {
            Ok(m) => self.info(m),
            Err(e) => self.error(e.to_string()),
        }
    }

    /// Describes the selected annotation for the status bar.
    pub fn selected_text(&self) -> Option<String> {
        let id = self.selected.as_ref()?;
        let a = self.doc.find_annotation(id)?;
        if self.doc.moves.is_empty() {
            return Some(tiza_core::protocol::describe(a));
        }
        // Where the moves of the step put it, as the list and Tab have it.
        let key = (id.clone(), self.revision, self.doc.step);
        if let Some((i, r, s, text)) = self.selected_shown.borrow().as_ref()
            && (i, *r, *s) == (&key.0, key.1, key.2)
        {
            return Some(text.clone());
        }
        let view = self.doc.view_at(self.doc.step);
        let text = tiza_core::protocol::describe(view.doc.find_annotation(id).unwrap_or(a));
        *self.selected_shown.borrow_mut() = Some((key.0, key.1, key.2, text.clone()));
        Some(text)
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
                    effect: None,
                    file: None,
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
    /// pending selection as its anchor ([`SelectionUse`]).
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
        "saveas",
        "scroll",
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
                    "mark" | "m" if words.len() <= 2 => self.kind_names(),
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
                // As in Vim: `:w PATH` writes a copy and the open file stays
                // the one saved to; `:saveas PATH` moves to PATH. An existing
                // file needs `!`.
                "w" | "write" | "w!" | "write!" if !rest.is_empty() => {
                    Self::typed_path(rest).and_then(|p| self.write_copy(&p, cmd.ends_with('!')))
                }
                "w" | "write" | "w!" | "write!" => self
                    .save()
                    .map(|_| format!("saved to {}", self.save_path.display())),
                "saveas" | "sav" | "saveas!" | "sav!" => self.save_as(rest, cmd.ends_with('!')),
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
                        Ok("no agent changes wait".into())
                    } else {
                        self.open_overlay(Overlay::Review)?;
                        Ok(String::new())
                    }
                }
                "wq" | "wq!" if !rest.is_empty() => {
                    let force = cmd == "wq!";
                    // What would refuse the quit (stale moves) refuses before
                    // the copy is written, so the same command can be run
                    // again; `!` quits over them, as `:q!`.
                    if !force && tiza_core::stale::on_close(&self.doc).is_some() {
                        let dirty = std::mem::replace(&mut self.dirty, false);
                        self.request_quit(false);
                        self.dirty = dirty;
                        return Ok(String::new());
                    }
                    // The copy holds the changes: quit as if they were saved.
                    self.write_copy(&Self::typed_path(rest)?, force)?;
                    self.quit = true;
                    Ok(String::new())
                }
                "wq" | "wq!" => {
                    // Quit only when the write succeeded (`:q!` leaves anyway),
                    // and not over stale moves without a word (`:wq!` quits
                    // over them, as `:q!`).
                    self.save()?;
                    self.request_quit(cmd == "wq!");
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
                    self.doc.follows = None;
                    self.frozen_views = self.doc.views.keys().cloned().collect();
                    self.touch();
                    // The code no longer comes from the file: there is
                    // nothing to follow (`:follow on` says the same).
                    if self.follow.is_some() {
                        self.set_follow(false)?;
                        return Ok(
                            "code frozen inside the .tiza; follow off: edits on disk no longer reach the screen (:link and :follow on to resume)"
                                .into(),
                        );
                    }
                    Ok("code frozen inside the .tiza".into())
                }
                "scroll" => self.cmd_scroll(rest),
                "link" if !rest.is_empty() && self.doc.step >= 1 => {
                    // At a step: a `code.link` move that shows `path` (with
                    // `A-B` as a window) from this step on. At step 0,
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
                        let (p, r) = fs::parse_target_in(rest, self.save_path.parent())?;
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
                        None => match self.follow_off_why() {
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
                        effect: None,
                        file: None,
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
                        effect: None,
                        file: None,
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
                            if n == max + 1 {
                                // Already there: it stays (a step change
                                // would go back to the last step).
                                if n != self.doc.step {
                                    self.doc.step = n;
                                    self.camera_changed();
                                    self.revision += 1;
                                }
                                return Ok(format!("step {n}/{n}: empty, new annotations go here"));
                            }
                            Ok(self.step_message_asked(n))
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
                                effect: None,
                                file: None,
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
                            .map(|_| {
                                // Not drawn at that step: the hint says why.
                                if self.doc.hidden_reason(id, n).is_some() {
                                    format!("{id} → step {n}")
                                } else {
                                    format!("{id} now appears at step {n}")
                                }
                            })
                        }
                        _ => Err(Error::Invalid("usage: :step N  |  :step ID N".into())),
                    }
                }
                "next" => {
                    if self.next_step() {
                        Ok(String::new())
                    } else {
                        Ok(self.step_label_line())
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
                    Ok(self.step_label_line())
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
                        return self.load_theme_file(std::path::Path::new(path), true);
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
                            format!("; {}", self.theme_notes.join("; "))
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
                            format!(
                                "language: {l} (:lang auto detects it again, :lang LANG sets another)"
                            )
                        } else {
                            format!("language: {l} (plain text; :lang LANG sets one)")
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
                            "usage: :lang LANG  |  :lang auto  |  :lang   (one language, such as rs, ts or py)".into(),
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
                    self.open_overlay(Overlay::List)?;
                    Ok(String::new())
                }
                "help" | "h" | "?" => {
                    self.open_overlay(Overlay::Help)?;
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
                    let (v, h) = self.effective_padding();
                    Ok(format!(
                        "padding {v} {h} ({})",
                        if self.config_defaults.pad.is_some() {
                            "from config.toml"
                        } else {
                            "default"
                        }
                    ))
                }
                "margin" => match rest.trim() {
                    "" => Ok(match self.doc.margin {
                        Some(n) => format!(
                            "margin {n} (notes wrap at {n} cells, and labels, legends and long notes are cut there; :margin auto removes the cap)"
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
                            "margin {cap}: notes wrap at {cap} cells, and labels, legends and long notes are cut there (saved in the document)"
                        ))
                    }
                },
                "numbers" | "nums" => match rest.trim() {
                    "" => Ok(format!(
                        "numbers {}{}",
                        if self.numbers() { "on" } else { "off" },
                        if self.doc.numbers.is_none() && self.config_defaults.numbers.is_some() {
                            " (from config.toml)"
                        } else {
                            ""
                        }
                    )),
                    "on" | "yes" | "off" | "no" => {
                        let on = matches!(rest.trim(), "on" | "yes");
                        // A document without `numbers` gets it, also when the
                        // config file gives the same value: the document then
                        // looks the same without the file.
                        if self.doc.numbers == Some(on) {
                            return Ok(format!(
                                "numbers already {}",
                                if on { "on" } else { "off" }
                            ));
                        }
                        self.history.record(&self.doc, "numbers");
                        self.doc.numbers = Some(on);
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
                                } else if self.doc.padding.is_none()
                                    && self.config_defaults.pad.is_some()
                                {
                                    " (from config.toml)"
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
                        self.no_save = false;
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
                // Test hook, only with the `test-hooks` feature and
                // TIZA_TEST_PANIC set: lets the crash hook be exercised from
                // a test. A release build has no `:panic`.
                #[cfg(feature = "test-hooks")]
                "panic" if std::env::var_os("TIZA_TEST_PANIC").is_some() => {
                    panic!("forced by :panic (TIZA_TEST_PANIC)")
                }
                other => Err(Error::Invalid(match nearest_commands(other) {
                    v if v.is_empty() => "unknown command (try :help)".to_string(),
                    v if v.len() == 1 => {
                        format!("unknown command: did you mean :{}?", v[0])
                    }
                    v => format!(
                        "unknown command: did you mean {}?",
                        v.iter()
                            .map(|c| format!(":{c}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                })),
            }
        };
        let result = exec();
        // A command that ran lets the selection go: a later `:mark`
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
        let (p, r) = fs::parse_target_in(rest.trim(), self.save_path.parent())?;
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

    /// `:scroll N | @'TEXT' | ID | annotation=ID [occurrence=N]
    /// [align=A]`: a `scroll` move at the step on screen. Only digits are a
    /// line; `@'…'` is a text; another word is the id of an annotation
    /// (`annotation=ID` for an id of digits only).
    fn cmd_scroll(&mut self, rest: &str) -> Result<String, Error> {
        const USAGE: &str = "usage: :scroll N | @'text' | ID | annotation=ID  [occurrence=N] [align=top|center|bottom|show]";
        if self.doc.step == 0 {
            return Err(Error::Invalid(
                "scroll: go to step 1 or later (step 0 is the code before any move)".into(),
            ));
        }
        let usage = || Error::Invalid(USAGE.into());
        let rest = rest.trim();
        let mut s = tiza_core::moves::ScrollTo::default();
        let options = if let Some(after_at) = rest.strip_prefix('@') {
            let (text, options) = take_marker(after_at).map_err(|_| usage())?;
            s.find = Some(text);
            options
        } else {
            let (target, options) = split_first(rest);
            if target.is_empty() {
                return Err(usage());
            }
            if let Some(id) = target.strip_prefix("annotation=") {
                s.annotation = Some(id.to_string());
            } else if target.contains('=') {
                return Err(usage());
            } else if let Some(n) = target
                .strip_prefix(['L', 'l'])
                .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                .or(Some(target).filter(|t| t.bytes().all(|b| b.is_ascii_digit())))
            {
                // `50`, or `L50` as `move.list` writes it.
                s.line = Some(n.parse().map_err(|_| usage())?);
            } else {
                s.annotation = Some(target.to_string());
            }
            options
        };
        for option in options.split_whitespace() {
            match option.split_once('=') {
                Some(("occurrence", n)) => {
                    s.occurrence = Some(n.parse().map_err(|_| usage())?);
                }
                Some(("align", a)) => {
                    s.align = serde_json::from_value(serde_json::Value::from(a)).map_err(|_| {
                        Error::Invalid(format!("align={a}: use top, center, bottom or show"))
                    })?;
                }
                _ => return Err(usage()),
            }
        }
        let target = match (&s.line, &s.find, &s.annotation) {
            (Some(l), _, _) => format!("line {l}"),
            (None, Some(t), _) => {
                let short: String = t.chars().take(30).collect();
                let more = if short.len() < t.len() { "…" } else { "" };
                let n = s.occurrence.map(|n| format!(" #{n}")).unwrap_or_default();
                format!("«{short}{more}»{n}")
            }
            (None, None, Some(id)) => {
                let short: String = id.chars().take(30).collect();
                let more = if short.len() < id.len() { "…" } else { "" };
                format!("{short}{more}")
            }
            (None, None, None) => return Err(usage()),
        };
        let align = s.align.name();
        let m = moves::Move {
            at_step: self.doc.step,
            action: moves::MoveAction::Scroll(s),
            rebind: Default::default(),
            stale: None,
            held: Vec::new(),
        };
        // A target that does not resolve adds no move: its error names no
        // move number.
        let (i, _) = self.add_move_noting(m).map_err(|e| match e {
            Error::MoveNoMatch { what, .. } => {
                Error::Invalid(format!("{what} matches nothing in the code at this step"))
            }
            Error::Invalid(msg) => {
                match msg.strip_prefix("move #").and_then(|r| r.split_once(": ")) {
                    Some((_, rest)) => Error::Invalid(rest.to_string()),
                    None => Error::Invalid(msg),
                }
            }
            other => other,
        })?;
        Ok(format!(
            "move #{} added: step {} scrolls to {target} ({align})",
            i + 1,
            self.doc.step
        ))
    }

    fn cmd_box(&mut self, rest: &str) -> Result<String, Error> {
        let (a, b, markers, rest) = self.parse_range_arg(rest).map_err(|e| match e {
            Error::Invalid(m) if m.starts_with("invalid range") => Error::Invalid(format!(
                "usage: :box A-B [label] [border=double] [corners=sharp] [fill=no] [effect=draw|fade]   or   :box @'start'..@'end' [label]   ({m})"
            )),
            other => other,
        })?;
        let (label, mut kv) = split_trailing_kv(rest);
        let effect = take_effect(&mut kv)?;
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
            effect,
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
            let s = self.resolve_span_strict(w, tiza_core::model::Edges::Start)?;
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
        edges: tiza_core::model::Edges,
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
        let span = match w.resolve_span_at(&self.doc, edges) {
            Err(Error::TextNotFound(text)) => return Err(self.not_in_the_code(&text)),
            // The command first, as above: `L@C` is the column.
            Err(Error::InsideCharacter {
                text,
                edge,
                character,
                line,
                col,
            }) => {
                return Err(Error::Invalid(format!(
                    "use {line}@{col} or the whole character {character:?} (the text {text:?} {edge} inside it)"
                )));
            }
            r => r?,
        };
        if let (Some(line), Some(text)) = (w.line, &w.find)
            && span.line != line
        {
            // The command first: a narrow bar cuts the end of the message.
            return Err(Error::Invalid(format!(
                "use {}:{text} («{text}» is on L{}, not L{line})",
                span.line, span.line
            )));
        }
        Ok(span)
    }

    /// Why the text of an `L:text` anchor matches nothing, in the code on
    /// screen: where it is when the demo has it elsewhere (outside the
    /// window shown, or in a file another step shows), else which code was
    /// searched, and that notes and labels are not.
    fn not_in_the_code(&self, text: &str) -> Error {
        let doc = &self.doc;
        let step = doc.step;
        let shown = doc.shown_view_at(step);
        let line_of = |code: &[String], first: u32| {
            code.iter()
                .position(|l| l.contains(text))
                .map(|i| first + i as u32)
        };
        if let Some((a, b)) = shown.window
            && let Ok((code, first)) = doc.view_code_at(&shown.file, step)
            && let Some(n) = line_of(&code, first)
        {
            return Error::Invalid(format!(
                "«{text}» is in {} at L{n}, outside the lines shown at step {step} ({a}-{b}).",
                doc.file_label(&shown.file)
            ));
        }
        // The step nearest to this one first (a later one on a tie).
        let mut steps: Vec<u32> = (0..=doc.max_step()).filter(|&s| s != step).collect();
        steps.sort_by_key(|&s| (s.abs_diff(step), s < step));
        for s in steps {
            let view = doc.shown_view_at(s);
            if view.file == shown.file {
                continue;
            }
            if let Ok((code, first)) = doc.code_at(s)
                && let Some(n) = line_of(&code, first)
            {
                return Error::Invalid(format!(
                    "«{text}» is in {} at L{n}, shown at step {s}.",
                    doc.file_label(&view.file)
                ));
            }
        }
        let searched = shown.label().unwrap_or_else(|| {
            let name = doc.file_label(&None);
            match &doc.code_src {
                tiza_core::model::CodeSource::File {
                    lines: Some(lines), ..
                } => format!("{name}:{lines}"),
                _ => name,
            }
        });
        Error::Invalid(format!(
            "«{text}» is not in the code shown ({searched}). L:text skips notes and labels."
        ))
    }

    fn cmd_arrow(&mut self, rest: &str) -> Result<String, Error> {
        let (from, rest) = split_first(rest);
        let (to, rest) = split_first(rest);
        if from.is_empty() || to.is_empty() {
            return Err(Error::Invalid(
                "usage: :arrow L:text L:text [label] [stroke=dashed] [heads=none,arrow] [route=sharp] [effect=draw|fade]"
                    .into(),
            ));
        }
        let (label, mut kv) = split_trailing_kv(rest);
        let effect = take_effect(&mut kv)?;
        let mut line = tiza_core::model::LineStyle::default();
        for (k, v) in &kv {
            apply_line_kv(&mut line, k, v)?;
        }
        let f = self.resolve_anchor_strict(&parse_where(from, self.cursor_line)?)?;
        let t = self.resolve_anchor_strict(&parse_where(to, self.cursor_line)?)?;
        let id = self.add_annotation_full(
            None,
            None,
            None,
            None,
            None,
            effect,
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
    /// (`.md`, fenced block), SVG, PNG or WebP. Without a path: `<doc>-stepN.txt`
    /// next to the document.
    fn export_command(&mut self, rest: &str) -> Result<String, Error> {
        let (path, kv) = split_trailing_kv(rest);
        // `step=`, when given; else the step on screen (a new empty step
        // after the last one too, see `:step`).
        let mut asked = None;
        let mut width = self.take_width();
        let mut theme = self.theme.clone();
        for (k, v) in kv {
            match k.as_str() {
                "step" => {
                    asked = Some(
                        v.parse::<u32>()
                            .map_err(|_| Error::Invalid("step= must be a number".into()))?,
                    );
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
        // A `step=` past the last one: the last, said in the message, as
        // `tiza render` and `export_image` say it.
        let last = self.doc.max_step();
        let past = asked
            .filter(|&s| s > last)
            .map(|s| format!("step {s} does not exist (the document has steps 0 to {last}); "));
        let step = asked.map_or(self.doc.step, |s| s.min(last));
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
            Self::typed_path(path)?
        };
        let format = ExportFormat::from_path(&path).ok_or_else(|| {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            if matches!(ext.as_deref(), Some("gif" | "mp4" | "webm" | "cast" | "tape")) {
                Error::Invalid(format!(
                    "{}: record every step from the shell with `tiza render --out {}`, which records this instance with no FILE",
                    path.display(),
                    path.display()
                ))
            } else {
                Error::Invalid(
                    "the path's extension picks the format (.txt .ansi .md .svg .png .webp)".into(),
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
                // `:export` never draws the live pointer.
                pointer: None,
                line_numbers: self.numbers(),
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
            ExportFormat::Svg | ExportFormat::Png | ExportFormat::Webp => {
                let f = match format {
                    ExportFormat::Svg => ImageFormat::Svg,
                    ExportFormat::Webp => ImageFormat::Webp,
                    _ => ImageFormat::Png,
                };
                let exporter = self.image_exporter.as_ref().ok_or_else(|| {
                    Error::Invalid(
                        "SVG/PNG/WebP export is not available in this build; use `tiza render`"
                            .into(),
                    )
                })?;
                exporter(&l.canvas, &theme, f, title.as_deref()).map_err(Error::Invalid)?
            }
        };
        Self::make_parent(&path)?;
        std::fs::write(&path, &bytes)
            .map_err(|e| Error::Invalid(format!("could not write {}: {e}", path.display())))?;
        Ok(format!(
            "{}exported step {step} to {} ({} bytes){}",
            past.unwrap_or_default(),
            Self::full_path(&path),
            bytes.len(),
            theme_notice.map(|n| format!("; {n}")).unwrap_or_default()
        ))
    }

    /// `:set ID key=value…` — changes properties of an existing annotation:
    /// `label`, `color`, `step` for any (`text` is the paragraph of a
    /// callout, the label of the rest); `stroke`, `route`, `heads`, `start`,
    /// `end` for arrows.
    fn cmd_set(&mut self, rest: &str) -> Result<String, Error> {
        let (id, rest) = split_first(rest);
        let kv = parse_kv(rest);
        if id.is_empty() || kv.is_empty() {
            return Err(Error::Invalid(
                "usage: :set ID key=value…   (label, color, step, effect (draw, fade, none); callouts: text, title; arrows: stroke, route, heads, start, end; boxes/callouts: border, corners, fill; highlights/marks: emphasis)"
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
        let is_callout = matches!(
            self.doc.find_annotation(id).map(|a| &a.shape),
            Some(Shape::Callout { .. })
        );
        // On a callout, `text` is its paragraph and `label` or `title` its
        // title; on the rest, `text` is the label (the text of a note).
        let mut callout_text: Option<String> = None;
        let mut p = UpdateParams {
            id: id.to_string(),
            step: None,
            color: None,
            label: None,
            effect: None,
            file: None,
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
                "text" if is_callout => callout_text = Some(v.clone()),
                "label" | "text" | "title" => p.label = Some(v.clone()),
                "color" => p.color = Some(v.clone()),
                "effect" => p.effect = Some(take_effect(&mut vec![(k.clone(), v.clone())])?),
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
        self.update_annotation_and_text(&p, callout_text.as_deref())?;
        Ok(format!("{id}: updated"))
    }

    fn cmd_highlight(&mut self, rest: &str) -> Result<String, Error> {
        // `emphasis=underline` (or `style=`) may trail the label.
        let (rest, mut kv) = split_trailing_kv(rest);
        let effect = take_effect(&mut kv)?;
        let mut emphasis = Emphasis::default();
        for (k, v) in &kv {
            match k.as_str() {
                "emphasis" | "style" => emphasis = parse_emphasis(v)?,
                _ => {
                    return Err(Error::Invalid(
                        "unknown key for :hl (emphasis=fill|underline|brackets|strike, effect=draw|fade)"
                            .to_string(),
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
                        "usage: :hl L:text [label] [effect=draw|fade]   (L:text = the text on line L; L: alone = the whole line; .:text = the cursor line)"
                            .into(),
                    ));
                }
                let w = parse_where(at, self.cursor_line)?;
                (
                    self.resolve_span_strict(&w, tiza_core::model::Edges::Both)?,
                    label,
                )
            }
        };
        let id = self.add_annotation_full(
            None,
            None,
            None,
            None,
            None,
            effect,
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
        let (rest, mut kv) = split_trailing_kv(rest);
        let effect = take_effect(&mut kv)?;
        let mut emphasis = Emphasis::default();
        for (k, v) in &kv {
            match k.as_str() {
                "emphasis" => emphasis = parse_emphasis(v)?,
                _ => {
                    return Err(Error::Invalid(
                        "unknown key for :mark (emphasis=fill|underline|brackets|strike, effect=draw|fade)"
                            .to_string(),
                    ));
                }
            }
        }
        let (kind_tok, rest) = split_first(rest);
        let (at, label) = split_first(rest);
        if kind_tok.is_empty() || (at.is_empty() && self.pending_sel.is_none()) {
            return Err(Error::Invalid(format!(
                "usage: :mark KIND[@COLOR] L:text [label] [effect=draw|fade]   KIND: {} | a :style name | a glyph",
                self.kind_names().join(" ")
            )));
        }
        let (kind_tok, color) = match kind_tok.rsplit_once('@') {
            Some((k, c)) if !k.is_empty() => (k, Some(c.to_string())),
            _ => (kind_tok, None),
        };
        let known = self.doc.styles.contains_key(kind_tok)
            || self.personal_styles.contains_key(kind_tok)
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
                    self.kind_names().join(" ")
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
                        "usage: :mark KIND[@COLOR] L:text [label] [effect=draw|fade]   (L:text = the text on line L; L: alone = the whole line)   KIND: {} | a :style name | a glyph",
                        self.kind_names().join(" ")
                    )));
                }
                let w = parse_where(at, self.cursor_line)?;
                (
                    self.resolve_span_strict(&w, tiza_core::model::Edges::Both)?,
                    label,
                )
            }
        };
        if inline {
            style.legend = opt(label);
        }
        // A personal style the document does not have yet: the mark
        // copies it in.
        let personal = (!inline && !self.doc.styles.contains_key(&kind))
            .then(|| self.personal_styles.get(&kind))
            .flatten()
            .filter(|_| tiza_core::model::builtin_kind(&kind).is_none())
            .cloned();
        let legend = self
            .doc
            .resolve_style(&kind, &style)
            .or_else(|| personal.as_ref().map(|p| p.merged(&style)))
            .and_then(|st| st.legend)
            .unwrap_or_else(|| kind.clone());
        let copied = personal.is_some().then(|| kind.clone());
        let id = self.add_annotation_full(
            None,
            None,
            None,
            None,
            None,
            effect,
            Shape::Mark {
                kind,
                span,
                label: if inline { None } else { opt(label) },
                style,
                emphasis,
            },
        )?;
        Ok(match copied {
            Some(name) => format!(
                "{id}: {legend} at L{} (style {name} from config.toml)",
                span.line
            ),
            None => format!("{id}: {legend} at L{}", span.line),
        })
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
            let personal: Vec<String> = self
                .personal_styles
                .iter()
                .filter(|(n, _)| !self.doc.styles.contains_key(*n))
                .map(|(n, st)| format!("{n} {}", st.glyph.as_deref().unwrap_or("")))
                .collect();
            if !personal.is_empty() {
                out.push(format!("personal: {}", personal.join("  ")));
            }
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
        let mut st = self.style_to_change(name);
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
        let mut effect = None;
        for tok in rest.split_whitespace() {
            if tok.starts_with("effect=") {
                effect = take_effect(&mut vec![split_token(tok)])?;
            } else if tok.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                range = Some(parse_range(tok)?);
            } else {
                ids.push(tok.to_string());
            }
        }
        if range.is_none() && ids.is_empty() {
            return Err(Error::Invalid(
                "usage: :spot A-B [ID…] [effect=draw|fade]  |  :spot ID… [effect=draw|fade]".into(),
            ));
        }
        let id = self.add_annotation_full(
            None,
            None,
            None,
            markers,
            None,
            effect,
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
                "usage: :callout A[-B] [@below|@margin|@auto] [title=\"…\"] text [effect=draw|fade]   (or @'start'..@'end')   ({m})"
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
        let (text, mut kv) = split_trailing_kv(rest);
        let effect = take_effect(&mut kv)?;
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
            effect,
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
                Error::Invalid(
                    "usage: :note L text [effect=draw|fade]  (L may be . for the cursor line)"
                        .into(),
                )
            })?
        };
        // Only a last word `effect=…` is the effect: the text of a note
        // can end in any other `key=value` (`x=1`).
        let (text, effect) = match text.trim_end().rsplit_once(char::is_whitespace) {
            Some((before, last)) if last.starts_with("effect=") => (
                before.trim_end(),
                take_effect(&mut vec![split_token(last)])?,
            ),
            None if text.trim_end().starts_with("effect=") => {
                ("", take_effect(&mut vec![split_token(text.trim_end())])?)
            }
            _ => (text, None),
        };
        if text.is_empty() {
            return Err(Error::Invalid("the note is empty".into()));
        }
        let id = self.add_annotation_full(
            None,
            None,
            None,
            None,
            None,
            effect,
            Shape::Note {
                line,
                text: text.into(),
            },
        )?;
        Ok(format!("{id}: note at L{line}"))
    }

    // ---------- local API ----------

    /// Answers one socket request: a panic while it runs (a bug)
    /// answers an `internal` error instead of closing the presenter's TUI,
    /// as the release profile's `panic = "unwind"` promises. The request is
    /// dropped whole: everything it could change in the instance — the
    /// document, the view (step, cursor, selection, scroll, focus, pointer,
    /// a typing animation it started), the mode and overlays, theme, save
    /// path and autosave, follow (a pause and the change it waits for, and
    /// what a load of the file wrote), supervision, the alerts — goes back
    /// to what it was ([`RequestState`]), the undo entries it recorded are
    /// forgotten, and the document is saved again when the request had
    /// saved a half-done change. The crash hook leaves the terminal alone
    /// meanwhile ([`answering_request`]); the panic's place and message go
    /// to `panics.log` in Tiza's runtime directory, and nothing of it is
    /// shown or returned (it may quote the document).
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
            // a new revision, and the file as the document is again. With
            // no save (autosave off), nothing on disk changed: the document
            // is as saved, or not, as before.
            let dirty = self.dirty;
            self.touch();
            if self.dirty && !self.autosave {
                self.dirty = dirty;
            }
            // An effect that played goes on at the new revision (at another
            // one it is over).
            if let Some(e) = &mut self.effect
                && e.revision == revision
            {
                e.revision = self.revision;
            }
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
        // Test hook, only with the `test-hooks` feature: a panic on
        // purpose after the request did its work, for the tests that a
        // caught panic leaves the instance as it was.
        #[cfg(feature = "test-hooks")]
        if crate::test_hook(&format!("request:{}", req.method)) {
            panic!("TIZA_TEST_HOOK at {}", req.method);
        }
        match result {
            Ok(v) => Response::ok(id, v),
            Err(e) => Response::err(id, e.code(), e.to_string()),
        }
    }

    /// Entry point of the event loop: answers now, or (supervised mode and a
    /// mutating method) parks the request as a proposal until the presenter
    /// decides. Read-only methods, `supervise.status`, a `supervise.set` that
    /// turns supervised mode on and a method that is not in `METHODS` are
    /// never parked.
    pub fn handle_ipc(&mut self, req: Request, reply: std::sync::mpsc::Sender<Response>) {
        self.handle_ipc_at(req, reply, std::time::Instant::now());
    }

    /// [`App::handle_ipc`] at `now`, the clock that groups the messages
    /// about agent changes the view does not show; tests pass
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
        // An unknown method is answered at once: there is nothing to
        // review.
        // `supervise.set` that turns supervised mode off waits too: an
        // agent must not end the review on its own.
        let parked = self.supervised
            && !READ_ONLY_METHODS.contains(&req.method.as_str())
            && (!req.method.starts_with("supervise.") || turns_supervision_off(&req))
            && METHODS.iter().any(|m| m.0 == req.method);
        // A `scene.clear` or a `supervise.set` with parameters that it
        // cannot read, and a `scene.put` with a newer schema, are refused
        // now, not parked as a change that would not happen. Other bad
        // parameters still fail when the presenter accepts the change.
        if parked && let Some(e) = refused_before_review(&req) {
            let _ = reply.send(Response::err(req.id, e.code(), e.to_string()));
            return;
        }
        if !parked {
            let verb = annotation_verb(&req.method);
            let before = self.message.clone();
            let revision = self.revision;
            let resp = self.handle_request(req);
            // An update that changed nothing (the revision stays) is no
            // change to tell.
            if let (Some(verb), Some(r)) = (verb, &resp.result)
                && self.revision != revision
            {
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
        // It waits as a chip on the bar: nothing opens over the code while
        // the presenter types (crates/tiza-tui/UI-GUIDELINES.md, section 11).
        self.info(format!(
            "{} agent change{} to review: :review shows {}",
            self.proposals.len(),
            if self.proposals.len() == 1 { "" } else { "s" },
            if self.proposals.len() == 1 {
                "it"
            } else {
                "them"
            }
        ));
    }

    /// Test hook, only with the `test-hooks` feature: the hold of the live
    /// checks of this instance ([`crate::checks::Hold`]), so that a test
    /// can ask for more checks while the first one runs. The test takes it
    /// once and sets it from its own thread, not from the TUI thread.
    #[cfg(feature = "test-hooks")]
    pub fn live_check_hold(&self) -> std::sync::Arc<crate::checks::Hold> {
        self.checks.hold()
    }

    /// `check`: answered from a thread of its own on a copy of the
    /// document as it is now, so the screen, the keys and the other
    /// requests do not wait for it ([`crate::checks`]). Its parameters are
    /// read here: a bad width is answered at once.
    fn ask_check(&mut self, req: Request, reply: std::sync::mpsc::Sender<Response>) {
        let width = match check_width(req.params) {
            // The width the check uses: `{}` and `{"width": 100}` are one
            // check and share one run. With no width, the config file's, as
            // `tiza check` does.
            Ok(w) => w
                .or(self.config_defaults.width)
                .unwrap_or(tiza_core::check::DEFAULT_WIDTH),
            Err(e) => {
                let _ = reply.send(Response::err(req.id, e.code(), e.to_string()));
                return;
            }
        };
        let base = self.save_path.parent().map(Path::to_path_buf);
        let doc = &self.doc;
        // The gutter the screen shows, also when the config file decides it.
        let numbers = self.numbers();
        let snapshot = || {
            let mut d = doc.clone();
            d.numbers = Some(numbers);
            d
        };
        self.checks
            .ask(req.id, reply, width, self.revision, snapshot, base);
    }

    /// An agent's `annotation.add` / `annotation.update` (socket or MCP,
    /// never the TUI) left `result`: when the view does not show it, the bar
    /// says so — `a17: box 3-5 added by agent (step 1, press n)` — and
    /// changes within [`AGENT_NOTICE_GROUP`] of the last one join it
    /// (`3 agent annotations not shown (step 1, press n)`). In presentation
    /// mode nothing is drawn: it waits for the bar. An error the
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

    /// The bar's text for agent changes, as the view stands now:
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
    /// step hint when it appears later (` (step 1, press n)`), the file
    /// when a link shows another one (` (src/queue.rs, not shown at step
    /// 2)`), else the reason in words.
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
        self.accept_proposal_at(0)
    }

    /// Applies the pending proposal at `i` in the queue.
    pub fn accept_proposal_at(&mut self, i: usize) -> Option<String> {
        if i >= self.proposals.len() {
            return None;
        }
        let p = self.proposals.remove(i);
        let mutates_annotation = annotation_verb(&p.req.method).is_some();
        let turns_off = turns_supervision_off(&p.req);
        let rejected_before = self.decided.1;
        let resp = self.handle_request(p.req);
        let ok = resp.error.is_none();
        // Accepted but not on screen: the acceptance says where.
        let tail = match (&resp.result, mutates_annotation) {
            (Some(r), true) => mutated_id(r)
                .and_then(|id| self.not_shown(&id, &mut None))
                .map(|why| self.hidden_tail(&why, &p.summary))
                .unwrap_or_default(),
            _ => String::new(),
        };
        // A turn-off rejects what still waited: the presenter is told.
        let rejected = self.decided.1 - rejected_before;
        let tail = if turns_off && rejected > 0 {
            format!(
                "{tail}; {rejected} waiting change{} rejected",
                if rejected == 1 { "" } else { "s" }
            )
        } else {
            tail
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
        self.reject_proposal_at(0)
    }

    /// Rejects the pending proposal at `i` in the queue.
    pub fn reject_proposal_at(&mut self, i: usize) -> Option<String> {
        if i >= self.proposals.len() {
            return None;
        }
        let p = self.proposals.remove(i);
        let _ = p.reply.send(Response::err(
            p.req.id.clone(),
            "rejected",
            "the presenter rejected this change (supervised mode)",
        ));
        self.decided.1 += 1;
        self.close_review_if_empty();
        Some(format!("rejected p{}: {}", p.id, p.summary))
    }

    /// Accepts (`accept`) or rejects the waiting proposals with these ids,
    /// in their order in the queue; how many it decided.
    pub fn decide_proposals(&mut self, ids: &[u64], accept: bool) -> usize {
        // An accepted turn-off rejects what still waits: the presenter's
        // other choices in the same decision go first.
        let mut ids = ids.to_vec();
        if accept {
            ids.sort_by_key(|id| {
                self.proposals
                    .iter()
                    .find(|p| p.id == *id)
                    .is_some_and(|p| turns_supervision_off(&p.req))
            });
        }
        let mut n = 0;
        for id in &ids {
            let Some(i) = self.proposals.iter().position(|p| p.id == *id) else {
                continue;
            };
            let done = if accept {
                self.accept_proposal_at(i)
            } else {
                self.reject_proposal_at(i)
            };
            n += usize::from(done.is_some());
        }
        n
    }

    /// Supervised mode just went off (`:supervise off`, or an agent's
    /// `supervise.set` that the presenter accepted): each waiting change is
    /// answered now. One that asked for this very thing gets the status, as
    /// it happened; the others are rejected, as the presenter did not decide
    /// them. Returns how many it rejected.
    fn answer_proposals_when_off(&mut self) -> usize {
        let mut rejected = 0;
        for p in std::mem::take(&mut self.proposals) {
            let resp = if turns_supervision_off(&p.req) {
                let mut st = self.supervise_status();
                st["message"] = json!("supervised mode off");
                Response::ok(p.req.id.clone(), st)
            } else {
                rejected += 1;
                self.decided.1 += 1;
                Response::err(
                    p.req.id.clone(),
                    "rejected",
                    "supervised mode went off before the presenter decided this change",
                )
            };
            let _ = p.reply.send(resp);
        }
        self.close_review_if_empty();
        rejected
    }

    fn close_review_if_empty(&mut self) {
        if self.proposals.is_empty() && self.overlay == Overlay::Review {
            self.close_overlay();
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
                    "agent change p{pid} expired after {}s without a decision (supervised mode)",
                    PROPOSAL_TIMEOUT.as_secs()
                ),
            ));
            self.decided.1 += 1;
        }
        self.close_review_if_empty();
        self.info(format!(
            "{} agent change{} expired without a decision",
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
            let n = self.answer_proposals_when_off();
            if n > 0 {
                format!(
                    "supervised mode off; {n} waiting agent change{} rejected",
                    if n == 1 { "" } else { "s" }
                )
            } else {
                "supervised mode off".into()
            }
        }
    }

    pub fn supervise_status(&self) -> Value {
        // A method the API does not have is never repeated.
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
                #[allow(unused_mut)]
                let mut st = serde_json::to_value(self.status())?;
                // Only in the tests' build: the live checks that wait in
                // the hold, and the requests that wait for the one that
                // runs.
                #[cfg(feature = "test-hooks")]
                {
                    st["checks"] = json!({
                        "held": self.checks.hold().held(),
                        "waiters": self.checks.running_waiters(),
                    });
                }
                st
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
                // Turned on paused, where a save waits: the presenter is
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
                // The characters the screen does not show as they are:
                // `text` keeps them (a `find` copies them), this names them.
                let mut invisible = Vec::new();
                for (i, text) in view.doc.code.iter().enumerate() {
                    let line = self.doc.source.first_line + i as u32;
                    out.push_str(&format!("{line:>4}  {text}\n"));
                    for (col, c) in tiza_core::printable::unseen(text) {
                        invisible.push(json!({"line": line, "col": col, "code_point": format!("U+{:04X}", c as u32), "name": tiza_core::printable::char_name(c)}));
                    }
                }
                json!({"language": self.doc.source.language, "file": self.shown_file(), "view": self.shown_view(), "first_line": self.doc.source.first_line, "last_line": self.doc.last_line(), "text": out, "invisible": invisible})
            }
            "scene.get" => {
                if !params.is_null() {
                    let _: tiza_core::protocol::SceneGetParams = parse(params)?;
                }
                // On the wire the code travels embedded: a client in another
                // directory could not resolve a relative file reference;
                // linked files travel embedded too.
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
                tiza_core::moves::refuse_unknown_keys(&params, "")?;
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
                if let Some(f) = &p.file {
                    p.file = Some(self.canonical_file(f));
                }
                let (shape, markers) = p.shape.resolve_with_markers(&self.doc)?;
                let id = self
                    .add_annotation_full(p.id, p.step, p.color, markers, p.file, p.effect, shape)?;
                // Where it stands against the view right now.
                serde_json::to_value(self.mutated(id))?
            }
            "annotation.update" => {
                let p: UpdateParams = parse(params).map_err(update_key_hint)?;
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
                let mut st = self.style_to_change(name);
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
                let personal: BTreeMap<&String, &MarkStyle> = self
                    .personal_styles
                    .iter()
                    .filter(|(k, _)| !self.doc.styles.contains_key(*k))
                    .collect();
                json!({
                "document": self.doc.styles,
                "personal": personal,
                "builtin": BUILTIN_KINDS.iter().map(|(k, g, l, c)| json!({"kind": k, "glyph": g, "legend": l, "color": c})).collect::<Vec<_>>(),
                "aliases": tiza_core::model::KIND_ALIASES.iter().map(|(a, k)| json!({"alias": a, "kind": k})).collect::<Vec<_>>(),
                })
            }
            "supervise.set" => {
                let p = supervise_params(&params)?;
                let msg = self.set_supervised(p.enabled);
                let mut st = self.supervise_status();
                st["message"] = json!(msg);
                st
            }
            "supervise.status" => self.supervise_status(),
            "scene.put" => {
                // The schema as when a file opens, before the document
                // changes: a newer one would be saved as it is, in a file
                // that this version cannot open; an older one is migrated.
                let mut params = params;
                if let Some(doc) = params.get_mut("document") {
                    fs::migrate_document_value(doc)?;
                }
                // A key of a move that this version does not know is an
                // error, never lost without a word.
                let moves = params
                    .get("document")
                    .and_then(|d| d.get("moves"))
                    .and_then(|m| m.as_array());
                for (i, m) in moves.into_iter().flatten().enumerate() {
                    tiza_core::moves::refuse_unknown_keys(m, &format!("move #{}: ", i + 1))?;
                }
                let mut p: PutParams = parse(params)?;
                p.document.schema = tiza_core::model::SCHEMA_VERSION;
                serde_json::to_value(self.put_scene(p.document)?)?
            }
            "scene.patch" => {
                let p: PatchParams = parse(params)?;
                serde_json::to_value(self.patch_scene(p)?)?
            }
            "scene.clear" => {
                let step = clear_params(&params)?.step;
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
                self.step_to_asked(p.step);
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
                        &tiza_core::check::CheckOptions {
                            width: width.or(self.config_defaults.width),
                            numbers: self.config_defaults.numbers,
                        },
                    ),
                    revision: self.revision,
                })?
            }
            "render.text" => {
                // The take, as `tiza render --format txt` writes it: without a
                // width, at the screen's, which the reply of `annotation.add`
                // (`visible`, `clipped`) and `check` at that width judge.
                let (width, layout) = match params.get("width").and_then(|w| w.as_u64()) {
                    Some(w) => {
                        let w =
                            tiza_core::layout::check_width(w.min(u64::from(u32::MAX)) as usize)?;
                        (w, self.take_at(w))
                    }
                    None => self.take_at_screen(),
                };
                json!({"text": layout.canvas.to_plain_text(), "width": width})
            }
            "focus.set" => {
                // A line given is a line number (a negative one is an error,
                // never a silent clear or cut of the focus); `null` or absent
                // is no line. The value is never repeated.
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
                // clamped, so the reply gives the focus as set, not the numbers sent.
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
                        "`theme` must be a string: dark, light, terminal, auto or toggle".into(),
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
            width: self.take_width(),
            rows: self.size.1,
            warnings: {
                let mut w = self.viewport_warnings();
                w.extend(self.doc.step_warnings());
                w.extend(self.not_known.clone());
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
            numbers: self.numbers(),
            margin: self.doc.margin,
            pointer: self.pointer,
            stale_moves: tiza_core::stale::count(&self.doc),
            save_error: self.save_error.clone(),
            follow: self
                .follow
                .is_some()
                .then(|| tiza_core::protocol::FollowState {
                    paused: self.follow_paused(),
                    pending: self.follow_pending(),
                    resume_step: self.resume_step(),
                    reads: self.follow.as_ref().map_or(0, |f| f.reads),
                    hash: self.follow.as_ref().and_then(|f| f.hash.clone()),
                }),
            effect: self.effect.is_some(),
        }
    }

    /// Warnings about the current viewport: code lines clipped, labels or
    /// notes of the step cut or not drawn, labels under the lines for lack
    /// of a margin. Empty when the size is unknown (0×0) or everything fits.
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
            .as_ref()
            .and_then(|(k, facts)| (*k == key).then(|| facts.clone()));
        let facts = known.unwrap_or_else(|| {
            let facts = ViewportFacts::of(&self.layout(width));
            *self.viewport.borrow_mut() = Some((key, facts.clone()));
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
        if let Some(w) = l.code_needs {
            let need = self.columns_for(w);
            out.push(format!("code needs {need} columns at {cols} columns"));
        }
        // Only what the step on screen really loses: the size alone is no
        // warning.
        let n = l.clipped_labels.len();
        if n > 0 {
            let mut ids: Vec<String> = l
                .clipped_labels
                .iter()
                .take(5)
                .map(|(id, dropped)| {
                    if *dropped {
                        format!("{id} (not drawn)")
                    } else {
                        id.clone()
                    }
                })
                .collect();
            if n > 5 {
                ids.push("…".into());
            }
            out.push(format!(
                "{n} label{} cut at {cols} columns: {}",
                if n == 1 { "" } else { "s" },
                ids.join(", ")
            ));
        }
        // A fact for agents (keep labels short), not a loss: no badge.
        if !l.labels_in_margin && self.doc.annotations.iter().any(|a| a.step <= self.doc.step) {
            out.push(format!("labels go under the lines at {cols} columns"));
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
/// cut a long message.
pub fn step_hint_start(msg: &str) -> Option<usize> {
    let i = msg.rfind(" (")?;
    let inner = msg[i + 2..].strip_suffix(')')?;
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    // ` (src/queue.rs, not shown at step 2)`: an agent change whose file a
    // link hides.
    if let Some((file, n)) = inner.rsplit_once(", not shown at step ") {
        // A file may have ` (` in its name (`src/stack (v2).rs`): the tail
        // starts at the first ` (` after which the file's parentheses
        // balance.
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
/// it lists the valid names and never repeats the one given (a closed
/// value's error quotes nothing the caller wrote).
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
    /// A button ended an enter effect, as a key does, and did nothing else.
    EndedEffect,
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
        let text = self.doc.line_text(line).unwrap_or("");
        let mut w = 0usize;
        let mut col = 0u32;
        for g in text.graphemes(true) {
            // As the line is drawn: an invisible character has its mark.
            let gw = if g == "\t" {
                self.hit.tab_width - (w % self.hit.tab_width)
            } else {
                tiza_core::canvas::code_grapheme_width(g).max(1)
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
    /// cursor three lines (which scrolls). In presentation mode the wheel
    /// moves the view, and a click or a drag does nothing.
    pub fn on_mouse(&mut self, kind: MouseKind, column: u16, row: u16) -> MouseOutcome {
        if !self.mouse || self.overlay != Overlay::None {
            return MouseOutcome::Ignored;
        }
        self.resolve_pending_view();
        // Presentation mode hides the status bar, the cursor and the
        // selection, so annotating with the mouse there would be blind: a
        // click or a drag does nothing (back in normal mode it works again).
        // The wheel moves the view three rows, as ↓ and ↑ do: with the live
        // pointer on, it moves the pointer.
        if self.present {
            let rows = match kind {
                MouseKind::ScrollDown => 3,
                MouseKind::ScrollUp => -3,
                _ => return MouseOutcome::Ignored,
            };
            if self.mode != Mode::Normal {
                return MouseOutcome::Ignored;
            }
            // A notch hides the result of a `:` command, as a key does: so
            // do ↓ and ↑ when the terminal sends the wheel as keys.
            self.present_message = None;
            if self.pointer.is_none() {
                self.view_by(rows);
            } else if self.is_typing() {
                // As ↓ and ↑ with the pointer on: the typing ends.
                self.finish_typing();
                self.revision += 1;
            } else if self.is_playing_effect() {
                // So does an effect, and the notch does nothing else.
                self.finish_effect();
                return MouseOutcome::EndedEffect;
            } else {
                self.move_cursor(rows);
            }
            return MouseOutcome::Scrolled;
        }
        // A button ends an enter effect and does nothing else, as a key
        // does; the wheel lets it play, as the keys that only move the view.
        // With the live pointer on, the wheel would move the pointer, and a
        // change of the pointer ends the effect: it ends it and does
        // nothing else, as a key does.
        if self.is_playing_effect()
            && (matches!(
                kind,
                MouseKind::Down | MouseKind::Drag | MouseKind::OtherButton
            ) || (self.pointer.is_some()
                && matches!(kind, MouseKind::ScrollDown | MouseKind::ScrollUp)))
        {
            self.finish_effect();
            self.drag_from = None;
            return MouseOutcome::EndedEffect;
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
            MouseKind::OtherButton => MouseOutcome::Ignored,
            MouseKind::Down => {
                let Some((line, col)) = self.hit_test(column, row) else {
                    return MouseOutcome::Ignored;
                };
                if let Mode::Command(_) = self.mode {
                    return MouseOutcome::Ignored;
                }
                // A new place on the code: the selection a `:` command could
                // still take is not the one meant any more.
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
    /// The right or the middle button, pressed or dragged.
    OtherButton,
}

/// `added` / `updated` for the methods that add or change one annotation,
/// `None` for the rest.
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
/// confirms it: `box 3-5`, `note L13`, `arrow L2 → L6`.
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
/// that is none of its names (a `type`, a method) is never repeated:
/// it reads [`tiza_core::error::ELIDED`].
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
        "supervise.set" => "turn supervised mode off".into(),
        "style.set" => format!("define style {}", s("name")),
        "move.add" => {
            // A patch by its op; any other move by its action.
            let what = [s("op"), s("action")]
                .into_iter()
                .find(|w| !w.is_empty())
                .unwrap_or_else(|| "code.patch".into());
            format!("add a {what} move at step {}", n("at_step").unwrap_or(0))
        }
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
    /// A lossless still, with the pixels of the PNG.
    Webp,
}

/// Renders a canvas to SVG, PNG or WebP bytes (`title` goes into the SVG
/// metadata).
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
    Webp,
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
            Some("webp") => Some(ExportFormat::Webp),
            _ => None,
        }
    }
}

/// Takes the `effect=` entries out of `kv`: `draw`, `fade` or `none`
/// (no effect). The last one counts.
fn take_effect(kv: &mut Vec<(String, String)>) -> Result<Option<Effect>, Error> {
    let mut effect = None;
    // The last one wins, as with the other keys.
    for (_, v) in kv.iter().filter(|(k, _)| k == "effect") {
        effect = Effect::parse(v).ok_or_else(|| Error::Invalid(UNKNOWN_EFFECT.into()))?;
    }
    kv.retain(|(k, _)| k != "effect");
    Ok(effect)
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
                "unknown key (border, corners, fill, effect)".to_string(),
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
                "unknown key (stroke, route, heads, start, end, effect)".to_string(),
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

/// An unknown key `text` of `annotation.update`, with what to send
/// instead: `text` is the `label` of a note, and a new `shape` for a
/// callout, whose `label` is its title (no alias). A key one or two letters
/// from an accepted one (`colour`) already names that one ([`parse`]).
fn update_key_hint(e: Error) -> Error {
    let Error::Invalid(msg) = &e else {
        return e;
    };
    // Only a key `text` of `annotation.update` itself: inside a position,
    // `text` is `find` (`moves::position_key_hint`).
    if !msg.contains("unknown field `text`, expected one of `id`") {
        return e;
    }
    Error::Invalid(format!(
        "{msg}. Use `label` to change the text of a note or the title of a callout. To change the text of a callout, send the whole callout as a new `shape`."
    ))
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

/// The parameters of `scene.clear`: no `step` (every annotation) or a
/// `step` that is a whole number, 0 or more. Anything else is refused, so
/// that a typo never clears the whole scene.
fn clear_params(params: &Value) -> Result<tiza_core::schema::ClearParams, Error> {
    // serde would read an array (`[]`) as a struct with no step.
    if !params.is_object() {
        return Err(Error::Invalid(format!(
            "{}send a JSON object. `{{}}` clears every annotation, and `{{\"step\": N}}` clears step N",
            tiza_core::error::INVALID_PARAMETERS
        )));
    }
    if let Some(step) = params.get("step")
        && !step.as_u64().is_some_and(|s| s <= u32::MAX as u64)
    {
        return Err(Error::Invalid(format!(
            "{}step must be a whole number, 0 or more. Leave it out to clear every annotation",
            tiza_core::error::INVALID_PARAMETERS
        )));
    }
    parse(params.clone())
}

/// A `supervise.set` that may turn supervised mode off: the one setting
/// change that waits for the presenter. Anything but a valid "on" counts,
/// so that no form of the parameters gets past the review; wrong ones are
/// then refused before it.
fn turns_supervision_off(req: &Request) -> bool {
    req.method == "supervise.set" && supervise_params(&req.params).map_or(true, |p| !p.enabled)
}

/// The parameters of `supervise.set`: a JSON object with `enabled`. serde
/// would read an array (`[false]`) as the struct, past the review.
fn supervise_params(params: &Value) -> Result<tiza_core::schema::SuperviseParams, Error> {
    if !params.is_object() {
        return Err(Error::Invalid(format!(
            "{}send a JSON object, such as {{\"enabled\": false}}",
            tiza_core::error::INVALID_PARAMETERS
        )));
    }
    parse(params.clone())
}

/// What makes a request fail before it changes anything, checked before
/// supervised mode parks it.
fn refused_before_review(req: &Request) -> Option<Error> {
    match req.method.as_str() {
        "scene.clear" => clear_params(&req.params).err(),
        "supervise.set" => supervise_params(&req.params).err(),
        "scene.put" => req
            .params
            .get("document")
            .cloned()
            .and_then(|mut d| fs::migrate_document_value(&mut d).err()),
        _ => None,
    }
}

/// Parameters of a socket request. Ones that do not deserialize: serde
/// quotes the value it could not read (`unknown variant `x``, `invalid
/// type: string "x"`); that value is taken out, like the MCP's own
/// deserialization errors, and serde's list of valid names stays.
fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, Error> {
    T::deserialize(&v).map_err(|e| {
        let msg = e.to_string();
        // An unknown key: also what to write instead, when a known key is
        // near (`Did you mean `col`?`, `start` is `col` in a position).
        let hint = tiza_core::error::unknown_field_hint(&msg)
            .map(|h| format!(". {h}"))
            .unwrap_or_default();
        Error::Invalid(format!(
            "{}{}{hint}",
            tiza_core::error::INVALID_PARAMETERS,
            tiza_core::error::serde_text(&msg)
        ))
    })
}

/// The width a `check` request asks for (`None`: the default).
fn check_width(params: Value) -> Result<Option<usize>, Error> {
    let p: tiza_core::schema::CheckParams = parse(params)?;
    p.width.map(tiza_core::layout::check_width).transpose()
}

/// The refusal of `draw` on a shape with no stroke to draw.
const DRAW_NEEDS_A_STROKE: &str = "effect draw needs a stroke: use fade, or put draw on a box, a callout, an arrow, or a highlight or mark with emphasis underline, brackets or strike";

/// The refusal of an effect that `:set` or an annotation command does not
/// know.
const UNKNOWN_EFFECT: &str = "unknown effect (use draw, fade or none)";

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
        // `ma` is ambiguous since `:margin`: both are offered.
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

/// The view that a `scroll` holds after its jump (see [`App::scroll_held`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollHeld {
    /// The first and the last line of the target: of a `scroll` move, or of
    /// what comes in at a step (a reveal).
    pub lines: (u32, u32),
    pub align: tiza_core::moves::Align,
    /// The annotation it shows, if any: one of a later step counts with the
    /// rows it will have ([`App::rows_to_come`]).
    pub annotation: Option<String>,
    /// The `scroll` move it comes from, by its id; `None` for a reveal
    /// alone. Only that `scroll` refreshes it.
    pub from: Option<(u32, usize)>,
    /// What comes in at the step, when it comes with the view of a `scroll`
    /// of before: the view shows it with the least move from the target.
    pub reveal: Option<(u32, u32)>,
    /// The step of the jump or of the reveal: a reveal holds the view only
    /// at its own step ([`ScrollHeld::reveals`]).
    pub step: u32,
    /// The cursor and the pointer at the jump: a change lets the view go.
    pub cursor: (u32, u32),
    pub pointer: Option<tiza_core::protocol::PointerAt>,
    /// [`App::rows_to_come`] for the target and the reveal, with what it
    /// depends on: the step, the width, a typing or a `wait` of the step,
    /// and the newest edit of the document (an agent that gives the target
    /// a label). A frame lays the code out once more only when one of them
    /// changes, not at each frame of a typing.
    pub to_come: Option<(ToComeKey, ToCome)>,
    /// With `show`: the row in the view of the first line of the target, as
    /// the first frame put it, and the size of the view on screen then (its
    /// rows and its width). Later frames of that size keep the target
    /// on that row, so it keeps its row when rows come in above it. At
    /// another size the view shows the target again from where it is.
    pub show_row: Option<(usize, (usize, usize))>,
}

/// An annotation that the screen draws ([`App::drawn_annotation_lines`]):
/// its id, its first line on the screen and its line ranges there.
type DrawnLines = (String, Option<u32>, Vec<(u32, u32)>);

/// What [`App::rows_to_come`] depends on: the step, the width, a typing or
/// a `wait` of the step, the newest edit of the document, and the stable
/// geometry of presentation mode.
pub type ToComeKey = (u32, usize, bool, u64, bool);

/// The rows that the target and the reveal of a held view do not have yet,
/// above and under each ([`App::rows_to_come`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToCome {
    pub target: (usize, usize),
    pub reveal: (usize, usize),
}

/// Whether `camera` is a camera of step `s` itself that places the view: a
/// `scroll` with a target, a `cursor` or a `code.link`. Then nothing comes
/// into the view at `s` by itself (B (a)).
fn own_camera(camera: &Camera, s: u32) -> bool {
    match camera {
        Camera::Scroll(cam) => cam.id.0 == s && cam.lines.is_some(),
        Camera::Cursor { at_step, .. } => *at_step == s,
        Camera::None => false,
    }
}

/// The rows of `lines` on `layout`: from the titles of the boxes over the
/// first line to the rows under the last one, up to the next line or the
/// top of a box that starts there. `None` when a line is not on it.
pub fn span_rows(layout: &tiza_core::layout::Layout, lines: (u32, u32)) -> Option<(usize, usize)> {
    use tiza_core::layout::Row;
    let rows = &layout.rows;
    let mut top = *layout.row_of_line.get(&lines.0)?;
    while top > 0 && matches!(rows[top - 1], Row::BoxTop(_)) {
        top -= 1;
    }
    let mut bottom = *layout.row_of_line.get(&lines.1)?;
    while bottom + 1 < rows.len() && !matches!(rows[bottom + 1], Row::Code(_) | Row::BoxTop(_)) {
        bottom += 1;
    }
    Some((top, bottom))
}

impl ScrollHeld {
    /// Whether it shows what came in at its step: a reveal alone, or the
    /// view of a `scroll` with a reveal.
    pub fn reveals(&self) -> bool {
        self.from.is_none() || self.reveal.is_some()
    }

    /// The first row in view that puts the target where `align` says, on
    /// `layout` with `visible` rows of `width` cells (the size of the view
    /// on screen), and then the reveal, if any, with the
    /// least move from there; `current` is the first row now (for `show`).
    /// `None` when the target is not on the layout. `to_come` are the rows
    /// that the target and the reveal do not have yet: the view keeps their
    /// place, so that their lines keep their rows when those rows come in.
    pub fn scroll_on(
        &mut self,
        layout: &tiza_core::layout::Layout,
        (visible, width): (usize, usize),
        current: usize,
        to_come: ToCome,
    ) -> Option<usize> {
        use tiza_core::moves::Align;
        let (top, bottom) = span_rows(layout, self.lines)?;
        let top = top.saturating_sub(to_come.target.0);
        let bottom = bottom + to_come.target.1;
        let height = bottom + 1 - top;
        let max = layout.rows.len().saturating_sub(visible);
        let size = (visible, width);
        let placed = self
            .show_row
            .filter(|(_, s)| *s == size)
            .map(|(row, _)| row);
        let at = if height >= visible {
            top
        } else {
            match self.align {
                Align::Top => top,
                // Near the start the target cannot be at the bottom.
                Align::Bottom => (bottom + 1).saturating_sub(visible),
                Align::Center => (top + bottom + 1).saturating_sub(visible) / 2,
                // Once placed, the target keeps its row.
                Align::Show if let Some(row) = placed => {
                    let first = *layout.row_of_line.get(&self.lines.0)?;
                    first.saturating_sub(row.min(visible.saturating_sub(1)))
                }
                Align::Show if top < current => top,
                Align::Show if bottom >= current + visible => bottom + 1 - visible,
                Align::Show => current,
            }
        };
        let mut at = at.min(max);
        if self.align == Align::Show && placed.is_none() {
            let first = *layout.row_of_line.get(&self.lines.0)?;
            self.show_row = first.checked_sub(at).map(|row| (row, size));
        }
        // What comes in at the step, from the view of the target: when it
        // does not fit, the start of its range.
        if let Some((rtop, rbottom)) = self.reveal.and_then(|r| span_rows(layout, r)) {
            let rtop = rtop.saturating_sub(to_come.reveal.0);
            let rbottom = rbottom + to_come.reveal.1;
            if rbottom + 1 - rtop >= visible || rtop < at {
                at = rtop;
            } else if rbottom >= at + visible {
                at = rbottom + 1 - visible;
            }
            at = at.min(max);
        }
        Some(at)
    }
}

/// A key that moves the view in presentation mode, kept until the next
/// frame applies it ([`App::view_by`], [`App::view_to`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ViewRequest {
    /// `g`/Home or `G`/End: the view starts from that end.
    pub edge: Option<ViewEdge>,
    /// Then this many rows down (up when negative).
    pub rows: i64,
}

/// An end of the code for [`ViewRequest`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewEdge {
    Top,
    Bottom,
}

/// Where the view of `top` is on `layout`, to keep it on its code when the
/// rows change: the line of the first row of code at or above row `top`,
/// and how many rows `top` is under that row. With no row of code above
/// (the title of a box on the first row), the first row of code, with a
/// negative offset. `None` with no code.
pub(crate) fn view_anchor_of(
    layout: &tiza_core::layout::Layout,
    top: usize,
) -> Option<(u32, isize)> {
    use tiza_core::layout::Row;
    let line = |i: usize| match layout.rows[i] {
        Row::Code(l) => Some((l, top as isize - i as isize)),
        _ => None,
    };
    let last = top.min(layout.rows.len().checked_sub(1)?);
    (0..=last)
        .rev()
        .find_map(line)
        .or_else(|| (0..layout.rows.len()).find_map(line))
}

/// The first row that puts `anchor` back where [`view_anchor_of`] found it,
/// on `layout`; `None` when its line is not on the layout.
pub(crate) fn view_row_of(
    layout: &tiza_core::layout::Layout,
    anchor: (u32, isize),
) -> Option<usize> {
    let row = *layout.row_of_line.get(&anchor.0)? as isize;
    Some((row + anchor.1).max(0) as usize)
}
