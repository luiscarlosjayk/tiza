//! Preflight: `tiza check FILE` loads a document the way the TUI
//! does, but instead of stopping at the first problem it lists them all —
//! every anchor and marker, every move at its step, every linked file,
//! every annotation a move hides (unanchored, outside a window, or of a
//! file its own step does not show),
//! every label clipped with `…` at the width of the take and every step
//! with nothing new — naming the annotation or move, the step, the file
//! (for linked views) and the line.
//!
//! Errors are what would stop the document from loading or a move from
//! applying; warnings are what would spoil a take. Nothing is written.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Serialize;

use crate::canvas::Canvas;
use crate::layout::{
    LayoutOptions, MAX_WIDTH, MIN_WIDTH, StableGeometry, layout_measures, layout_with,
    stable_geometry,
};
use crate::model::{CodeSource, Document, SCHEMA_VERSION, Shape, sanitize_texts};
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
    /// `steps`, `unanchored`, `outside_window` (outside the `code.link`
    /// window of a step that shows its file), `not_shown` (its own step
    /// shows another file), `clipped_label`, `empty_step`, `effect` (the
    /// enter effect `draw` on a shape with no stroke), `until` (a
    /// spotlight's `until` before its `step`), and `stale` (a live
    /// instance's move the code changed under).
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
    /// `clipped_label` only: the full label text.
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
    /// The gutter when the document does not set `numbers` (the config
    /// file's default); `None`: on.
    pub numbers: Option<bool>,
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
                    // A width, as every other one here: what `--width`
                    // takes, not the width of the code alone.
                    if let Some(c) = n.code_fits_from.filter(|&c| width < c) {
                        out.push_str(&format!(
                            "no label is clipped at width {width}, but code lines are: they fit from width {c} (`tiza render --width {c}`)\n"
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
    Ok(check_text(&text, path.parent(), opts))
}

/// Checks the text of a `.tiza`; `base_dir` resolves its relative files.
pub fn check_text(text: &str, base_dir: Option<&Path>, opts: &CheckOptions) -> Report {
    check_text_as(text, base_dir, opts, None)
}

/// [`check_text`]; `main`: where the live document's own code comes from,
/// when the text embeds it only to check the code held in memory. The
/// document then names its main file as a fresh load of the saved `.tiza`
/// does (`src/stack.rs`, as `code.file` says, not `stack.rs`), so a
/// finding reads the same live and on disk.
fn check_text_as(
    text: &str,
    base_dir: Option<&Path>,
    opts: &CheckOptions,
    main: Option<CodeSource>,
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
                ".tiza document with schema {found}: this version of Tiza reads schema {SCHEMA_VERSION} at most"
            ),
        ));
        return Report::new(width, out, None, None);
    }
    let migrated = crate::fs::MIGRATIONS.iter().any(|(from, _)| found <= *from);
    for (from, migrate) in crate::fs::MIGRATIONS {
        if found <= *from {
            migrate(&mut raw);
        }
    }
    // An effect of a newer Tiza: read as no effect, with a warning below.
    let unknown = crate::fs::take_unknown_effects(&mut raw);
    // The moves of a newer Tiza: skipped, or with a default, with a
    // warning in `tidy`.
    let newer = crate::fs::take_newer_moves(&mut raw);
    let changed = !unknown.is_empty()
        || !newer.skipped.is_empty()
        || !newer.defaults.is_empty()
        || !newer.positions.is_empty();
    let parsed: Result<Document, serde_json::Error> = if migrated || changed {
        serde_json::from_value(raw)
    } else {
        // Straight from the text: the error keeps its line and column.
        serde_json::from_str(text)
    };
    let mut doc = match parsed {
        Ok(d) => d,
        Err(e) => {
            let line = (e.line() > 0).then_some(e.line() as u32);
            // A key that the load took out can be the one that a position
            // needs, under another name (`col` in a span).
            let note = crate::fs::taken_keys_note(&newer.positions)
                .map(|n| format!(". {n}"))
                .unwrap_or_default();
            out.push(
                Finding::new(
                    Level::Error,
                    "schema",
                    format!(
                        "does not follow the .tiza format: {}{note}",
                        crate::error::shorten_quoted_value(&e.to_string())
                    ),
                )
                .line(line),
            );
            return Report::new(width, out, None, None);
        }
    };
    doc.schema = SCHEMA_VERSION;
    doc.unknown_effects = crate::model::UnknownEffects(unknown);
    doc.newer_moves = newer;
    // The gutter the take draws when the document leaves it to the config.
    if doc.numbers.is_none() {
        doc.numbers = opts.numbers;
    }
    // ---- the code ----
    if let Err(e) = crate::fs::resolve_code(&mut doc, base_dir) {
        out.push(Finding::new(Level::Error, "code", e.to_string()));
        return Report::new(width, out, None, None);
    }
    if let Some(src @ CodeSource::File { .. }) = main {
        doc.code_src = src;
    }
    // ---- the numbers: a step past the cap or a first line near
    // `u32::MAX` is one error, before anything is laid out step by step.
    if let Err(e) = doc.check_bounds() {
        out.push(Finding::new(Level::Error, "bounds", e.to_string()));
        return Report::new(width, out, None, None);
    }
    let missing = load_links(&mut doc, base_dir, &mut out);
    let doc = tidy(doc, &missing, &mut out);
    let take = Take::of(&doc);
    let theme = Theme::dark();
    let clean = check_loaded(&doc, &take, &theme, &missing, width, &mut out);
    let max = last_step(&doc);
    let mut report = Report::new(width, out, Some(max), clean);
    if let Some(c) = clean
        && width < c
        && !report.findings.iter().any(|f| f.code == "clipped_label")
    {
        report.narrower = Some(narrower(&take, &theme, c));
    }
    report
}

/// Checks a document already loaded in memory — the live one of a TUI, for
/// the socket's `check`: the same checks as `tiza check` on the
/// file, on the code and linked files the TUI holds. Only the report
/// leaves. `base_dir` resolves relative files the document names but did
/// not load.
pub fn check_document(doc: &Document, base_dir: Option<&Path>, opts: &CheckOptions) -> Report {
    // What the file held that this version does not know is not in the
    // live document, so this check does not warn about it: the document
    // that it checks is the one that a save writes.
    let mut d = doc.clone();
    d.sync_code_src();
    let embedded = CodeSource::Embedded(d.base().to_vec());
    let main = std::mem::replace(&mut d.code_src, embedded);
    for file in d.link_files() {
        if let Some(code) = d.view_code.get(&file) {
            d.views.insert(file, code.clone());
        }
    }
    match serde_json::to_string(&d) {
        Ok(text) => {
            let mut report = check_text_as(&text, base_dir, opts, Some(main));
            stale_findings(doc, &mut report);
            report
        }
        Err(e) => Report::new(
            opts.width.unwrap_or(DEFAULT_WIDTH),
            vec![Finding::new(Level::Error, "schema", e.to_string())],
            None,
            None,
        ),
    }
}

/// The live instance's stale moves (the code changed under them, a follow
/// reload or a paste), each one error `stale`, in place of what the load
/// check said about the same move: a fresh load does not see one on line
/// numbers whose text changed (the file keeps no copy of that text), and
/// refuses the document over one on text.
fn stale_findings(doc: &Document, report: &mut Report) {
    for (i, m) in doc.moves.iter().enumerate() {
        let Some(s) = &m.stale else { continue };
        let id = format!("move #{}", i + 1);
        report
            .findings
            .retain(|f| !(f.level == Level::Error && f.id.as_deref() == Some(&id)));
        let next = match crate::stale::first_line(m) {
            Some(l) => format!("a fresh load would apply it on line {l} as written"),
            None => "a fresh load would refuse the document".into(),
        };
        report.findings.insert(
            0,
            Finding::new(
                Level::Error,
                "stale",
                format!(
                    "the code changed under it (a reload or a paste) and it no longer matches: {}; it is skipped. Fix it before closing Tiza: {next}",
                    s.reason
                ),
            )
            .id(id)
            .step(m.at_step)
            .file(s.file.clone()),
        );
    }
    report.errors = report
        .findings
        .iter()
        .filter(|f| f.level == Level::Error)
        .count();
    report.ok = report.errors == 0;
}

/// Which widths below `min_clean` are clean too (see [`Narrower`]).
fn narrower(take: &Take, theme: &Theme, min_clean: usize) -> Narrower {
    // What each width shows at every step: (labels whole, code whole,
    // labels in the margin at step 0).
    let widths: Vec<usize> = (MIN_WIDTH..min_clean).collect();
    let mut seen = vec![(true, true, false); widths.len()];
    for (i, step) in take.steps.iter().enumerate() {
        let found = take.sweep(i, theme, &widths, None, |l| {
            (
                l.clipped_labels.is_empty(),
                l.code_needs.is_none(),
                l.labels_in_margin,
            )
        });
        for (k, (labels_ok, code_ok, margin)) in found {
            let s = &mut seen[k];
            s.0 &= labels_ok;
            s.1 &= code_ok;
            if step.step == 0 {
                s.2 = margin;
            }
        }
    }
    let mut clean: Vec<[usize; 2]> = Vec::new();
    let mut code_fits_from = None;
    let mut margin_from = None;
    for (&w, &(labels_ok, code_ok, margin)) in widths.iter().zip(&seen) {
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
    // Aliases of one file are one view.
    crate::fs::canonicalize_links(doc, base_dir);
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
                .map(|(i, s, _)| format!("#{} (step {s})", doc.move_number(*i)))
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
                .id(format!("move #{}", doc.move_number(*first)))
                .step(*step)
                .file(Some(key.clone())),
            );
            missing.insert(key);
        }
    }
    missing
}

/// The moves of a newer Tiza ([`crate::model::NewerMoves`]), in the order
/// of the file: a warning for each move that Tiza skips, each value that
/// falls back to its default, and each move with keys that this version
/// does not know.
fn newer_moves(doc: &Document, out: &mut Vec<Finding>) {
    let found = &doc.newer_moves;
    let mut warnings: Vec<(usize, Option<u64>, String)> = Vec::new();
    for m in &found.skipped {
        warnings.push((
            m.number,
            m.at_step,
            format!(
                "the {} {} is not known to this version of Tiza: use {}. Tiza skips the move, and a save does not keep it.",
                m.field,
                m.value,
                or_list(&m.known)
            ),
        ));
    }
    for d in &found.defaults {
        warnings.push((
            d.number,
            d.at_step,
            format!(
                "the {} {} is not known to this version of Tiza: use {}. The move uses {}, and a save does not keep the value.",
                d.field,
                d.value,
                or_list(d.known),
                d.default
            ),
        ));
    }
    for k in &found.keys {
        let known: Vec<&str> = moves::MOVE_KEYS
            .iter()
            .chain(moves::action_keys(&k.action).unwrap_or_default())
            .copied()
            .collect();
        let near: Vec<String> = k
            .keys
            .iter()
            .filter_map(|key| moves::near_key(key, &known).map(|n| (key, n)))
            .map(|(key, n)| {
                if k.keys.len() == 1 {
                    n.to_string()
                } else {
                    format!("{n} for {key}")
                }
            })
            .collect();
        let hint = if near.is_empty() {
            String::new()
        } else {
            format!(" Did you mean {}?", and_list(&near))
        };
        let one = k.keys.len() == 1;
        warnings.push((
            k.number,
            k.at_step,
            format!(
                "the {} {} {} not known to this version of Tiza. Tiza ignores {}, and a save does not keep {}.{hint}",
                if one { "key" } else { "keys" },
                and_list(&k.keys),
                if one { "is" } else { "are" },
                if one { "it" } else { "them" },
                if one { "it" } else { "them" },
            ),
        ));
    }
    // The keys inside a position: of a move here, of an annotation below.
    let mut annotations = Vec::new();
    for p in &found.positions {
        let message = position_keys_message(p);
        match &p.owner {
            crate::model::KeysOwner::Move { number, at_step } => {
                warnings.push((*number, *at_step, message));
            }
            crate::model::KeysOwner::Annotation { id, step } => {
                annotations.push((id.clone(), *step, message));
            }
        }
    }
    warnings.sort_by_key(|(number, _, _)| *number);
    for (number, at_step, message) in warnings {
        let f = Finding::new(Level::Warning, "move", message).id(format!("move #{number}"));
        out.push(match at_step.and_then(|s| u32::try_from(s).ok()) {
            Some(s) => f.step(s),
            None => f,
        });
    }
    for (id, step, message) in annotations {
        let f = Finding::new(Level::Warning, "anchor", message).id(id);
        out.push(match step.and_then(|s| u32::try_from(s).ok()) {
            Some(s) => f.step(s),
            None => f,
        });
    }
}

/// The warning of the keys inside one position that this version does not
/// know, with what to write instead: `in from, the key colum is not known
/// to this version of Tiza. … Did you mean `col`?`.
fn position_keys_message(p: &crate::model::PositionKeys) -> String {
    let one = p.keys.len() == 1;
    let hints: Vec<String> = p
        .keys
        .iter()
        .filter_map(|k| {
            let hint = moves::position_key_hint(k, p.known)?;
            Some(if one { hint } else { format!("{k}: {hint}") })
        })
        .collect();
    let hint = if hints.is_empty() {
        String::new()
    } else {
        format!(" {}", hints.join(" "))
    };
    format!(
        "in {}, the {} {} {} not known to this version of Tiza. Tiza ignores {}, and a save does not keep {}.{hint}",
        p.field,
        if one { "key" } else { "keys" },
        and_list(&p.keys),
        if one { "is" } else { "are" },
        if one { "it" } else { "them" },
        if one { "it" } else { "them" },
    )
}

/// `a, b or c`.
fn or_list(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}

/// `a, b and c`.
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A move that does nothing, put where a broken one was so the moves after
/// it keep their numbers and are still checked.
fn placeholder(at_step: u32) -> Move {
    Move {
        at_step: at_step.max(1),
        action: MoveAction::Wait(Wait { ms: 1 }),
        rebind: BTreeMap::new(),
        stale: None,
        held: Vec::new(),
    }
}

/// A move's error without the `move #N, step S` the finding already
/// names. `number` is the number of the move in the messages
/// ([`Document::move_number`]).
fn move_message(e: crate::Error, number: usize, at_step: u32) -> String {
    let tag = format!("move #{number}, step {at_step}");
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
    // ---- moves of a newer Tiza ----
    newer_moves(&doc, out);
    // ---- moves: the static rules ----
    let mut moves = doc.moves.clone();
    let mut broken = vec![false; moves.len()];
    for (i, m) in moves.iter_mut().enumerate() {
        let name = format!("move #{}", doc.move_number(i));
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
        if let MoveAction::Scroll(s) = &m.action
            && let Some(why) = s.shape_problem()
        {
            out.push(
                Finding::new(Level::Error, "move", why)
                    .id(name.clone())
                    .step(m.at_step),
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
                let r = moves::IgnoredRebind {
                    index: i,
                    number: doc.move_number(i),
                    at_step: m.at_step,
                    id: id.clone(),
                };
                out.push(
                    Finding::new(Level::Warning, "unknown_id", r.message())
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
        match moves::project(&d, &with, u32::MAX) {
            Ok(_) => accepted = with,
            Err(e) => {
                let file = d.shown_view_at(trial[i].at_step).file;
                let n = doc.move_number(i);
                let msg = in_file(move_message(e, n, trial[i].at_step), &file);
                out.push(
                    Finding::new(Level::Error, "move", msg.clone())
                        .id(format!("move #{n}"))
                        .step(trial[i].at_step)
                        .file(file)
                        .line(first_line_in(&msg)),
                );
                broken[i] = true;
                trial[i] = placeholder(trial[i].at_step);
            }
        }
    }
    // ---- scroll targets: a warning, the move does not place the view ----
    {
        let mut d = doc.clone();
        d.moves = trial.clone();
        for (i, m) in trial.iter().enumerate() {
            if broken[i] || !matches!(m.action, MoveAction::Scroll(_)) {
                continue;
            }
            let why = match d.scroll_problem(i) {
                Some(why) => format!("scroll: {why}, so it does not place the view"),
                None => match d.scroll_note(i) {
                    Some(note) => format!("scroll: {note}"),
                    None => continue,
                },
            };
            let file = d.shown_view_at(m.at_step).file;
            out.push(
                Finding::new(Level::Warning, "move", in_file(why, &file))
                    .id(format!("move #{}", doc.move_number(i)))
                    .step(m.at_step)
                    .file(file),
            );
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
            let file = d.shown_view_at(m.at_step).file;
            let n = doc.move_number(i);
            let msg = in_file(move_message(e, n, m.at_step), &file);
            out.push(
                Finding::new(Level::Error, "rebind", msg)
                    .id(format!("move #{n}"))
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

/// The code a marker of `a` is looked for in: its file at its step.
fn marker_code(doc: &Document, a: &crate::model::Annotation) -> Document {
    let mut d = doc.clone();
    if let Ok((code, first)) = doc.view_code_at(&a.file, a.step) {
        d.moves.clear();
        d.base_code.clear();
        d.code = code;
        d.source.first_line = first;
    }
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

/// A range made from two markers: an error when it cannot be found
/// again, a warning when a marker matches more than once (the range takes
/// the first match, so an edit above it can move it).
fn check_markers(
    doc: &Document,
    a: &crate::model::Annotation,
    m: &crate::model::RangeMarkers,
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
            let msg = match e {
                crate::Error::NoMatch(t) | crate::Error::TextNotFound(t) => {
                    format!("{t} matches nothing")
                }
                other => other.to_string(),
            };
            out.push(finding(
                Level::Error,
                format!("{msg}: the range cannot be found again when the code changes"),
            ));
        }
        Ok((from, to)) => {
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
        Shape::Highlight { span, .. } | Shape::Mark { span, .. } => Some(span.line),
        Shape::Arrow { from, .. } => Some(from.line),
        Shape::Note { line, .. } => Some(*line),
        Shape::Box { from_line, .. } | Shape::Callout { from_line, .. } => Some(*from_line),
        Shape::Spotlight { from_line, .. } => *from_line,
    }
}

/// `4`, `4-7`, `2, 5-6`.
/// `2, 4-6`: steps in runs.
pub fn steps_text(steps: &[u32]) -> String {
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
fn clipped_at(take: &Take, theme: &Theme, width: usize) -> Vec<((String, String, bool), Vec<u32>)> {
    let mut out: Vec<((String, String, bool), Vec<u32>)> = Vec::new();
    let mut spare = None;
    // Laid out once per step where something changes; a label is
    // clipped at every step of that step's run.
    for (i, step) in take.steps.iter().enumerate() {
        if !step.labels {
            continue;
        }
        let l = take.measure(i, theme, width, spare.take());
        for c in l.clipped_labels {
            let key = (c.id, c.text, c.dropped);
            // Cut at some steps and left out at others: two findings, each
            // with its own steps, so each step is reported as it draws.
            match out.iter_mut().find(|(k, _)| *k == key) {
                Some((_, steps)) => steps.extend(step.run.clone()),
                None => out.push((key, step.run.clone().collect())),
            }
        }
        spare = Some(l.canvas);
    }
    out
}

/// A take of a document as `check` lays it out: one entry per step where
/// something changes, each with the document as that step shows it (the
/// moves applied, the shown file cropped to its window). Replaying the
/// moves is the same for every width, so it is done once per step here
/// instead of once per layout; laying out that document gives the same
/// labels as laying out the whole one at that step.
struct Take<'a> {
    doc: &'a Document,
    geometry: StableGeometry,
    steps: Vec<TakeStep<'a>>,
    /// The sweeps use [`layout_measures`] (see [`sweeps_measure`]).
    measures: bool,
}

struct TakeStep<'a> {
    step: u32,
    /// The steps that draw like this one (up to the next change).
    run: std::ops::RangeInclusive<u32>,
    shown: std::borrow::Cow<'a, Document>,
    /// Some annotation is drawn at this step: without one, no width can
    /// clip a label here.
    labels: bool,
}

impl<'a> Take<'a> {
    fn of(doc: &'a Document) -> Self {
        let steps = doc
            .change_runs(last_step(doc))
            .into_iter()
            .map(|(step, run)| {
                let shown = if doc.moves.is_empty() {
                    std::borrow::Cow::Borrowed(doc)
                } else {
                    std::borrow::Cow::Owned(doc.view_at(step).doc)
                };
                let labels = shown.annotations.iter().any(|a| a.step <= step);
                TakeStep {
                    step,
                    run,
                    shown,
                    labels,
                }
            })
            .collect();
        Take {
            doc,
            // From the whole document (every step, every file it shows),
            // as a take sizes it.
            geometry: stable_geometry(doc, &take_options(doc, MAX_WIDTH, 0)),
            steps,
            measures: sweeps_measure(),
        }
    }

    /// The widest width step `i` must be searched from: `bound` (see
    /// [`saturation_width`]) when its take is the one at [`MAX_WIDTH`], else
    /// [`MAX_WIDTH`]; whether that take clips a label; and which of those it
    /// was.
    fn top(&self, i: usize, theme: &Theme, bound: usize) -> (usize, bool, Bound) {
        if bound >= MAX_WIDTH {
            return (MAX_WIDTH, false, Bound::Widest);
        }
        let widest = self.layout(i, theme, MAX_WIDTH);
        let at = self.layout(i, theme, bound);
        let same = same_take(&at, &widest, theme);
        debug_assert!(
            same,
            "step {}: the take at {bound} columns is not the one at {MAX_WIDTH}",
            self.steps[i].step
        );
        if same {
            (bound, !at.clipped_labels.is_empty(), Bound::Labels)
        } else {
            (
                MAX_WIDTH,
                !widest.clipped_labels.is_empty(),
                Bound::Fallback,
            )
        }
    }

    /// The layout of step `i` (an index in `steps`) at `width`.
    fn layout(&self, i: usize, theme: &Theme, width: usize) -> crate::layout::Layout {
        let s = &self.steps[i];
        layout_with(
            &s.shown,
            theme,
            &take_options(self.doc, width, s.step),
            Some(&self.geometry),
        )
    }

    /// [`Take::layout`] for a sweep, which reads only what is clipped,
    /// whether the code fits and where the labels go: without the text of
    /// the code, into `spare` when it is given ([`layout_measures`]).
    fn measure(
        &self,
        i: usize,
        theme: &Theme,
        width: usize,
        spare: Option<Canvas>,
    ) -> crate::layout::Layout {
        if !self.measures {
            return self.layout(i, theme, width);
        }
        let s = &self.steps[i];
        layout_measures(
            &s.shown,
            theme,
            &take_options(self.doc, width, s.step),
            Some(&self.geometry),
            spare,
        )
    }

    /// Lays step `i` out at each of `widths` (on every core: each layout
    /// stands alone) and returns `seen` of each, by index in `widths`, in
    /// no particular order. With `widest`, `widths` must be in descending
    /// order and the sweep is a search for the widest width whose layout
    /// clips a label: a width at or below the widest one found so far
    /// (`widest`, shared with the caller, 0 for none) is skipped, since it
    /// cannot be the widest; every width above the final value is laid out.
    fn sweep<T: Send>(
        &self,
        i: usize,
        theme: &Theme,
        widths: &[usize],
        widest: Option<&AtomicUsize>,
        seen: impl Fn(&crate::layout::Layout) -> T + Sync,
    ) -> Vec<(usize, T)> {
        if widths.is_empty() {
            return Vec::new();
        }
        let first = self.measure(i, theme, widths[0], None);
        if let Some(w) = widest
            && !first.clipped_labels.is_empty()
        {
            w.fetch_max(widths[0], Ordering::Relaxed);
        }
        let mut out = vec![(0, seen(&first))];
        let next = AtomicUsize::new(1);
        let work = || {
            let mut mine = Vec::new();
            // One canvas per thread, laid out again at each width.
            let mut spare = None;
            loop {
                let k = next.fetch_add(1, Ordering::Relaxed);
                let Some(&width) = widths.get(k) else { break };
                if let Some(w) = widest
                    && width <= w.load(Ordering::Relaxed)
                {
                    break;
                }
                let l = self.measure(i, theme, width, spare.take());
                if let Some(w) = widest
                    && !l.clipped_labels.is_empty()
                {
                    w.fetch_max(width, Ordering::Relaxed);
                }
                mine.push((k, seen(&l)));
                spare = Some(l.canvas);
            }
            mine
        };
        let threads = sweep_threads(self.doc, widths.len());
        std::thread::scope(|s| {
            let handles: Vec<_> = (1..threads)
                .filter_map(|n| {
                    // A thread that cannot be spawned only means less
                    // parallelism: the others take its widths.
                    std::thread::Builder::new()
                        .name(format!("tiza-check-{n}"))
                        .spawn_scoped(s, work)
                        .ok()
                })
                .collect();
            out.extend(work());
            for h in handles {
                match h.join() {
                    Ok(mine) => out.extend(mine),
                    Err(panic) => std::panic::resume_unwind(panic),
                }
            }
        });
        out
    }
}

#[cfg(test)]
thread_local! {
    /// The tests that compare the sweeps with full layouts turn this on.
    static FULL_LAYOUTS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the sweeps of a take use [`layout_measures`]: always, except in
/// the tests that check they give what full layouts give.
fn sweeps_measure() -> bool {
    #[cfg(test)]
    if FULL_LAYOUTS.with(|f| f.get()) {
        return false;
    }
    true
}

/// Threads for a sweep: one per core (at most 8), fewer when there is
/// little to lay out (a short document lays out in microseconds, less than
/// starting a thread costs).
fn sweep_threads(doc: &Document, widths: usize) -> usize {
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    // About a microsecond per row and width: a thread pays for itself from
    // a few thousand.
    let rows: usize = doc.code.len() + doc.annotations.len();
    let work = rows.saturating_mul(widths);
    (work / 4_000).clamp(1, cpus.clamp(1, 8))
}

/// How a take lays `doc` out: what `render` and the recordings pass.
fn take_options(doc: &Document, width: usize, step: u32) -> LayoutOptions {
    LayoutOptions {
        width,
        step,
        focus: None,
        pointer: None,
        line_numbers: doc.numbers_or(true),
        tab_width: 4,
        typing: false,
        stable: true,
        margin_cap: doc.margin.map(usize::from),
    }
}

/// Whether `step`, one of the steps of `take`, draws at `width` as the
/// step before it. A spotlight that starts or ends at a step makes it one
/// of them ([`Document::change_steps`]); its end changes nothing when
/// another spotlight keeps the same lines bright, or when its file is not
/// the one shown.
fn same_as_before(take: &Take, theme: &Theme, width: usize, step: u32) -> bool {
    match take.steps.iter().position(|s| s.step == step) {
        Some(i) if i > 0 => same_take(
            &take.layout(i - 1, theme, width),
            &take.layout(i, theme, width),
            theme,
        ),
        _ => false,
    }
}

/// The warning of an annotation from a file with `draw` and no line.
const EFFECT_WITHOUT_STROKE: &str = "effect draw needs a stroke, so this annotation comes in with no effect. Use fade, or put draw on a box, a callout, an arrow, or a highlight or mark with emphasis underline, brackets or strike.";

fn check_loaded(
    doc: &Document,
    take: &Take,
    theme: &Theme,
    missing: &BTreeSet<String>,
    width: usize,
    out: &mut Vec<Finding>,
) -> Option<usize> {
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
            let code = if matches!(e, crate::Error::NotFound(_) | crate::Error::ViewNotFound(_)) {
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
        if a.effect == Some(crate::effects::Effect::Draw) && !a.shape.has_stroke() {
            out.push(
                Finding::new(Level::Warning, "effect", EFFECT_WITHOUT_STROKE)
                    .id(a.id.clone())
                    .step(a.step)
                    .file(a.file.clone())
                    .line(anchor_line(&a.shape)),
            );
        }
        // A spotlight active from its step to an `until` before it: at no
        // step.
        if let Shape::Spotlight {
            until: Some(until), ..
        } = a.shape
            && until < a.step
        {
            bad.insert(a.id.clone());
            out.push(
                Finding::new(
                    Level::Error,
                    "until",
                    format!(
                        "until {until} is before its step {step}: the spotlight never shows. Give an until of {step} or later, or remove it.",
                        step = a.step
                    ),
                )
                .id(a.id.clone())
                .step(a.step)
                .file(a.file.clone())
                .line(anchor_line(&a.shape)),
            );
        }
        // An effect of a newer Tiza: the file opens, with no effect there.
        for (_, value) in doc.unknown_effects.0.iter().filter(|(id, _)| *id == a.id) {
            // What to write instead, for this annotation.
            let fix = if value == "\"none\"" {
                "leave out effect for no effect"
            } else if a.shape.has_stroke() {
                "use draw or fade"
            } else {
                "use fade"
            };
            out.push(
                Finding::new(
                    Level::Warning,
                    "effect",
                    format!(
                        "the effect {value} is not known to this version of Tiza: {fix}. The annotation comes in with no effect, and a save does not keep the value."
                    ),
                )
                .id(a.id.clone())
                .step(a.step)
                .file(a.file.clone())
                .line(anchor_line(&a.shape)),
            );
        }
        if let Some(m) = &a.markers {
            check_markers(doc, a, m, out);
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
    // ---- annotations outside the window of a step that shows their file ----
    for (id, steps, window) in outside_windows(doc) {
        let Some(a) = doc.find_annotation(&id).filter(|a| !bad.contains(&a.id)) else {
            continue;
        };
        out.push(
            Finding::new(
                Level::Warning,
                "outside_window",
                format!(
                    "outside the window at step{} {} ({window}): kept, but not drawn there",
                    if steps.len() == 1 { "" } else { "s" },
                    steps_text(&steps)
                ),
            )
            .id(id)
            .step(steps[0])
            .file(a.file.clone())
            .line(anchor_line(&a.shape)),
        );
    }
    // ---- annotations of a file their own step does not show ----
    for u in unshown(doc) {
        let Some(a) = doc.find_annotation(&u.id).filter(|a| !bad.contains(&a.id)) else {
            continue;
        };
        out.push(
            Finding::new(
                Level::Warning,
                "not_shown",
                format!("{}: kept, but not drawn at its step", u.text(false)),
            )
            .id(u.id)
            .step(u.step)
            .file(a.file.clone())
            .line(anchor_line(&a.shape)),
        );
    }
    // ---- steps with nothing new ----
    for step in 1..=max {
        // A step of a skipped move: its own warning says why it is empty.
        // The start or the end of a spotlight is new when it changes the
        // take.
        let has = doc.annotations.iter().any(|a| a.step == step)
            || doc.moves.iter().any(|m| m.at_step == step)
            || doc
                .newer_moves
                .skipped
                .iter()
                .any(|m| m.at_step == Some(u64::from(step)))
            || (doc.spots_change_at(step) && !same_as_before(take, theme, width, step));
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
    for ((id, text, dropped), steps) in clipped_at(take, theme, width) {
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
        let mut f = Finding::new(
            Level::Warning,
            "clipped_label",
            format!("label «{text}» {how} {when}"),
        )
        .id(id)
        .step(steps[0])
        .file(a.and_then(|a| a.file.clone()))
        .line(a.and_then(|a| anchor_line(&a.shape)));
        f.text = Some(text);
        out.push(f);
    }
    widest_clipped(take, theme).map_or(Some(MIN_WIDTH), clean_above)
}

/// The annotations a step would show but its `code.link` window crops
/// away, from their own step on with their lines carried there by the
/// moves. A window that comes from a link after the annotation's step is
/// the author zooming in: what it never showed is left out, what was inside
/// it at the link's step and a later patch pushes out is said, as is what
/// is outside the window of the annotation's own step. Per id, in document order: the steps and the first such
/// window (`src/queue.rs:17-23`). Empty for a document with no window.
pub fn outside_windows(doc: &Document) -> Vec<(String, Vec<u32>, String)> {
    outside_windows_from(doc, 1, false)
}

/// [`outside_windows`] from step `from` on; `zoom`: also the windows of
/// links after an annotation's step.
pub fn outside_windows_from(
    doc: &Document,
    from: u32,
    zoom: bool,
) -> Vec<(String, Vec<u32>, String)> {
    use crate::moves::MoveAction;
    let links: Vec<(usize, &crate::moves::Move)> = doc
        .moves
        .iter()
        .enumerate()
        .filter(|(_, m)| m.stale.is_none() && matches!(m.action, MoveAction::CodeLink(_)))
        .collect();
    if !links
        .iter()
        .any(|(_, m)| matches!(&m.action, MoveAction::CodeLink(l) if l.lines.is_some()))
    {
        return Vec::new();
    }
    // One replay for every step: the patches up to a step are a prefix.
    let Ok(proj) = crate::moves::project(doc, &doc.moves, u32::MAX) else {
        return Vec::new();
    };
    let mut found: BTreeMap<String, (Vec<u32>, String)> = BTreeMap::new();
    for step in from.max(1)..=last_step(doc) {
        // The link shown at `step`: the last one by then, in replay order.
        let Some((_, link)) = links
            .iter()
            .filter(|(_, m)| m.at_step <= step)
            .max_by_key(|(i, m)| (m.at_step, *i))
        else {
            continue;
        };
        let MoveAction::CodeLink(l) = &link.action else {
            continue;
        };
        let Ok(Some((wa, wb))) = l.window() else {
            continue;
        };
        let file = doc.view_key(&l.file);
        let shapes = doc.shapes_at(&proj, step);
        // The shapes at the step of the link, for a window a later link
        // sets: the author's zoom keeps out what it never showed, and what
        // was inside it there and a patch pushes out is said.
        let at_link = (!zoom && link.at_step < step).then(|| doc.shapes_at(&proj, link.at_step));
        let inside = |shape: &crate::Shape| {
            shape
                .line_ranges()
                .iter()
                .all(|(f, t)| *f >= wa && *t <= wb)
        };
        for (i, (a, shape)) in doc.annotations.iter().zip(&shapes).enumerate() {
            let Some(shape) = shape else { continue };
            if a.file != file || a.step > step || inside(shape) {
                continue;
            }
            if !zoom && link.at_step > a.step {
                let was_in = match &at_link {
                    Some(at) => at[i].as_ref().is_some_and(inside),
                    None => false,
                };
                if !was_in {
                    continue;
                }
            }
            found
                .entry(a.id.clone())
                .or_insert_with(|| (Vec::new(), format!("{}:{wa}-{wb}", doc.file_label(&a.file))))
                .0
                .push(step);
        }
    }
    doc.annotations
        .iter()
        .filter_map(|a| found.remove(&a.id).map(|(s, w)| (a.id.clone(), s, w)))
        .collect()
}

/// An annotation whose own step shows another file (said even when its
/// file shows at other steps, since it is never drawn at its own): a link
/// of that step shows another one, or its file is linked only at other
/// steps. The files as [`Document::file_label`] names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unshown {
    pub id: String,
    pub step: u32,
    pub file: String,
    pub shown: String,
}

impl Unshown {
    /// `its file src/stack.rs is not shown at step 4 (src/queue.rs is)`,
    /// or `their file …` for several annotations; the same words as
    /// [`crate::model::Hidden::OtherFile`].
    pub fn text(&self, several: bool) -> String {
        format!(
            "{} file {} is not shown at step {} ({} is)",
            if several { "their" } else { "its" },
            self.file,
            self.step,
            self.shown
        )
    }
}

/// Every annotation whose file is not the one shown at its own step, in
/// document order. A window does not count (an annotation of the shown
/// file outside its window is [`outside_windows`]).
pub fn unshown(doc: &Document) -> Vec<Unshown> {
    if doc.moves.is_empty() {
        return Vec::new();
    }
    doc.annotations
        .iter()
        .filter_map(|a| {
            let shown = doc.shown_view_at(a.step).file;
            (a.file != shown).then(|| Unshown {
                id: a.id.clone(),
                step: a.step,
                file: doc.file_label(&a.file),
                shown: doc.file_label(&shown),
            })
        })
        .collect()
}

/// What a document leaves unseen, taken before a move is added or deleted
/// so that the reply names only what that move hides: the annotations
/// outside a `code.link` window from step `from` on (the windows of later
/// links included: the move may be that zoom) and those whose own step
/// shows another file.
#[derive(Debug, Clone, Default)]
pub struct Unseen {
    from: u32,
    last: u32,
    outside: Vec<(String, Vec<u32>, String)>,
    unshown: Vec<Unshown>,
    /// The file each step shows, from step 0 to `last`.
    shown: Vec<ViewKey>,
}

impl Unseen {
    pub fn of(doc: &Document, from: u32) -> Unseen {
        let last = last_step(doc);
        Unseen {
            from,
            last,
            // From the last step at most, which stands for the steps after it.
            outside: outside_windows_from(doc, from.min(last), true),
            unshown: unshown(doc),
            shown: (0..=last).map(|s| doc.shown_view_at(s).file).collect(),
        }
    }

    /// The file step `s` showed (a step past the last one shows what the
    /// last one does).
    fn shown_at(&self, s: u32) -> Option<&ViewKey> {
        self.shown.get(s.min(self.last) as usize)
    }

    /// The `warning` of a `move.add` or `move.delete` that turned the
    /// document `self` was taken of into `after`, in one line, or `None`
    /// when it hides nothing new: `a3, a4: outside the window at step 3
    /// (src/queue.rs:10-36): kept, not drawn there; a6: its file
    /// src/stack.rs is not shown at step 4 (src/queue.rs is): kept, not
    /// drawn there`. Only what was drawn at a step before the change and is
    /// not after it is named: not what was already outside the window
    /// there, nor what that step did not show because it showed another
    /// file. A step the move creates past the old last one counts as that
    /// last one (nothing else happens there).
    pub fn warning(&self, after: &Document) -> Option<String> {
        let now = Unseen::of(after, self.from);
        let mut parts: Vec<String> = Vec::new();
        let new: Vec<(String, Vec<u32>, String)> = now
            .outside
            .into_iter()
            .filter_map(|(id, steps, window)| {
                let was: &[u32] = self
                    .outside
                    .iter()
                    .find(|(b, _, _)| *b == id)
                    .map_or(&[], |(_, s, _)| s.as_slice());
                let file = after.find_annotation(&id).map(|a| a.file.clone());
                let was_shown = |s: u32| file.as_ref().is_some_and(|f| self.shown_at(s) == Some(f));
                let was_out = |s: u32| was.contains(&s.min(self.last));
                let steps: Vec<u32> = steps
                    .into_iter()
                    .filter(|s| *s >= self.from && was_shown(*s) && !was_out(*s))
                    .collect();
                (!steps.is_empty()).then_some((id, steps, window))
            })
            .collect();
        if let Some((_, steps, window)) = new.first() {
            let ids: Vec<&str> = new.iter().map(|(id, _, _)| id.as_str()).collect();
            parts.push(format!(
                "{}: outside the window at step{} {} ({window}): kept, not drawn there",
                ids.join(", "),
                if steps.len() == 1 { "" } else { "s" },
                steps_text(steps)
            ));
        }
        // Grouped by step and files, in document order.
        let mut groups: Vec<(Unshown, Vec<String>)> = Vec::new();
        for u in now.unshown {
            // Already unseen at its step: of another file, or outside the
            // window of its own.
            let was_outside = self
                .outside
                .iter()
                .any(|(b, steps, _)| *b == u.id && steps.contains(&u.step));
            if was_outside || self.unshown.iter().any(|b| b.id == u.id) {
                continue;
            }
            let same = |g: &Unshown| (g.step, &g.file, &g.shown) == (u.step, &u.file, &u.shown);
            match groups.iter_mut().find(|(g, _)| same(g)) {
                Some((_, ids)) => ids.push(u.id),
                None => {
                    let id = u.id.clone();
                    groups.push((u, vec![id]));
                }
            }
        }
        for (u, ids) in groups {
            parts.push(format!(
                "{}: {}: kept, not drawn there",
                ids.join(", "),
                u.text(ids.len() > 1)
            ));
        }
        (!parts.is_empty()).then(|| parts.join("; "))
    }
}

/// The narrowest width from which no label is clipped at any step: every
/// width from it up to [`MAX_WIDTH`] shows them whole (a narrow terminal
/// can fit them by moving them under the lines, but a take recorded a bit
/// wider than that would clip them again). `None`: clipped even at
/// [`MAX_WIDTH`].
pub fn min_clean_width(doc: &Document, theme: &Theme) -> Option<usize> {
    widest_clipped(&Take::of(doc), theme).map_or(Some(MIN_WIDTH), clean_above)
}

/// The width from which no step of `doc` changes any more, other than by
/// blank columns on the right (at most [`MAX_WIDTH`]). It follows the rules
/// of [`layout_with`] on a stable take, which sizes everything from
/// [`StableGeometry`]:
/// - the code column ends at `margin_x` = line numbers + two cells per
///   arrow lane + the box band on both sides of the code + its two gaps +
///   the widest code line (at least 10, `code_w` never goes below) + the
///   two cells before the margin; every arrow is counted as a lane, since a
///   step may need more lanes than the take's own count when it estimates
///   the margin;
/// - with a cap (`:margin N`) the frame and the margin stop growing one
///   column past `margin_x + N`;
/// - without one, the margin is past every decision once it holds a margin
///   callout (its block is at most `max(width or 34, 8, title + 4, its
///   widest character) + 4` wide, `auto` moves into the margin at `width +
///   19`, a block goes beside the blocks it shares rows with once it fits
///   there, and the labels beside the blocks get their rows once the
///   blocks, each with its gap, plus 14 fit) and, beside them, the longest
///   label on one row (its `┆ ` and the gap after it), and the rows under
///   the lines hold that label whole too; a label is measured as the canvas
///   draws it (a zero-width space takes no cell there). Blocks only share
///   rows with the blocks they can reach: a block of `h` rows from line `L`
///   covers at most lines `L` to `L + h - 1`, so the blocks side by side
///   are bounded by the widest chain of blocks whose lines meet, in the
///   order the layout places them;
/// - a margin callout with no room for its block is a block under its
///   line, as wide as its text: inside the code column when a box holds
///   its rows, else up to 8 cells of text, one cell short of the margin.
///
/// Each step is laid out at the bound and at [`MAX_WIDTH`]: when the two
/// takes are the same the bound is that step's; otherwise the step is
/// searched from [`MAX_WIDTH`] (a change in the layout's rules costs time,
/// never a wrong answer).
pub fn saturation_width(doc: &Document, theme: &Theme) -> usize {
    let take = Take::of(doc);
    let bound = take_saturation(&take);
    (0..take.steps.len())
        .filter(|&i| take.steps[i].labels)
        .map(|i| take.top(i, theme, bound).0)
        .max()
        .unwrap_or(bound)
}

/// Which bound a step of [`saturation_width`] is searched from.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    /// The bound from the labels and the margin blocks.
    Labels,
    /// The bound itself is [`MAX_WIDTH`]: there is nothing above it.
    Widest,
    /// The take at the bound is not the one at [`MAX_WIDTH`], so the
    /// step is searched from [`MAX_WIDTH`]: the bound no longer follows the
    /// layout's rules.
    Fallback,
}

/// For each step that draws an annotation, which bound it is searched
/// from (for the tests: [`Bound::Fallback`] means the bound is wrong).
#[doc(hidden)]
pub fn saturation_bounds(doc: &Document, theme: &Theme) -> Vec<(u32, Bound)> {
    let take = Take::of(doc);
    let bound = take_saturation(&take);
    (0..take.steps.len())
        .filter(|&i| take.steps[i].labels)
        .map(|i| (take.steps[i].step, take.top(i, theme, bound).2))
        .collect()
}

/// The bound of [`saturation_width`].
fn take_saturation(take: &Take) -> usize {
    use crate::canvas::{display_width, drawn_width};
    let doc = take.doc;
    let g = &take.geometry;
    // A label as it is drawn (the two measures agree now that a
    // zero-width character takes no cell; the larger one stays as a guard).
    let w = |t: &str| display_width(t, 4).max(drawn_width(t));
    let arrows = doc
        .annotations
        .iter()
        .filter(|a| matches!(a.shape, Shape::Arrow { .. }))
        .count();
    let margin_x =
        g.num_w + 2 * arrows.max(g.lane_count) + 2 * g.box_depth + 2 + g.code_full_w.max(10) + 2;
    // A margin callout's block: its inner width at most (`width`, 34 by
    // default, at least 8, its title with the `─ ` before and ` ─` after, and
    // a character wider than the text column, which is a line of its own and
    // grows the block) and its rows (its words wrapped at `width`, and the
    // two borders).
    let block = |text: &str, title: &Option<String>, width: Option<usize>| {
        let widest = unicode_segmentation::UnicodeSegmentation::graphemes(text, true)
            .map(&w)
            .max()
            .unwrap_or(0);
        let text_w = width.unwrap_or(34).max(8);
        let inner = text_w
            .max(title.as_deref().map_or(0, |t| w(t) + 4))
            .max(widest);
        let rows: usize = text
            .split('\n')
            .map(|p| {
                if p.trim().is_empty() {
                    1
                } else {
                    crate::layout::wrap_words(p, text_w).len()
                }
            })
            .sum();
        (inner, rows + 2)
    };
    // The widest label; the widest margin callout's text column.
    let (mut longest, mut callout) = (0, 0);
    for a in &doc.annotations {
        let text = match &a.shape {
            Shape::Note { text, .. } => w(text),
            Shape::Highlight { label, .. }
            | Shape::Box { label, .. }
            | Shape::Arrow { label, .. } => label.as_deref().map_or(0, w),
            Shape::Mark {
                kind, style, label, ..
            } => {
                // `glyph legend: label`, the legend defaulting to the kind.
                let st = doc.resolve_style(kind, style).unwrap_or_default();
                st.glyph.as_deref().map_or(0, |g| w(g) + 1)
                    + st.legend.as_deref().map_or(w(kind), w)
                    + label.as_deref().map_or(0, |l| 2 + w(l))
            }
            Shape::Callout {
                text, title, width, ..
            } => {
                callout = callout.max(block(text, title, *width).0);
                0
            }
            Shape::Spotlight { .. } => 0,
        };
        longest = longest.max(text);
    }
    // `blocks`: the widest run of margin blocks side by side, each with the
    // gap after it, at any step. The layout places the blocks from the first
    // line down (one line's in the document's order), each right after the
    // rightmost block it would overlap; so a block ends at most at its own
    // width past the end of the widest chain of earlier blocks whose lines
    // meet its own.
    let mut blocks = 0;
    for s in &take.steps {
        let mut margin: Vec<(u32, u32, usize)> = s
            .shown
            .annotations
            .iter()
            .filter(|a| a.step <= s.step)
            .filter_map(|a| match &a.shape {
                Shape::Callout {
                    from_line,
                    text,
                    title,
                    width,
                    placement,
                    ..
                } if *placement != crate::model::Placement::Below => {
                    let (inner, h) = block(text, title, *width);
                    let last = from_line.saturating_add(u32::try_from(h - 1).unwrap_or(u32::MAX));
                    Some((*from_line, last, inner + 4))
                }
                _ => None,
            })
            .collect();
        margin.sort_by_key(|&(first, _, _)| first);
        // Where each block ends, past the margin's start, at most.
        let mut ends: Vec<usize> = Vec::with_capacity(margin.len());
        for (k, &(first, last, width)) in margin.iter().enumerate() {
            let start = (0..k)
                .filter(|&j| margin[j].0 <= last && first <= margin[j].1)
                .map(|j| ends[j] + 1)
                .max()
                .unwrap_or(0);
            ends.push(start + width);
            blocks = blocks.max(start + width + 1);
        }
    }
    // A margin of `room` cells: past every margin decision (14 cells),
    // past the callout rules (`auto` moves in at `width + 19`, the labels
    // beside the blocks get their rows from `blocks + 14`), and with the
    // longest label on one row after its `┆ ` and a gap, also beside the
    // blocks.
    let mut room = 14usize.max(3 + longest);
    if callout > 0 {
        room = room
            .max(callout + 19)
            .max(blocks + 14)
            .max(blocks + 3 + longest);
    }
    let open = margin_x + room + 2;
    let capped = doc
        .margin
        .map_or(open, |cap| open.min(margin_x + usize::from(cap) + 1));
    capped.clamp(MIN_WIDTH, MAX_WIDTH)
}

/// Two layouts of one step at two widths draw the same take: the same rows
/// and labels, the same cells, and only blank columns on the right of the
/// wider one.
fn same_take(a: &crate::layout::Layout, b: &crate::layout::Layout, theme: &Theme) -> bool {
    let (narrow, wide) = if a.canvas.width <= b.canvas.width {
        (a, b)
    } else {
        (b, a)
    };
    let blank = crate::canvas::Cell {
        style: theme.base(),
        ..Default::default()
    };
    narrow.rows == wide.rows
        && narrow.row_of_line == wide.row_of_line
        && narrow.code_x == wide.code_x
        && narrow.code_needs == wide.code_needs
        && narrow.labels_in_margin == wide.labels_in_margin
        && narrow.clipped_labels == wide.clipped_labels
        && narrow.canvas.height == wide.canvas.height
        && (0..wide.canvas.height).all(|y| {
            (0..wide.canvas.width).all(|x| match narrow.canvas.get(x, y) {
                Some(c) => wide.canvas.get(x, y) == Some(c),
                None => wide.canvas.get(x, y) == Some(&blank),
            })
        })
}

/// The width above the widest one that clips: `None` when that one is
/// [`MAX_WIDTH`] itself.
fn clean_above(clipped: usize) -> Option<usize> {
    (clipped < MAX_WIDTH).then_some(clipped + 1)
}

/// The widest width from [`MIN_WIDTH`] to [`MAX_WIDTH`] at which a label
/// is clipped at some step (`None`: no width clips one). Every width above
/// it, up to the one from which the take no longer changes
/// ([`saturation_width`]), is laid out at every step: a wider take does not
/// always clip less (where the margin first appears it is narrower than the
/// rows under the lines were, and a callout that moves into the margin can
/// find no room there), so no width is assumed from its neighbours.
fn widest_clipped(take: &Take, theme: &Theme) -> Option<usize> {
    let bound = take_saturation(take);
    let widest = AtomicUsize::new(0);
    // The last steps first: they usually draw the most labels, so the
    // widest clip is found early and the other steps stop above it.
    for i in (0..take.steps.len()).rev() {
        if !take.steps[i].labels {
            continue;
        }
        let (top, clips, _) = take.top(i, theme, bound);
        if clips && top < MAX_WIDTH {
            // The same take, so the same clip, from the bound up.
            widest.store(MAX_WIDTH, Ordering::Relaxed);
            break;
        }
        let above = widest.load(Ordering::Relaxed);
        let todo: Vec<usize> = (MIN_WIDTH..=top).rev().filter(|&w| w > above).collect();
        take.sweep(i, theme, &todo, Some(&widest), |_| ());
    }
    match widest.into_inner() {
        0 => None,
        w => Some(w),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    /// Documents that put labels everywhere the margin rules decide: wide
    /// characters, tabs and emoji in the code; every kind of annotation, with
    /// labels too long for a narrow margin; a patch; a link to another file;
    /// and a longer one with many steps.
    fn documents() -> Vec<(&'static str, Value)> {
        let wide: Vec<String> = (0..24)
            .map(|i| match i % 4 {
                0 => format!("\tlet 名前{i} = \"中文字符 {i}\"; // 🎉 emoji"),
                1 => format!("fn 関数{i}(x: &str) -> usize {{ x.len() + {i} }} // 😀"),
                2 => format!("\t\tif x == {i} {{ println!(\"東京 {i} 🚀\"); }}"),
                _ => format!("struct Café{i} {{ naïve: bool, ünïcode: String }}"),
            })
            .collect();
        let long = |i: usize| format!("a label {i} with 中文 and 🎉 too long for a narrow margin");
        let mut kinds = Vec::new();
        for i in 0..18usize {
            let line = 1 + (i * 5) % 20;
            let step = 1 + i % 6;
            let id = format!("k{i}");
            kinds.push(match i % 6 {
                0 => json!({"id": id, "step": step, "type": "note", "line": line, "text": long(i)}),
                1 => json!({"id": id, "step": step, "type": "box", "from_line": line, "to_line": line + 2, "label": long(i)}),
                2 => json!({"id": id, "step": step, "type": "mark", "kind": "borrow", "span": {"line": line, "start": 0, "end": 3}, "label": long(i)}),
                3 => json!({"id": id, "step": step, "type": "arrow", "from": {"line": line, "col": 1}, "to": {"line": line + 3, "col": 2}, "label": long(i)}),
                4 => json!({"id": id, "step": step, "type": "callout", "from_line": line, "to_line": line + 2, "title": "part", "text": long(i)}),
                _ => json!({"id": id, "step": step, "type": "highlight", "span": {"line": line, "start": 0, "end": 2}, "label": long(i)}),
            });
        }
        kinds.push(
            json!({"id": "s1", "step": 6, "type": "spotlight", "from_line": 3, "to_line": 9}),
        );
        // A crowd: more labels on lines 11-12 than their rows hold.
        for j in 0..8usize {
            kinds.push(json!({"id": format!("c{j}"), "step": 1, "type": "note", "line": 11 + j % 2, "text": long(j)}));
        }
        let other: Vec<String> = (0..12)
            .map(|i| format!("pub fn other{i}() -> u32 {{ {i} }}"))
            .collect();
        let long_code: Vec<String> = (0..120)
            .map(|i| match i % 5 {
                0 => format!("/// Item {i}: a doc comment that is longer than most lines here"),
                1 => format!("pub struct Item{i} {{ name: String, items: Vec<u32> }}"),
                2 => format!("    fn take(self) -> usize {{ self.items.len() * {i} }}"),
                3 => "}".to_string(),
                _ => String::new(),
            })
            .collect();
        let many: Vec<Value> = (0..24usize)
            .map(|i| {
                let line = 1 + (i * 7) % 115;
                json!({"id": format!("m{i}"), "step": 1 + i % 8, "type": "note", "line": line, "text": format!("note {i} {}", "word ".repeat(i % 9))})
            })
            .collect();
        vec![
            (
                "wide, every kind, a patch",
                json!({"schema": 1, "source": {"language": "rs", "first_line": 1}, "code": wide,
                       "annotations": kinds, "step": 0,
                       "moves": [{"at_step": 3, "action": "code.patch", "op": "replace", "find": wide[4], "content": format!("{} // 変更", wide[4])}]}),
            ),
            (
                "a link to another file",
                json!({"schema": 1, "source": {"language": "rs", "first_line": 1}, "code": wide,
                       "views": {"other.rs": other},
                       "annotations": [
                           {"id": "o1", "step": 2, "type": "note", "file": "other.rs", "line": 3, "text": long(1)},
                           {"id": "o2", "step": 2, "type": "box", "file": "other.rs", "from_line": 5, "to_line": 8, "label": long(2)},
                           {"id": "n1", "step": 3, "type": "note", "line": 2, "text": long(3)}],
                       "step": 0,
                       "moves": [{"at_step": 2, "action": "code.link", "file": "other.rs", "lines": "2-10"},
                                 {"at_step": 3, "action": "code.link", "file": "main.rs"}]}),
            ),
            (
                "120 lines, 8 steps",
                json!({"schema": 1, "source": {"language": "rs", "first_line": 1}, "code": long_code,
                       "annotations": many, "step": 0}),
            ),
            (
                // A margin label that goes on over the rows under its own
                // line, where what is painted decides if it has room
                // (`margin_blank`).
                "a label over the rows under its line",
                json!({"schema": 1, "source": {"language": "sh", "first_line": 1},
                       "code": ["let value = compute(a, b)", "B=2", "C=3", "D=4", "E=5"],
                       "annotations": [
                           {"id": "n1", "step": 1, "type": "note", "line": 1, "text": "short one"},
                           {"id": "n2", "step": 1, "type": "note", "line": 1, "text": "short two"},
                           {"id": "ar", "step": 1, "type": "arrow", "from": {"line": 1, "col": 4}, "to": {"line": 1, "col": 12},
                            "label": "this arrow label is long enough to wrap over a few rows of the margin"}]}),
            ),
        ]
    }

    fn load(doc: &Value) -> Document {
        let mut d: Document = serde_json::from_value(doc.clone()).unwrap();
        crate::fs::resolve_code(&mut d, None).unwrap();
        let mut found = Vec::new();
        let missing = load_links(&mut d, None, &mut found);
        tidy(d, &missing, &mut found)
    }

    /// Several widths in a row with one canvas, as a sweep lays them out
    /// (every third one: the label over the rows under its line is
    /// clipped differently from 46 columns, the others from far wider).
    fn widths() -> impl Iterator<Item = usize> {
        (MIN_WIDTH..=100).step_by(3).chain((105..=160).step_by(5))
    }

    #[test]
    fn a_sweep_reads_what_a_full_layout_gives() {
        let theme = Theme::dark();
        // What the documents make the layout decide: labels clipped, and
        // labels in the margin and under their lines.
        let (mut clipped, mut margin, mut under) = (0, 0, 0);
        for (name, doc) in documents() {
            let doc = load(&doc);
            let take = Take::of(&doc);
            assert!(take.measures, "{name}: the sweeps lay out measures only");
            for i in 0..take.steps.len() {
                let mut spare = None;
                for width in widths() {
                    let full = take.layout(i, &theme, width);
                    let fast = take.measure(i, &theme, width, spare.take());
                    assert_eq!(
                        (&fast.clipped_labels, fast.code_needs, fast.labels_in_margin),
                        (&full.clipped_labels, full.code_needs, full.labels_in_margin),
                        "{name}: step {} at {width} columns",
                        take.steps[i].step
                    );
                    clipped += full.clipped_labels.len();
                    if full.labels_in_margin {
                        margin += 1;
                    } else {
                        under += 1;
                    }
                    spare = Some(fast.canvas);
                }
            }
        }
        assert!(
            clipped > 0 && margin > 0 && under > 0,
            "clipped {clipped}, in the margin {margin}, under {under}"
        );
    }

    #[test]
    fn a_report_is_the_one_full_layouts_give() {
        for (name, doc) in documents() {
            let text = doc.to_string();
            for width in [40, 100] {
                let opts = CheckOptions {
                    width: Some(width),
                    numbers: None,
                };
                let fast = check_text(&text, None, &opts);
                FULL_LAYOUTS.with(|f| f.set(true));
                let full = check_text(&text, None, &opts);
                FULL_LAYOUTS.with(|f| f.set(false));
                assert_eq!(fast, full, "{name} at {width} columns");
            }
        }
    }
}
