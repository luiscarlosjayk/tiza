//! Internal crate of [Tiza](https://github.com/luiscarlosjayk/tiza),
//! published only so that `cargo install tiza` works: it has no stable
//! API. Use the `tiza` binary.
//!
//! `tiza-tui`: the Tiza terminal interface and its local API server.

pub mod app;
pub mod appearance;
pub mod checks;
pub mod early_keys;
pub mod follow;
pub mod ipc;
pub mod keys;
pub mod list;
pub mod panel;
pub mod reanchor;
pub mod runtime;
pub mod ui;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use crossterm::{execute, queue};
use tiza_core::fs::Opened;

use crate::app::{App, Mode, MouseKind, MouseOutcome, Overlay, ViewEdge};

pub struct RunOptions {
    pub step: Option<u32>,
    pub theme: Option<tiza_core::ThemeName>,
    /// `--theme auto`: follow the terminal even when the document names a
    /// theme.
    pub theme_auto: bool,
    /// `--theme-file PATH`: per-token overrides applied as the document's.
    pub theme_file: Option<PathBuf>,
    /// The `[theme]` table of the config file (loaded by the binary).
    pub config_overrides: Option<tiza_core::theme::ThemeOverrides>,
    /// The `[defaults]` of the config file: under the document, `--mouse`,
    /// `--no-save`, `--padding` and the `:` commands.
    pub config_defaults: tiza_core::config::Defaults,
    /// The `[styles]` of the config file (personal mark styles).
    pub personal_styles: std::collections::BTreeMap<String, tiza_core::MarkStyle>,
    /// What the config file had wrong, file name and line first: the first
    /// one goes to the bar.
    pub config_warnings: Vec<String>,
    pub present: bool,
    /// `None` = no local API server.
    pub socket: Option<PathBuf>,
    pub focus: Option<(u32, u32)>,
    /// `--padding`: shown this session, never saved into the document.
    pub padding: Option<(u16, u16)>,
    /// `--follow`: reload when the source file changes on disk.
    pub follow: bool,
    /// Renders a canvas as SVG/PNG/WebP for `:export`; the TUI crate does not link
    /// the image stack, so the binary provides it (`None`: text formats only).
    pub image_exporter: Option<app::ImageExporter>,
    /// `--smoke`: draw one frame and exit (CI check that the terminal setup,
    /// `/dev/tty` with piped stdin included, starts and restores cleanly).
    pub smoke: bool,
    /// `--mouse`: start with mouse capture on (`:mouse on` toggles it live).
    pub mouse: bool,
    /// `--no-save`: never autosave the sidecar (`:w` still saves).
    pub no_save: bool,
}

/// Starts the TUI with an opened document. Returns when it closes.
pub fn run(opened: Opened, opts: RunOptions) -> io::Result<()> {
    let mut app = App::new(opened.document, opened.save_path);
    app.config_defaults = opts.config_defaults;
    app.personal_styles = opts.personal_styles;
    // Before any start-up step that could save (`--theme-file` does):
    // with `--no-save`, nothing is written. `autosave = false` in the
    // config file is `:autosave off`.
    app.autosave = !opts.no_save && app.config_defaults.autosave != Some(false);
    app.no_save = opts.no_save;
    app.layout_stable = app.config_defaults.layout == Some(tiza_core::config::LayoutMode::Stable);
    app.config_overrides = opts.config_overrides;
    app.rebuild_theme();
    if let Some(path) = &opts.theme_file {
        // Not an edit that `u` takes back, as for `--theme`.
        match app.load_theme_file(path, false) {
            Ok(m) => app.info(m),
            Err(e) => app.error(e.to_string()),
        }
        app.dirty = false;
    }
    if let Some(t) = opts.theme {
        app.set_theme_name(t, true);
    }
    if opts.theme_auto {
        app.theme_follow_host();
    }
    // The host's appearance, and how wide it draws the glyphs of Tiza's own
    // text: asked before crossterm owns the input, one `COLOR_QUERY_WAIT`
    // with no answer and about three at most, silent fallback to dark and
    // to the glyphs as they are. The colours only for a theme that follows
    // the terminal; the probe always. Keys typed meanwhile were read with
    // the answers: handled before the first frame.
    let start_up = appearance::query_host(app.theme_auto || app.theme.is_terminal());
    if let Some(host) = start_up.host {
        app.set_host(host);
    }
    app.wide = start_up.wide;
    let early: Vec<_> = start_up
        .typed
        .iter()
        .flat_map(|p| early_keys::keys(p))
        .collect();
    let cut_answer = start_up.cut;
    if let Some(s) = opts.step {
        app.set_step(s);
        // The step it opens at shows its final drawing, with no enter
        // effect, as when the document opens at its own step: also after
        // the typing of its moves.
        app.finish_effect();
        if let Some(t) = app.typing.as_mut() {
            t.then_effect = false;
        }
    }
    app.present = opts.present;
    app.reveal_at_open();
    app.focus = opts.focus;
    app.padding_override = opts.padding;
    app.image_exporter = opts.image_exporter;
    // The flag wins over the config file; the config over the built-in.
    app.mouse = opts.mouse || app.config_defaults.mouse == Some(true);
    // A bad entry of the config file was ignored: the first one on the
    // bar, file name, line and key first (the binary printed every one on
    // stderr with the full path), and how many there are: a cut keeps the
    // count and `tiza config` whole. The notice of the document comes
    // before it, never under it.
    let config_warning = opts.config_warnings.first().map(|first| {
        format!(
            "{first}{}",
            ui::config_warnings_tail(opts.config_warnings.len())
        )
    });
    // The effects and the moves this version does not know: their names
    // went to stderr as the TUI started; the bar says it in a few words, so
    // it fits.
    let notice = opened.notice.as_ref().map(|n| {
        match (
            tiza_core::fs::not_known_notice(&app.doc),
            tiza_core::fs::not_known_short(&app.doc),
        ) {
            (Some(full), Some(short)) => n.replace(&full, &short),
            _ => n.clone(),
        }
    });
    match (&notice, config_warning) {
        (Some(n), Some(c)) => app.error(format!("{n} · {c}")),
        (Some(n), None) => app.info(n.clone()),
        (None, Some(c)) => app.error(c),
        (None, None) => {}
    }
    // A warning of the load below takes the bar: what this version does
    // not know stays before it, in a few words.
    let short = tiza_core::fs::not_known_short(&app.doc);
    let after_short = |m: String| match &short {
        Some(s) => format!("{s} · {m}"),
        None => m,
    };
    // A `steps` entry past the last step is kept, never dropped in
    // silence.
    if let Some(w) = app.doc.step_warnings().first() {
        app.error(after_short(w.clone()));
    } else if let Some(r) = tiza_core::moves::ignored_rebinds(&app.doc).first() {
        // A `rebind` entry whose annotation is gone (`tiza check` lists
        // every one).
        app.info(after_short(format!("{}: {}", r.place(), r.message())));
    }
    app.show_unknown_effects(std::time::Instant::now());
    if opts.follow {
        match app.set_follow(true) {
            // At a step with a patch applied follow starts paused: said
            // unless the bar already has something from the load.
            Ok(m) if app.follow_waits().is_some() && app.message.is_none() => app.info(m),
            Ok(_) => {}
            Err(e) => app.error(e.to_string()),
        }
    }

    // Before the socket exists, so a SIGHUP never leaves it behind.
    let hangup = HangUp::install();
    let ipc = match &opts.socket {
        Some(path) => match ipc::start(path) {
            Ok(s) => {
                app.socket_path = Some(s.path.clone());
                Some(s)
            }
            Err(e) => {
                app.error(format!("local API disabled: {e}"));
                None
            }
        },
        None => None,
    };

    install_crash_hook(&app.save_path);
    // No terminal at all (a service, `setsid` with no tty): a plain error,
    // not a crash; nothing was drawn, so only the raw mode may need undoing.
    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            let _ = crossterm::terminal::disable_raw_mode();
            return Err(io::Error::other(format!(
                "the TUI needs a terminal to read keys from ({e}); use `tiza render` for text output or `tiza check` to check a document"
            )));
        }
    };
    spare_answered_requests();
    // Keys typed while the colour query waited, at the terminal's size and
    // before the mouse capture follows `app.mouse` (`:mouse on` may be one
    // of them); one that quits ends them, and the loop does not start.
    if !opts.smoke {
        if let Ok(size) = terminal.size() {
            app.size = (size.width, size.height);
        }
        for (code, mods) in early {
            on_key(&mut app, code, mods);
            if app.quit {
                break;
            }
        }
    }
    // The rest of an answer that the wait cut short comes as keys, after
    // the keys typed before it: the capture takes it, not the editor.
    if let Some(cut) = cut_answer {
        app.osc.resume(cut);
    }
    execute!(io::stdout(), EnableBracketedPaste)?;
    if app.mouse {
        let _ = execute!(io::stdout(), EnableMouseCapture);
    }
    let result = if opts.smoke {
        // One frame, then one poll of the input source: with stdin piped the
        // events come from /dev/tty (crossterm `use-dev-tty`), which is what
        // the smoke test is there to prove.
        draw_frame(&mut terminal, &mut app)
            .and_then(|_| event::poll(Duration::from_millis(100)))
            .map(|_| End::Quit)
    } else if app.quit {
        Ok(End::Quit)
    } else {
        event_loop(&mut terminal, &mut app, ipc.as_ref(), &hangup)
    };
    // The terminal went away: Ctrl-C's rules for what is saved.
    if matches!(result, Ok(End::HangUp)) {
        app.retry_save_on_exit = true;
    }
    let gone = silence_dead_stderr();
    let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture);
    ratatui::restore();
    // Lines that panics.log could not take, now that nothing draws over
    // them. They can quote the document: shown, never sent raw. With the
    // terminal gone they have nowhere to go.
    let held = std::mem::take(&mut *HELD_LOG_LINES.lock().unwrap_or_else(|p| p.into_inner()));
    if !gone {
        for line in held {
            eprintln!("tiza: {}", tiza_core::printable::printable_text(&line));
        }
    }
    // Quit over stale moves (`:q!`, Ctrl-C): their marks are lost, so say
    // what the next load does with them where it will be read.
    if let Some(s) = tiza_core::stale::close_summary(&app.doc) {
        say_on_exit(gone, &s);
    }
    // No way out writes the sidecar: with autosave on every change is on
    // disk already; with it off (or a read-only file) the author kept the
    // file as it is. Only Ctrl-C, with autosave on, tries a save that
    // failed once more. What is left unsaved is said where it will be read.
    if app.retry_save_on_exit
        && app.autosave
        && app.dirty
        && app.save_error.is_some()
        && let Err(e) = app.save()
    {
        let msg = format!("could not save {}: {e}", app.save_path.display());
        say_on_exit(gone, &msg);
    }
    if let Some(s) = app.unsaved_summary() {
        say_on_exit(gone, &s);
    }
    drop(ipc);
    result.map(|_| ())
}

/// How the event loop ended.
enum End {
    /// `:q`, `:q!`, Ctrl-C…
    Quit,
    /// The terminal went away.
    HangUp,
}

/// One line for the author once the terminal is restored: on stderr, or in
/// Tiza's log when stderr was a terminal that is gone.
fn say_on_exit(gone: bool, msg: &str) {
    if gone {
        let msg = tiza_core::printable::printable_text(msg);
        log_line(&format!("the terminal went away: {msg}"));
    } else {
        eprintln!("tiza: {}", tiza_core::printable::printable_text(msg));
    }
}

/// Lines for `panics.log` that Tiza could not write there (it refused its
/// runtime folder): said on stderr once the terminal is restored, never
/// over the screen.
static HELD_LOG_LINES: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Appends one line to `panics.log` in Tiza's runtime directory (owner
/// only, like the sockets): the time, the version, the process and `what`.
/// Returns the log's path when it was written. When Tiza refuses that
/// folder (another user's, see [`runtime`]), the line waits in
/// `HELD_LOG_LINES`, with why, so that it is not lost.
pub(crate) fn log_line(what: &str) -> Option<PathBuf> {
    let path = tiza_core::socket::runtime_dir().join("panics.log");
    if let Err(e) = runtime::prepare_parent(&path) {
        HELD_LOG_LINES
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(format!("{what} (not written to panics.log: {e})"));
        return None;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .ok()?;
    writeln!(
        f,
        "{stamp} tiza {} ({}) pid {}: {what}",
        tiza_core::VERSION,
        tiza_core::GIT_SHA,
        std::process::id()
    )
    .ok()?;
    Some(path)
}

/// Points stderr at /dev/null when it is a hung-up terminal, and says so.
/// ratatui reports a restore it could not do (and its `Terminal`,
/// dropped at the end, a cursor it could not show) with `eprintln!`, which
/// panics on a write error, and a panic in the panic hook aborts. Asked of
/// stderr itself, however the session ended: a `kill -HUP` on a live
/// terminal still prints there, and a `2>file` keeps its file.
fn silence_dead_stderr() -> bool {
    if !hung_up(io::stderr()) {
        return false;
    }
    if let Ok(null) = std::fs::OpenOptions::new().write(true).open("/dev/null") {
        let _ = rustix::stdio::dup2_stderr(&null);
    }
    true
}

/// The terminal going away: a pane or a tab closed, an SSH session
/// dropped. SIGHUP is caught as a flag, so the loop ends in order (the
/// socket removed, Ctrl-C's save rules) instead of the process dying where
/// it stands — also when it came in ignored (`nohup`): without its terminal
/// Tiza has nothing left to do. Where no SIGHUP comes (a pty that is not the
/// controlling terminal) the tty itself says so.
struct HangUp(Arc<AtomicBool>);

impl HangUp {
    fn install() -> Self {
        let flag = Arc::new(AtomicBool::new(false));
        let _ = signal_hook::flag::register(signal_hook::consts::SIGHUP, Arc::clone(&flag));
        Self(flag)
    }

    fn happened(&self) -> bool {
        self.0.load(Ordering::Relaxed) || hung_up(io::stdout())
    }
}

/// Whether `fd` is a hung-up terminal (stdout: the one Tiza draws on): a
/// zero-byte write fails with EIO on a hung-up tty (Linux and macOS, also
/// one that is not the controlling terminal) and does nothing on a live one
/// or on a file. It never reads, so no key is taken from crossterm.
fn hung_up(fd: impl std::os::fd::AsFd) -> bool {
    rustix::io::write(fd, &[]) == Err(rustix::io::Errno::IO)
}

/// The event loop's answer to an I/O error: the end of the session when the
/// terminal went away (a draw or a read on a hung-up tty fails), the error
/// otherwise.
fn hung_up_or(hangup: &HangUp, e: io::Error) -> io::Result<End> {
    if hangup.happened() {
        Ok(End::HangUp)
    } else {
        Err(e)
    }
}

/// What the crash hook needs to say where the work is.
static CRASH_INFO: std::sync::OnceLock<std::sync::Mutex<PathBuf>> = std::sync::OnceLock::new();

/// The document moved (`:saveas`): a crash names the file it is saved to now.
pub(crate) fn note_save_path(save_path: &std::path::Path) {
    if let Some(cell) = CRASH_INFO.get()
        && let Ok(mut g) = cell.lock()
    {
        *g = save_path.to_path_buf();
    }
}

/// A panic anywhere in the TUI (a command, the draw loop, a worker) must
/// never leave the terminal in the alternate screen with raw mode on: the
/// hook restores it first, then prints one line that says what happened,
/// where the document is and how to report it. `ratatui::init` installs a
/// hook of its own that only restores; this one wraps it with the message.
fn install_crash_hook(save_path: &std::path::Path) {
    let cell = CRASH_INFO.get_or_init(|| std::sync::Mutex::new(PathBuf::new()));
    if let Ok(mut g) = cell.lock() {
        *g = save_path.to_path_buf();
    }
    std::panic::set_hook(Box::new(|info| {
        // Leave the alternate screen, show the cursor, raw mode off, mouse
        // and bracketed paste off (the sequences a clean `:q` sends).
        let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture);
        ratatui::restore();
        let reason = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown error".into());
        let at = info
            .location()
            .map(|l| format!(" ({}:{})", l.file(), l.line()))
            .unwrap_or_default();
        let path = CRASH_INFO
            .get()
            .and_then(|c| c.lock().ok().map(|g| g.clone()))
            .unwrap_or_default();
        let work = if path.as_os_str().is_empty() {
            String::new()
        } else {
            format!(
                " Your work is in {} (autosaved on every change, unless autosave was off).",
                path.display()
            )
        };
        // The payload can quote a document or a file name: shown, never
        // sent raw to the terminal.
        let what = format!("{reason}{at}.{work}");
        eprintln!(
            "tiza crashed: {} Please report it with the command you typed: https://github.com/luiscarlosjayk/tiza/issues",
            tiza_core::printable::printable_text(&what)
        );
    }));
}

/// Wraps the hooks `ratatui::init` chained on top of the crash hook:
/// a panic while a socket request is answered is caught by
/// `App::handle_request` and answered as an error, and the TUI keeps
/// running, so its screen is not restored and nothing is printed over it
/// (the panic is logged in `panics.log` instead).
fn spare_answered_requests() {
    let inner = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if crate::app::answering_request() {
            // Caught and answered: its place and message go to panics.log.
            crate::app::note_request_panic(info);
        } else {
            // A synchronized update that a panic left open (one inside a
            // frame) ends first, before ratatui's hook leaves the alternate
            // screen. With no update open, the terminal ignores it.
            let _ = execute!(io::stdout(), EndSynchronizedUpdate);
            // A dead terminal on stderr would make ratatui's restore and the
            // crash message panic in the hook, an abort.
            silence_dead_stderr();
            inner(info);
        }
    }));
}

/// The `draw:slow-first-frame` test hook slows only the first frame.
#[cfg(feature = "test-hooks")]
static SLOW_FIRST_FRAME_DONE: AtomicBool = AtomicBool::new(false);

/// Whether the test hook `place` is on: only with the `test-hooks`
/// feature, when `TIZA_TEST_HOOK` names it. Several hooks are separated by
/// commas, for example `effect:slow,request:step.set`.
#[cfg(feature = "test-hooks")]
pub(crate) fn test_hook(place: &str) -> bool {
    std::env::var("TIZA_TEST_HOOK").is_ok_and(|v| v.split(',').any(|h| h == place))
}

/// Draws one frame, and the terminal paints it at once: the output of the
/// frame goes in a synchronized update (DEC mode 2026), so that a big
/// change never shows half the frame before and half the new one. The
/// update begins after the layout, when ratatui writes the frame, so that
/// a slow layout never keeps it open; it ends after the flush, also when
/// the draw fails. A terminal without mode 2026 ignores both.
fn draw_frame(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    let drawn = terminal.draw(|f| {
        ui::draw(f, app);
        // Test hooks, only with the `test-hooks` feature, as on a busy
        // machine: the first frame takes 1.5 s more, or each frame takes
        // 100 ms more.
        #[cfg(feature = "test-hooks")]
        if test_hook("draw:slow-first-frame")
            && !SLOW_FIRST_FRAME_DONE.swap(true, Ordering::Relaxed)
        {
            std::thread::sleep(Duration::from_millis(1500));
        } else if test_hook("draw:slow-frames") {
            std::thread::sleep(Duration::from_millis(100));
        }
        // Test hooks, only with the `test-hooks` feature: a panic at the
        // end of the layout, before the update begins, and one inside the
        // update, for the tests that no update stays open.
        #[cfg(feature = "test-hooks")]
        if test_hook("draw:panic-in-layout") {
            panic!("TIZA_TEST_HOOK at the layout");
        }
        // The backend writes to the same `Stdout`: its buffer keeps this
        // before the frame.
        let _ = queue!(io::stdout(), BeginSynchronizedUpdate);
        #[cfg(feature = "test-hooks")]
        if test_hook("draw:panic-after-begin") {
            panic!("TIZA_TEST_HOOK at draw");
        }
    });
    let ended = execute!(io::stdout(), EndSynchronizedUpdate);
    drawn.and(ended)
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    ipc: Option<&ipc::IpcServer>,
    hangup: &HangUp,
) -> io::Result<End> {
    // crossterm creates its input source, and starts catching SIGWINCH, on
    // the first poll. Done here, before the first frame: a resize
    // until now is read by that frame (ratatui asks the size on every draw),
    // and one from now on arrives as `Event::Resize`. Polling only after the
    // first draw lost a resize that came while it was laid out, and nothing
    // redrew until a key.
    if let Err(e) = event::poll(Duration::ZERO) {
        return hung_up_or(hangup, e);
    }
    let mut needs_draw = true;
    let mut mouse_captured = app.mouse;
    // While an enter effect plays: its start, and when its next frame is
    // due ([`frame_due_of`]).
    let mut frame_due: Option<(Instant, Instant)> = None;
    let started = |app: &App| app.effect.as_ref().map(|e| e.started);
    let mut first_read = true;
    loop {
        if needs_draw {
            if let Err(e) = draw_frame(terminal, app) {
                return hung_up_or(hangup, e);
            }
            needs_draw = false;
            // The end of the frame: after a slow frame, the loop rests.
            let drawn = Instant::now();
            let period = app.effect_frame_period();
            frame_due = started(app).map(|s| {
                let due = frame_due_of(frame_due, Some(s));
                (s, next_frame_due(due, drawn, period))
            });
        }
        // `:mouse on|off` changed the flag: tell the terminal.
        if app.mouse != mouse_captured {
            mouse_captured = app.mouse;
            let _ = if mouse_captured {
                execute!(io::stdout(), EnableMouseCapture)
            } else {
                execute!(io::stdout(), DisableMouseCapture)
            };
        }
        if let Some(server) = ipc {
            while let Ok(r) = server.rx.try_recv() {
                let method = r.req.method.clone();
                let before = (app.revision, app.message.clone());
                app.handle_ipc(r.req, r.reply);
                let changed = (app.revision, app.message.clone()) != before;
                if request_redraws(&method, changed) {
                    needs_draw = true;
                }
            }
        }
        if app.follow_tick() {
            needs_draw = true;
        }
        if app.expire_proposals() {
            needs_draw = true;
        }
        if app.typing_tick() {
            needs_draw = true;
        }
        // An effect asks for a frame when one is due, and for the final
        // drawing when it ends. An event that wakes the loop before (the
        // mouse moves) draws no frame more.
        if app.effect_tick()
            && frame_due_of(frame_due, started(app)).is_none_or(|due| Instant::now() >= due)
        {
            needs_draw = true;
        }
        if app.auto_advance_tick(std::time::Instant::now()) {
            needs_draw = true;
        }
        if app.present_message_tick(std::time::Instant::now()) {
            needs_draw = true;
        }
        // While typing, poll often enough for a smooth animation. While an
        // effect plays, wait for its next frame: an event of the terminal
        // can wake the loop before, and only one that changes the screen
        // (a key, a scroll) draws a frame more.
        let wait = if app.is_typing() {
            Duration::from_millis(15)
        } else if app.is_playing_effect() {
            frame_wait(frame_due_of(frame_due, started(app)), Instant::now())
        } else {
            Duration::from_millis(50)
        };
        // The rest of an answer that the start-up wait cut short comes
        // with the first keys, after the first frame: its second starts
        // at this first read, so a slow first frame does not use it.
        if std::mem::take(&mut first_read) {
            app.osc.restart_clock();
        }
        let ready = match event::poll(wait) {
            Ok(ready) => ready,
            Err(e) => return hung_up_or(hangup, e),
        };
        // Nothing came: was it the terminal going away? crossterm takes a
        // hung-up tty for one that is always readable and empty, and would
        // poll it again at once, at 100 % of a CPU.
        if !ready && hangup.happened() {
            return Ok(End::HangUp);
        }
        if ready {
            let ev = match event::read() {
                Ok(ev) => ev,
                Err(e) => return hung_up_or(hangup, e),
            };
            match ev {
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    on_key(app, k.code, k.modifiers);
                    // A key of a color answer that is not complete changes
                    // nothing on the screen: no frame, so that the rest of
                    // the answer comes in its second.
                    if !app.osc.active() {
                        needs_draw = true;
                    }
                }
                Event::Paste(text) => {
                    app.paste(&text);
                    needs_draw = true;
                }
                Event::Resize(_, _) => {
                    // A theme change usually comes with a resize (or a
                    // `kill -WINCH`): ask the terminal again; the answer
                    // comes back through the key stream.
                    if app.theme_auto || app.theme.is_terminal() {
                        appearance::request_host();
                    }
                    needs_draw = true;
                }
                Event::Mouse(m) => {
                    let kind = match m.kind {
                        MouseEventKind::Down(MouseButton::Left) => Some(MouseKind::Down),
                        MouseEventKind::Drag(MouseButton::Left) => Some(MouseKind::Drag),
                        MouseEventKind::Up(MouseButton::Left) => Some(MouseKind::Up),
                        MouseEventKind::ScrollUp => Some(MouseKind::ScrollUp),
                        MouseEventKind::ScrollDown => Some(MouseKind::ScrollDown),
                        MouseEventKind::Down(_) | MouseEventKind::Drag(_) => {
                            Some(MouseKind::OtherButton)
                        }
                        _ => None,
                    };
                    if let Some(k) = kind
                        && app.on_mouse(k, m.column, m.row) != MouseOutcome::Ignored
                    {
                        needs_draw = true;
                    }
                }
                _ => {}
            }
        }
        if app.quit {
            return Ok(End::Quit);
        }
    }
}

/// When the next frame of the enter effect that started at `started` is
/// due, from the last frame due (its effect's start, its time): `None`
/// when no effect plays, or when that frame belongs to an effect before,
/// so that a new effect starts with no frame due.
pub fn frame_due_of(
    frame_due: Option<(Instant, Instant)>,
    started: Option<Instant>,
) -> Option<Instant> {
    let started = started?;
    frame_due.filter(|(s, _)| *s == started).map(|(_, due)| due)
}

/// When the next frame of an enter effect is due, after a frame that
/// ended at `now`: the frames keep to a grid of `period`. A frame that
/// ends before `due` is one more (the mouse wheel or a resize during
/// the effect) and does not move the grid; a frame on time moves it one period; a
/// late frame starts it again one period after `now`, so the loop rests
/// and does not draw in a burst to catch up.
pub fn next_frame_due(due: Option<Instant>, now: Instant, period: Duration) -> Instant {
    match due {
        Some(due) if now < due => due,
        Some(due) if due + period > now => due + period,
        _ => now + period,
    }
}

/// How long the loop waits for the next frame of an enter effect that
/// plays: until `due`, rounded up to whole milliseconds, so that it
/// never wakes before. With no frame due yet, no wait: the first frame
/// shows at once, also when the effect starts with no key (a socket
/// request, the auto-advance, the end of the typing).
pub fn frame_wait(due: Option<Instant>, now: Instant) -> Duration {
    due.map_or(Duration::ZERO, |due| {
        let left = due.saturating_duration_since(now);
        Duration::from_millis(left.as_micros().div_ceil(1000) as u64)
    })
}

/// Whether the screen is drawn again after a socket request: always after
/// one that may change the document or the view, and after a read-only one
/// (`status`, `render.text`, `code.get`…) only when it changed something
/// anyway (`changed`: the revision or the bar's message). A frame of a long
/// file is a full layout, and agents poll `status`.
pub fn request_redraws(method: &str, changed: bool) -> bool {
    changed || !app::READ_ONLY_METHODS.contains(&method)
}

/// Handles one key press exactly as the event loop does (public so QA tests
/// can drive the real keyboard logic without a terminal).
pub fn on_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    // A step change in this turn of the loop: its view first.
    app.resolve_pending_view();
    key(app, code, mods);
    // The live pointer follows the cursor or the selection.
    app.sync_pointer();
}

fn key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    // An OSC answer to a re-query arrives as key presses: collect
    // it and keep it away from the editor.
    let (swallowed, host) = app.osc.feed(code, mods);
    if let Some(h) = host {
        app.set_host(h);
    }
    if swallowed {
        return;
    }
    // The next key hides the result of a `:` command in presentation mode,
    // also one that ends a typing or works in a panel; the message of that
    // key does not take its place there.
    app.present_message = None;
    // Ctrl-C is looked at before anything else can take it as text (the
    // command line would type a `c`, a typing animation would only end).
    if code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
        cancel_or_quit(app);
        return;
    }
    // A typing animation ends on any key; the key itself is consumed. In
    // presentation mode a key that only moves the view lets it type on, as
    // an enter effect does below.
    if app.is_typing() {
        if presenter_view_key(app, code, mods) {
            view_key(app, code);
            return;
        }
        app.finish_typing();
        app.revision += 1;
        return;
    }
    // So does an enter effect: the final drawing shows at once. In
    // presentation mode a key that only moves the view lets it play, and
    // the view moves: a trackpad or a mouse wheel comes as ↑ and ↓. With
    // the live pointer on, that key would move the pointer, and a change
    // of the pointer ends the effect: it ends it at once, as another key.
    if app.is_playing_effect() && !presenter_view_key(app, code, mods) {
        app.finish_effect();
        return;
    }
    // `:` on the review (not while its filter is typed) hides it and opens
    // the command line.
    if app.overlay == Overlay::Review && code == KeyCode::Char(':') && !app.overlay_typing {
        // Typing a command from the list: hide it and open the command
        // line, so Enter ends the command instead of accepting a proposal.
        app.close_overlay();
    }
    if app.overlay == Overlay::Review {
        review_key(app, code, mods);
        return;
    }
    if app.overlay == Overlay::Help {
        help_key(app, code, mods);
        return;
    }
    if app.overlay == Overlay::List {
        list_key(app, code, mods);
        return;
    }
    if let Mode::Command(input) = &mut app.mode {
        match code {
            KeyCode::Esc => close_command_line(app),
            // Enter on an empty line runs nothing: it closes it as Esc does,
            // selection included.
            KeyCode::Enter if input.trim().is_empty() => close_command_line(app),
            KeyCode::Enter => {
                let cmd = input.clone();
                app.mode = Mode::Normal;
                app.run_command(&cmd);
                app.command_finished(std::time::Instant::now());
            }
            KeyCode::Backspace => {
                if input.pop().is_none() {
                    close_command_line(app);
                }
            }
            KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => input.clear(),
            KeyCode::Char(c) => input.push(c),
            KeyCode::Tab => {
                let current = input.clone();
                let (text, many) = app.complete(&current);
                if let Mode::Command(input) = &mut app.mode {
                    *input = text;
                }
                if !many.is_empty() {
                    app.info(many.join("  "));
                }
            }
            KeyCode::Up => {
                if let Some(h) = app.history_step(-1)
                    && let Mode::Command(input) = &mut app.mode
                {
                    *input = h;
                }
            }
            KeyCode::Down => {
                if let Some(h) = app.history_step(1)
                    && let Mode::Command(input) = &mut app.mode
                {
                    *input = h;
                }
            }
            _ => {}
        }
        return;
    }
    app.message = None;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    // The selection a `:` command left is for the command typed
    // right after (`:` ↑ Enter runs `:hl`/`:mark` again on it), also after
    // an undo or a redo; any other key lets it go, so a later `:mark` with
    // no anchor never lands on old text. An action key in selection mode
    // (`m`, `n`, …) sets it again from the selection on screen.
    let undo_or_redo =
        (!ctrl && matches!(code, KeyCode::Char('u' | 'U'))) || (ctrl && code == KeyCode::Char('r'));
    if code != KeyCode::Char(':') && !undo_or_redo {
        app.pending_sel = None;
    }
    // Ctrl with a letter is not that letter (Ctrl-U is no undo, Ctrl-Q no
    // quit): only the keys the help names act, Ctrl-C above and Ctrl-R.
    if ctrl && matches!(code, KeyCode::Char(c) if c != 'r') {
        return;
    }
    // ---- selection mode: motions extend, action keys consume ----
    if matches!(app.mode, Mode::Select { .. }) {
        match code {
            // Presentation mode does not show the selection: `Tab` closes
            // it and goes on, and the pointer to the line of the `Tab`.
            KeyCode::Tab | KeyCode::BackTab if app.present => {
                app.mode = Mode::Normal;
                app.arrow_from = None;
                app.select_annotation(if code == KeyCode::Tab { 1 } else { -1 });
            }
            KeyCode::Esc => {
                app.mode = Mode::Normal;
                app.arrow_from = None;
            }
            KeyCode::Char('v') | KeyCode::Char('V') => app.mode = Mode::Normal,
            // The pointer on the selected lines.
            KeyCode::Char('f') if !ctrl => app.toggle_pointer(),
            KeyCode::Char('h') if mods.is_empty() && app.selection().is_some() => {
                // `h` is both "move left" and "highlight": left needs Shift-less motion via arrows;
                // in selection mode `h` acts, `←` moves.
                app.select_action('h');
            }
            KeyCode::Char(c) if "mnbsaCr".contains(c) => {
                app.select_action(c);
            }
            // The command line and the help stay reachable with a selection.
            KeyCode::Char(':') => app.mode = Mode::Command(String::new()),
            KeyCode::Char('?') => open(app, Overlay::Help),
            KeyCode::Char('L') => open(app, Overlay::List),
            KeyCode::Left => app.move_col(-1),
            KeyCode::Char('l') | KeyCode::Right => app.move_col(1),
            KeyCode::Char('j') | KeyCode::Down => app.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => app.move_cursor(-1),
            KeyCode::Char('w') => app.word_forward(),
            KeyCode::Char('e') => app.word_end(),
            KeyCode::Char('B') => app.word_backward(),
            KeyCode::Char('0') | KeyCode::Home => app.line_start(),
            KeyCode::Char('$') | KeyCode::End => app.line_end(),
            KeyCode::Char('i') => app.select_word(),
            _ => {}
        }
        return;
    }
    // ---- quick edits on the annotation selected with Tab ----
    if app.selected.is_some() {
        let handled = match code {
            KeyCode::Enter => app.selected_action('\n'),
            KeyCode::Char(c @ ('x' | 'c' | 'e' | '+' | '-')) if !ctrl => app.selected_action(c),
            _ => false,
        };
        if handled {
            return;
        }
    }
    // In presentation mode these keys move the view, as in a web browser.
    if presenter_view_key(app, code, mods) {
        view_key(app, code);
        return;
    }
    match code {
        KeyCode::Tab => app.select_annotation(1),
        KeyCode::BackTab => app.select_annotation(-1),
        KeyCode::Char('v') => app.start_select(false),
        KeyCode::Char('V') => app.start_select(true),
        KeyCode::Char('i') => app.select_word(),
        // ←/→ change the step in presentation mode (a clicker sends them);
        // elsewhere they move the column, as h/l, so a presenter back from a
        // selection does not move the take by mistake. PageDown/PageUp
        // (and n/p) change the step in both.
        KeyCode::Right if app.present => {
            app.next_step();
        }
        KeyCode::Left if app.present => app.set_step(app.doc.step.saturating_sub(1)),
        KeyCode::Char('h') | KeyCode::Left => app.move_col(-1),
        KeyCode::Char('l') | KeyCode::Right => app.move_col(1),
        KeyCode::Char('w') => app.word_forward(),
        KeyCode::Char('e') => app.word_end(),
        KeyCode::Char('b') => app.word_backward(),
        KeyCode::Char('0') => app.line_start(),
        KeyCode::Char('$') => app.line_end(),
        KeyCode::Char('L') => open(app, Overlay::List),
        KeyCode::Char('q') => app.request_quit(false),
        KeyCode::Char(':') => app.mode = Mode::Command(String::new()),
        KeyCode::Char('?') => open(app, Overlay::Help),
        KeyCode::Char('j') | KeyCode::Down => app.move_cursor(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_cursor(-1),
        KeyCode::Char('J') => app.move_cursor(app.size.1 as i64 / 2),
        KeyCode::Char('K') => app.move_cursor(-(app.size.1 as i64) / 2),
        KeyCode::Char('g') | KeyCode::Home => app.cursor_line = app.doc.source.first_line,
        KeyCode::Char('G') | KeyCode::End => app.cursor_line = app.doc.last_line(),
        KeyCode::Char('n') | KeyCode::PageDown => {
            app.next_step();
        }
        KeyCode::Char('p') | KeyCode::PageUp => app.set_step(app.doc.step.saturating_sub(1)),
        // Presenting, not editing: in presentation mode these keys do
        // nothing (`:undo` and `:redo` still work, and show their result).
        KeyCode::Char('u') if !app.present => app.undo(),
        KeyCode::Char('U') if !app.present => app.redo(),
        KeyCode::Char('r') if ctrl && !app.present => app.redo(),
        KeyCode::Char('t') => app.toggle_theme(),
        KeyCode::Char('P') => app.set_present(!app.present),
        // The live pointer: on the cursor line, or off.
        KeyCode::Char('f') if !ctrl => app.toggle_pointer(),
        KeyCode::Esc => {
            if app.pointer.is_some() {
                app.clear_pointer();
            } else if app.present {
                // Like `P`: what the take did not show reaches the bar.
                app.set_present(false);
            } else if app.selected.is_some() {
                app.selected = None;
            } else if app.arrow_from.is_some() {
                app.arrow_from = None;
                app.info("arrow cancelled");
            } else {
                app.focus = None;
            }
        }
        _ => {}
    }
}

/// The keys that only move the view in presentation mode, with nothing
/// open: ↑ ↓ j k, J K (half a screen), g G Home End. ← and → change the
/// step there.
fn moves_the_view(app: &App, code: KeyCode, mods: KeyModifiers) -> bool {
    !mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        && app.mode == Mode::Normal
        && app.overlay == Overlay::None
        && matches!(
            code,
            KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::Char('j' | 'k' | 'J' | 'K' | 'g' | 'G')
        )
}

/// A key of [`moves_the_view`] in presentation mode with the live pointer
/// off: it moves the view, and the hidden cursor stays inside it. With the
/// pointer on, the key moves the cursor and the pointer, as elsewhere.
fn presenter_view_key(app: &App, code: KeyCode, mods: KeyModifiers) -> bool {
    app.present && app.pointer.is_none() && moves_the_view(app, code, mods)
}

/// Moves the view for a [`presenter_view_key`]: one row, half the rows of
/// the code on screen (J, K), or an end of the code.
fn view_key(app: &mut App, code: KeyCode) {
    let half = (app.screen_height() as i64 / 2).max(1);
    match code {
        KeyCode::Char('j') | KeyCode::Down => app.view_by(1),
        KeyCode::Char('k') | KeyCode::Up => app.view_by(-1),
        KeyCode::Char('J') => app.view_by(half),
        KeyCode::Char('K') => app.view_by(-half),
        KeyCode::Char('g') | KeyCode::Home => app.view_to(ViewEdge::Top),
        KeyCode::Char('G') | KeyCode::End => app.view_to(ViewEdge::Bottom),
        _ => {}
    }
}

/// Closes the command line without running it (Esc, Ctrl-C, Backspace on
/// an empty line). The selection that opened it (`v` then `m`, `n`, `b`…)
/// goes with it: it is the anchor of that command only, and a later command
/// typed without an anchor must not land on a selection that was cancelled.
fn close_command_line(app: &mut App) {
    app.mode = Mode::Normal;
    app.pending_sel = None;
}

/// Ctrl-C, as in vim: it cancels what is open, the way Esc does, and
/// quits when nothing is. Open means waiting for more input from the
/// author: an overlay (help, list, review), the command line (its text is
/// dropped, never run), a `v`/`V` selection, an arrow whose source is
/// chosen. The rest is a state of the view, not something to cancel — the
/// presentation mode, the live pointer, a typing animation, an annotation
/// selected with Tab, a notice on the bar — so there it quits. Quitting is
/// `:q!`: nothing is written, except that a failed autosave is tried once
/// more on the way out (`run` does it, and prints what was not saved).
fn cancel_or_quit(app: &mut App) {
    if app.overlay != Overlay::None {
        // Like Esc on any overlay: a pending proposal stays pending.
        app.close_overlay();
    } else if matches!(app.mode, Mode::Command(_)) {
        close_command_line(app);
    } else if matches!(app.mode, Mode::Select { .. }) {
        // Like Esc in selection mode: the arrow being drawn goes too, and
        // so does the bar's hint about what the selection can do.
        app.message = None;
        app.mode = Mode::Normal;
        app.arrow_from = None;
    } else if app.arrow_from.is_some() {
        app.arrow_from = None;
        app.info("arrow cancelled");
    } else {
        app.retry_save_on_exit = true;
        app.quit = true;
    }
}

/// Opens a panel, or says on the bar why it cannot open.
fn open(app: &mut App, which: Overlay) {
    if let Err(e) = app.open_overlay(which) {
        app.error(e.to_string());
    }
}

/// A key on the help panel (crates/tiza-tui/UI-GUIDELINES.md, section 1). While the
/// filter is typed, every printable key goes to it; otherwise the panel's
/// keys scroll it, `/` types the filter, and Esc clears the filter, then
/// closes. `←`/`→` do nothing, so a clicker cannot move the panel.
fn help_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    let drawn = app.overlay_drawn.get();
    // A resize or a `G` before the next frame can leave it past the end.
    app.overlay_scroll = app.overlay_scroll.min(drawn.max_scroll);
    let page = drawn.page.max(1);
    let scroll_by = |app: &mut App, by: i64| {
        let max = app.overlay_drawn.get().max_scroll as i64;
        app.overlay_scroll = (app.overlay_scroll as i64 + by).clamp(0, max.max(0)) as usize;
    };
    if app.overlay_typing {
        filter_key(app, code, mods, scroll_by);
        return;
    }
    match code {
        KeyCode::Esc if !app.overlay_filter.is_empty() => {
            app.overlay_filter.clear();
            app.overlay_scroll = 0;
        }
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | '?' | 'L') => app.close_overlay(),
        KeyCode::Char('/') => app.overlay_typing = true,
        KeyCode::Char('j') | KeyCode::Down => scroll_by(app, 1),
        KeyCode::Char('k') | KeyCode::Up => scroll_by(app, -1),
        KeyCode::Char(' ') | KeyCode::PageDown => scroll_by(app, page as i64),
        KeyCode::PageUp => scroll_by(app, -(page as i64)),
        KeyCode::Char('g') | KeyCode::Home => app.overlay_scroll = 0,
        KeyCode::Char('G') | KeyCode::End => app.overlay_scroll = drawn.max_scroll,
        _ => {}
    }
}

/// A key while the filter of a panel is typed: a printable key goes to the
/// filter, Backspace and Ctrl-U edit it, Enter keeps it and stops the
/// typing, Esc clears it. `↑`/`↓` call `up_down` (the help scrolls, the
/// list and the review move their selection).
fn filter_key(app: &mut App, code: KeyCode, mods: KeyModifiers, up_down: impl Fn(&mut App, i64)) {
    let changed = |app: &mut App| {
        app.overlay_scroll = 0;
        app.overlay_sel = 0;
        app.overlay_pick = None;
    };
    match code {
        KeyCode::Esc => {
            app.overlay_filter.clear();
            app.overlay_typing = false;
            changed(app);
        }
        KeyCode::Enter => app.overlay_typing = false,
        KeyCode::Backspace => {
            app.overlay_filter.pop();
            changed(app);
        }
        KeyCode::Char('u') if mods.contains(KeyModifiers::CONTROL) => {
            app.overlay_filter.clear();
            changed(app);
        }
        KeyCode::Down => up_down(app, 1),
        KeyCode::Up => up_down(app, -1),
        KeyCode::Char(c) if !mods.contains(KeyModifiers::CONTROL) => {
            app.overlay_filter.push(c);
            changed(app);
        }
        _ => {}
    }
}

/// Moves the selection by `by` rows among the rows shown, by their keys
/// (`ListItem::key`, `p:N`), and keeps the key of the row it lands on.
fn select_by(app: &mut App, by: i64, keys: &[String]) {
    let Some(sel) = crate::list::selected(app, keys) else {
        return;
    };
    let to = (sel as i64 + by).clamp(0, keys.len() as i64 - 1) as usize;
    app.overlay_sel = to;
    app.overlay_pick = Some(keys[to].clone());
}

/// The rows of the list and their keys.
fn list_rows(app: &App) -> (Vec<crate::list::ListItem>, Vec<String>) {
    let items = crate::list::list(app).1;
    let keys = items.iter().map(crate::list::ListItem::key).collect();
    (items, keys)
}

/// A key on the annotation list (crates/tiza-tui/UI-GUIDELINES.md, section 2): `j/k`
/// select, Enter goes to the step of the selected row, `x` deletes it and
/// `u`, `U` or Ctrl-R undo and redo with the list open. Esc clears the
/// filter, then closes.
fn list_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    if app.overlay_typing {
        filter_key(app, code, mods, |app, by| {
            let (_, keys) = list_rows(app);
            select_by(app, by, &keys)
        });
        return;
    }
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    // Ctrl with a letter is not that letter here either: only Ctrl-R.
    if ctrl && matches!(code, KeyCode::Char(c) if c != 'r') {
        return;
    }
    let (items, keys) = list_rows(app);
    let n = keys.len() as i64;
    let page = app.overlay_drawn.get().page.max(1) as i64;
    let chosen = crate::list::selected(app, &keys).map(|i| items[i].clone());
    match code {
        KeyCode::Esc if !app.overlay_filter.is_empty() => {
            app.overlay_filter.clear();
            app.overlay_sel = 0;
            app.overlay_pick = None;
            app.overlay_scroll = 0;
        }
        KeyCode::Esc | KeyCode::Char('q' | 'L') => app.close_overlay(),
        KeyCode::Char('?') => open(app, Overlay::Help),
        KeyCode::Char('/') => app.overlay_typing = true,
        KeyCode::Char('j') | KeyCode::Down => select_by(app, 1, &keys),
        KeyCode::Char('k') | KeyCode::Up => select_by(app, -1, &keys),
        KeyCode::Char(' ') | KeyCode::PageDown => select_by(app, page, &keys),
        KeyCode::PageUp => select_by(app, -page, &keys),
        KeyCode::Char('g') | KeyCode::Home => select_by(app, -n, &keys),
        KeyCode::Char('G') | KeyCode::End => select_by(app, n, &keys),
        // With no row to go to, Enter closes, as on the help.
        KeyCode::Enter => {
            app.close_overlay();
            if let Some(item) = chosen {
                app.go_to_list_item(&item);
            }
        }
        // In presentation mode the list edits nothing, as the keys of an
        // annotation reached with Tab.
        KeyCode::Char('x') if !app.present => {
            if let Some(item) = chosen {
                app.delete_list_item(&item);
            }
        }
        KeyCode::Char('u') if !app.present => app.undo(),
        KeyCode::Char('U') if !app.present => app.redo(),
        KeyCode::Char('r') if ctrl && !app.present => app.redo(),
        _ => {}
    }
    // A stale move says why on the bar while it is selected.
    let (items, keys) = list_rows(app);
    if app.overlay == Overlay::List
        && matches!(
            code,
            KeyCode::Char('j' | 'k' | 'g' | 'G' | ' ')
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Home
                | KeyCode::End
        )
        && let Some(crate::list::ListItem::Move { index, .. }) =
            crate::list::selected(app, &keys).map(|i| &items[i])
        && let Some(stale) = app.doc.moves.get(index - 1).and_then(|m| m.stale.as_ref())
    {
        let why = format!("#{index} is stale: {}. x deletes it", stale.reason);
        app.info(why);
    }
}

/// A key on the agent review: `y` or Enter accepts the selected change,
/// `n` or `x` rejects it, `a` and `d` decide the changes shown, `j/k`
/// select. Esc clears the filter, then hides the review (`:review` shows
/// it again).
fn review_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    let rows = |app: &App| {
        let items = crate::list::review(app).1;
        let keys: Vec<String> = items
            .iter()
            .map(|i| format!("p:{}", app.proposals[*i].id))
            .collect();
        (items, keys)
    };
    if app.overlay_typing {
        filter_key(app, code, mods, |app, by| {
            let (_, keys) = rows(app);
            select_by(app, by, &keys)
        });
        return;
    }
    if mods.contains(KeyModifiers::CONTROL) && matches!(code, KeyCode::Char(_)) {
        return;
    }
    let (items, keys) = rows(app);
    let n = keys.len() as i64;
    let chosen = crate::list::selected(app, &keys).map(|i| items[i]);
    // The changes shown, by id: a filter keeps the others waiting.
    let shown: Vec<u64> = items.iter().map(|i| app.proposals[*i].id).collect();
    let msg = match code {
        KeyCode::Enter if chosen.is_none() => {
            app.close_overlay();
            None
        }
        KeyCode::Char('y') | KeyCode::Enter => chosen.and_then(|i| app.accept_proposal_at(i)),
        KeyCode::Char('n') | KeyCode::Char('x') => chosen.and_then(|i| app.reject_proposal_at(i)),
        KeyCode::Char(c @ ('a' | 'd')) => {
            let accept = c == 'a';
            let n = app.decide_proposals(&shown, accept);
            Some(format!(
                "{} {n} agent change{}",
                if accept { "accepted" } else { "rejected" },
                if n == 1 { "" } else { "s" }
            ))
        }
        KeyCode::Esc if !app.overlay_filter.is_empty() => {
            app.overlay_filter.clear();
            app.overlay_sel = 0;
            app.overlay_pick = None;
            None
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            app.close_overlay();
            None
        }
        KeyCode::Char('/') => {
            app.overlay_typing = true;
            None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            select_by(app, 1, &keys);
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            select_by(app, -1, &keys);
            None
        }
        KeyCode::Char('g') | KeyCode::Home => {
            select_by(app, -n, &keys);
            None
        }
        KeyCode::Char('G') | KeyCode::End => {
            select_by(app, n, &keys);
            None
        }
        _ => None,
    };
    if let Some(m) = msg {
        app.info(m);
    }
}
