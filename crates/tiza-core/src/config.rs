//! `~/.config/tiza/config.toml`: personal settings for every session.
//!
//! - `[theme]`: per-token color overrides (see [`crate::theme`]).
//! - `[defaults]`: the values a session starts with (`pad`, `numbers`,
//!   `mouse`, `autosave`, `layout`, `width`), each written as its command
//!   takes it. Built-in < config < document < flags and `:` commands.
//! - `[styles]`: personal mark styles, the fields of `:style`. A document
//!   gets a copy of one the first time a mark uses it.
//!
//! A bad entry never stops Tiza: it becomes a warning that names the
//! file, the line and the key, and only that entry is ignored.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use toml::Spanned;
use toml::de::{DeTable, DeValue};
use toml_parser::lexer::{Token, TokenKind};

use crate::model::MarkStyle;
use crate::theme::ThemeOverrides;

/// The keys `[defaults]` accepts, in the order `tiza config` lists them.
pub const DEFAULT_KEYS: &[&str] = &[
    "pad", "numbers", "mouse", "autosave", "layout", "width", "effects",
];

/// The fields of a `[styles.NAME]` table (those of `:style`).
pub const STYLE_FIELDS: &[&str] = &["glyph", "legend", "color"];

/// `:layout stable` or `:layout dynamic`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    Stable,
    Dynamic,
}

impl LayoutMode {
    pub fn name(self) -> &'static str {
        match self {
            LayoutMode::Stable => "stable",
            LayoutMode::Dynamic => "dynamic",
        }
    }

    /// The words `:layout` takes.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "stable" | "take" => Some(LayoutMode::Stable),
            "dynamic" | "auto" => Some(LayoutMode::Dynamic),
            _ => None,
        }
    }
}

/// The `[defaults]` table: `None` for a key the file does not set (or
/// sets to a bad value), so the built-in value applies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Defaults {
    /// `pad = "V H"`: blank cells around the drawing.
    pub pad: Option<(u16, u16)>,
    /// `numbers = false`: the line-number gutter.
    pub numbers: Option<bool>,
    /// `mouse = true`: mouse capture.
    pub mouse: Option<bool>,
    /// `autosave = false`: write the sidecar on every change.
    pub autosave: Option<bool>,
    /// `layout = "stable"`: the authoring screen's layout.
    pub layout: Option<LayoutMode>,
    /// `width = 120`: the width of `tiza render` and `tiza check`.
    pub width: Option<usize>,
    /// `effects = "draw,dim"`: the enter effects (experimental) of a
    /// document that does not set them.
    pub effects: Option<crate::effects::Effects>,
}

/// What the config file holds, after every entry was checked.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// The file that was read; `None` when there is none.
    pub path: Option<PathBuf>,
    /// Where Tiza looked for it.
    pub looked_at: Option<PathBuf>,
    pub theme: Option<ThemeOverrides>,
    pub defaults: Defaults,
    pub styles: BTreeMap<String, MarkStyle>,
    /// One per ignored entry, in the order of the lines.
    pub warnings: Vec<Warning>,
    /// The lines of the entries that Tiza applied.
    applied: BTreeSet<usize>,
}

/// An entry of the file that Tiza ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    /// The line in the file, from 1; `None` for the file as a whole.
    pub line: Option<usize>,
    /// `[defaults] colour: unknown key …`.
    pub what: String,
    /// The lines that Tiza ignores for it, `(first, last)`, in order.
    pub ignored: Vec<(usize, usize)>,
}

/// The file is there and cannot be read.
#[derive(Debug, Clone)]
pub struct LoadError {
    pub path: PathBuf,
    pub error: String,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.error)
    }
}

impl Config {
    /// The config of a file that cannot be read: one warning, and
    /// nothing else applies.
    pub fn unreadable(e: &LoadError) -> Self {
        Config {
            path: Some(e.path.clone()),
            looked_at: Some(e.path.clone()),
            warnings: vec![Warning {
                line: None,
                what: format!("Tiza cannot read this file and ignores it: {}", e.error),
                ignored: Vec::new(),
            }],
            ..Config::default()
        }
    }

    /// Each warning as `PATH:LINE: what`, with the full path (stderr,
    /// `tiza config`).
    pub fn warning_lines(&self) -> Vec<String> {
        let path = self.path.as_ref().or(self.looked_at.as_ref());
        self.lines_with(path.map(|p| p.display().to_string()))
    }

    /// Each warning as `NAME:LINE: key: what`, with the file name only and
    /// without the table (the status bar, where a long path would hide the
    /// line and the key).
    pub fn short_warning_lines(&self) -> Vec<String> {
        let path = self.path.as_ref().or(self.looked_at.as_ref());
        let file = path.map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string())
        });
        let short = Config {
            warnings: self
                .warnings
                .iter()
                .map(|w| Warning {
                    line: w.line,
                    ignored: w.ignored.clone(),
                    what: w
                        .what
                        .strip_prefix('[')
                        .and_then(|r| r.split_once("] "))
                        .map_or_else(|| w.what.clone(), |(_, rest)| rest.to_string()),
                })
                .collect(),
            ..Config::default()
        };
        short.lines_with(file)
    }

    fn lines_with(&self, file: Option<String>) -> Vec<String> {
        let file = file.unwrap_or_else(|| "config.toml".into());
        self.warnings
            .iter()
            .map(|w| match w.line {
                Some(n) => format!("{file}:{n}: {}", w.what),
                None => format!("{file}: {}", w.what),
            })
            .collect()
    }
}

/// The lines of the text, what Tiza applied, and the warnings so far.
struct Ctx {
    /// The text as read (lines that are not UTF-8 hold `U+FFFD`).
    text: String,
    starts: Vec<usize>,
    /// The last line with text: a newline at the end starts no line.
    last: usize,
    /// The lines of the entries that Tiza applied (the tests check that
    /// every other line with text is in a warning).
    applied: BTreeSet<usize>,
    /// The table headers of the text that parses (`[[x]]` too), with the
    /// table each names (`[styles.io]`: `styles`, `io`).
    headers: Vec<(usize, Vec<String>)>,
    /// The tokens of the text as read, and its whole table headers: the
    /// names of the keys in the warnings.
    tokens: Vec<Token>,
    text_headers: Vec<usize>,
    warnings: Vec<Warning>,
}

impl Ctx {
    fn new(text: &str) -> Self {
        // A byte order mark is three spaces: what follows it starts its
        // line, and the offsets do not change.
        let text = &match text.strip_prefix('\u{FEFF}') {
            Some(rest) => format!("   {rest}"),
            None => text.to_string(),
        };
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        let last = starts.len() - usize::from(text.ends_with('\n'));
        let mut cx = Ctx {
            text: text.to_string(),
            starts,
            last: last.max(1),
            applied: BTreeSet::new(),
            headers: Vec::new(),
            tokens: lex(text),
            text_headers: Vec::new(),
            warnings: Vec::new(),
        };
        cx.text_headers = whole_headers(text, &cx, &cx.tokens);
        cx
    }

    /// The line (from 1) of a byte offset; the end of the text is on the
    /// last line.
    fn line(&self, offset: usize) -> usize {
        self.starts.partition_point(|&s| s <= offset).min(self.last)
    }

    /// The bytes of line `n` (from 1), without the newline.
    fn line_range(&self, n: usize) -> std::ops::Range<usize> {
        let start = self.starts[n - 1];
        let end = self.starts.get(n).map_or(self.text.len(), |e| e - 1);
        start..end
    }

    /// Whether line `n` has text other than a comment.
    fn has_text(&self, n: usize) -> bool {
        let l = self.text[self.line_range(n)].trim();
        !l.is_empty() && !l.starts_with('#')
    }

    /// The lines from `a` to `b` (byte offsets, `b` not included).
    fn lines_of(&self, a: usize, b: usize) -> std::ops::RangeInclusive<usize> {
        self.line(a)..=self.line(b.saturating_sub(1).max(a))
    }

    /// Every line of the entry `key = value`: its value can take more
    /// lines, and a table takes the lines of its entries.
    fn entry_lines(
        &self,
        key: &DeKey<'_>,
        value: &Spanned<DeValue<'_>>,
        out: &mut BTreeSet<usize>,
    ) {
        out.extend(self.lines_of(key.span().start, key.span().end));
        out.extend(self.lines_of(value.span().start, value.span().end));
        // A table of headers (the key is on one of them): every header of
        // it, as a table can be in the file two times.
        let n = self.line(key.span().start);
        if let Some((_, path)) = self.headers.iter().find(|(m, _)| *m == n) {
            out.extend(
                self.headers
                    .iter()
                    .filter(|(_, p)| p == path)
                    .map(|(m, _)| *m),
            );
        }
        match value.get_ref() {
            DeValue::Table(t) => {
                for (k, v) in t {
                    self.entry_lines(k, v, out);
                }
            }
            DeValue::Array(items) => {
                for item in items {
                    if let DeValue::Table(t) = item.get_ref() {
                        for (k, v) in t {
                            self.entry_lines(k, v, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Lines as ranges `(first, last)`: lines with no text between two
    /// lines join them.
    fn ranges(&self, lines: &BTreeSet<usize>) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for &n in lines {
            match out.last_mut() {
                Some((_, b)) if (*b + 1..n).all(|m| !self.has_text(m)) => *b = n,
                _ => out.push((n, n)),
            }
        }
        out
    }

    /// The line of the key of a table whose entries apply one by one, and
    /// the lines of its braces when it is an inline table of several lines.
    fn apply_key(&mut self, key: &DeKey<'_>, value: &Spanned<DeValue<'_>>) {
        self.applied
            .extend(self.lines_of(key.span().start, key.span().end));
        let s = value.span();
        if self.text.as_bytes().get(s.start) == Some(&b'{') {
            self.applied.insert(self.line(s.start));
            self.applied
                .insert(self.line(s.end.saturating_sub(1).max(s.start)));
        }
    }

    fn apply(&mut self, key: &DeKey<'_>, value: &Spanned<DeValue<'_>>) {
        let mut lines = BTreeSet::new();
        self.entry_lines(key, value, &mut lines);
        self.applied.extend(lines);
    }

    /// The header lines of a table that Tiza reads (a table can be in the
    /// file two times).
    fn apply_table(&mut self, path: &[&str]) {
        for (n, p) in &self.headers {
            if p.iter().map(String::as_str).eq(path.iter().copied()) {
                self.applied.insert(*n);
            }
        }
    }

    /// A warning on line `n` that makes Tiza ignore `ignored`.
    fn warn(&mut self, n: usize, what: String, ignored: Vec<(usize, usize)>) {
        self.warnings.push(Warning {
            line: Some(n),
            what,
            ignored,
        });
    }

    /// Ignores the entry `key = value` with a warning on the line of its
    /// key; when it takes more lines (a value of several lines, a table and
    /// its entries), the warning says which.
    fn ignore(&mut self, key: &DeKey<'_>, value: &Spanned<DeValue<'_>>, what: String) {
        let mut lines = BTreeSet::new();
        self.entry_lines(key, value, &mut lines);
        let n = self.line(key.span().start);
        let ranges = self.ranges(&lines);
        let what = if ranges == [(n, n)] {
            what
        } else {
            format!("{what}, so Tiza ignores {}", lines_text(&ranges))
        };
        self.warn(n, what, ranges);
    }
}

/// A key of a table, with its place in the text.
type DeKey<'i> = Spanned<std::borrow::Cow<'i, str>>;

/// `line 3`, `lines 3-5`, `lines 3-5 and 8`.
fn lines_text(ranges: &[(usize, usize)]) -> String {
    let one = |(a, b): &(usize, usize)| {
        if a == b {
            a.to_string()
        } else {
            format!("{a}-{b}")
        }
    };
    let many = ranges.len() > 1 || ranges.first().is_some_and(|(a, b)| a != b);
    let parts: Vec<String> = ranges.iter().map(one).collect();
    let list = match parts.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    format!("{} {list}", if many { "lines" } else { "line" })
}

/// Reads the config file. A missing file at the usual place is no config;
/// a missing `$TIZA_CONFIG` file is a warning. `Err` only when the file is
/// there and cannot be read.
pub fn load() -> Result<Config, LoadError> {
    let Some(path) = crate::theme::config_path() else {
        return Ok(Config::default());
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut cfg = Config {
                looked_at: Some(path.clone()),
                ..Config::default()
            };
            if std::env::var("TIZA_CONFIG").is_ok_and(|p| !p.trim().is_empty()) {
                cfg.warnings.push(Warning {
                    line: None,
                    what: "TIZA_CONFIG names this file, but it does not exist".into(),
                    ignored: Vec::new(),
                });
            }
            return Ok(cfg);
        }
        Err(e) => {
            return Err(LoadError {
                path,
                error: e.to_string(),
            });
        }
    };
    Ok(parse_bytes(&bytes, &path))
}

/// Parses a config file as read from `path`. A line with bytes that are
/// not UTF-8 is a warning, and its entries are ignored; the rest applies.
pub fn parse_bytes(bytes: &[u8], path: &Path) -> Config {
    let mut bad = BTreeSet::new();
    let mut rest = bytes;
    let mut base = 0;
    // The line of each bad byte, counted from the one before.
    let (mut line, mut counted) = (1, 0);
    while let Err(e) = std::str::from_utf8(rest) {
        let at = base + e.valid_up_to();
        line += bytes[counted..at].iter().filter(|b| **b == b'\n').count();
        counted = at;
        bad.insert(line);
        let skip = e.error_len().unwrap_or(rest.len() - e.valid_up_to());
        base = at + skip;
        rest = &bytes[base..];
    }
    parse_with(&String::from_utf8_lossy(bytes), path, bad)
}

/// Parses the text of a config file read from `path`.
pub fn parse(text: &str, path: &Path) -> Config {
    parse_with(text, path, Default::default())
}

/// The warning of a line with bytes that are not UTF-8.
const NOT_UTF8: &str = "this line is not valid UTF-8: Tiza ignores its entries";

/// How many errors Tiza recovers from, one at a time; after that it
/// ignores the rest of the file.
const MAX_ERRORS: usize = 200;

/// What is wrong at a place of the file.
enum Problem<'e> {
    /// A parse error (its message is made only for the error that a
    /// warning says: a file of noise has thousands of them).
    Parse(&'e toml::de::Error),
    /// A byte that is not UTF-8, on this line.
    NotUtf8(usize),
}

fn parse_with(text: &str, path: &Path, bad_lines: BTreeSet<usize>) -> Config {
    let mut cfg = Config {
        path: Some(path.to_path_buf()),
        looked_at: Some(path.to_path_buf()),
        ..Config::default()
    };
    let mut cx = Ctx::new(text);
    let work = recover(&mut cx, &bad_lines);
    let scan = Scan::new(&work, &cx);
    cx.headers = scan
        .headers
        .iter()
        .filter_map(|&n| Some((n, header_path(work[cx.line_range(n)].trim())?)))
        .collect();
    let (root, errors) = DeTable::parse_recoverable(&work);
    // After MAX_ERRORS of them, one warning takes the lines of the rest.
    let mut more: Vec<(usize, usize)> = Vec::new();
    for (i, e) in errors.into_iter().enumerate() {
        // `recover` left only the keys that are in the file two times.
        let Some(span) = e.span() else {
            cx.warnings.push(Warning {
                line: None,
                what: message(&e),
                ignored: Vec::new(),
            });
            continue;
        };
        let n = cx.line(span.start);
        // Its value can take more lines.
        let last = scan
            .holder(&cx, span.start)
            .filter(|h| cx.line(h.key_at) == n)
            .map_or(n, |h| cx.line(h.end.saturating_sub(1)).max(n));
        if i >= MAX_ERRORS {
            more.push((n, last));
            continue;
        }
        let what = duplicate(&cx, span);
        if last > n {
            let what = format!("{what}, so Tiza ignores {}", lines_text(&[(n, last)]));
            cx.warn(n, what, vec![(n, last)]);
        } else {
            cx.warn(n, what, Vec::new());
        }
    }
    if let Some(&(n, _)) = more.first() {
        let what = format!(
            "{} more keys that are in the file two times: Tiza uses the first value of each",
            more.len()
        );
        cx.warn(n, what, more);
    }
    let mut theme = None;
    let mut defaults = Defaults::default();
    let mut styles = BTreeMap::new();
    for (key, value) in root.get_ref() {
        match (key.get_ref().as_ref(), value.get_ref()) {
            ("theme", DeValue::Table(t)) => {
                cx.apply_table(&["theme"]);
                theme = Some(theme_table(t, &mut cx));
            }
            ("defaults", DeValue::Table(t)) => {
                cx.apply_table(&["defaults"]);
                defaults = defaults_table(t, &mut cx);
            }
            ("styles", DeValue::Table(t)) => {
                cx.apply_table(&["styles"]);
                styles = styles_table(t, &mut cx);
            }
            (name @ ("theme" | "defaults" | "styles"), other) => cx.ignore(
                key,
                value,
                format!(
                    "{name}: expected a table, got {}",
                    article(other.type_str())
                ),
            ),
            (other, _) => cx.ignore(
                key,
                value,
                format!(
                    "{other}: unknown key (Tiza reads the tables [theme], [defaults] and [styles])"
                ),
            ),
        }
    }
    // One warning per entry, in the order of the lines.
    let mut seen = BTreeSet::new();
    cx.warnings
        .retain(|w| seen.insert((w.line, w.what.clone())));
    cx.warnings.sort_by_key(|w| w.line);
    cfg.warnings = cx.warnings;
    cfg.applied = cx.applied;
    cfg.theme = theme;
    cfg.defaults = defaults;
    cfg.styles = styles;
    cfg
}

/// Takes out of the text, one at a time and the first in the file first,
/// what does not parse, with a warning each, and gives the text that is
/// left: the lines Tiza ignores are blank, so the offsets do not change
/// and what follows keeps its own meaning.
///
/// - A value that holds the error (a string, an array or an inline table
///   that is not closed takes the lines after it): Tiza ignores its entry
///   and the lines it took, up to the next table header for an array or
///   an inline table (a header cannot be in them). So does text after a
///   value of several lines, on its last line.
/// - What Tiza ignores never starts or ends inside a value: a value that
///   is open at the start of its first line or at the end of its last line
///   takes its lines too.
/// - A table header with an error or a byte that is not UTF-8: Tiza
///   cannot tell which table the entries under it go to (the parser can
///   give them to another one), so it ignores them up to the next header.
/// - Any other line with an error: Tiza ignores that line.
///
/// Each pass blanks at least one byte, so the loop ends: when the lines of
/// an error are already blank, its cause is above them, and Tiza ignores
/// the line above too. Past `MAX_ERRORS`, one warning takes the rest of the
/// file, and the lines above it if an error stays.
fn recover(cx: &mut Ctx, bad_lines: &BTreeSet<usize>) -> String {
    let mut work = cx.text.clone().into_bytes();
    let mut blank: BTreeSet<usize> = BTreeSet::new();
    for pass in 0.. {
        let now = String::from_utf8(work.clone()).expect("blank lines are ASCII spaces");
        let scan = Scan::new(&now, cx);
        let (_, errors) = DeTable::parse_recoverable(&now);
        // A key or a table that is in the file two times is said after
        // (`duplicate`), but a table header that clashes with a definition
        // above it of another kind is an error of its header.
        let errors: Vec<(usize, &toml::de::Error)> = errors
            .iter()
            .filter_map(|e| Some((e.span()?.start, e)))
            .filter(|(at, e)| {
                e.message().trim() != "duplicate key" || clashes(&now, cx, &scan, *at)
            })
            .collect();
        let parse = errors
            .iter()
            .enumerate()
            .map(|(i, (at, e))| ((cx.line(*at), 1, i), *at, Problem::Parse(e)));
        let bytes = bad_lines.iter().filter(|n| !blank.contains(n)).map(|&n| {
            let r = cx.line_range(n);
            let at = now[r.clone()]
                .find('\u{FFFD}')
                .map_or(r.start, |i| r.start + i);
            ((n, 0, 0), at, Problem::NotUtf8(n))
        });
        // The first line with a problem; on that line, a byte that is not
        // UTF-8 first (the parse error comes from it), then the first error
        // that the parser found (the next ones often come from it).
        let Some((_, at, problem)) = parse.chain(bytes).min_by_key(|(order, ..)| *order) else {
            break;
        };
        let n = cx.line(at);
        let has_bytes = |m: usize| work[cx.line_range(m)].iter().any(|&c| c != b' ');
        // A line that is not UTF-8 has a warning of its own, unless a value
        // of several lines starts on it.
        let header = now[cx.line_range(n)].trim_start().starts_with('[');
        // A table header with a value that starts on it keeps the rule of
        // headers, up to the next header after the value.
        let held = scan
            .holder(cx, at)
            .filter(|h| !(header && cx.line(h.key_at) == n));
        let (mut first, mut last, what) = if pass >= MAX_ERRORS {
            // A value open at the error starts the lines at its key, as
            // below. After the cap, a net for the end of the loop: no known
            // input gets there (an error that stays has its cause above).
            let from = if pass > MAX_ERRORS {
                (1..=cx.last).find(|&m| has_bytes(m)).unwrap_or(n)
            } else {
                n
            };
            (from, cx.last, "too many errors".to_string())
        } else if let Some(h) = held {
            let first = cx.line(h.key_at);
            let mut last = cx.line(h.end.saturating_sub(1).max(h.key_at)).max(n);
            if h.bracketed
                && let Some(next) = scan.next_header(first)
            {
                last = last.min(next - 1);
            }
            // A value that is not closed ends with the text: the blank and
            // comment lines at its end are not its lines.
            while last > first && !cx.has_text(last) {
                last -= 1;
            }
            let what = match problem {
                // A value that is not closed says so, wherever the parser
                // saw it.
                Problem::Parse(e) => message(
                    errors
                        .iter()
                        .filter(|(at, _)| h.value_at <= *at && *at <= h.end)
                        .map(|(_, e)| *e)
                        .find(|e| e.message().trim().starts_with("unclosed"))
                        .unwrap_or(e),
                ),
                Problem::NotUtf8(b) => format!("line {b} is not valid UTF-8"),
            };
            let what = if h.name.is_empty() {
                what
            } else {
                format!("{}: {what}", h.name)
            };
            (first, last.max(first), what)
        } else if header {
            // A string that the header opens hides the headers in it: the
            // scan reads the tokens of the whole text.
            let mut last = scan.next_header(n).map_or(cx.last, |next| next - 1);
            while last > n && !cx.has_text(last) {
                last -= 1;
            }
            let what = match problem {
                Problem::Parse(e) if e.message().trim() == "duplicate key" => format!(
                    "{}: a table or a value above it has this name",
                    now[cx.line_range(n)].trim()
                ),
                Problem::Parse(e) => format!("{}: the table header has an error", message(e)),
                Problem::NotUtf8(_) => {
                    "this line is not valid UTF-8: the table header has an error".to_string()
                }
            };
            (n, last, what)
        } else {
            let what = match problem {
                Problem::Parse(e) => match line_key(cx, n) {
                    Some(name) => format!("{name}: {}", message(e)),
                    None => message(e),
                },
                Problem::NotUtf8(_) => NOT_UTF8.to_string(),
            };
            (n, n, what)
        };
        // The lines that Tiza ignores never start or end inside a value, else
        // the rest of the value would parse as entries: a value that is open
        // at the start of the first one (it closes there) takes its lines
        // from its key, and a value that is open at the end of the last one
        // (it starts there, after what holds the error) takes its lines too.
        if let Some(h) = scan
            .holder(cx, cx.line_range(first).start)
            .filter(|h| cx.line(h.key_at) < first)
        {
            first = cx.line(h.key_at);
        }
        while last < cx.last
            && let Some(h) = scan.holder(cx, cx.line_range(last).end)
        {
            let mut end = cx.line(h.end.saturating_sub(1));
            if h.bracketed
                && let Some(next) = scan.next_header(last)
            {
                end = end.min(next - 1);
            }
            while end > last && !cx.has_text(end) {
                end -= 1;
            }
            if end <= last {
                break;
            }
            last = end;
        }
        // A net for the end of the loop: no known input gets here, but a
        // pass that blanks nothing would repeat for ever.
        if !(first..=last).any(has_bytes) {
            match (1..first).rev().find(|&m| has_bytes(m)) {
                Some(m) => first = m,
                None => match (last + 1..=cx.last).find(|&m| has_bytes(m)) {
                    Some(m) => last = m,
                    // Only spaces are left, and they parse.
                    None => break,
                },
            }
        }
        let what = if last > first {
            format!("{what}, so Tiza ignores {}", lines_text(&[(first, last)]))
        } else {
            what
        };
        if pass > MAX_ERRORS {
            // This warning takes the lines of the one of the cap too.
            cx.warnings.pop();
        }
        cx.warn(first, what, vec![(first, last)]);
        for m in first..=last {
            let r = cx.line_range(m);
            work[r].fill(b' ');
            blank.insert(m);
        }
    }
    String::from_utf8(work).expect("blank lines are ASCII spaces")
}

/// Whether the error at `at` is on a whole table header that clashes with a
/// definition above it, which the parser keeps: a value of that name, or a
/// header of the other kind. `[[x]]` two times is an array of tables, with
/// no error, so an error on one is a clash; `[x]` two times is not one, as
/// TOML reads the entries of both.
fn clashes(text: &str, cx: &Ctx, scan: &Scan, at: usize) -> bool {
    let n = cx.line(at);
    if !scan.headers.contains(&n) {
        return false;
    }
    let line = |m: usize| text[cx.line_range(m)].trim();
    let table = |m: usize| (!line(m).starts_with("[[")).then(|| header_path(line(m)));
    table(n).is_none()
        || !scan
            .headers
            .iter()
            .take_while(|m| **m < n)
            .any(|&m| table(m) == table(n))
}

/// The message of a parse error, on one line.
fn message(e: &toml::de::Error) -> String {
    e.message().trim().replace('\n', "; ")
}

/// The entry `key = value` whose value holds a place of the text.
struct Holder {
    key_at: usize,
    value_at: usize,
    /// Where its value ends (the end of the text for a string that is not
    /// closed).
    end: usize,
    /// An array or an inline table: no table header can be in it.
    bracketed: bool,
    /// `[styles.io] legend`.
    name: String,
}

/// The tokens of a text, as the parser reads it (`toml_parser`'s lexer):
/// the recovery never splits a string or takes the text in one as entries.
fn lex(text: &str) -> Vec<Token> {
    toml_parser::Source::new(text).lex().into_vec()
}

/// A value of the text that can take more lines: a multi-line string, or
/// an array or an inline table from its bracket to the bracket that closes
/// it (any closing bracket, as the author meant it: `[1, 2}` is a value of
/// one line) or to the end of the text.
struct Value {
    start: usize,
    /// After its closing delimiter, or the end of the text.
    end: usize,
    bracketed: bool,
}

/// What the recovery reads in a text: its outermost values and its whole
/// table headers, from one pass of the lexer.
struct Scan {
    values: Vec<Value>,
    headers: Vec<usize>,
}

impl Scan {
    fn new(text: &str, cx: &Ctx) -> Scan {
        let tokens = lex(text);
        let mut values = Vec::new();
        let mut depth = 0usize;
        let mut open = 0;
        for t in &tokens {
            let (s, e) = (t.span().start(), t.span().end());
            match t.kind() {
                TokenKind::MlBasicString | TokenKind::MlLiteralString if depth == 0 => {
                    values.push(Value {
                        start: s,
                        end: e,
                        bracketed: false,
                    });
                }
                TokenKind::LeftSquareBracket | TokenKind::LeftCurlyBracket => {
                    if depth == 0 {
                        open = s;
                    }
                    depth += 1;
                }
                TokenKind::RightSquareBracket | TokenKind::RightCurlyBracket if depth > 0 => {
                    depth -= 1;
                    if depth == 0 {
                        values.push(Value {
                            start: open,
                            end: e,
                            bracketed: true,
                        });
                    }
                }
                _ => {}
            }
        }
        if depth > 0 {
            values.push(Value {
                start: open,
                end: text.len(),
                bracketed: true,
            });
        }
        Scan {
            values,
            headers: whole_headers(text, cx, &tokens),
        }
    }

    /// The first whole table header after line `after`.
    fn next_header(&self, after: usize) -> Option<usize> {
        self.headers.iter().copied().find(|n| *n > after)
    }

    /// The entry whose value holds the byte `at`: the outermost value that
    /// is open at `at`, or one that opens after `at` on its line and goes
    /// on below it (the error is on the line of its key, and its lines must
    /// not parse as entries). The text also finds an entry that the parser
    /// left out of its tree, such as a key that is in the file two times.
    /// Its name is the key on its line.
    fn holder(&self, cx: &Ctx, at: usize) -> Option<Holder> {
        let line_end = cx.line_range(cx.line(at)).end;
        // The values do not overlap and are in the order of the text.
        let i = self.values.partition_point(|v| v.start < at);
        let before = i.checked_sub(1).map(|j| &self.values[j]);
        // (An error at the end of the text in a value that is not closed is
        // the start of its last line's: the rule of starts takes it.)
        let v = before.filter(|v| at < v.end).or_else(|| {
            self.values
                .get(i)
                .filter(|v| v.start < line_end && v.end > line_end)
        })?;
        let n = cx.line(v.start);
        Some(Holder {
            key_at: cx.starts[n - 1],
            value_at: v.start,
            end: v.end,
            bracketed: v.bracketed,
            name: line_key(cx, n).unwrap_or_default(),
        })
    }
}

/// The table named by a whole header: `[styles.io]` is `styles`, `io`, and
/// `[[x]]` is `x`.
fn header_path(line: &str) -> Option<Vec<String>> {
    let root = DeTable::parse(line).ok()?;
    let mut table = root.get_ref();
    let mut path = Vec::new();
    loop {
        let mut entries = table.iter();
        let (k, v) = entries.next()?;
        if entries.next().is_some() {
            return None;
        }
        path.push(k.get_ref().to_string());
        match v.get_ref() {
            DeValue::Table(t) if t.is_empty() => return Some(path),
            DeValue::Table(t) => table = t,
            DeValue::Array(items) => {
                let mut items = items.iter();
                return match (items.next().map(|i| i.get_ref()), items.next()) {
                    (Some(DeValue::Table(t)), None) if t.is_empty() => Some(path),
                    _ => None,
                };
            }
            _ => return None,
        }
    }
}

/// The lines that are a whole table header (`[name]` or `[[name]]` that
/// parses on its own): a `[` that starts a line, outside a string, on a
/// line that parses as a header (an element of an array, `[1, 2],`, does
/// not).
fn whole_headers(text: &str, cx: &Ctx, tokens: &[Token]) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let mut line_start = true;
    for t in tokens {
        match t.kind() {
            TokenKind::Newline => line_start = true,
            TokenKind::Whitespace => {}
            kind => {
                if line_start && kind == TokenKind::LeftSquareBracket {
                    let n = cx.line(t.span().start());
                    if out.last() != Some(&n)
                        && DeTable::parse(text[cx.line_range(n)].trim()).is_ok()
                    {
                        out.push(n);
                    }
                }
                line_start = false;
            }
        }
    }
    out
}

/// The name of the entry on line `n` (`[defaults] numbers`), read from the
/// tokens of the text: the key before `=` (after a lone CR, which the lexer
/// takes as a newline), in the table of the last whole header above it.
fn line_key(cx: &Ctx, n: usize) -> Option<String> {
    let r = cx.line_range(n);
    let from = cx.tokens.partition_point(|t| t.span().start() < r.start);
    let to = cx.tokens.partition_point(|t| t.span().start() < r.end);
    let mut line = &cx.tokens[from..to];
    // The end of the line itself (the CR of a CRLF is in the line) is not a
    // lone CR.
    while let Some((last, rest)) = line.split_last()
        && matches!(last.kind(), TokenKind::Newline | TokenKind::Whitespace)
    {
        line = rest;
    }
    let line = match line.iter().rposition(|t| t.kind() == TokenKind::Newline) {
        Some(i) => &line[i + 1..],
        None => line,
    };
    let key = &line[..line.iter().position(|t| t.kind() == TokenKind::Equals)?];
    let is_key = |t: &&Token| {
        matches!(
            t.kind(),
            TokenKind::Atom
                | TokenKind::BasicString
                | TokenKind::LiteralString
                | TokenKind::Dot
                | TokenKind::Whitespace
        )
    };
    if !key.iter().all(|t| is_key(&t)) {
        return None;
    }
    let key = cx.text[key.first()?.span().start()..key.last()?.span().end()].trim();
    if key.is_empty() {
        return None;
    }
    Some(match table_above(cx, n) {
        Some(t) => format!("[{t}] {key}"),
        None => key.to_string(),
    })
}

/// The table of line `n`: the name in the last whole header above it.
fn table_above(cx: &Ctx, n: usize) -> Option<String> {
    let m = *cx.text_headers.iter().rev().find(|m| **m < n)?;
    let t = cx.text[cx.line_range(m)].trim();
    Some(
        t.trim_start_matches('[')
            .split(']')
            .next()
            .unwrap_or("")
            .trim()
            .to_string(),
    )
}

/// The warning of a key or a table that is in the file two times: TOML
/// keeps the first value of a key, and reads the entries of both tables.
fn duplicate(cx: &Ctx, span: std::ops::Range<usize>) -> String {
    let header = |l: &str| {
        l.strip_prefix('[').map(|h| {
            h.trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
    };
    let line_of = |n: usize| cx.text[cx.line_range(n)].trim();
    let line = cx.line(span.start);
    if let Some(table) = header(line_of(line)) {
        return format!("[{table}]: duplicate table: Tiza reads the entries of both");
    }
    let key = cx.text.get(span).unwrap_or("").trim();
    match table_above(cx, line) {
        Some(table) => format!("[{table}] {key}: duplicate key, the first value is used"),
        None => format!("{key}: duplicate key, the first value is used"),
    }
}

/// `an integer`, `a table`: a type name for the messages.
fn article(kind: &str) -> String {
    if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
        format!("an {kind}")
    } else {
        format!("a {kind}")
    }
}

/// `[theme]`: color strings, and `dark` / `light` / `terminal` subtables
/// of color strings, through the converter of the theme files; the token
/// names are checked when the theme is built.
fn theme_table(t: &DeTable<'_>, cx: &mut Ctx) -> ThemeOverrides {
    let (ov, left) = crate::theme::overrides_from_table(t);
    let out: BTreeSet<usize> = left.iter().map(|l| l.key_at).collect();
    for (k, v) in t {
        match v.get_ref() {
            DeValue::Table(sub) => {
                cx.apply_table(&["theme", k.get_ref()]);
                cx.apply_key(k, v);
                for (k2, v2) in sub {
                    if !out.contains(&k2.span().start) {
                        cx.apply(k2, v2);
                    }
                }
            }
            _ if !out.contains(&k.span().start) => cx.apply(k, v),
            _ => {}
        }
    }
    // The entries of `[theme]` and of its tables, by the place of the key.
    let subs = t.iter().filter_map(|(_, v)| match v.get_ref() {
        DeValue::Table(sub) => Some(sub),
        _ => None,
    });
    let entries: Vec<_> = t.iter().chain(subs.flatten()).collect();
    for l in left {
        let table = match &l.table {
            Some(sub) => format!("theme.{sub}"),
            None => "theme".to_string(),
        };
        let what = format!(
            "[{table}] {}: expected a color string, got {}",
            l.key,
            article(l.got)
        );
        match entries.iter().find(|(k, _)| k.span().start == l.key_at) {
            // A table (`[theme.dark.x]`) takes its entries too.
            Some((k, v)) => cx.ignore(k, v, what),
            None => {
                let n = cx.line(l.key_at);
                cx.warn(n, what, vec![(n, n)]);
            }
        }
    }
    ov
}

/// A value of `[defaults]` as text, for the messages.
fn shown(v: &DeValue<'_>) -> String {
    match v {
        DeValue::String(s) => format!("\"{s}\""),
        DeValue::Integer(i) => i.to_string(),
        DeValue::Boolean(b) => b.to_string(),
        other => article(other.type_str()),
    }
}

/// `true`, `false` or the words `:numbers`, `:mouse` and `:autosave` take.
fn on_off(v: &DeValue<'_>) -> Option<bool> {
    match v {
        DeValue::Boolean(b) => Some(*b),
        DeValue::String(s) => match s.trim() {
            "on" | "yes" => Some(true),
            "off" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn defaults_table(t: &DeTable<'_>, cx: &mut Ctx) -> Defaults {
    let mut d = Defaults::default();
    for (k, v) in t {
        let key = k.get_ref().as_ref();
        let value = v.get_ref();
        let hint = match key {
            "pad" => {
                let parsed = match value {
                    DeValue::String(s) if matches!(s.trim(), "default" | "reset") => {
                        cx.apply(k, v);
                        continue;
                    }
                    DeValue::String(s) => parse_padding(s),
                    DeValue::Integer(i) => i
                        .as_str()
                        .parse::<u16>()
                        .ok()
                        .filter(|_| i.radix() == 10)
                        .map(|v| (v, v.saturating_mul(2))),
                    _ => None,
                };
                match parsed {
                    Some(p) => {
                        d.pad = Some(p);
                        None
                    }
                    None => Some("use \"V H\" or \"V\", as :pad takes it".to_string()),
                }
            }
            "numbers" | "mouse" | "autosave" => match on_off(value) {
                Some(b) => {
                    let slot = match key {
                        "numbers" => &mut d.numbers,
                        "mouse" => &mut d.mouse,
                        _ => &mut d.autosave,
                    };
                    *slot = Some(b);
                    None
                }
                None => Some("use true or false".to_string()),
            },
            "layout" => match value {
                DeValue::String(s) if LayoutMode::parse(s).is_some() => {
                    d.layout = LayoutMode::parse(s);
                    None
                }
                _ => Some("use \"stable\" or \"dynamic\"".to_string()),
            },
            "width" => {
                let n = match value {
                    DeValue::Integer(i) if i.radix() == 10 => i.as_str().parse::<usize>().ok(),
                    _ => None,
                };
                match n.and_then(|n| crate::layout::check_width(n).ok()) {
                    Some(n) => {
                        d.width = Some(n);
                        None
                    }
                    None => Some(format!(
                        "use a number of columns between {} and {}",
                        crate::layout::MIN_WIDTH,
                        crate::layout::MAX_WIDTH
                    )),
                }
            }
            "effects" => match value {
                DeValue::String(s) => match crate::effects::Effects::parse(s) {
                    Ok(fx) => {
                        d.effects = Some(fx);
                        None
                    }
                    Err(_) => {
                        Some("use \"draw\", \"dim\", \"draw,dim\", \"on\" or \"off\"".to_string())
                    }
                },
                _ => Some("use \"draw\", \"dim\", \"draw,dim\", \"on\" or \"off\"".to_string()),
            },
            other => {
                cx.ignore(
                    k,
                    v,
                    format!(
                        "[defaults] {other}: unknown key (keys: {})",
                        DEFAULT_KEYS.join(", ")
                    ),
                );
                continue;
            }
        };
        match hint {
            None => cx.apply(k, v),
            Some(hint) => cx.ignore(
                k,
                v,
                format!("[defaults] {key}: {} is not valid: {hint}", shown(value)),
            ),
        }
    }
    d
}

/// `[styles.NAME]` tables: the fields of `:style`. A name of a built-in
/// kind or alias is refused (built-in kinds do not change this way).
fn styles_table(t: &DeTable<'_>, cx: &mut Ctx) -> BTreeMap<String, MarkStyle> {
    let mut out = BTreeMap::new();
    for (k, v) in t {
        let name = k.get_ref().as_ref();
        if crate::model::builtin_kind(name).is_some() {
            cx.ignore(
                k,
                v,
                format!(
                    "[styles] {name}: a built-in kind has this name; personal styles cannot change built-in kinds"
                ),
            );
            continue;
        }
        if name.trim().is_empty() {
            cx.ignore(k, v, "[styles] \"\": a style needs a name".into());
            continue;
        }
        if name.chars().any(char::is_whitespace) {
            cx.ignore(
                k,
                v,
                format!("[styles] \"{name}\": a style name has no spaces"),
            );
            continue;
        }
        let DeValue::Table(fields) = v.get_ref() else {
            cx.ignore(
                k,
                v,
                format!(
                    "[styles] {name}: expected a table with glyph, legend and color, got {}",
                    article(v.get_ref().type_str())
                ),
            );
            continue;
        };
        let mut st = MarkStyle::default();
        // The fields that the style takes: applied only with the style.
        let mut taken: Vec<(&DeKey<'_>, &Spanned<DeValue<'_>>)> = Vec::new();
        for (f, fv) in fields {
            let field = f.get_ref().as_ref();
            let text = match fv.get_ref() {
                DeValue::String(s) => s.to_string(),
                other => {
                    let what = if STYLE_FIELDS.contains(&field) {
                        format!(
                            "[styles.{name}] {field}: expected a string, got {}",
                            article(other.type_str())
                        )
                    } else {
                        unknown_field(name, field)
                    };
                    cx.ignore(f, fv, what);
                    continue;
                }
            };
            match field {
                "glyph" if !text.trim().is_empty() => st.glyph = Some(text),
                "glyph" => {
                    cx.ignore(
                        f,
                        fv,
                        format!("[styles.{name}] glyph: a glyph cannot be empty"),
                    );
                    continue;
                }
                "legend" => st.legend = Some(text),
                "color" if crate::Theme::dark().named_color(&text).is_some() => {
                    st.color = Some(text);
                }
                "color" => {
                    cx.ignore(
                        f,
                        fv,
                        format!(
                            "[styles.{name}] color: \"{text}\" is not valid: use {} or #rrggbb",
                            crate::theme::PALETTE.join(", ")
                        ),
                    );
                    continue;
                }
                _ => {
                    cx.ignore(f, fv, unknown_field(name, field));
                    continue;
                }
            }
            taken.push((f, fv));
        }
        if st.glyph.is_none() {
            cx.ignore(k, v, format!("[styles] {name}: a style needs a glyph"));
            continue;
        }
        cx.apply_table(&["styles", name]);
        cx.apply_key(k, v);
        for (f, fv) in taken {
            cx.apply(f, fv);
        }
        out.insert(name.to_string(), st);
    }
    out
}

fn unknown_field(name: &str, field: &str) -> String {
    format!(
        "[styles.{name}] {field}: unknown field (fields: {})",
        STYLE_FIELDS.join(", ")
    )
}

/// `V`, `V H` or `V,H` as `:pad` takes them: one number is `V` with `H`
/// twice as wide; more than two numbers keep the first two.
pub fn parse_padding(text: &str) -> Option<(u16, u16)> {
    let nums: Vec<u16> = text
        .split([' ', ','])
        .filter(|t| !t.is_empty())
        .map(|t| t.parse::<u16>())
        .collect::<Result<_, _>>()
        .ok()?;
    match nums.as_slice() {
        [] => None,
        [v] => Some((*v, v.saturating_mul(2))),
        [v, h, ..] => Some((*v, *h)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::OverrideValue;

    fn cfg(text: &str) -> Config {
        parse_in_time(text.as_bytes())
    }

    /// Parses in a thread: a parse that does not end fails the test in
    /// 10 s.
    fn parse_in_time(bytes: &[u8]) -> Config {
        let text = bytes.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            // After the time out, nothing waits for the answer.
            let _ = tx.send(parse_bytes(&text, Path::new("/c/config.toml")));
        });
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("no end in 10 s: {:?}", String::from_utf8_lossy(bytes)))
    }

    #[test]
    fn defaults_take_the_values_of_their_commands() {
        let c = cfg(
            "[defaults]\npad = \"1 3\"\nnumbers = false\nmouse = \"on\"\nautosave = false\nlayout = \"stable\"\nwidth = 120\neffects = \"draw,dim\"\n",
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warning_lines());
        assert_eq!(
            c.defaults,
            Defaults {
                pad: Some((1, 3)),
                numbers: Some(false),
                mouse: Some(true),
                autosave: Some(false),
                layout: Some(LayoutMode::Stable),
                width: Some(120),
                effects: Some(crate::effects::Effects::parse("draw,dim").unwrap()),
            }
        );
        assert_eq!(cfg("[defaults]\npad = 2").defaults.pad, Some((2, 4)));
        assert_eq!(cfg("[defaults]\npad = \"2,1\"").defaults.pad, Some((2, 1)));
    }

    #[test]
    fn bad_entries_are_warnings_with_their_line_and_are_ignored() {
        let c = cfg("[defaults]\nnumbers = false\ncolour = \"x\"\nwidth = \"wide\"\n");
        assert_eq!(c.defaults.numbers, Some(false));
        assert_eq!(c.defaults.width, None);
        let w = c.warning_lines();
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(
            w[0].starts_with("/c/config.toml:3: [defaults] colour: unknown key"),
            "{:?}",
            c.warning_lines()
        );
        assert!(
            w[1].starts_with("/c/config.toml:4: [defaults] width: \"wide\""),
            "{:?}",
            c.warning_lines()
        );
        // A syntax error: a warning with its line; the rest still applies.
        let c = cfg("[defaults]\nmouse = true\nnumbers = \n");
        assert_eq!(c.defaults.mouse, Some(true));
        assert!(
            c.warning_lines()
                .iter()
                .any(|w| w.starts_with("/c/config.toml:3:")),
            "{:?}",
            c.warning_lines()
        );
    }

    #[test]
    fn styles_are_checked_and_built_in_names_refused() {
        let c = cfg(
            "[styles.io]\nglyph = \"⇄\"\nlegend = \"network I/O\"\ncolor = \"cyan\"\n[styles.warn]\nglyph = \"!\"\n[styles.x]\nlegend = \"no glyph\"\n[styles.y]\nglyph = \"y\"\ncolor = \"mauve\"\nsize = 2\n",
        );
        assert_eq!(c.styles.len(), 2, "{:?}", c.styles);
        assert_eq!(
            c.styles["io"],
            MarkStyle::new("⇄", "network I/O", "cyan"),
            "{:?}",
            c.warning_lines()
        );
        assert_eq!(c.styles["y"].color, None);
        let all = c.warning_lines().join("\n");
        assert!(all.contains(":5: [styles] warn: a built-in kind"), "{all}");
        assert!(
            all.contains(":7: [styles] x: a style needs a glyph"),
            "{all}"
        );
        assert!(all.contains(":11: [styles.y] color:"), "{all}");
        assert!(all.contains(":12: [styles.y] size: unknown field"), "{all}");
    }

    #[test]
    fn the_theme_table_keeps_its_shape() {
        let c = cfg(
            "[theme]\naccent = \"#a6e3a1\"\nmuted = 3\n[theme.dark]\nnote = \"#f9e2af\"\n[other]\nx = 1\n",
        );
        let t = c.theme.clone().unwrap();
        assert_eq!(
            t.0.get("accent"),
            Some(&OverrideValue::Color("#a6e3a1".into()))
        );
        assert!(matches!(t.0.get("dark"), Some(OverrideValue::Table(_))));
        assert!(!t.0.contains_key("muted"));
        let all = c.warning_lines().join("\n");
        assert!(
            all.contains(":3: [theme] muted: expected a color string"),
            "{all}"
        );
        assert!(all.contains(":6: other: unknown key"), "{all}");
    }

    #[test]
    fn duplicates_name_their_table_and_key_once() {
        let c = cfg(
            "[defaults]\nnumbers = false\nnumbers = true\n[defaults]\nmouse = true\nnumbers = true\n",
        );
        assert_eq!(c.defaults.numbers, Some(false), "the first value");
        assert_eq!(c.defaults.mouse, Some(true), "both tables are read");
        assert_eq!(
            c.warning_lines(),
            vec![
                "/c/config.toml:3: [defaults] numbers: duplicate key, the first value is used",
                "/c/config.toml:4: [defaults]: duplicate table: Tiza reads the entries of both",
                "/c/config.toml:6: [defaults] numbers: duplicate key, the first value is used",
            ]
        );
    }

    #[test]
    fn warnings_come_in_line_order_and_once_per_line() {
        let c = cfg(
            "[defaults]\nwidth = []\nlayout = 1.5\nmouse = \npad = true\n[styles.\"\"]\nglyph = \"x\"\n",
        );
        let w = c.warning_lines();
        let lines: Vec<&str> = w.iter().map(|l| l.split(':').nth(1).unwrap()).collect();
        assert_eq!(lines, ["2", "3", "4", "5", "6"], "{w:?}");
        assert!(w[0].contains("an array"), "{w:?}");
        assert!(w[1].contains("a float"), "{w:?}");
        assert!(w[4].contains("a style needs a name"), "{w:?}");
        // The file name alone and no table, for the status bar.
        assert!(c.short_warning_lines()[0].starts_with("config.toml:2: width: an array"));
    }

    #[test]
    fn a_line_that_is_not_utf8_is_a_warning_and_only_its_entry_goes() {
        let c = parse_bytes(
            b"# caf\xe9\n[defaults]\nnumbers = false\nmouse = \"o\xffn\"\nwidth = 70\n",
            Path::new("/c/config.toml"),
        );
        assert_eq!(c.defaults.numbers, Some(false));
        assert_eq!(c.defaults.width, Some(70));
        assert_eq!(c.defaults.mouse, None);
        let w = c.warning_lines();
        assert_eq!(
            w,
            vec![
                "/c/config.toml:1: this line is not valid UTF-8: Tiza ignores its entries",
                "/c/config.toml:4: this line is not valid UTF-8: Tiza ignores its entries",
            ]
        );
    }

    /// The rule for what Tiza does not apply: every line with text that
    /// it does not apply is in the lines of a warning, or on the line of
    /// one; and the warnings are on `errors`, the lines that have an error.
    fn check_lines(case: &str, text: &str, c: &Config, errors: &[usize]) {
        let cx = Ctx::new(text);
        for n in (1..=cx.last).filter(|n| cx.has_text(*n) && !c.applied.contains(n)) {
            assert!(
                c.warnings
                    .iter()
                    .any(|w| w.line == Some(n)
                        || w.ignored.iter().any(|(a, b)| (*a..=*b).contains(&n))),
                "{case}: line {n} does not apply and no warning names it: {:?}",
                c.warning_lines()
            );
        }
        let warned: Vec<usize> = c.warnings.iter().filter_map(|w| w.line).collect();
        assert_eq!(warned, errors, "{case}: {:?}", c.warning_lines());
    }

    #[test]
    fn no_line_goes_without_a_warning() {
        let only_accent = |c: &Config| {
            c.theme.as_ref().is_some_and(|t| {
                t.0.len() == 1 && t.0["accent"] == OverrideValue::Color("#ffffff".into())
            })
        };
        let none = Defaults::default();
        let numbers_off = Defaults {
            numbers: Some(false),
            ..Default::default()
        };
        let io = |c: &Config| c.styles.get("io").and_then(|s| s.glyph.as_deref()) == Some("x");
        type Check = Box<dyn Fn(&Config) -> bool>;
        // (case, file, the lines with an error, what applies)
        let cases: Vec<(&str, &[u8], &[usize], Check)> = vec![
            (
                "a header that is not closed",
                b"[theme]\naccent = \"#ffffff\"\n[defaults\nnumbers = false\n",
                &[3],
                Box::new(move |c| only_accent(c) && c.defaults == none),
            ),
            (
                "a key of both tables under a broken header",
                b"[theme]\naccent = \"#ffffff\"\n[defaults\naccent = \"#000000\"\nnumbers = false\n",
                &[3],
                Box::new(move |c| only_accent(c) && c.defaults == none),
            ),
            (
                "a whole header after a broken one",
                b"[theme]\naccent = \"#ffffff\"\n[defaults\nnumbers = false\n[styles.io]\nglyph = \"x\"\n",
                &[3],
                Box::new(move |c| only_accent(c) && c.defaults == none && io(c)),
            ),
            (
                "a broken subtable header",
                b"[theme]\naccent = \"#ffffff\"\n[theme.dark\nfg = \"#111111\"\naccent = \"#222222\"\n",
                &[3],
                Box::new(only_accent),
            ),
            (
                "a broken style header",
                b"[defaults]\nnumbers = false\n[styles.io\nglyph = \"x\"\nmouse = true\n",
                &[3],
                Box::new(move |c| c.defaults == numbers_off && c.styles.is_empty()),
            ),
            (
                "text after a header",
                b"[theme]\naccent = \"#ffffff\"\n[defaults] x\nnumbers = false\n[styles.io]\nglyph = \"x\"\n",
                &[3],
                Box::new(move |c| only_accent(c) && c.defaults == none && io(c)),
            ),
            (
                "two broken headers",
                b"[theme\naccent = \"#ffffff\"\n[defaults\nnumbers = false\n[styles.io]\nglyph = \"x\"\n",
                &[1],
                Box::new(move |c| c.theme.is_none() && c.defaults == none && io(c)),
            ),
            (
                "a header with a byte that is not UTF-8",
                b"[theme]\naccent = \"#ffffff\"\n[defa\xffults]\nnumbers = false\n[styles.io]\nglyph = \"x\"\n",
                &[3],
                Box::new(move |c| only_accent(c) && c.defaults == none && io(c)),
            ),
            (
                "a header line in a multi-line string under a broken header",
                b"[theme\nlegend = \"\"\"a\n[defaults]\nb\"\"\"\naccent = \"#ffffff\"\n[styles.io]\nglyph = \"x\"\n",
                &[1],
                Box::new(move |c| c.theme.is_none() && c.defaults == none && io(c)),
            ),
            (
                "a header line in a multi-line literal string under a broken header",
                b"[theme\nlegend = '''a\n[defaults]\nb'''\naccent = \"#ffffff\"\n[styles.io]\nglyph = \"x\"\n",
                &[1],
                Box::new(move |c| c.theme.is_none() && c.defaults == none && io(c)),
            ),
            (
                "a multi-line literal string that is not closed",
                b"[styles.io]\nglyph = \"x\"\nlegend = '''network\n[defaults]\nnumbers = false\n",
                &[3],
                Box::new(move |c| c.styles["io"].legend.is_none() && io(c) && c.defaults == none),
            ),
            (
                "an array element under a broken header",
                b"[defaults\nwidth = [\n[1, 2],\n]\nnumbers = false\n[styles.io]\nglyph = \"x\"\n",
                &[1],
                Box::new(move |c| c.defaults == none && io(c)),
            ),
            (
                "an array of tables",
                b"[[styles]]\nglyph = \"x\"\n[defaults]\nnumbers = false\n",
                &[1],
                Box::new(move |c| c.styles.is_empty() && c.defaults == numbers_off),
            ),
            (
                "a string that is not closed",
                b"[defaults]\nlayout = \"stable\nnumbers = false\nmouse = true\n",
                &[2],
                Box::new(|c| {
                    c.defaults
                        == Defaults {
                            numbers: Some(false),
                            mouse: Some(true),
                            ..Default::default()
                        }
                }),
            ),
            (
                "a multi-line string that is not closed",
                b"[defaults]\nlayout = \"\"\"stable\nnumbers = false\n[theme]\naccent = \"#ffffff\"\n",
                &[2],
                Box::new(move |c| c.theme.is_none() && c.defaults == none),
            ),
            (
                "a multi-line string of a style that is not closed",
                b"[styles.io]\nglyph = \"x\"\nlegend = \"\"\"network\n[defaults]\nnumbers = false\n",
                &[3],
                Box::new(move |c| c.styles["io"].legend.is_none() && io(c) && c.defaults == none),
            ),
            (
                "an array that is not closed",
                b"[defaults]\nwidth = [1,\nnumbers = false\nmouse = true\n",
                &[2],
                Box::new(move |c| c.defaults == none),
            ),
            (
                "an inline table that is not closed",
                b"[theme]\ndark = {fg = \"#111111\"\naccent = \"#222222\"\n[defaults]\nnumbers = false\n",
                &[2],
                Box::new(move |c| {
                    c.theme.as_ref().is_some_and(|t| t.0.is_empty()) && c.defaults == numbers_off
                }),
            ),
            (
                "a byte that is not UTF-8 in a multi-line value",
                b"[styles.io]\nglyph = \"x\"\nlegend = \"\"\"ok\ncaf\xe9\"\"\"\n[defaults]\nnumbers = false\n",
                &[3],
                Box::new(move |c| c.styles["io"].legend.is_none() && io(c) && c.defaults == numbers_off),
            ),
            (
                "a table that is in the file two times",
                b"[defaults]\nnumbers = false\n[defaults]\nmouse = true\n",
                &[3],
                Box::new(|c| {
                    c.defaults
                        == Defaults {
                            numbers: Some(false),
                            mouse: Some(true),
                            ..Default::default()
                        }
                }),
            ),
            (
                "a value of several lines with the wrong type",
                b"[defaults]\nwidth = [\n  1,\n  2,\n]\nnumbers = false\n",
                &[2],
                Box::new(move |c| c.defaults == numbers_off),
            ),
            (
                "a style that is refused, with its fields",
                b"[styles.warn]\nglyph = \"!\"\nlegend = \"mine\"\n\n[styles.io]\nglyph = \"x\"\n",
                &[1],
                Box::new(move |c| c.styles.len() == 1 && io(c)),
            ),
            // The parser leaves the second entry of a key out of its tree;
            // the text tells which value is open.
            (
                "a key two times, with a multi-line string that is not closed",
                b"[defaults]\nnumbers = false\nnumbers = \"\"\"x\nmouse = true\n",
                &[3],
                Box::new(move |c| c.defaults == numbers_off),
            ),
            (
                "a key two times, with a multi-line literal string that is not closed",
                b"[defaults]\nnumbers = false\nnumbers = '''x\nmouse = true\n",
                &[3],
                Box::new(move |c| c.defaults == numbers_off),
            ),
            (
                "a key two times, with an array that is not closed",
                b"[defaults]\nnumbers = false\nnumbers = [1,\n2,\n[styles.io]\nglyph = \"x\"\n",
                &[3],
                Box::new(move |c| c.defaults == numbers_off && io(c)),
            ),
            (
                "a key two times, with an inline table that is not closed",
                b"[defaults]\nnumbers = false\nnumbers = {a = 1,\nb = 2\n[styles.io]\nglyph = \"x\"\n",
                &[3],
                Box::new(move |c| c.defaults == numbers_off && io(c)),
            ),
            // A wrong closing bracket closes the value, as the author meant
            // it (the parser keeps it open to the end).
            (
                "an array with a wrong closing bracket",
                b"[defaults]\nwidth = [1, 2}\nnumbers = false\nmouse = true\n",
                &[2],
                Box::new(|c| {
                    c.defaults
                        == Defaults {
                            numbers: Some(false),
                            mouse: Some(true),
                            ..Default::default()
                        }
                }),
            ),
            (
                "an inline table with a wrong closing bracket",
                b"[theme]\ndark = {fg = \"#111111\"]\naccent = \"#ffffff\"\n",
                &[2],
                Box::new(only_accent),
            ),
            (
                "text after the end of a multi-line string",
                b"[styles.io]\nlegend = \"\"\"a\nb\"\"\" x\nglyph = \"x\"\n",
                &[2],
                Box::new(move |c| c.styles["io"].legend.is_none() && io(c)),
            ),
            // The error is on the line that opens a value of several
            // lines: the lines of the value are not entries.
            (
                "a bad key on the line that opens a multi-line string",
                b"[defaults]\nwidth = 100\n\n[styles.io]\nglyph = \"x\"\nlong legend = \"\"\"network\n[defaults]\nwidth = 333\nnumbers = false\n\"\"\"\n",
                &[6],
                Box::new(move |c| {
                    c.defaults
                        == Defaults {
                            width: Some(100),
                            ..Default::default()
                        }
                        && io(c)
                        && c.styles["io"].legend.is_none()
                }),
            ),
            (
                "a byte that is not UTF-8 on the line that opens a multi-line string",
                b"[styles.io]\nglyph = \"x\"\nlegend = \"\"\"caf\xe9\n[defaults]\nnumbers = false\n\"\"\"\n",
                &[3],
                Box::new(move |c| c.defaults == none && io(c)),
            ),
            (
                "a broken header that opens a multi-line string",
                b"[styles.io] legend = \"\"\"long\n[defaults]\nnumbers = false\n\"\"\"\n",
                &[1],
                Box::new(move |c| c.defaults == none && c.styles.is_empty()),
            ),
            (
                "a bad key on the line that opens an inline table of several lines",
                b"[theme]\ndark theme = {\n  fg = \"#111111\",\n  accent = \"#ffffff\"\n}\n",
                &[2],
                Box::new(|c| c.theme.as_ref().is_some_and(|t| t.0.is_empty())),
            ),
            // What Tiza ignores never starts or ends inside a value.
            (
                "a line that closes a string and opens an inline table",
                b"[defaults]\nextra = '''first\nwidth = 333\nlast'''\r{\nmouse = true\n[styles.io]\nglyph = \"x\"\nlegend = '''a\nwidth = 333\nb'''\n",
                &[2],
                Box::new(move |c| {
                    c.defaults == none
                        && io(c)
                        && c.styles["io"].legend.as_deref() == Some("a\nwidth = 333\nb")
                }),
            ),
            (
                "a string that opens after the end of an array, on its line",
                b"[defaults]\nnumbers = false\nlist = [\n  1,\n}  '''first\nmouse = false\nlast'''\nmouse = true\n",
                &[3],
                Box::new(|c| {
                    c.defaults
                        == Defaults {
                            numbers: Some(false),
                            mouse: Some(true),
                            ..Default::default()
                        }
                }),
            ),
            (
                "a broken header that opens a string with a header in it",
                b"[defaults]\n[extra = '''first\nnumbers = false\n[defaults]\nlast'''\nmouse = true\n[styles.io]\nglyph = \"x\"\n",
                &[2],
                Box::new(move |c| c.defaults == none && io(c)),
            ),
            (
                "a broken header that opens a string, and a string after it",
                b"[defaults]\n[extra = '''first\nlast'''\n[styles.db]\nglyph = \"d\"\nlegend = '''first\n[defaults]\nmouse = false\nlast'''\n",
                &[2],
                Box::new(move |c| {
                    c.defaults == none
                        && c.styles["db"].legend.as_deref()
                            == Some("first\n[defaults]\nmouse = false\nlast")
                }),
            ),
            // Entries that Tiza ignores name every line they take.
            (
                "a key two times, with a value of several lines",
                b"[defaults]\nwidth = 100\nwidth = [\n  1,\n]\n",
                &[3],
                Box::new(|c| c.defaults.width == Some(100)),
            ),
            (
                "a key two times, with a multi-line string",
                b"[styles.io]\nglyph = \"x\"\nlegend = \"a\"\nlegend = \"\"\"long\ntext\"\"\"\n",
                &[4],
                Box::new(|c| c.styles["io"].legend.as_deref() == Some("a")),
            ),
            (
                "a table in a table of [theme]",
                b"[theme.dark.x]\ny = \"1\"\nz = \"2\"\n[defaults]\nnumbers = false\n",
                &[1],
                Box::new(move |c| c.defaults == numbers_off),
            ),
            (
                "an array of tables in [theme], two times",
                b"[theme]\n[[theme.x]]\ny = \"1\"\n[[theme.x]]\nz = \"2\"\n",
                &[2],
                Box::new(|c| c.theme.as_ref().is_some_and(|t| t.0.is_empty())),
            ),
            (
                "a refused style table, two times",
                b"[styles.warn]\nglyph = \"!\"\n[styles.warn]\nlegend = \"x\"\n",
                &[3, 3],
                Box::new(|c| c.styles.is_empty()),
            ),
            (
                "an unknown table, two times",
                b"[x]\nk = 1\n[x]\na = 2\n[defaults]\nnumbers = false\n",
                &[3, 3],
                Box::new(move |c| c.defaults == numbers_off),
            ),
            (
                "an array of tables of [styles], two times",
                b"[[styles]]\nglyph = \"x\"\n[[styles]]\nglyph = \"y\"\n",
                &[1],
                // Its headers are not the header of `[styles]`.
                Box::new(|c| c.styles.is_empty() && !c.applied.contains(&1) && !c.applied.contains(&3)),
            ),
            // The tokens are the ones of the parser: a string starts where
            // the parser sees one.
            (
                "a quote at the end of a word does not start a string",
                b"[styles.io]\nglyph = \"x\"\nlegend = network I/O\"\"\"\n[defaults]\nwidth = 120\nnumbers = false\n",
                &[3],
                Box::new(move |c| {
                    io(c)
                        && c.defaults
                            == Defaults {
                                width: Some(120),
                                numbers: Some(false),
                                ..Default::default()
                            }
                }),
            ),
            (
                "the text of a string after a quote at the end of a word",
                b"[defaults]\nx = abc\"\"\"\ny = 1\nz = \"\"\"\nnumbers = true\n\"\"\"\n",
                &[2, 3, 4],
                Box::new(move |c| c.defaults == none),
            ),
            (
                "a lone carriage return ends a comment",
                b"[defaults]\n# note\rx = \"\"\"\nnumbers = true\n\"\"\"\nmouse = true\n",
                &[2],
                Box::new(|c| {
                    c.defaults
                        == Defaults {
                            mouse: Some(true),
                            ..Default::default()
                        }
                }),
            ),
            (
                "quotes after the end of a multi-line string",
                b"[defaults]\na = '''x''''''\nb = 1\nc = '''\nnumbers = false\n'''\n",
                &[2, 3, 4],
                Box::new(move |c| c.defaults == none),
            ),
            (
                "a comment on the line of an error hides the quotes after it",
                b"[defaults]\nbad#a = \"\"\"x\nnumbers = false\n",
                &[2],
                Box::new(move |c| c.defaults == numbers_off),
            ),
            // A table header that clashes with a definition above it: the
            // first one stays.
            (
                "an array of tables after a table of the same name",
                b"[defaults]\nnumbers = false\n[[defaults]]\nmouse = true\n",
                &[3],
                Box::new(move |c| c.defaults == numbers_off),
            ),
            (
                "a table for a key that has a value",
                b"[defaults]\nmouse = true\n[defaults.mouse]\nx = 1\n",
                &[3],
                Box::new(|c| c.defaults.mouse == Some(true)),
            ),
            (
                "a style table for a name that has a value",
                b"[styles]\nio = \"x\"\n[styles.io]\nglyph = \"EVIL\"\n",
                &[2, 3],
                Box::new(|c| c.styles.is_empty()),
            ),
            // Lines that apply are counted as such.
            (
                "a byte order mark before the first header",
                b"\xef\xbb\xbf[defaults]\nnumbers = false\n",
                &[],
                Box::new(move |c| c.defaults == numbers_off),
            ),
            (
                "an inline table of several lines",
                b"[styles]\nio = {\n  glyph = \"x\",\n  legend = \"y\",\n}\n",
                &[],
                Box::new(move |c| io(c) && c.styles["io"].legend.as_deref() == Some("y")),
            ),
            (
                "a line with a bracket after its key is not a header",
                b"[defaults]\nwidth = [1,\nmouse = [1]\n[styles.io]\nglyph = \"x\"\n",
                &[2],
                Box::new(move |c| io(c) && c.defaults == none),
            ),
            (
                "an array of tables that clashes, after another one",
                b"[[styles]]\nglyph = \"x\"\n[defaults]\nnumbers = false\n[[defaults]]\nmouse = true\n",
                &[1, 5],
                Box::new(move |c| c.defaults == numbers_off),
            ),
            (
                "three lines that are not UTF-8",
                b"[defaults]\nx = \"\xff\"\ny = \"\xff\"\nz = \"\xff\"\nnumbers = false\n",
                &[2, 3, 4],
                Box::new(move |c| c.defaults == numbers_off),
            ),
            (
                "a value that TOML does not have",
                b"[defaults]\nmouse = true\nnumbers = tru\n",
                &[3],
                Box::new(|c| {
                    c.defaults
                        == Defaults {
                            mouse: Some(true),
                            ..Default::default()
                        }
                }),
            ),
            (
                "a byte order mark before a comment",
                b"\xef\xbb\xbf# note\n[defaults]\nnumbers = false\n",
                &[],
                Box::new(move |c| c.defaults == numbers_off),
            ),
            (
                "an inline table of several lines in [theme]",
                b"[theme]\ndark = {\n  fg = \"#111111\",\n}\n",
                &[],
                Box::new(|c| c.theme.as_ref().is_some_and(|t| t.0.contains_key("dark"))),
            ),
        ];
        for (case, bytes, errors, applies) in cases {
            let c = parse_in_time(bytes);
            let text = String::from_utf8_lossy(bytes);
            check_lines(case, &text, &c, errors);
            assert!(
                applies(&c),
                "{case}: theme {:?}, defaults {:?}, styles {:?}",
                c.theme,
                c.defaults,
                c.styles
            );
            // The same file with CRLF line ends: the same warnings, the same
            // lines and the same values (a string of several lines keeps
            // its CR).
            let crlf: Vec<u8> = bytes
                .iter()
                .flat_map(|&b| {
                    if b == b'\n' {
                        vec![b'\r', b'\n']
                    } else {
                        vec![b]
                    }
                })
                .collect();
            let d = parse_in_time(&crlf);
            check_lines(case, &String::from_utf8_lossy(&crlf), &d, errors);
            assert_eq!(d.warning_lines(), c.warning_lines(), "{case}, CRLF");
            let ignored = |c: &Config| {
                c.warnings
                    .iter()
                    .map(|w| w.ignored.clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(ignored(&d), ignored(&c), "{case}, CRLF");
            let values = |c: &Config| {
                format!("{:?} {:?} {:?}", c.defaults, c.styles, c.theme).replace("\\r\\n", "\\n")
            };
            assert_eq!(values(&d), values(&c), "{case}, CRLF");
        }
    }

    #[test]
    fn a_warning_names_the_lines_that_it_ignores() {
        let lines = |text: &str| {
            cfg(text)
                .warnings
                .iter()
                .map(|w| w.ignored.clone())
                .collect::<Vec<_>>()
        };
        // A broken header: up to the next whole header, less blank lines.
        assert_eq!(
            lines("[theme\naccent = \"#ffffff\"\n\n[defaults]\nnumbers = false\n"),
            [vec![(1, 2)]]
        );
        // A multi-line string takes the lines to its end; an array or an
        // inline table stops at the next header.
        assert_eq!(
            lines("[defaults]\nlayout = \"\"\"x\nnumbers = false\n[theme]\naccent = \"#ffffff\"\n"),
            [vec![(2, 5)]]
        );
        assert_eq!(
            lines(
                "[theme]\ndark = {fg = \"#111111\"\naccent = \"#222222\"\n[defaults]\nnumbers = false\n"
            ),
            [vec![(2, 3)]]
        );
        // Text after a value of several lines: its entry, to that line.
        assert_eq!(
            lines("[styles.io]\nlegend = \"\"\"a\nb\"\"\" x\nglyph = \"x\"\n"),
            [vec![(2, 3)]]
        );
        // A value that is not closed, at the end: less blank and comment
        // lines.
        assert_eq!(
            lines("[defaults]\nwidth = [1, 2\nnumbers = false\n\n# end\n"),
            [vec![(2, 3)]]
        );
        // A lone carriage return is a line break for the parser, not for the
        // lines of the warnings: they count `\n`, as `tiza config` shows.
        let c = cfg("[defaults]\n# note\rx = \"\"\"\nnumbers = true\n\"\"\"\nmouse = true\n");
        assert_eq!(c.warnings[0].ignored, [(2, 4)]);
        assert!(
            c.warning_lines()[0].starts_with("/c/config.toml:2: [defaults] x: "),
            "{:?}",
            c.warning_lines()
        );
        // A bracket before the `=` of a line: no key to name.
        let c = cfg("[defaults]\nwidth [1 = 2\n");
        assert!(
            !c.warnings[0].what.starts_with("[defaults]"),
            "{:?}",
            c.warning_lines()
        );
        // A table header that clashes names itself and its lines.
        let c = cfg("[defaults]\nnumbers = false\n[[defaults]]\nmouse = true\n");
        assert_eq!(c.warnings[0].ignored, [(3, 4)]);
        assert!(
            c.warnings[0].what.starts_with("[[defaults]]: "),
            "{:?}",
            c.warning_lines()
        );
        // Past MAX_ERRORS keys that are in the file two times, one warning
        // takes the lines of the rest.
        let dups: String = (0..MAX_ERRORS + 10).map(|_| "numbers = true\n").collect();
        let text = format!("[defaults]\nnumbers = false\n{dups}");
        let c = cfg(&text);
        assert_eq!(c.warnings.len(), MAX_ERRORS + 1);
        assert_eq!(c.warnings[MAX_ERRORS].ignored.len(), 10);
        let warned: Vec<usize> = (3..=MAX_ERRORS + 3).collect();
        check_lines("keys two times past the cap", &text, &c, &warned);
        assert_eq!(c.defaults.numbers, Some(false));
        // A refused table takes its entries, and blank lines join them.
        assert_eq!(
            lines("[styles.warn]\nglyph = \"!\"\n\nlegend = \"mine\"\n"),
            [vec![(1, 4)]]
        );
        // Past MAX_ERRORS errors, one warning takes the rest of the file.
        let text: String = (1..=MAX_ERRORS + 10)
            .map(|i| format!("k{i} = \n"))
            .collect();
        let c = cfg(&text);
        assert_eq!(c.warnings.len(), MAX_ERRORS + 1);
        assert_eq!(
            c.warnings[MAX_ERRORS].ignored,
            [(MAX_ERRORS + 1, MAX_ERRORS + 10)]
        );
        // The error at the cap is a value that is open: the warning starts
        // at its key, and the lines above it still apply.
        let errors: String = (1..=MAX_ERRORS).map(|i| format!("k{i} = \n")).collect();
        let c = cfg(&format!(
            "[defaults]\nnumbers = false\n{errors}numbers = \"\"\"x\ny\n"
        ));
        assert_eq!(c.warnings.len(), MAX_ERRORS + 1, "{:?}", c.warning_lines());
        assert_eq!(
            c.warnings[MAX_ERRORS].ignored,
            [(MAX_ERRORS + 3, MAX_ERRORS + 4)]
        );
        assert_eq!(c.defaults.numbers, Some(false));
        let c = cfg("[styles.warn]\nglyph = \"!\"\n\nlegend = \"mine\"\n");
        assert!(
            c.warnings[0].what.ends_with(", so Tiza ignores lines 1-4"),
            "{:?}",
            c.warning_lines()
        );
    }
}
