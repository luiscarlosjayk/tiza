//! `codedraw-tui`: the CodeDraw terminal interface and its local API server.

pub mod app;
pub mod follow;
pub mod ipc;
pub mod reanchor;
pub mod ui;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use codedraw_core::fs::Opened;
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::execute;

use crate::app::{App, Mode, Overlay};

pub struct RunOptions {
    pub step: Option<u32>,
    pub theme: Option<codedraw_core::ThemeName>,
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
}

/// Starts the TUI with an opened document. Returns when it closes.
pub fn run(opened: Opened, opts: RunOptions) -> io::Result<()> {
    let mut app = App::new(opened.document, opened.save_path);
    if let Some(t) = opts.theme {
        app.theme = codedraw_core::Theme::from_name(t);
    }
    if let Some(s) = opts.step {
        app.set_step(s);
    }
    app.present = opts.present;
    app.focus = opts.focus;
    app.padding_override = opts.padding;
    app.image_exporter = opts.image_exporter;
    if let Some(n) = &opened.notice {
        app.info(n.clone());
    }
    if opts.follow
        && let Err(e) = app.set_follow(true)
    {
        app.error(e.to_string());
    }

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

    let mut terminal = ratatui::init();
    execute!(io::stdout(), EnableBracketedPaste)?;
    let result = if opts.smoke {
        // One frame, then one poll of the input source: with stdin piped the
        // events come from /dev/tty (crossterm `use-dev-tty`), which is what
        // the smoke test is there to prove.
        terminal
            .draw(|f| ui::draw(f, &mut app))
            .and_then(|_| event::poll(Duration::from_millis(100)))
            .map(|_| ())
    } else {
        event_loop(&mut terminal, &mut app, ipc.as_ref())
    };
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    if app.dirty && app.save().is_err() {
        // The terminal is restored by now: say it where it will be read.
        eprintln!(
            "codedraw: {}",
            app.save_error
                .as_deref()
                .unwrap_or("could not save the document")
        );
    }
    drop(ipc);
    result
}

fn event_loop(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    ipc: Option<&ipc::IpcServer>,
) -> io::Result<()> {
    let mut needs_draw = true;
    loop {
        if needs_draw {
            terminal.draw(|f| ui::draw(f, app))?;
            needs_draw = false;
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
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
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
                Event::Resize(_, _) => needs_draw = true,
                _ => {}
            }
        }
        if app.quit {
            return Ok(());
        }
    }
}

/// Handles one key press exactly as the event loop does (public so QA tests
/// can drive the real keyboard logic without a terminal).
pub fn on_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    // The review overlay has its own keys; the rest only scroll or close.
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
            KeyCode::Esc => app.mode = Mode::Normal,
            KeyCode::Enter => {
                let cmd = input.clone();
                app.mode = Mode::Normal;
                app.run_command(&cmd);
            }
            KeyCode::Backspace => {
                if input.pop().is_none() {
                    app.mode = Mode::Normal;
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
    // ---- selection mode: motions extend, action keys consume ----
    if matches!(app.mode, Mode::Select { .. }) {
        match code {
            KeyCode::Esc => {
                app.mode = Mode::Normal;
                app.arrow_from = None;
            }
            KeyCode::Char('v') | KeyCode::Char('V') => app.mode = Mode::Normal,
            KeyCode::Char('h') if mods.is_empty() && app.selection().is_some() => {
                // `h` is both "move left" and "highlight": left needs Shift-less motion via arrows;
                // in selection mode `h` acts, `←` moves.
                app.select_action('h');
            }
            KeyCode::Char(c) if "mnbsaC".contains(c) => {
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
        KeyCode::Char('c') if ctrl => app.quit = true,
        KeyCode::Tab => app.select_annotation(1),
        KeyCode::BackTab => app.select_annotation(-1),
        KeyCode::Char('v') => app.start_select(false),
        KeyCode::Char('V') => app.start_select(true),
        KeyCode::Char('i') => app.select_word(),
        KeyCode::Char('h') => app.move_col(-1),
        KeyCode::Char('l') => app.move_col(1),
        // ←/→ advance the steps (a clicker sends them); h/l move the column.
        KeyCode::Right => app.set_step(app.doc.step + 1),
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
        KeyCode::Char('n') | KeyCode::PageDown => app.set_step(app.doc.step + 1),
        KeyCode::Char('p') | KeyCode::PageUp => app.set_step(app.doc.step.saturating_sub(1)),
        KeyCode::Char('u') => app.undo(),
        KeyCode::Char('U') => app.redo(),
        KeyCode::Char('r') if ctrl => app.redo(),
        KeyCode::Char('t') => app.toggle_theme(),
        KeyCode::Char('P') => app.present = !app.present,
        KeyCode::Esc => {
            if app.present {
                app.present = false;
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
