//! Preflight (t-101): `tiza check FILE` loads a document the way the TUI
//! does, but instead of stopping at the first problem it lists them all —
//! every anchor and marker, every move at its step, every linked file,
//! every redact a move widens or drops, every annotation a move hides,
//! every label clipped with `…` at the width of the take and every step
//! with nothing new — naming the annotation or move, the step, the file
//! (for linked views) and the line.
//!
//! Errors are what would stop the document from loading or a move from
//! applying; warnings are what would spoil a take. Nothing is written,
//! and no finding quotes text under a redact (t-117): a move target is
//! scrubbed to `«…»` on a document with redacts, as `move.add` does
//! (t-154), and a clipped label under a mask is named without its text.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use crate::layout::{
    LayoutOptions, MAX_WIDTH, MIN_WIDTH, StableGeometry, layout_with, stable_geometry,
};
use crate::model::{Document, SCHEMA_VERSION, Shape, sanitize_texts};
use crate::moves::{self, Move, MoveAction, ViewKey, Wait};
use crate::theme::Theme;

/// Width of a take when neither the command line nor the document says
/// otherwise: the same as `tiza render`.
pub const DEFAULT_WIDTH: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warning,
}

/// One problem `check` found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Finding {
    pub level: Level,
    /// Stable kind, for scripts: `schema`, `code`, `link`, `duplicate_id`,
    /// `unknown_id`, `unknown_file`, `anchor`, `marker`, `move`, `rebind`,
    /// `steps`, `redact`, `unanchored`, `clipped_label`, `empty_step`,
    /// `masked_text` (a label, note or title repeating a masked value, or
    /// holding masked text, t-174), `clear_value` (an embedded `.tiza`
    /// nobody saved in Tiza holds a masked value in clear, t-174).
    pub code: &'static str,
    /// The annotation id (`a3`) or the move (`move #2`, 1-based as in
    /// `move.list`) the finding is about.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The step it happens at (the first one, when it lasts several).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    /// The linked file it is in (absent: the document's main file).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The code line of the anchor or the move target; for a `schema`
    /// finding, the line of the `.tiza` itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    pub message: String,
    /// `clipped_label` only: the full label text (absent when the label is
    /// under a redact: never quoted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl Finding {
    fn new(level: Level, code: &'static str, message: impl Into<String>) -> Self {
        Finding {
            level,
            code,
            id: None,
            step: None,
            file: None,
            line: None,
            message: message.into(),
            text: None,
        }
    }
    fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
    fn step(mut self, step: u32) -> Self {
        self.step = Some(step);
        self
    }
    fn file(mut self, file: Option<String>) -> Self {
        self.file = file;
        self
    }
    fn line(mut self, line: Option<u32>) -> Self {
        self.line = line;
        self
    }

    /// One line for a terminal: `error: move #2, step 4, L3: …`.
    pub fn to_line(&self) -> String {
        let mut place: Vec<String> = Vec::new();
        if let Some(id) = &self.id {
            place.push(id.clone());
        }
        if let Some(s) = self.step {
            place.push(format!("step {s}"));
        }
        if let Some(f) = &self.file {
            place.push(f.clone());
        }
        if let Some(l) = self.line {
            place.push(if self.code == "schema" {
                format!("line {l}")
            } else {
                format!("L{l}")
            });
        }
        let level = match self.level {
            Level::Error => "error",
            Level::Warning => "warning",
        };
        if place.is_empty() {
            format!("{level}: {}", self.message)
        } else {
            format!("{level}: {}: {}", place.join(", "), self.message)
        }
    }
}

/// What `check` was asked.
#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    /// Width to check the labels at; `None`: [`DEFAULT_WIDTH`].
    pub width: Option<usize>,
    /// The `.tiza`'s file name, which the warnings about an embedded file
    /// nobody saved in Tiza name (`demo.tiza still holds it in clear`);
    /// `None`: `the file`.
    pub name: Option<String>,
    /// What the `.tiza` holds of the texts the filter masks, when the
    /// caller knows it (the live instance, [`crate::redact::Kept`]); `None`:
    /// as a reader finds the file ([`crate::redact::Kept::reading`]).
    pub kept: Option<crate::redact::Kept>,
    /// The linked files the `.tiza` holds in `views` when the caller knows
    /// them (the live instance, whose report freezes every linked file);
    /// `None`: the document's own `views`.
    pub frozen: Option<Vec<String>>,
}

/// Everything `check` found about one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Report {
    /// No errors (warnings do not count; `--strict` makes them count).
    pub ok: bool,
    pub errors: usize,
    pub warnings: usize,
    /// The width the labels were checked at.
    pub width: usize,
    /// Last step of the document (`None` when it could not be loaded).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_step: Option<u32>,
    /// Narrowest width from which no label is clipped at any step (every
    /// width from it up shows them whole), with the stable geometry of a
    /// take (`None`: the document could not be loaded, or even width 400
    /// clips a label).
    pub min_clean_width: Option<usize>,
    /// Only when `width` is below `min_clean_width` and no label is clipped
    /// there (labels go under the lines on narrow takes): which narrower
    /// widths are clean too.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narrower: Option<Narrower>,
    pub findings: Vec<Finding>,
    /// The masked values of the document (in memory only, never written
    /// or printed): [`Report::to_text`] masks the file name with them.
    #[serde(skip)]
    #[schemars(skip)]
    pub values: crate::redact::KnownValues,
}

/// The widths below `min_clean_width` where a take shows every label
/// whole: below the width at which labels go to the right margin, notes
/// and labels go under their lines and fit again, until the code itself
/// no longer fits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct Narrower {
    /// Width ranges `[from, to]` (inclusive) below `min_clean_width` where
    /// no label and no code line is clipped at any step.
    pub clean: Vec<[usize; 2]>,
    /// Narrowest width at which no code line is clipped at any step.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_fits_from: Option<usize>,
    /// Width from which labels and notes go to the right margin (below it
    /// they go under the lines).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_from: Option<usize>,
}

/// `79-93`, `30-40, 79-93`.
fn ranges_text(r: &[[usize; 2]]) -> String {
    r.iter()
        .map(|[a, b]| {
            if a == b {
                a.to_string()
            } else {
                format!("{a}-{b}")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

impl Report {
    fn new(
        width: usize,
        mut findings: Vec<Finding>,
        max_step: Option<u32>,
        clean: Option<usize>,
    ) -> Self {
        // Errors first; each group in the order it was found.
        findings.sort_by_key(|f| f.level);
        let errors = findings.iter().filter(|f| f.level == Level::Error).count();
        Report {
            ok: errors == 0,
            errors,
            warnings: findings.len() - errors,
            width,
            max_step,
            min_clean_width: clean,
            narrower: None,
            findings,
            values: Default::default(),
        }
    }

    /// The report for a terminal: one line per finding, a summary, and the
    /// narrowest clean width.
    pub fn to_text(&self, name: &str) -> String {
        let mut out = String::new();
        for f in &self.findings {
            out.push_str(&f.to_line());
            out.push('\n');
        }
        let plural = |n: usize, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
        // The file's name is shown text (t-174 review: `9vK7wPx3Lm.tiza`).
        let name = crate::redact::shown_with(name, &self.values.0);
        out.push_str(&format!(
            "{name}: {}, {}{}\n",
            plural(self.errors, "error"),
            plural(self.warnings, "warning"),
            match self.max_step {
                Some(0) => format!(" (step 0 only, labels at width {})", self.width),
                Some(m) => format!(" (steps 0-{m}, labels at width {})", self.width),
                None => String::new(),
            }
        ));
        if self.max_step.is_some() {
            match (self.min_clean_width, &self.narrower) {
                (Some(w), Some(n)) => {
                    let width = self.width;
                    if n.code_fits_from.is_some_and(|c| width < c) {
                        out.push_str(&format!(
                            "no label is clipped at width {width}, but code lines are: the code needs {} columns\n",
                            n.code_fits_from.unwrap_or(w)
                        ));
                    }
                    let under = n
                        .margin_from
                        .map(|m| format!(" (notes go under the lines below {m} columns)"))
                        .unwrap_or_default();
                    out.push_str(&if n.clean.is_empty() {
                        format!(
                            "no label is clipped at {w} or wider; no narrower width shows every label and every code line whole\n"
                        )
                    } else {
                        format!(
                            "no label is clipped at {w} or wider; {} {} clean too{under}\n",
                            ranges_text(&n.clean),
                            if n.clean.len() == 1 && n.clean[0][0] == n.clean[0][1] {
                                "is"
                            } else {
                                "are"
                            }
                        )
                    });
                }
                (Some(w), None) => out.push_str(&format!(
                    "no label is clipped at {w} or wider (`tiza render --width {w}`)\n"
                )),
                (None, _) => out.push_str(&format!(
                    "no width up to {MAX_WIDTH} shows every label whole: shorten the labels\n"
                )),
            }
        }
        out
    }
}

/// The last step a take of `doc` shows: 0 for a document with nothing on
/// it (one image: the code), else its last step.
pub fn last_step(doc: &Document) -> u32 {
    if doc.annotations.is_empty() && doc.moves.is_empty() {
        0
    } else {
        doc.max_step()
    }
}

/// Checks a `.tiza` read from disk (its relative files resolve next to it).
pub fn check_path(path: &Path, opts: &CheckOptions) -> Result<Report, crate::Error> {
    let text = std::fs::read_to_string(path)?;
    let mut opts = opts.clone();
    if opts.name.is_none() {
        opts.name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    }
    Ok(check_text(&text, path.parent(), &opts))
}

/// Checks the text of a `.tiza`; `base_dir` resolves its relative files.
pub fn check_text(text: &str, base_dir: Option<&Path>, opts: &CheckOptions) -> Report {
    check_text_with(text, base_dir, opts, &Default::default())
}

/// [`check_text`] knowing the values a live instance keeps in memory
/// ([`crate::redact::KnownValues`], t-174): a `.tiza` text never holds them.
fn check_text_with(
    text: &str,
    base_dir: Option<&Path>,
    opts: &CheckOptions,
    known: &crate::redact::KnownValues,
) -> Report {
    let width = opts.width.unwrap_or(DEFAULT_WIDTH);
    let mut out: Vec<Finding> = Vec::new();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // ---- the format ----
    let mut raw: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            out.push(
                Finding::new(Level::Error, "schema", format!("not valid JSON: {e}"))
                    .line(Some(e.line() as u32)),
            );
            return Report::new(width, out, None, None);
        }
    };
    let found = raw.get("schema").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
    if found > SCHEMA_VERSION {
        out.push(Finding::new(
            Level::Error,
            "schema",
            format!(
                ".tiza file with schema {found}; this version understands up to {SCHEMA_VERSION}"
            ),
        ));
        return Report::new(width, out, None, None);
    }
    // The values serde quotes in a parse error, on a document with a
    // redact, go through the redact filter (t-167).
    let guard = crate::redact::json_has_redact(&raw).then(|| crate::redact::json_strings(&raw));
    let migrated = crate::fs::MIGRATIONS.iter().any(|(from, _)| found <= *from);
    let parsed: Result<Document, serde_json::Error> = if migrated {
        for (from, migrate) in crate::fs::MIGRATIONS {
            if found <= *from {
                migrate(&mut raw);
            }
        }
        serde_json::from_value(raw)
    } else {
        // Straight from the text: the error keeps its line and column.
        serde_json::from_str(text)
    };
    let mut doc = match parsed {
        Ok(d) => d,
        Err(e) => {
            let line = (e.line() > 0).then_some(e.line() as u32);
            out.push(
                Finding::new(
                    Level::Error,
                    "schema",
                    match guard {
                        Some(written) => crate::redact::Echo::closed()
                            .written(written)
                            .serde_text(&format!("does not follow the .tiza format: {e}")),
                        None => format!("does not follow the .tiza format: {e}"),
                    },
                )
                .line(line),
            );
            return Report::new(width, out, None, None);
        }
    };
    doc.schema = SCHEMA_VERSION;
    doc.known_values = known.clone();
    // ---- the code ----
    if let Err(e) = crate::fs::resolve_code(&mut doc, base_dir) {
        out.push(Finding::new(Level::Error, "code", e.to_string()));
        return Report::new(width, out, None, None);
    }
    // ---- the numbers (t-166): a step past the cap or a first line near
    // `u32::MAX` is one error, before anything is laid out step by step.
    if let Err(e) = doc.check_bounds() {
        let e = crate::redact::Echo::loading(&doc).error(e);
        out.push(Finding::new(Level::Error, "bounds", e.to_string()));
        return Report::new(width, out, None, None);
    }
    let missing = load_links(&mut doc, base_dir, &mut out);
    let doc = tidy(doc, &missing, &mut out);
    // A text that repeats a masked value is masked on every surface; said
    // here, never quoted (t-174).
    let kept = opts.kept.clone().unwrap_or_else(|| {
        crate::redact::Kept::reading(&doc, opts.name.as_deref().unwrap_or("the file"))
    });
    for m in crate::redact::masked_texts(&doc, &kept) {
        let f = Finding::new(
            Level::Warning,
            "masked_text",
            match (&m.id, m.step) {
                (Some(_), _) | (None, Some(_)) => m.message(),
                (None, None) => m.notice(),
            },
        );
        out.push(match (m.id, m.step) {
            (Some(id), Some(step)) => f.id(id).step(step),
            (None, Some(step)) => f.step(step),
            _ => f,
        });
    }
    // An embedded `.tiza` nobody saved in Tiza (written by hand or by
    // another tool) that holds a value under a redact in clear: Tiza would
    // save it masked (t-174 rework point 3).
    let frozen: Vec<String> = opts
        .frozen
        .clone()
        .unwrap_or_else(|| doc.views.keys().cloned().collect());
    let clear = match &kept {
        crate::redact::Kept::InClear { file } => Some((file.clone(), true)),
        // A reader of a file reference: its frozen linked files still count.
        crate::redact::Kept::OnScreen if opts.kept.is_none() => Some((
            crate::redact::shown(opts.name.as_deref().unwrap_or("the file"), &doc),
            false,
        )),
        _ => None,
    };
    if let Some((file, main)) = &clear {
        let (ids, moves) = crate::redact::values_in_clear(&doc, *main, &frozen);
        for id in ids {
            out.push(
                Finding::new(
                    Level::Warning,
                    "clear_value",
                    if *main {
                        format!(
                            "the embedded code holds the value under this redact in clear; {file} still holds it — open and save it in Tiza"
                        )
                    } else {
                        // A file reference keeps what it holds when saved;
                        // `:embed` freezes everything masked, but embeds the
                        // main code too, and the message says so.
                        format!(
                            "the linked file frozen in `views` holds the value under this redact in clear; {file} still holds it — drop it from `views` (Tiza then reads the file), or :embed in Tiza (which embeds the main code too)"
                        )
                    },
                )
                .id(crate::redact::shown(&id, &doc)),
            );
        }
        for i in moves {
            out.push(
                Finding::new(
                    Level::Warning,
                    "clear_value",
                    format!(
                        "its content holds a masked value in clear; {file} still holds it — open and save it in Tiza"
                    ),
                )
                .id(format!("move #{}", i + 1)),
            );
        }
    }
    let clean = check_loaded(&doc, &missing, width, &mut out);
    let max = last_step(&doc);
    // Last line of defence (t-117): whatever a finding says, the text under
    // a mask never leaves `check`.
    scrub_secrets(&doc, &mut out);
    let mut report = Report::new(width, out, Some(max), clean);
    report.values = crate::redact::KnownValues(crate::redact::all_values(&doc));
    if let Some(c) = clean
        && width < c
        && !report.findings.iter().any(|f| f.code == "clipped_label")
    {
        report.narrower = Some(narrower(&doc, &Theme::dark(), c));
    }
    report
}

/// Checks a document already loaded in memory — the live one of a TUI, for
/// the socket's `check` (t-101): the same checks as `tiza check` on the
/// file, on the real (unmasked) code and linked files the TUI holds, not on
/// the masked copy an agent reads. Only the report leaves; it never quotes
/// text under a redact. `base_dir` resolves relative files the document
/// names but did not load.
pub fn check_document(doc: &Document, base_dir: Option<&Path>, opts: &CheckOptions) -> Report {
    let mut opts = opts.clone();
    if opts.frozen.is_none() {
        opts.frozen = Some(doc.views.keys().cloned().collect());
    }
    let opts = &opts;
    let mut d = doc.clone();
    d.sync_code_src();
    d.code_src = crate::model::CodeSource::Embedded(d.base().to_vec());
    for file in d.link_files() {
        if let Some(code) = d.view_code.get(&file) {
            d.views.insert(file, code.clone());
        }
    }
    match serde_json::to_string(&d) {
        Ok(text) => check_text_with(&text, base_dir, opts, &doc.known_values),
        Err(e) => Report::new(
            opts.width.unwrap_or(DEFAULT_WIDTH),
            vec![Finding::new(Level::Error, "schema", e.to_string())],
            None,
            None,
        ),
    }
}

/// The texts under every mask at any step (see
/// [`crate::redact::masked_values`]).
fn secrets(doc: &Document) -> Vec<String> {
    crate::redact::all_values(doc).to_vec()
}

/// Takes every text under a mask out of the findings.
fn scrub_secrets(doc: &Document, out: &mut [Finding]) {
    let secrets = secrets(doc);
    if secrets.is_empty() {
        return;
    }
    // Shown text (t-174): an id, a message or a clipped label sharing text
    // with a masked value is masked, as on screen.
    let filter = crate::redact::Echo::for_messages(doc);
    for f in out.iter_mut() {
        if let Some(id) = &mut f.id {
            *id = filter.shown(id);
        }
        f.message = filter.shown_message(&f.message);
        if let Some(t) = &mut f.text {
            *t = filter.shown(t);
        }
    }
    for f in out {
        for s in &secrets {
            if f.message.contains(s.as_str()) {
                f.message = f.message.replace(s.as_str(), "…");
            }
            if f.text.as_deref().is_some_and(|t| t.contains(s.as_str())) {
                f.text = None;
            }
        }
    }
}

/// Which widths below `min_clean` are clean too (see [`Narrower`]).
fn narrower(doc: &Document, theme: &Theme, min_clean: usize) -> Narrower {
    let geometry = stable_geometry(doc, &take_options(doc, MAX_WIDTH, 0));
    let mut clean: Vec<[usize; 2]> = Vec::new();
    let mut code_fits_from = None;
    let mut margin_from = None;
    for w in MIN_WIDTH..min_clean {
        let (mut labels_ok, mut code_ok, mut margin) = (true, true, false);
        for step in doc.change_steps(last_step(doc)) {
            let l = layout_with(doc, theme, &take_options(doc, w, step), Some(&geometry));
            labels_ok &= l.clipped_labels.is_empty();
            code_ok &= l.clipped_code.is_none();
            if step == 0 {
                margin = l.labels_in_margin;
            }
        }
        if code_ok {
            code_fits_from.get_or_insert(w);
        } else {
            code_fits_from = None;
        }
        if margin {
            margin_from.get_or_insert(w);
        } else {
            margin_from = None;
        }
        if labels_ok && code_ok {
            match clean.last_mut() {
                Some(r) if r[1] + 1 == w => r[1] = w,
                _ => clean.push([w, w]),
            }
        }
    }
    Narrower {
        clean,
        code_fits_from,
        margin_from,
    }
}

/// Loads every linked file; one error per file that cannot be read (not
/// one per annotation or move in it). Returns the files that are missing.
fn load_links(
    doc: &mut Document,
    base_dir: Option<&Path>,
    out: &mut Vec<Finding>,
) -> BTreeSet<String> {
    let mut missing = BTreeSet::new();
    let mut by_file: BTreeMap<String, Vec<(usize, u32, String)>> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for (i, m) in doc.moves.iter().enumerate() {
        if let MoveAction::CodeLink(l) = &m.action
            && let Some(key) = doc.view_key(&l.file)
        {
            if !by_file.contains_key(&key) {
                order.push(key.clone());
            }
            by_file
                .entry(key)
                .or_default()
                .push((i, m.at_step, l.file.clone()));
        }
    }
    for key in order {
        let links = &by_file[&key];
        if let Some(code) = doc
            .views
            .get(&key)
            .cloned()
            .or_else(|| links.iter().find_map(|(_, _, f)| doc.views.get(f).cloned()))
        {
            doc.view_code.insert(
                key,
                if code.is_empty() {
                    vec![String::new()]
                } else {
                    code
                },
            );
            continue;
        }
        let (first, step, file) = &links[0];
        if let Err(e) = crate::fs::load_view(doc, file, base_dir) {
            let which = links
                .iter()
                .map(|(i, s, _)| format!("#{} (step {s})", i + 1))
                .collect::<Vec<_>>()
                .join(", ");
            out.push(
                Finding::new(
                    Level::Error,
                    "link",
                    format!(
                        "{e}; linked by move {which}: its moves and annotations are not checked"
                    ),
                )
                .id(format!("move #{}", first + 1))
                .step(*step)
                .file(Some(key.clone())),
            );
            missing.insert(key);
        }
    }
    missing
}

/// A move that does nothing, put where a broken one was so the moves after
/// it keep their numbers and are still checked.
fn placeholder(at_step: u32) -> Move {
    Move {
        at_step: at_step.max(1),
        action: MoveAction::Wait(Wait { ms: 1 }),
        rebind: BTreeMap::new(),
    }
}

/// A move's error without the `move #N, step S` the finding already
/// names.
fn move_message(e: crate::Error, index: usize, at_step: u32) -> String {
    let tag = format!("move #{}, step {at_step}", index + 1);
    match e {
        crate::Error::MoveNoMatch { what, .. } => {
            format!("{what} matches nothing in the code at this step")
        }
        crate::Error::NoMatch(what) => format!("{what} matches nothing in the code at this step"),
        other => {
            let m = other.to_string();
            m.strip_prefix(&format!("{tag}: "))
                .unwrap_or(&m)
                .to_string()
        }
    }
}

/// The first `L<n>` of a message: the line a move error points at.
fn first_line_in(msg: &str) -> Option<u32> {
    let b = msg.as_bytes();
    (0..b.len()).find_map(|i| {
        if b[i] == b'L' && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let digits: String = msg[i + 1..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            digits.parse().ok()
        } else {
            None
        }
    })
}

/// What `normalize_loaded` does, collecting instead of stopping: ids,
/// named steps, files, and every move at its step (a broken move is
/// reported and replaced by one that does nothing, so the rest are still
/// checked against the code they would really see).
fn tidy(mut doc: Document, missing: &BTreeSet<String>, out: &mut Vec<Finding>) -> Document {
    if doc.code.is_empty() {
        doc.code.push(String::new());
    }
    let redacted = crate::redact::has_redacts(&doc);
    // ---- ids ----
    let mut seen = BTreeSet::new();
    doc.annotations.retain(|a| {
        if seen.insert(a.id.clone()) {
            return true;
        }
        out.push(
            Finding::new(
                Level::Error,
                "duplicate_id",
                format!(
                    "an annotation with id {} already exists; this one is not checked",
                    a.id
                ),
            )
            .id(a.id.clone())
            .step(a.step),
        );
        false
    });
    for a in &mut doc.annotations {
        sanitize_texts(&mut a.shape);
    }
    if let Err(e) = crate::steps::normalize(&mut doc.steps) {
        out.push(Finding::new(Level::Error, "steps", e.to_string()));
        doc.steps.clear();
    }
    for w in doc.step_warnings() {
        out.push(Finding::new(Level::Warning, "steps", w));
    }
    for (file, code) in doc.views.clone() {
        let key = Document::normalize_file(&file);
        doc.view_code.entry(key).or_insert_with(|| {
            if code.is_empty() {
                vec![String::new()]
            } else {
                code
            }
        });
    }
    let keys: Vec<Option<ViewKey>> = doc
        .annotations
        .iter()
        .map(|a| a.file.as_deref().map(|f| doc.view_key(f)))
        .collect();
    for (a, k) in doc.annotations.iter_mut().zip(keys) {
        if let Some(k) = k {
            a.file = k;
        }
    }
    let ids: BTreeSet<String> = doc.annotations.iter().map(|a| a.id.clone()).collect();
    // ---- moves: the static rules ----
    let mut moves = doc.moves.clone();
    let mut broken = vec![false; moves.len()];
    for (i, m) in moves.iter_mut().enumerate() {
        let name = format!("move #{}", i + 1);
        if m.at_step == 0 {
            out.push(
                Finding::new(
                    Level::Error,
                    "move",
                    "at_step must be >= 1 (step 0 is the untouched code)",
                )
                .id(name.clone())
                .step(0),
            );
            broken[i] = true;
            continue;
        }
        if !m.rebind.is_empty() && !matches!(m.action, MoveAction::CodePatch(_)) {
            out.push(
                Finding::new(
                    Level::Error,
                    "rebind",
                    format!("rebind only applies to code.patch, not {}", m.action.name()),
                )
                .id(name.clone())
                .step(m.at_step),
            );
            m.rebind.clear();
        }
        let mut drop = Vec::new();
        for (id, spec) in &m.rebind {
            if !ids.contains(id) {
                out.push(
                    Finding::new(
                        Level::Error,
                        "unknown_id",
                        format!("rebind names unknown annotation «{id}»"),
                    )
                    .id(name.clone())
                    .step(m.at_step),
                );
                drop.push(id.clone());
            } else if spec.is_empty() {
                out.push(
                    Finding::new(
                        Level::Error,
                        "rebind",
                        format!("rebind «{id}» says nowhere to go: use `at` {{find | line, col}}, `from`/`to` (arrows) or `from_line`/`to_line` (boxes, callouts, spotlights)"),
                    )
                    .id(name.clone())
                    .step(m.at_step),
                );
                drop.push(id.clone());
            }
        }
        for id in drop {
            m.rebind.remove(&id);
        }
    }
    // Moves that edit a file that could not be read: said once, by the
    // file's own error.
    let mut order: Vec<usize> = (0..moves.len()).collect();
    order.sort_by_key(|&i| (moves[i].at_step, i));
    let mut shown: ViewKey = None;
    for &i in &order {
        if let MoveAction::CodeLink(l) = &moves[i].action {
            shown = doc.view_key(&l.file);
            if shown.as_ref().is_some_and(|k| missing.contains(k)) {
                broken[i] = true;
            }
        } else if shown.as_ref().is_some_and(|k| missing.contains(k)) {
            broken[i] = true;
        }
    }
    let mut trial: Vec<Move> = moves
        .iter()
        .zip(&broken)
        .map(|(m, b)| {
            if *b {
                placeholder(m.at_step)
            } else {
                m.clone()
            }
        })
        .collect();
    // ---- moves: each one on the code at its step ----
    doc.base_code = doc.code.clone();
    doc.base_first_line = doc.source.first_line;
    doc.moves.clear();
    let mut accepted: Vec<Move> = trial.iter().map(|m| placeholder(m.at_step)).collect();
    for &i in &order {
        if broken[i] {
            continue;
        }
        let mut with = accepted.clone();
        with[i] = trial[i].clone();
        let mut d = doc.clone();
        d.moves = with.clone();
        match moves::project(&d, &with, u32::MAX, true) {
            Ok(_) => accepted = with,
            Err(e) => {
                let e = if redacted {
                    d.scrub_targets(&trial[i], e)
                } else {
                    e
                };
                let file = d.shown_view_at(trial[i].at_step).file;
                let msg = in_file(move_message(e, i, trial[i].at_step), &file);
                out.push(
                    Finding::new(Level::Error, "move", msg.clone())
                        .id(format!("move #{}", i + 1))
                        .step(trial[i].at_step)
                        .file(file)
                        .line(first_line_in(&msg)),
                );
                broken[i] = true;
                trial[i] = placeholder(trial[i].at_step);
            }
        }
    }
    // ---- rebind finds, one move at a time ----
    for i in 0..trial.len() {
        let m = trial[i].clone();
        if m.rebind.is_empty() {
            continue;
        }
        let mut only: Vec<Move> = trial.clone();
        for (j, o) in only.iter_mut().enumerate() {
            if j != i {
                o.rebind.clear();
            }
        }
        let mut d = doc.clone();
        d.moves = only.clone();
        if let Err(e) = moves::validate(&d, &only, &d.annotations) {
            let e = if redacted { d.scrub_targets(&m, e) } else { e };
            let file = d.shown_view_at(m.at_step).file;
            let msg = in_file(move_message(e, i, m.at_step), &file);
            out.push(
                Finding::new(Level::Error, "rebind", msg)
                    .id(format!("move #{}", i + 1))
                    .step(m.at_step)
                    .file(file),
            );
            trial[i].rebind.clear();
        }
    }
    doc.moves = trial;
    if doc.moves.is_empty() {
        doc.base_code.clear();
    } else {
        doc.reframe_redacts();
        doc.sync_code_to_step();
    }
    if doc.step > doc.max_step() {
        doc.step = doc.max_step();
    }
    doc
}

/// A message about a linked file says which one: «src/queue.rs spans 1 to
/// 30», not «the document spans…» (the document's own file keeps it).
fn in_file(msg: String, file: &Option<String>) -> String {
    match file {
        Some(f) => msg.replace("(the document spans ", &format!("({f} spans ")),
        None => msg,
    }
}

/// The code a marker of `a` is looked for in: its file at its step, with
/// the masks of that step drawn on it (t-154's rule for move targets): a
/// marker whose text is under a redact is simply not there, with the same
/// answer as text that does not exist.
fn marker_code(doc: &Document, a: &crate::model::Annotation) -> Document {
    let mut d = doc.clone();
    if let Ok((code, first)) = doc.view_code_at(&a.file, a.step) {
        d.moves.clear();
        d.base_code.clear();
        d.code = code;
        d.source.first_line = first;
    }
    if crate::redact::has_redacts(doc) {
        let masks: Vec<crate::model::Annotation> = if doc.moves.is_empty() {
            doc.annotations
                .iter()
                .filter(|r| r.file == a.file && crate::redact::active(doc, r, a.step))
                .cloned()
                .collect()
        } else if doc.shown_view_at(a.step).file == a.file {
            let v = doc.view_at(a.step).doc;
            v.annotations
                .iter()
                .filter(|r| crate::redact::active(&v, r, a.step))
                .cloned()
                .collect()
        } else {
            crate::redact::base_redacts(doc, &a.file)
        };
        d.code = crate::redact::mask_lines(&d.code, d.source.first_line, &masks);
    }
    // The masks are drawn on the lines: nothing else to skip.
    d.annotations.clear();
    d
}

/// The lines `text` is on in `d`, once per hit, from line `from` (and,
/// on that line, from grapheme column `col`).
fn hits(d: &Document, text: &str, from: u32, col: u32) -> Vec<u32> {
    let mut out = Vec::new();
    for line in from..=d.last_line() {
        let Some(t) = d.line_text(line) else {
            continue;
        };
        let t = if line == from {
            crate::model::grapheme_byte_offset(t, col).map_or("", |b| &t[b..])
        } else {
            t
        };
        out.extend(std::iter::repeat_n(line, t.matches(text).count()));
    }
    out
}

/// `L3, L6` (a line hit twice is named twice: `L3, L3`).
fn lines_text(lines: &[u32]) -> String {
    lines
        .iter()
        .map(|l| format!("L{l}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A range made from two markers (t-097): an error when it cannot be found
/// again, a warning when a marker matches more than once (the range takes
/// the first match, so an edit above it can move it). On a document with
/// redacts the markers are matched on the masked code and an error never
/// quotes them (t-154).
fn check_markers(
    doc: &Document,
    a: &crate::model::Annotation,
    m: &crate::model::RangeMarkers,
    redacted: bool,
    out: &mut Vec<Finding>,
) {
    let d = marker_code(doc, a);
    let finding = |level, msg: String| {
        Finding::new(level, "marker", msg)
            .id(a.id.clone())
            .step(a.step)
            .file(a.file.clone())
            .line(anchor_line(&a.shape))
    };
    match d.resolve_markers(&m.start, &m.end) {
        Err(e) => {
            let mut msg = match e {
                crate::Error::NoMatch(t) | crate::Error::TextNotFound(t) => {
                    format!("{t} matches nothing")
                }
                other => other.to_string(),
            };
            if redacted {
                for t in [&m.start, &m.end] {
                    msg = msg.replace(&format!("«{t}»"), "«…»");
                }
                msg.push_str(
                    " (on a document with redacts, text under a mask counts as not there)",
                );
            }
            out.push(finding(
                Level::Error,
                format!("{msg}: the range cannot be found again when the code changes"),
            ));
        }
        Ok((from, to)) => {
            // Both markers were found on the masked code: what they match is
            // on screen, so quoting them gives nothing away.
            let starts = hits(&d, &m.start, d.source.first_line, 0);
            if starts.len() > 1 {
                out.push(finding(
                    Level::Warning,
                    format!(
                        "start marker «{}» matches {} times ({}): the range takes the first one (L{from}); make it unique, or an edit above it can move the range",
                        m.start,
                        starts.len(),
                        lines_text(&starts)
                    ),
                ));
            }
            let s = d.find_text(&m.start, 1, None);
            let ends = s.map_or_else(Vec::new, |s| hits(&d, &m.end, s.line, s.end));
            if ends.len() > 1 {
                out.push(finding(
                    Level::Warning,
                    format!(
                        "end marker «{}» matches {} times after the start ({}): the range ends at the first one (L{to}); make it unique, or an edit inside the range can move its end",
                        m.end,
                        ends.len(),
                        lines_text(&ends)
                    ),
                ));
            }
        }
    }
}

/// The line an annotation is anchored on (its first line).
fn anchor_line(shape: &Shape) -> Option<u32> {
    match shape {
        Shape::Highlight { span, .. } | Shape::Mark { span, .. } | Shape::Redact { span, .. } => {
            Some(span.line)
        }
        Shape::Arrow { from, .. } => Some(from.line),
        Shape::Note { line, .. } => Some(*line),
        Shape::Box { from_line, .. } | Shape::Callout { from_line, .. } => Some(*from_line),
        Shape::Spotlight { from_line, .. } => *from_line,
    }
}

/// `4`, `4-7`, `2, 5-6`.
fn steps_text(steps: &[u32]) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < steps.len() {
        let mut j = i;
        while j + 1 < steps.len() && steps[j + 1] == steps[j] + 1 {
            j += 1;
        }
        out.push(if i == j {
            steps[i].to_string()
        } else {
            format!("{}-{}", steps[i], steps[j])
        });
        i = j + 1;
    }
    out.join(", ")
}

/// The labels clipped at `width`, at every step of the take: `(id, text,
/// dropped)` → the steps it is clipped at.
fn clipped_at(
    doc: &Document,
    theme: &Theme,
    geometry: &StableGeometry,
    width: usize,
    first_only: bool,
) -> Vec<((String, String, bool), Vec<u32>)> {
    let mut out: Vec<((String, String, bool), Vec<u32>)> = Vec::new();
    // Laid out once per step where something changes (t-166); a label is
    // clipped at every step of that step's run.
    for (step, run) in doc.change_runs(last_step(doc)) {
        let l = layout_with(doc, theme, &take_options(doc, width, step), Some(geometry));
        for c in l.clipped_labels {
            let key = (c.id, c.text, c.dropped);
            match out.iter_mut().find(|(k, _)| k.0 == key.0 && k.1 == key.1) {
                Some((_, steps)) => steps.extend(run.clone()),
                None => out.push((key, run.clone().collect())),
            }
        }
        if first_only && !out.is_empty() {
            break;
        }
    }
    out
}

/// How a take lays `doc` out: what `render` and the recordings pass.
fn take_options(doc: &Document, width: usize, step: u32) -> LayoutOptions {
    LayoutOptions {
        width,
        step,
        focus: None,
        pointer: None,
        line_numbers: doc.numbers,
        tab_width: 4,
        typing: false,
        redact: true,
        stable: true,
        margin_cap: doc.margin.map(usize::from),
    }
}

/// Whether the label of annotation `id` is under a redact at `step`: the
/// redact's own label, or a label anchored on a masked span.
fn under_redact(doc: &Document, id: &str, step: u32) -> bool {
    let shown = if doc.moves.is_empty() {
        doc.clone()
    } else {
        doc.view_at(step).doc
    };
    let masked = crate::redact::masked(&shown, step);
    let Some(a) = masked.find_annotation(id) else {
        return false;
    };
    let masks: Vec<_> = masked
        .annotations
        .iter()
        .filter(|r| {
            matches!(r.shape, Shape::Redact { .. }) && crate::redact::active(&masked, r, step)
        })
        .cloned()
        .collect();
    let on = |line: u32, start: u32, end: u32| {
        crate::redact::overlaps_mask(&masks, line, start, end.max(start + 1))
    };
    match &a.shape {
        Shape::Redact { .. } => true,
        Shape::Highlight { span, .. } | Shape::Mark { span, .. } => {
            on(span.line, span.start, span.end)
        }
        Shape::Arrow { from, to, .. } => {
            on(from.line, from.col, from.col + 1) || on(to.line, to.col, to.col + 1)
        }
        _ => false,
    }
}

/// The checks on a loaded document. Returns the narrowest clean width.
fn check_loaded(
    doc: &Document,
    missing: &BTreeSet<String>,
    width: usize,
    out: &mut Vec<Finding>,
) -> Option<usize> {
    let redacted = crate::redact::has_redacts(doc);
    let max = last_step(doc);
    // Annotations already reported as broken: not reported again as hidden.
    let mut bad: BTreeSet<String> = BTreeSet::new();
    // ---- annotations: anchors, markers, files ----
    for a in &doc.annotations {
        if let Some(f) = &a.file {
            if missing.contains(f) {
                bad.insert(a.id.clone());
                continue;
            }
            if !doc.view_code.contains_key(f) {
                out.push(
                    Finding::new(
                        Level::Error,
                        "unknown_file",
                        format!(
                            "file «{f}» is not a view of this document (no code.link move names it)"
                        ),
                    )
                    .id(a.id.clone())
                    .step(a.step)
                    .file(Some(f.clone()))
                    .line(anchor_line(&a.shape)),
                );
                bad.insert(a.id.clone());
                continue;
            }
        }
        if let Err(e) = doc.validate_annotation(a) {
            bad.insert(a.id.clone());
            let code = if matches!(e, crate::Error::NotFound(_)) {
                "unknown_id"
            } else {
                "anchor"
            };
            out.push(
                Finding::new(Level::Error, code, in_file(e.to_string(), &a.file))
                    .id(a.id.clone())
                    .step(a.step)
                    .file(a.file.clone())
                    .line(anchor_line(&a.shape)),
            );
            continue;
        }
        if let Some(m) = &a.markers {
            check_markers(doc, a, m, redacted, out);
        }
    }
    // ---- redacts a move widened or dropped (the TUI's own words) ----
    let mut said: BTreeSet<String> = BTreeSet::new();
    let mut moved: BTreeSet<String> = BTreeSet::new();
    for step in doc.change_steps(max).into_iter().filter(|s| *s >= 1) {
        for (id, n) in doc.redact_notices_by_id(step, true) {
            if said.insert(n.clone()) {
                let a = doc.find_annotation(&id);
                moved.insert(id.clone());
                out.push(
                    Finding::new(Level::Warning, "redact", n)
                        .id(id)
                        .step(step)
                        .file(a.and_then(|a| a.file.clone())),
                );
            }
        }
    }
    // A redact saved WIDENED by a reload.
    for a in &doc.annotations {
        if let Shape::Redact {
            widened: true,
            span,
            ..
        } = &a.shape
            && !moved.contains(&a.id)
        {
            out.push(
                Finding::new(
                    Level::Warning,
                    "redact",
                    format!("redact {} is WIDENED: it could not follow its text through a reload, mark the value again", a.id),
                )
                .id(a.id.clone())
                .step(a.step)
                .file(a.file.clone())
                .line(Some(span.line)),
            );
        }
    }
    // ---- annotations a move hides ----
    if !doc.moves.is_empty() {
        let mut hidden: BTreeMap<String, Vec<u32>> = BTreeMap::new();
        for (step, run) in doc.change_runs(max) {
            for id in doc.view_at(step).unanchored {
                hidden.entry(id).or_default().extend(run.clone());
            }
        }
        for a in &doc.annotations {
            let Some(steps) = hidden.get(&a.id).filter(|_| !bad.contains(&a.id)) else {
                continue;
            };
            out.push(
                Finding::new(
                    Level::Warning,
                    "unanchored",
                    format!(
                        "hidden as (unanchored) at step{} {}: a move removed what it was on (on purpose? if not, give the move a `rebind`)",
                        if steps.len() == 1 { "" } else { "s" },
                        steps_text(steps)
                    ),
                )
                .id(a.id.clone())
                .step(steps[0])
                .file(a.file.clone())
                .line(anchor_line(&a.shape)),
            );
        }
    }
    // ---- steps with nothing new ----
    for step in 1..=max {
        let has = doc.annotations.iter().any(|a| a.step == step)
            || doc.moves.iter().any(|m| m.at_step == step);
        if !has {
            out.push(
                Finding::new(
                    Level::Warning,
                    "empty_step",
                    "nothing new on this step: the presenter presses a key and nothing changes",
                )
                .step(step),
            );
        }
    }
    // ---- labels clipped with … at the width of the take ----
    let theme = Theme::dark();
    let geometry = stable_geometry(doc, &take_options(doc, width, 0));
    for ((id, text, dropped), steps) in clipped_at(doc, &theme, &geometry, width, false) {
        let a = doc.find_annotation(&id);
        // A mark's label is drawn behind its glyph: the text is what the
        // author wrote (`legend: label`).
        let text = match a.map(|a| &a.shape) {
            Some(Shape::Mark { kind, style, .. }) => doc
                .resolve_style(kind, style)
                .and_then(|st| st.glyph)
                .and_then(|g| text.strip_prefix(&format!("{g} ")).map(str::to_string))
                .unwrap_or(text),
            _ => text,
        };
        let secret = under_redact(doc, &id, steps[0]);
        let how = if dropped {
            "has no room and is not drawn"
        } else {
            "is clipped with «…»"
        };
        // The step the finding names is the first one it is clipped at.
        let when = match steps.as_slice() {
            [one] => format!("at width {width}, only at step {one}"),
            [first, rest @ ..] => format!(
                "at width {width}, first at step {first}, then at step{} {}",
                if rest.len() == 1 { "" } else { "s" },
                steps_text(rest)
            ),
            [] => format!("at width {width}"),
        };
        let message = if secret {
            format!("a label under a redact {how} {when}")
        } else {
            format!("label «{text}» {how} {when}")
        };
        let mut f = Finding::new(Level::Warning, "clipped_label", message)
            .id(id)
            .step(steps[0])
            .file(a.and_then(|a| a.file.clone()))
            .line(a.and_then(|a| anchor_line(&a.shape)));
        if !secret {
            f.text = Some(text);
        }
        out.push(f);
    }
    min_clean_width(doc, &theme)
}

/// The narrowest width from which no label is clipped at any step: every
/// width from it up to [`MAX_WIDTH`] shows them whole (a narrow terminal
/// can fit them by moving them under the lines, but a take recorded a bit
/// wider than that would clip them again). `None`: clipped even at
/// [`MAX_WIDTH`].
pub fn min_clean_width(doc: &Document, theme: &Theme) -> Option<usize> {
    let geometry = stable_geometry(doc, &take_options(doc, MAX_WIDTH, 0));
    let clips = |w: usize| !clipped_at(doc, theme, &geometry, w, true).is_empty();
    // Down from the widest in strides, then cell by cell above the first
    // stride that clips: in the margin a wider take only gives a label more
    // room, so this finds the same width as trying every one, in a tenth
    // of the layouts.
    const STRIDE: usize = 8;
    let mut w = MAX_WIDTH;
    let coarse = loop {
        if clips(w) {
            break Some(w);
        }
        if w == MIN_WIDTH {
            break None;
        }
        w = w.saturating_sub(STRIDE).max(MIN_WIDTH);
    };
    let clipped = coarse.map(|c| {
        (c + 1..(c + STRIDE).min(MAX_WIDTH + 1))
            .rev()
            .find(|&w| clips(w))
            .unwrap_or(c)
    });
    match clipped {
        None => Some(MIN_WIDTH),
        Some(MAX_WIDTH) => None,
        Some(w) => Some(w + 1),
    }
}
