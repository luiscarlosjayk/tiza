//! `codedraw-tui`: the CodeDraw terminal interface and its local API server.

pub mod app;
pub mod ipc;
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
    let result = event_loop(&mut terminal, &mut app, ipc.as_ref());
    let _ = execute!(io::stdout(), DisableBracketedPaste);
    ratatui::restore();
    if app.dirty {
        let _ = app.save();
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
                let resp = app.handle_request(r.req);
                let _ = r.reply.send(resp);
                needs_draw = true;
            }
        }
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(k) if k.kind != KeyEventKind::Release => {
                    on_key(app, k.code, k.modifiers);
                    needs_draw = true;
                }
                Event::Paste(text) => {
                    match app.replace_code(&text) {
                        Ok(()) => app.info(format!("{} lines pasted", app.doc.code.len())),
                        Err(e) => app.error(e.to_string()),
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

fn on_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    // Overlays: any of these keys closes them.
    if app.overlay != Overlay::None {
        if matches!(
            code,
            KeyCode::Esc
                | KeyCode::Enter
                | KeyCode::Char('q')
                | KeyCode::Char('?')
                | KeyCode::Char('l')
        ) {
            app.overlay = Overlay::None;
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
            _ => {}
        }
        return;
    }
    app.message = None;
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    match code {
        KeyCode::Char('c') if ctrl => app.quit = true,
        KeyCode::Char('q') => app.quit = true,
        KeyCode::Char(':') => app.mode = Mode::Command(String::new()),
        KeyCode::Char('?') => app.overlay = Overlay::Help,
        KeyCode::Char('l') => app.overlay = Overlay::List,
        KeyCode::Char('j') | KeyCode::Down => app.move_cursor(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_cursor(-1),
        KeyCode::Char('J') | KeyCode::PageDown => app.move_cursor(app.size.1 as i64 / 2),
        KeyCode::Char('K') | KeyCode::PageUp => app.move_cursor(-(app.size.1 as i64) / 2),
        KeyCode::Char('g') | KeyCode::Home => app.cursor_line = app.doc.source.first_line,
        KeyCode::Char('G') | KeyCode::End => app.cursor_line = app.doc.last_line(),
        KeyCode::Char('n') | KeyCode::Right => app.set_step(app.doc.step + 1),
        KeyCode::Char('p') | KeyCode::Left => app.set_step(app.doc.step.saturating_sub(1)),
        KeyCode::Char('u') => app.undo(),
        KeyCode::Char('U') => app.redo(),
        KeyCode::Char('r') if ctrl => app.redo(),
        KeyCode::Char('t') => app.toggle_theme(),
        KeyCode::Char('P') => app.present = !app.present,
        KeyCode::Esc => {
            if app.present {
                app.present = false;
            } else {
                app.focus = None;
            }
        }
        _ => {}
    }
}
