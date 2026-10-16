//! Estado y lógica de la aplicación, sin dependencias de dibujo. Tanto el
//! teclado como la API local pasan por aquí, así un agente y el presentador
//! hacen exactamente lo mismo.

use std::path::PathBuf;

use codedraw_core::history::History;
use codedraw_core::layout::{Layout, LayoutOptions, layout};
use codedraw_core::model::{Annotation, Document, MarkKind, Shape, ThemeName};
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

    // ---------- mutaciones ----------

    fn touch(&mut self) {
        self.revision += 1;
        self.dirty = true;
        if self.autosave
            && let Err(e) = self.save()
        {
            self.error(format!("no se pudo guardar: {e}"));
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
                "color desconocido: {c} (usa {} o #rrggbb)",
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
                    "color desconocido: {c} (usa {} o #rrggbb)",
                    codedraw_core::theme::PALETTE.join(", ")
                )));
            }
        }
        if let Some(spec) = &p.shape {
            ann.shape = spec.resolve(&self.doc)?;
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

    pub fn set_step(&mut self, step: u32) {
        let max = self.doc.max_step();
        let s = step.clamp(1, max.max(1));
        if s != self.doc.step {
            self.doc.step = s;
            self.revision += 1;
        }
    }

    pub fn undo(&mut self) {
        if self.history.undo(&mut self.doc) {
            self.touch();
            self.info("deshecho");
        } else {
            self.info("nada que deshacer");
        }
    }

    pub fn redo(&mut self) {
        if self.history.redo(&mut self.doc) {
            self.touch();
            self.info("rehecho");
        } else {
            self.info("nada que rehacer");
        }
    }

    pub fn toggle_theme(&mut self) {
        self.theme = self.theme.toggled();
        self.doc.theme = Some(self.theme.name);
        self.revision += 1;
    }

    /// Reemplaza el código (pegar, stdin). Solo si no hay anotaciones que se perderían.
    pub fn replace_code(&mut self, text: &str) -> Result<(), Error> {
        if !self.doc.annotations.is_empty() {
            return Err(Error::Invalid(
                "el documento ya tiene anotaciones; abre otro archivo para pegar código nuevo"
                    .into(),
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

    // ---------- navegación ----------

    pub fn move_cursor(&mut self, delta: i64) {
        let first = self.doc.source.first_line as i64;
        let last = self.doc.last_line() as i64;
        self.cursor_line = (self.cursor_line as i64 + delta).clamp(first, last) as u32;
    }

    // ---------- comandos ':' ----------

    pub fn run_command(&mut self, input: &str) {
        let input = input.trim();
        if input.is_empty() {
            return;
        }
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
                        .map(|_| format!("guardado en {}", self.save_path.display()))
                }
                "wq" => {
                    self.quit = true;
                    self.save().map(|_| String::new())
                }
                "box" => self.cmd_box(rest),
                "arrow" | "ar" => self.cmd_arrow(rest),
                "hl" | "highlight" => self.cmd_highlight(rest),
                "mark" | "m" => self.cmd_mark(rest),
                "note" | "n" => self.cmd_note(rest),
                "del" | "delete" | "rm" => {
                    let mut n = 0;
                    for id in rest.split_whitespace() {
                        self.delete_annotation(id)?;
                        n += 1;
                    }
                    if n == 0 {
                        Err(Error::Invalid("uso: :del ID [ID…]".into()))
                    } else {
                        Ok(format!("{n} eliminada(s)"))
                    }
                }
                "color" | "col" => {
                    let (id, c) = split_first(rest);
                    if id.is_empty() {
                        return Err(Error::Invalid(format!(
                            "uso: :color ID nombre|#rrggbb|auto   (nombres: {})",
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
                    })
                    .map(|_| {
                        format!(
                            "{id}: color {}",
                            if c.is_empty() { "automático" } else { c }
                        )
                    })
                }
                "label" => {
                    let (id, text) = rest
                        .split_once(char::is_whitespace)
                        .ok_or_else(|| Error::Invalid("uso: :label ID texto".into()))?;
                    self.update_annotation(&UpdateParams {
                        id: id.into(),
                        step: None,
                        color: None,
                        label: Some(text.trim().into()),
                        shape: None,
                    })
                    .map(|_| format!("{id} actualizada"))
                }
                "step" | "s" => {
                    let parts: Vec<&str> = rest.split_whitespace().collect();
                    match parts.as_slice() {
                        [n] => {
                            let n: u32 = n.parse().map_err(|_| {
                                Error::Invalid("uso: :step N  |  :step ID N".into())
                            })?;
                            self.set_step(n);
                            Ok(format!("paso {}/{}", self.doc.step, self.doc.max_step()))
                        }
                        [id, n] => {
                            let n: u32 = n
                                .parse()
                                .map_err(|_| Error::Invalid("uso: :step ID N".into()))?;
                            self.update_annotation(&UpdateParams {
                                id: id.to_string(),
                                step: Some(n),
                                color: None,
                                label: None,
                                shape: None,
                            })
                            .map(|_| format!("{id} ahora aparece en el paso {n}"))
                        }
                        _ => Err(Error::Invalid("uso: :step N  |  :step ID N".into())),
                    }
                }
                "next" => {
                    self.set_step(self.doc.step + 1);
                    Ok(format!("paso {}/{}", self.doc.step, self.doc.max_step()))
                }
                "prev" => {
                    self.set_step(self.doc.step.saturating_sub(1));
                    Ok(format!("paso {}/{}", self.doc.step, self.doc.max_step()))
                }
                "focus" | "f" => {
                    if rest.is_empty() {
                        self.focus = None;
                        Ok("enfoque quitado".into())
                    } else {
                        let (a, b) = parse_range(rest)?;
                        self.focus = Some((a, b));
                        Ok(format!("enfoque en {a}-{b}"))
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
                    Ok(format!("tema {}", theme_label(self.theme.name)))
                }
                "present" | "p" => {
                    self.present = !self.present;
                    Ok(if self.present {
                        "modo presentación (P para salir)".into()
                    } else {
                        String::new()
                    })
                }
                "lang" => {
                    if rest.is_empty() {
                        Ok(format!("lenguaje: {}", self.doc.source.language))
                    } else {
                        self.doc.source.language =
                            codedraw_core::highlight::normalize_language(rest);
                        self.touch();
                        Ok(format!("lenguaje: {}", self.doc.source.language))
                    }
                }
                "title" => {
                    self.doc.title = if rest.is_empty() {
                        None
                    } else {
                        Some(rest.into())
                    };
                    self.touch();
                    Ok("título actualizado".into())
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
                "autosave" => {
                    self.autosave = !matches!(rest, "off" | "no");
                    Ok(format!(
                        "autoguardado {}",
                        if self.autosave {
                            "activado"
                        } else {
                            "desactivado"
                        }
                    ))
                }
                other => Err(Error::Invalid(format!(
                    "comando desconocido: {other} (prueba :help)"
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
            parse_range(range).map_err(|_| Error::Invalid("uso: :box A-B [etiqueta]".into()))?;
        let id = self.add_annotation(
            None,
            None,
            Shape::Box {
                from_line: a,
                to_line: b,
                label: opt(label),
            },
        )?;
        Ok(format!("{id}: caja {a}-{b}"))
    }

    fn cmd_arrow(&mut self, rest: &str) -> Result<String, Error> {
        let (from, rest) = split_first(rest);
        let (to, label) = split_first(rest);
        if from.is_empty() || to.is_empty() {
            return Err(Error::Invalid(
                "uso: :arrow L:texto L:texto [etiqueta]   (ej. :arrow 2:s 7:takes move)".into(),
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
        Ok(format!("{id}: flecha L{} → L{}", f.line, t.line))
    }

    fn cmd_highlight(&mut self, rest: &str) -> Result<String, Error> {
        let (at, label) = split_first(rest);
        if at.is_empty() {
            return Err(Error::Invalid("uso: :hl L:texto [etiqueta]".into()));
        }
        let span = parse_where(at, self.cursor_line)?.resolve_span(&self.doc)?;
        let id = self.add_annotation(
            None,
            None,
            Shape::Highlight {
                span,
                label: opt(label),
            },
        )?;
        Ok(format!("{id}: resalte en L{}", span.line))
    }

    fn cmd_mark(&mut self, rest: &str) -> Result<String, Error> {
        let (kind, rest) = split_first(rest);
        let (at, label) = split_first(rest);
        let kind = MarkKind::parse(kind).ok_or_else(|| {
            Error::Invalid("uso: :mark move|borrow|mut|drop|life L:texto [etiqueta]".into())
        })?;
        if at.is_empty() {
            return Err(Error::Invalid(
                "uso: :mark move|borrow|mut|drop|life L:texto [etiqueta]".into(),
            ));
        }
        let span = parse_where(at, self.cursor_line)?.resolve_span(&self.doc)?;
        let id = self.add_annotation(
            None,
            None,
            Shape::Mark {
                kind,
                span,
                label: opt(label),
            },
        )?;
        Ok(format!("{id}: {} en L{}", kind.legend(), span.line))
    }

    fn cmd_note(&mut self, rest: &str) -> Result<String, Error> {
        let (line, text) = split_first(rest);
        let line: u32 = if line == "." {
            self.cursor_line
        } else {
            line.parse().map_err(|_| {
                Error::Invalid(
                    "uso: :note L texto  (L puede ser . para la línea del cursor)".into(),
                )
            })?
        };
        if text.is_empty() {
            return Err(Error::Invalid("la nota está vacía".into()));
        }
        let id = self.add_annotation(
            None,
            None,
            Shape::Note {
                line,
                text: text.into(),
            },
        )?;
        Ok(format!("{id}: nota en L{line}"))
    }

    // ---------- API local ----------

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
                .map_err(|e| Error::Invalid(format!("parámetros inválidos: {e}")))
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
            "scene.get" => serde_json::to_value(Scene {
                revision: self.revision,
                step: self.doc.step,
                max_step: self.doc.max_step(),
                document: self.doc.clone(),
            })?,
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
            other => return Err(Error::Invalid(format!("método desconocido: {other}"))),
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
        }
    }
}

pub fn theme_label(t: ThemeName) -> &'static str {
    match t {
        ThemeName::Dark => "oscuro",
        ThemeName::Light => "claro",
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

fn split_first(s: &str) -> (&str, &str) {
    let s = s.trim();
    match s.split_once(char::is_whitespace) {
        Some((a, b)) => (a, b.trim()),
        None => (s, ""),
    }
}

/// `A-B`, `A..B`, `A:B` o `A` → (A, B).
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
        .map_err(|_| Error::Invalid(format!("rango inválido: {s}")))?
        .as_slice()
    {
        [a] => Ok((*a, *a)),
        [a, b] => Ok((*a.min(b), *a.max(b))),
        _ => Err(Error::Invalid(format!("rango inválido: {s}"))),
    }
}

/// `L:texto[/N]`, `L`, `.:texto` (línea del cursor) o `texto` (buscar en todo el documento).
pub fn parse_where(s: &str, cursor_line: u32) -> Result<Where, Error> {
    let s = s.trim();
    if s.is_empty() {
        return Err(Error::Invalid("ancla vacía".into()));
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
                .map_err(|_| Error::Invalid(format!("línea inválida: {h}")))?,
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
        return Err(Error::Invalid(format!("ancla inválida: {s}")));
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
        a.run_command("note . aquí empieza");
        assert_eq!(a.doc.annotations.len(), 4);
        assert!(matches!(a.message, Some((_, false))));
        a.run_command("color a2 blue");
        assert_eq!(a.doc.annotations[1].color.as_deref(), Some("blue"));
        a.run_command("color a2 fucsia");
        assert!(
            matches!(a.message, Some((_, true))),
            "color desconocido debe dar error"
        );
        a.run_command("color a2 auto");
        assert_eq!(a.doc.annotations[1].color, None);
        a.run_command("del a1");
        assert_eq!(a.doc.annotations.len(), 3);
        a.run_command("arrow 2:zzz 5:t");
        assert!(
            matches!(a.message, Some((_, true))),
            "texto inexistente debe dar error"
        );
        a.undo();
        assert_eq!(a.doc.annotations.len(), 4);
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
        let resp = a.handle_request(Request {
            id: json!("y"),
            method: "annotation.delete".into(),
            params: json!({"id": "nope"}),
        });
        assert_eq!(resp.error.unwrap().code, "not_found");
    }
}
