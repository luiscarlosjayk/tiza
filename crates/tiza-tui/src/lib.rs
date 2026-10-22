//! `tiza-tui`: the Tiza terminal interface and its local API server.

pub mod app;
pub mod appearance;
pub mod follow;
pub mod ipc;
pub mod reanchor;
pub mod ui;

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::execute;
use tiza_core::fs::Opened;

use crate::app::{App, Mode, MouseKind, MouseOutcome, Overlay};

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
    pub present: bool,
    /// `None` = no local API server.
    pub socket: Option<PathBuf>,
    pub focus: Option<(u32, u32)>,
    /// `--padding`: shown this session, never saved into the document.
    pub padding: Option<(u16, u16)>,
    /// `--follow`: reload when the source file changes on disk.
    pub follow: bool,
    /// Renders a canvas as SVG/PNG for `:export`; the TUI crate does not link
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
    app.config_overrides = opts.config_overrides;
    app.rebuild_theme();
    if let Some(path) = &opts.theme_file {
        match app.load_theme_file(path) {
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
    // The host's appearance (t-119): asked before crossterm owns the
    // input, 100 ms at most, silent fallback to dark.
    if (app.theme_auto || app.theme.is_terminal())
        && let Some(host) = appearance::query_host(Duration::from_millis(100))
    {
        app.set_host(host);
    }
    if let Some(s) = opts.step {
        app.set_step(s);
    }
    app.present = opts.present;
    app.focus = opts.focus;
    app.padding_override = opts.padding;
    app.image_exporter = opts.image_exporter;
    app.mouse = opts.mouse;
    app.autosave = !opts.no_save;
    if let Some(n) = &opened.notice {
        app.info(n.clone());
    }
    // A `steps` entry past the last step is kept, never dropped in
    // silence (t-099).
    if let Some(w) = app.doc.step_warnings().first() {
        app.error(w.clone());
    } else if let Some(r) = tiza_core::moves::ignored_rebinds(&app.doc).first() {
        // A `rebind` entry whose annotation is gone (`tiza check` lists
        // every one).
        app.info(format!("{}: {}", r.place(), r.message()));
    }
    if opts.follow
        && let Err(e) = app.set_follow(true)
    {
        app.error(e.to_string());
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
    let mut terminal = ratatui::init();
    spare_answered_requests();
    execute!(io::stdout(), EnableBracketedPaste)?;
    if app.mouse {
        let _ = execute!(io::stdout(), EnableMouseCapture);
    }
    let result = if opts.smoke {
        // One frame, then one poll of the input source: with stdin piped the
        // events come from /dev/tty (crossterm `use-dev-tty`), which is what
        // the smoke test is there to prove.
        terminal
            .draw(|f| ui::draw(f, &mut app))
            .and_then(|_| event::poll(Duration::from_millis(100)))
            .map(|_| End::Quit)
    } else {
        event_loop(&mut terminal, &mut app, ipc.as_ref(), &hangup)
    };
    // The terminal went away (t-205): Ctrl-C's rules for what is saved.
    if matches!(result, Ok(End::HangUp)) {
        app.retry_save_on_exit = true;
    }
    let gone = silence_dead_stderr();
    let _ = execute!(io::stdout(), DisableBracketedPaste, DisableMouseCapture);
    ratatui::restore();
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
    /// The terminal went away (t-205).
    HangUp,
}

/// One line for the author once the terminal is restored: on stderr, or in
/// Tiza's log when stderr was a terminal that is gone (t-205).
fn say_on_exit(gone: bool, msg: &str) {
    if gone {
        let msg = tiza_core::printable::printable_text(msg);
        log_line(&format!("the terminal went away: {msg}"));
    } else {
        eprintln!("tiza: {}", tiza_core::printable::printable_text(msg));
    }
}

/// Appends one line to `panics.log` in Tiza's runtime directory (owner
/// only, like the sockets): the time, the version, the process and `what`.
/// Returns the log's path when it was written.
pub(crate) fn log_line(what: &str) -> Option<PathBuf> {
    let path = tiza_core::socket::runtime_dir().join("panics.log");
    tiza_core::socket::prepare_dir(&path).ok()?;
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

/// Points stderr at /dev/null when it is a hung-up terminal, and says so
/// (t-205). ratatui reports a restore it could not do (and its `Terminal`,
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

/// The terminal going away (t-205): a pane or a tab closed, an SSH session
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
        // sent raw to the terminal (t-182).
        let what = format!("{reason}{at}.{work}");
        eprintln!(
            "tiza crashed: {} Please report it with the command you typed: https://github.com/luiscarlosjayk/tiza/issues",
            tiza_core::printable::printable_text(&what)
        );
    }));
}

/// Wraps the hooks `ratatui::init` chained on top of the crash hook
/// (t-166): a panic while a socket request is answered is caught by
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
            // A dead terminal on stderr would make ratatui's restore and the
            // crash message panic in the hook, an abort (t-205).
            silence_dead_stderr();
            inner(info);
        }
    }));
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    ipc: Option<&ipc::IpcServer>,
    hangup: &HangUp,
) -> io::Result<End> {
    // crossterm creates its input source, and starts catching SIGWINCH, on
    // the first poll (t-180). Done here, before the first frame: a resize
    // until now is read by that frame (ratatui asks the size on every draw),
    // and one from now on arrives as `Event::Resize`. Polling only after the
    // first draw lost a resize that came while it was laid out, and nothing
    // redrew until a key.
    if let Err(e) = event::poll(Duration::ZERO) {
        return hung_up_or(hangup, e);
    }
    let mut needs_draw = true;
    let mut mouse_captured = app.mouse;
    loop {
        if needs_draw {
            if let Err(e) = terminal.draw(|f| ui::draw(f, app)) {
                return hung_up_or(hangup, e);
            }
            needs_draw = false;
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
                app.handle_ipc(r.req, r.reply);
                needs_draw = true;
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
        if app.auto_advance_tick(std::time::Instant::now()) {
            needs_draw = true;
        }
        if app.present_message_tick(std::time::Instant::now()) {
            needs_draw = true;
        }
        // While typing, poll often enough for a smooth animation.
        let wait = if app.is_typing() { 15 } else { 50 };
        let ready = match event::poll(Duration::from_millis(wait)) {
            Ok(ready) => ready,
            Err(e) => return hung_up_or(hangup, e),
        };
        // Nothing came: was it the terminal going away? crossterm takes a
        // hung-up tty for one that is always readable and empty, and would
        // poll it again at once, at 100 % of a CPU (t-205).
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
                    needs_draw = true;
                }
                Event::Paste(text) => {
                    if let Mode::Command(input) = &mut app.mode {
                        // Pasting into the command line: text, not a new document.
                        input.push_str(text.lines().next().unwrap_or("").trim_end());
                    } else if let Err(e) = app.replace_code(&text) {
                        app.error(e.to_string());
                    }
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

/// Handles one key press exactly as the event loop does (public so QA tests
/// can drive the real keyboard logic without a terminal).
pub fn on_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    key(app, code, mods);
    // The live pointer follows the cursor or the selection (t-030).
    app.sync_pointer();
}

fn key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    // An OSC answer to a re-query arrives as key presses (t-119): collect
    // it and keep it away from the editor.
    let (swallowed, host) = app.osc.feed(code, mods);
    if let Some(h) = host {
        app.set_host(h);
    }
    if swallowed {
        return;
    }
    // Ctrl-C is looked at before anything else can take it as text (the
    // command line would type a `c`, a typing animation would only end).
    if code == KeyCode::Char('c') && mods.contains(KeyModifiers::CONTROL) {
        cancel_or_quit(app);
        return;
    }
    // A typing animation (t-098) ends on any key; the key itself is consumed.
    if app.is_typing() {
        let _ = (code, mods);
        app.finish_typing();
        app.revision += 1;
        return;
    }
    // The review overlay has its own keys; the rest only scroll or close.
    if app.overlay == Overlay::Review && code == KeyCode::Char(':') {
        // Typing a command from the list: hide it and open the command
        // line, so Enter ends the command instead of accepting a proposal.
        app.overlay = Overlay::None;
        app.overlay_scroll = 0;
    }
    if app.overlay == Overlay::Review {
        let msg = match code {
            KeyCode::Char('y') | KeyCode::Enter => app.accept_proposal(),
            KeyCode::Char('n') | KeyCode::Char('x') => app.reject_proposal(),
            KeyCode::Char('a') => {
                Some(format!("accepted {} proposals", app.accept_all_proposals()))
            }
            KeyCode::Char('d') => {
                Some(format!("rejected {} proposals", app.reject_all_proposals()))
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                app.overlay = Overlay::None;
                app.overlay_scroll = 0;
                None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                app.overlay_scroll += 1;
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.overlay_scroll = app.overlay_scroll.saturating_sub(1);
                None
            }
            _ => None,
        };
        if let Some(m) = msg {
            app.info(m);
        }
        return;
    }
    // Overlays: any of these keys closes them.
    if app.overlay != Overlay::None {
        match code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | '?' | 'L') => {
                app.overlay = Overlay::None;
                app.overlay_scroll = 0;
            }
            KeyCode::Char('j') | KeyCode::Down => app.overlay_scroll += 1,
            KeyCode::Char('k') | KeyCode::Up => {
                app.overlay_scroll = app.overlay_scroll.saturating_sub(1)
            }
            KeyCode::PageDown | KeyCode::Char(' ') => app.overlay_scroll += 10,
            KeyCode::PageUp => app.overlay_scroll = app.overlay_scroll.saturating_sub(10),
            KeyCode::Char('g') | KeyCode::Home => app.overlay_scroll = 0,
            KeyCode::Char('G') | KeyCode::End => app.overlay_scroll = usize::MAX / 2,
            _ => {}
        }
        return;
    }
    if let Mode::Command(input) = &mut app.mode {
        match code {
            KeyCode::Esc => close_command_line(app),
            // Enter on an empty line runs nothing: it closes it as Esc does,
            // selection included (t-203).
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
    // The selection a `:` command left (t-203) is for the command typed
    // right after (`:` ↑ Enter runs `:hl`/`:mark` again on it), also after
    // an undo or a redo; any other key lets it go, so a later `:mark` with
    // no anchor never lands on old text. An action key in selection mode
    // (`m`, `n`, …) sets it again from the selection on screen.
    let undo_or_redo =
        matches!(code, KeyCode::Char('u' | 'U')) || (ctrl && code == KeyCode::Char('r'));
    if code != KeyCode::Char(':') && !undo_or_redo {
        app.pending_sel = None;
    }
    // ---- selection mode: motions extend, action keys consume ----
    if matches!(app.mode, Mode::Select { .. }) {
        match code {
            KeyCode::Esc => {
                app.mode = Mode::Normal;
                app.arrow_from = None;
            }
            KeyCode::Char('v') | KeyCode::Char('V') => app.mode = Mode::Normal,
            // The pointer on the selected lines (t-030).
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
            KeyCode::Char('?') => app.overlay = Overlay::Help,
            KeyCode::Char('L') => app.overlay = Overlay::List,
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
    match code {
        KeyCode::Tab => app.select_annotation(1),
        KeyCode::BackTab => app.select_annotation(-1),
        KeyCode::Char('v') => app.start_select(false),
        KeyCode::Char('V') => app.start_select(true),
        KeyCode::Char('i') => app.select_word(),
        KeyCode::Char('h') => app.move_col(-1),
        KeyCode::Char('l') => app.move_col(1),
        // ←/→ advance the steps (a clicker sends them); h/l move the column.
        KeyCode::Right => {
            app.next_step();
        }
        KeyCode::Left => app.set_step(app.doc.step.saturating_sub(1)),
        KeyCode::Char('w') => app.word_forward(),
        KeyCode::Char('e') => app.word_end(),
        KeyCode::Char('b') => app.word_backward(),
        KeyCode::Char('0') => app.line_start(),
        KeyCode::Char('$') => app.line_end(),
        KeyCode::Char('L') => app.overlay = Overlay::List,
        KeyCode::Char('q') => app.request_quit(false),
        KeyCode::Char(':') => app.mode = Mode::Command(String::new()),
        KeyCode::Char('?') => app.overlay = Overlay::Help,
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
        KeyCode::Char('u') => app.undo(),
        KeyCode::Char('U') => app.redo(),
        KeyCode::Char('r') if ctrl => app.redo(),
        KeyCode::Char('t') => app.toggle_theme(),
        KeyCode::Char('P') => app.set_present(!app.present),
        // The live pointer (t-030): on the cursor line, or off.
        KeyCode::Char('f') if !ctrl => app.toggle_pointer(),
        KeyCode::Esc => {
            if app.pointer.is_some() {
                app.clear_pointer();
            } else if app.present {
                // Like `P`: what the take did not show reaches the bar (t-155).
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
        app.overlay = Overlay::None;
        app.overlay_scroll = 0;
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
