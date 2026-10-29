//! Open and save: source files (`main.rs`, `main.rs:12-48`) and `.tiza` documents.

use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::highlight::detect_language;
use crate::model::{CodeSource, Document, SCHEMA_VERSION};

pub const EXT: &str = "tiza";

/// Splits `path[:A[-B]]`. Returns the path and the line range (1-based,
/// inclusive). Accepts `A-B`, `A..B` and `A`. Text after the last `:` that
/// starts with a digit, or that has only digits, `-` and `.`, is a range: a
/// malformed one, `A > B` or `A == 0` is an error, never a part of the file
/// name. Other text after a `:` (`C:thing`, `a:b.rs`) is part of the path.
pub fn parse_target(spec: &str) -> Result<(PathBuf, Option<(u32, u32)>), Error> {
    if let Some((path, range)) = spec.rsplit_once(':')
        && !path.is_empty()
        && !range.is_empty()
        && (range.starts_with(|c: char| c.is_ascii_digit())
            || range
                .chars()
                .all(|c| c.is_ascii_digit() || c == '-' || c == '.'))
    {
        let malformed = || {
            Error::Invalid(format!(
                "malformed line range `{range}`: use A, A-B or A..B"
            ))
        };
        let (a, b): (u32, u32) = match range.split_once("..").or_else(|| range.split_once('-')) {
            Some((a, b)) => (
                a.parse().map_err(|_| malformed())?,
                b.parse().map_err(|_| malformed())?,
            ),
            None => {
                let a = range.parse().map_err(|_| malformed())?;
                (a, a)
            }
        };
        {
            if a == 0 {
                return Err(Error::Invalid("line ranges start at 1".into()));
            }
            if a > b {
                return Err(Error::Invalid(format!(
                    "range {a}-{b}: the first line must not be after the last"
                )));
            }
            return Ok((PathBuf::from(path), Some((a, b))));
        }
    }
    Ok((PathBuf::from(spec), None))
}

/// Path of the `.tiza` that sits next to a source file (`src/main.rs` → `src/main.rs.tiza`).
pub fn sidecar_path(source: &Path) -> PathBuf {
    let mut s = source.as_os_str().to_owned();
    s.push(".");
    s.push(EXT);
    PathBuf::from(s)
}

pub fn is_document_path(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some(EXT)
}

/// Result of opening something: the document and where it will be saved.
#[derive(Debug, Clone)]
pub struct Opened {
    pub document: Document,
    pub save_path: PathBuf,
    /// true if the document was loaded from an existing `.tiza`.
    pub existing: bool,
    /// Something worth telling the user (e.g. a range clamped to the file's end).
    pub notice: Option<String>,
}

/// Opens a `.tiza` or a source file (with an optional range). If the source
/// has a `.tiza` sidecar next to it, the sidecar is preferred (it keeps the annotations).
/// One line naming what of the document Tiza does not draw as the file
/// says: the annotations whose lines fall outside the code (they are kept
/// in the file but cannot be drawn), and what this version does not know
/// ([`not_known_notice`]), or `None`.
pub fn open_notice(doc: &Document) -> Option<String> {
    let notices = [outside_annotations(doc), not_known_notice(doc)];
    let notices: Vec<String> = notices.into_iter().flatten().collect();
    (!notices.is_empty()).then(|| notices.join("; "))
}

/// What the file holds that this version does not know, as a newer Tiza
/// can write it, or `None`: the effects (the annotation comes in with no
/// effect), the moves that Tiza skips and the values of moves that fall
/// back to their default. One line that names each one, for stderr and
/// `tiza render`.
pub fn not_known_notice(doc: &Document) -> Option<String> {
    let notices = [unknown_effects_notice(doc), newer_moves_notice(doc)];
    let notices: Vec<String> = notices.into_iter().flatten().collect();
    (!notices.is_empty()).then(|| notices.join("; "))
}

/// [`not_known_notice`] in a few words, for the status bar: how many of
/// each, and where the names are.
pub fn not_known_short(doc: &Document) -> Option<String> {
    let counts = [
        (doc.unknown_effects.0.len(), "effect"),
        (doc.newer_moves.skipped.len(), "move"),
        (doc.newer_moves.defaults.len(), "value"),
    ];
    let parts: Vec<String> = counts
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, what)| format!("{n} {what}{}", if *n == 1 { "" } else { "s" }))
        .collect();
    let total: usize = counts.iter().map(|(n, _)| n).sum();
    let verb = if total == 1 { "is" } else { "are" };
    match parts.as_slice() {
        [] => None,
        [one] => Some(format!("{one} {verb} not known: see tiza check")),
        [rest @ .., last] => Some(format!(
            "{} and {last} {verb} not known: see tiza check",
            rest.join(", ")
        )),
    }
}

/// The moves that Tiza skips and the values that fall back to their
/// default ([`take_newer_moves`]), or `None`.
fn newer_moves_notice(doc: &Document) -> Option<String> {
    let found = &doc.newer_moves;
    let mut notices = Vec::new();
    if !found.skipped.is_empty() {
        let list: Vec<String> = found.skipped.iter().map(skipped_move_text).collect();
        let n = found.skipped.len();
        notices.push(format!(
            "Tiza skips {n} move{} that this version does not know, and a save does not keep {}: {}",
            if n == 1 { "" } else { "s" },
            if n == 1 { "it" } else { "them" },
            list.join(", ")
        ));
    }
    if !found.defaults.is_empty() {
        let list: Vec<String> = found
            .defaults
            .iter()
            .map(|d| {
                format!(
                    "move #{} ({}{} {} → {})",
                    d.number,
                    step_text(d.at_step),
                    d.field,
                    d.value,
                    d.default
                )
            })
            .collect();
        let n = found.defaults.len();
        notices.push(format!(
            "{n} value{} of moves {} not known to this version of Tiza, so the move uses the default, and a save does not keep {}: {}",
            if n == 1 { "" } else { "s" },
            if n == 1 { "is" } else { "are" },
            if n == 1 { "it" } else { "them" },
            list.join(", ")
        ));
    }
    (!notices.is_empty()).then(|| notices.join("; "))
}

/// `move #3 (step 2, action "teleport")`.
fn skipped_move_text(m: &crate::model::SkippedMove) -> String {
    format!(
        "move #{} ({}{} {})",
        m.number,
        step_text(m.at_step),
        m.field,
        m.value
    )
}

/// `step 2, `, or nothing when the move has no step that is a number.
fn step_text(at_step: Option<u64>) -> String {
    at_step.map(|s| format!("step {s}, ")).unwrap_or_default()
}

/// The moves with keys that this version does not know, or `None`: one
/// line that names each one, for `tiza scene put`, which refuses them as
/// the socket does. A file opens without them, and `tiza check` warns.
pub fn unknown_keys_notice(doc: &Document) -> Option<String> {
    let found = &doc.newer_moves.keys;
    if found.is_empty() {
        return None;
    }
    let list: Vec<String> = found
        .iter()
        .map(|k| {
            format!(
                "move #{} ({}{} {})",
                k.number,
                step_text(k.at_step),
                if k.keys.len() == 1 { "key" } else { "keys" },
                k.keys.join(", ")
            )
        })
        .collect();
    let n = found.len();
    Some(format!(
        "{n} move{} {} a key that this version of Tiza does not know: {}",
        if n == 1 { "" } else { "s" },
        if n == 1 { "has" } else { "have" },
        list.join(", ")
    ))
}

/// The annotations whose `effect` this version does not know, or `None`:
/// one line that names each one.
fn unknown_effects_notice(doc: &Document) -> Option<String> {
    let found = &doc.unknown_effects.0;
    if found.is_empty() {
        return None;
    }
    let list: Vec<String> = found.iter().map(|(id, v)| format!("{id} ({v})")).collect();
    let (n, one) = (found.len(), found.len() == 1);
    Some(format!(
        "{n} annotation{} {} an effect that this version of Tiza does not know, so {} with no effect: {}",
        if one { "" } else { "s" },
        if one { "has" } else { "have" },
        if one { "it comes in" } else { "they come in" },
        list.join(", ")
    ))
}

/// The annotations outside the code, or `None`.
fn outside_annotations(doc: &Document) -> Option<String> {
    let bad: Vec<String> = doc
        .annotations
        .iter()
        .filter(|a| doc.validate_annotation(a).is_err())
        .map(|a| match &a.file {
            // Each with the lines of its own file.
            Some(f) => {
                let n = doc.view_code.get(f).map_or(0, Vec::len);
                format!("{} ({f}, lines 1-{n})", a.id)
            }
            None => format!(
                "{} (lines {}-{})",
                a.id,
                doc.base_first(),
                doc.base_first() + doc.base().len().saturating_sub(1) as u32
            ),
        })
        .collect();
    if bad.is_empty() {
        return None;
    }
    Some(format!(
        "{} annotation{} outside the code and not drawn: {}",
        bad.len(),
        if bad.len() == 1 { "" } else { "s" },
        bad.join(", ")
    ))
}

pub fn open(spec: &str) -> Result<Opened, Error> {
    open_with(spec, true)
}

/// [`open`] where a range shows only its lines, without the document of
/// that range: `tiza scene put --as-code`.
pub fn open_range_as_code(spec: &str) -> Result<Opened, Error> {
    open_with(spec, false)
}

/// [`parse_target`] for a file in `base` (the current folder when `None`):
/// a name that is a file there stays a file name, also with a `:` and a
/// digit in it (`notes 12:00.md`).
pub fn parse_target_in(
    spec: &str,
    base: Option<&Path>,
) -> Result<(PathBuf, Option<(u32, u32)>), Error> {
    let path = Path::new(spec);
    let full = match base {
        Some(b) if path.is_relative() => b.join(path),
        _ => path.to_path_buf(),
    };
    if full.is_file() {
        return Ok((path.to_path_buf(), None));
    }
    parse_target(spec)
}

/// `range_document`: open the `FILE.A-B.tiza` of a range when it exists.
fn open_with(spec: &str, range_document: bool) -> Result<Opened, Error> {
    let (path, asked) = parse_target_in(spec, None)?;
    if is_document_path(&path) {
        let document = load_document(&path)?;
        let notice = open_notice(&document);
        return Ok(Opened {
            document,
            save_path: path,
            existing: true,
            notice,
        });
    }
    let sidecar = sidecar_path(&path);
    if asked.is_none() && sidecar.exists() {
        let document = load_document(&sidecar)?;
        let notice = open_notice(&document);
        return Ok(Opened {
            document,
            save_path: sidecar,
            existing: true,
            notice,
        });
    }
    let text = std::fs::read_to_string(&path)?;
    let mut document = Document::from_code(&text, "", Some(path.to_string_lossy().into_owned()));
    document.source.language =
        detect_language(Some(&path.to_string_lossy()), &document.code).language;
    let mut notice = None;
    let range = asked.map(|(a, b)| {
        let last = document.code.len() as u32;
        if a > last || b > last {
            notice = Some(format!(
                "the file has {last} line{}: range {a}-{b} clamped to {}-{}",
                if last == 1 { "" } else { "s" },
                a.min(last.max(1)),
                b.min(last.max(1))
            ));
        }
        let a = a.clamp(1, last.max(1));
        (a, b.clamp(a, last.max(1)))
    });
    if let Some((a, b)) = range {
        document.code = document.code[(a - 1) as usize..b as usize].to_vec();
        document.source.first_line = a;
    }
    // The sidecar lives next to the source: it references it by name, so it
    // always shows the current file and not a frozen copy (`:embed` freezes it).
    let file_name = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    document.reference_file(file_name, range);
    let save_path = match range {
        Some((a, b)) => {
            let mut s = path.as_os_str().to_owned();
            s.push(format!(".{a}-{b}.{EXT}"));
            PathBuf::from(s)
        }
        None => sidecar,
    };
    // The document of this range from an earlier session, found under the
    // clamped lines: opened as the sidecar is without a range, never
    // written over. One that does not load is an error, as a sidecar.
    if range_document && range.is_some() && save_path.exists() {
        // The error names that document: the user gave only `FILE:A-B`.
        let document = load_document(&save_path).map_err(|e| Error::Within {
            what: save_path.display().to_string(),
            inner: Box::new(e),
        })?;
        let opened = format!("opened the existing {}", save_path.display());
        let notice = [notice, Some(opened), open_notice(&document)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("; ");
        return Ok(Opened {
            document,
            save_path,
            existing: true,
            notice: Some(notice),
        });
    }
    Ok(Opened {
        document,
        save_path,
        existing: false,
        notice,
    })
}

/// Opens what came through stdin (`-`): a `.tiza` document when the text
/// is JSON with a `schema` field, otherwise code in `language` (`None`:
/// detected from a shebang or modeline, else plain text).
pub fn from_stdin(text: &str, language: Option<&str>) -> Result<Opened, Error> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.starts_with('{') && trimmed.contains("\"schema\"") {
        let document = parse_document(text, std::env::current_dir().ok().as_deref())?;
        let notice = open_notice(&document);
        return Ok(Opened {
            document,
            save_path: PathBuf::from(format!("tiza-stdin.{EXT}")),
            existing: true,
            notice,
        });
    }
    if text.trim().is_empty() {
        return Err(Error::Invalid("empty stdin".into()));
    }
    Ok(from_stdin_text(text, language))
}

/// Creates a document from text read on stdin (no path).
pub fn from_stdin_text(text: &str, language: Option<&str>) -> Opened {
    let mut document = Document::from_code(text, language.unwrap_or(""), None);
    if language.is_none() {
        document.source.language = detect_language(None, &document.code).language;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Opened {
        document,
        save_path: PathBuf::from(format!("tiza-{stamp}.{EXT}")),
        existing: false,
        notice: None,
    }
}

pub fn load_document(path: &Path) -> Result<Document, Error> {
    let text = std::fs::read_to_string(path)?;
    parse_document(&text, path.parent())
}

/// Checks the `schema` of a document as JSON, a file or a `scene.put`: a
/// newer one is refused, and an older one is migrated step by step to the
/// current schema, in place.
pub fn migrate_document_value(raw: &mut serde_json::Value) -> Result<(), Error> {
    let found = raw.get("schema").and_then(|v| v.as_u64()).unwrap_or(1);
    if found > u64::from(SCHEMA_VERSION) {
        return Err(Error::UnsupportedSchema {
            found,
            supported: SCHEMA_VERSION,
        });
    }
    for (from, migrate) in MIGRATIONS {
        if found <= u64::from(*from) {
            migrate(raw);
        }
    }
    Ok(())
}

/// Parses a `.tiza` and resolves its code. `base_dir` is the document's
/// directory, against which relative `code.file` paths are resolved.
pub fn parse_document(text: &str, base_dir: Option<&Path>) -> Result<Document, Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut raw: serde_json::Value = serde_json::from_str(text)?;
    migrate_document_value(&mut raw)?;
    let unknown = take_unknown_effects(&mut raw);
    let newer = take_newer_moves(&mut raw);
    let mut doc: Document = serde_json::from_value(raw)?;
    doc.unknown_effects = crate::model::UnknownEffects(unknown);
    doc.newer_moves = newer;
    doc.schema = SCHEMA_VERSION;
    resolve_code(&mut doc, base_dir)?;
    // A linked file that cannot be read is named by the document's own
    // `code.link`.
    load_views(&mut doc, base_dir)?;
    // The numbers and the rules of each move first, so that an error after
    // them is one of a move that does not apply.
    doc.check_bounds()?;
    crate::moves::validate_shapes(&doc, &doc.moves, &doc.annotations)?;
    doc.normalize_loaded()
        .map_err(|e| skipped_moves_hint(e, &doc.newer_moves.skipped))?;
    Ok(doc)
}

/// A move that does not apply after Tiza skipped a move: the skipped move
/// can change the code that the later move needs. The error names the
/// skipped moves before it too. The error of a move that does not apply
/// starts with `move #N, ` ([`crate::moves::validate`] and
/// [`crate::moves::name_move`]), with the number of the move in the file.
fn skipped_moves_hint(e: Error, skipped: &[crate::model::SkippedMove]) -> Error {
    let failed = match &e {
        Error::MoveNoMatch { index, .. } => Some(*index),
        e => e
            .to_string()
            .strip_prefix("move #")
            .and_then(|rest| rest.split([',', ':']).next())
            .and_then(|n| n.parse().ok()),
    };
    let Some(failed) = failed else {
        return e;
    };
    let before: Vec<&crate::model::SkippedMove> =
        skipped.iter().filter(|m| m.number < failed).collect();
    if before.is_empty() {
        return e;
    }
    let list: Vec<String> = before.iter().map(|m| skipped_move_text(m)).collect();
    let one = before.len() == 1;
    Error::Because {
        message: format!(
            "{e}. Tiza skips {}, which this version does not know, and a later move can need the code that {} {}",
            list.join(", "),
            if one { "it" } else { "they" },
            if one { "changes" } else { "change" },
        ),
        inner: Box::new(e),
    }
}

/// Takes out of the moves of `raw` what this version does not know, as a
/// newer Tiza can write it ([`crate::model::NewerMoves`]):
/// - a move whose `action` is a text that this version does not know is
///   taken out of the list (a move with no `action`, or one that is not a
///   text, is a broken file and stays, for serde to refuse);
/// - a value of a field with a list of values that this version does not
///   know ([`crate::moves::LIST_FIELDS`]) is taken out, so that the field
///   gets its default; with no default, the move is taken out;
/// - a key that this version does not know is only noted: serde ignores
///   it.
pub(crate) fn take_newer_moves(raw: &mut serde_json::Value) -> crate::model::NewerMoves {
    use crate::model::{DefaultValue, SkippedMove, UnknownKeys};
    use crate::moves::{ACTION_KEYS, LIST_FIELDS, ListField, action_keys};
    let mut out = crate::model::NewerMoves::default();
    let Some(moves) = raw.get_mut("moves").and_then(|m| m.as_array_mut()) else {
        return out;
    };
    let mut kept = Vec::with_capacity(moves.len());
    for (i, m) in std::mem::take(moves).into_iter().enumerate() {
        let number = i + 1;
        let serde_json::Value::Object(mut o) = m else {
            kept.push(m);
            continue;
        };
        let at_step = o.get("at_step").and_then(|v| v.as_u64());
        let Some(action) = o.get("action").and_then(|v| v.as_str()).map(str::to_string) else {
            kept.push(serde_json::Value::Object(o));
            continue;
        };
        if action_keys(&action).is_none() {
            out.skipped.push(SkippedMove {
                number,
                at_step,
                field: "action",
                value: shown_value(&o["action"]),
                known: ACTION_KEYS.iter().map(|(name, _)| *name).collect(),
            });
            continue;
        }
        let fields = || LIST_FIELDS.iter().filter(|f| f.action == action);
        let not_known = |o: &serde_json::Map<String, serde_json::Value>, f: &ListField| {
            o.get(f.key)
                .is_some_and(|v| !v.as_str().is_some_and(|s| f.values.contains(&s)))
        };
        if let Some(f) = fields().find(|f| f.default.is_none() && not_known(&o, f)) {
            out.skipped.push(SkippedMove {
                number,
                at_step,
                field: f.key,
                value: shown_value(&o[f.key]),
                known: f.values.to_vec(),
            });
            continue;
        }
        for f in fields() {
            let Some(default) = f.default else {
                continue;
            };
            if not_known(&o, f) {
                let value = o.remove(f.key).unwrap_or_default();
                out.defaults.push(DefaultValue {
                    number,
                    at_step,
                    field: f.key,
                    value: shown_value(&value),
                    default,
                    known: f.values,
                });
            }
        }
        let unknown = crate::moves::unknown_keys(&o, &action);
        if !unknown.is_empty() {
            out.keys.push(UnknownKeys {
                number,
                at_step,
                action,
                keys: unknown
                    .iter()
                    .map(|k| crate::printable::printable_line(&crate::moves::cut(k)).into_owned())
                    .collect(),
            });
        }
        kept.push(serde_json::Value::Object(o));
    }
    *moves = kept;
    out.gaps = out.skipped.iter().map(|m| m.number).collect();
    out
}

/// A value as a file writes it, for a message: JSON, cut when it is long
/// (an object, a long text).
fn shown_value(v: &serde_json::Value) -> String {
    crate::moves::cut(&v.to_string())
}

/// Takes out of the annotations of `raw` an `effect` that this version
/// does not know: a newer Tiza can write an effect that this one cannot
/// draw. The document opens, and the annotation comes in with no effect
/// ([`crate::model::UnknownEffects`]). Returns (the id of the annotation,
/// the value as written).
pub(crate) fn take_unknown_effects(raw: &mut serde_json::Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(annotations) = raw.get_mut("annotations").and_then(|a| a.as_array_mut()) else {
        return out;
    };
    for a in annotations.iter_mut().filter_map(|a| a.as_object_mut()) {
        let Some(value) = a.get("effect") else {
            continue;
        };
        if value.is_null()
            || serde_json::from_value::<crate::effects::Effect>(value.clone()).is_ok()
        {
            continue;
        }
        let value = a.remove("effect").unwrap_or_default();
        let id = a
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.push((id, shown_value(&value)));
    }
    out
}

/// Readies the document a live instance handed out (`scene.get`) for an
/// export, as [`parse_document`] readies a `.tiza`: its main code
/// and the code of every file a `code.link` move names come embedded in it
/// (the instance's own copy, [`Document::embedded_copy`]), so the export
/// draws the files the live
/// TUI shows, never what this process would read from its own directory.
/// A linked file the instance did not send is an error naming it, never a
/// guess from disk.
pub fn load_scene(doc: &mut Document) -> Result<(), Error> {
    resolve_code(doc, None)?;
    for (i, m) in doc.moves.iter().enumerate() {
        let crate::moves::MoveAction::CodeLink(l) = &m.action else {
            continue;
        };
        let Some(key) = doc.view_key(&l.file) else {
            continue; // the main file: always sent
        };
        if !doc.views.contains_key(&key) && !doc.views.contains_key(&l.file) {
            return Err(Error::Invalid(format!(
                "move #{}, step {}: code.link «{}»: the live instance did not send its code (open the document again in Tiza, or render the .tiza file)",
                i + 1,
                m.at_step,
                l.file
            )));
        }
    }
    // Every view comes from `views` now: nothing is read from disk.
    load_views(doc, None)?;
    doc.normalize_loaded()
}

/// Absolute path of a file a `code.link` names, resolved like `code.file`:
/// against `base_dir` (the document's directory) when relative.
pub fn view_path(file: &str, base_dir: Option<&Path>) -> PathBuf {
    let raw = Path::new(file);
    let full = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        base_dir
            .filter(|d| !d.as_os_str().is_empty())
            .map(|d| d.join(raw))
            .unwrap_or_else(|| raw.to_path_buf())
    };
    std::path::absolute(&full).unwrap_or(full)
}

/// Whether two paths name one file: the same path once `.`/`..` are
/// resolved, the same canonical path, or (Unix) the same device and inode
/// — a symlink, a hard link, or another case of the name on a
/// case-insensitive disk (APFS).
fn same_file(a: &Path, b: &Path) -> bool {
    let lexical = |p: &Path| Document::normalize_file(&p.to_string_lossy());
    if lexical(a) == lexical(b) {
        return true;
    }
    if let (Ok(x), Ok(y)) = (std::fs::canonicalize(a), std::fs::canonicalize(b))
        && x == y
    {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(x), Ok(y)) = (std::fs::metadata(a), std::fs::metadata(b)) {
            return x.dev() == y.dev() && x.ino() == y.ino();
        }
    }
    false
}

/// The name `file` should go by in `doc`: an alias of the main file or of
/// a file in `known` (`../dir/app.env`, its absolute path, a symlink,
/// `APP.env` on a case-insensitive disk) becomes that file's own name, so
/// it is that file's view, with its annotations — never a second view of
/// the same file. Otherwise `file` normalized.
pub fn canonical_link(
    doc: &Document,
    file: &str,
    base_dir: Option<&Path>,
    known: &[String],
) -> String {
    let f = Document::normalize_file(file);
    if doc.view_key(&f).is_none() {
        return f;
    }
    let target = view_path(&f, base_dir);
    if let CodeSource::File { file: main, .. } = &doc.code_src
        && let Some(path) = source_file(doc, base_dir)
        && same_file(&target, &path)
    {
        return main.clone();
    }
    if let Some(p) = &doc.source.path
        && same_file(&target, &view_path(p, base_dir))
    {
        return p.clone();
    }
    known
        .iter()
        .find(|k| **k != f && same_file(&target, &view_path(k, base_dir)))
        .cloned()
        .unwrap_or(f)
}

/// Every `code.link` of `doc`, and every annotation's `file`, by the name
/// [`canonical_link`] gives it: the first name a file goes by wins.
pub fn canonicalize_links(doc: &mut Document, base_dir: Option<&Path>) {
    let mut known: Vec<String> = doc.views.keys().cloned().collect();
    known.extend(doc.view_code.keys().cloned());
    for i in 0..doc.moves.len() {
        let crate::moves::MoveAction::CodeLink(l) = &doc.moves[i].action else {
            continue;
        };
        let name = canonical_link(doc, &l.file, base_dir, &known);
        if let Some(k) = doc.view_key(&name)
            && !known.contains(&k)
        {
            known.push(k);
        }
        if let crate::moves::MoveAction::CodeLink(l) = &mut doc.moves[i].action {
            l.file = name;
        }
    }
    for i in 0..doc.annotations.len() {
        if let Some(f) = doc.annotations[i].file.clone() {
            let name = canonical_link(doc, &f, base_dir, &known);
            doc.annotations[i].file = doc.view_key(&name);
        }
    }
}

/// Loads the code of every file the document's `code.link` moves name:
/// from the embedded `views` when present, else from disk next to
/// the document. A missing file is an error naming the move and the path.
/// Aliases of one file are one view first ([`canonicalize_links`]).
pub fn load_views(doc: &mut Document, base_dir: Option<&Path>) -> Result<(), Error> {
    canonicalize_links(doc, base_dir);
    let links: Vec<(usize, u32, String)> = doc
        .moves
        .iter()
        .enumerate()
        .filter_map(|(i, m)| match &m.action {
            crate::moves::MoveAction::CodeLink(l) => Some((i, m.at_step, l.file.clone())),
            _ => None,
        })
        .collect();
    for (i, at_step, file) in links {
        let Some(key) = doc.view_key(&file) else {
            continue; // the main file: always loaded
        };
        if doc.view_code.contains_key(&key) {
            continue;
        }
        if let Some(code) = doc.views.get(&key).or_else(|| doc.views.get(&file)) {
            let code = if code.is_empty() {
                vec![String::new()]
            } else {
                code.clone()
            };
            doc.view_code.insert(key, code);
            continue;
        }
        let code = read_view_file(&file, base_dir).map_err(|e| {
            let n = doc.move_number(i);
            Error::Invalid(format!("move #{n}, step {at_step}: {e}"))
        })?;
        doc.view_code.insert(key, code);
    }
    Ok(())
}

/// Reads one linked file (whole) for `doc`, adding it to the loaded views.
/// A no-op when the file is the main file or is already loaded.
pub fn load_view(doc: &mut Document, file: &str, base_dir: Option<&Path>) -> Result<(), Error> {
    let Some(key) = doc.view_key(file) else {
        return Ok(());
    };
    if doc.view_code.contains_key(&key) {
        return Ok(());
    }
    let code = read_view_file(file, base_dir)?;
    doc.view_code.insert(key, code);
    Ok(())
}

/// The code of a file a `code.link` names, read whole from disk (`file`
/// resolved like `code.file`, against `base_dir`).
pub fn read_view_file(file: &str, base_dir: Option<&Path>) -> Result<Vec<String>, Error> {
    let full = view_path(file, base_dir);
    let text = std::fs::read_to_string(&full).map_err(|e| {
        Error::Invalid(format!(
            "code.link «{file}»: could not read «{}» ({e})",
            full.display()
        ))
    })?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
    if lines.is_empty() {
        lines.push(String::new());
    }
    Ok(lines)
}

/// Migrations between schema versions: `(from, f)` turns a schema-`from`
/// JSON value into schema `from + 1`. Applied in order on load; files are
/// written in the current schema. Every bump of `SCHEMA_VERSION` adds one
/// entry here and a fixture under `qa/fixtures/compat/` (a QA test checks both).
pub const MIGRATIONS: &[(u32, Migration)] = &[];

/// A function that rewrites a raw document of one schema into the next.
pub type Migration = fn(&mut serde_json::Value);

/// Absolute path of the file a document's code comes from, when it is a
/// file reference (`code: {file, lines}`); `None` for embedded code.
/// Relative references resolve against `base_dir` (the document's directory).
pub fn source_file(doc: &Document, base_dir: Option<&Path>) -> Option<PathBuf> {
    code_source_file(&doc.code_src, base_dir)
}

/// [`source_file`] of a code source.
pub fn code_source_file(src: &CodeSource, base_dir: Option<&Path>) -> Option<PathBuf> {
    match src {
        CodeSource::Embedded(_) => None,
        CodeSource::File { file, .. } => {
            let raw = Path::new(file);
            let full = if raw.is_absolute() {
                raw.to_path_buf()
            } else {
                base_dir
                    .filter(|d| !d.as_os_str().is_empty())
                    .map(|d| d.join(raw))
                    .unwrap_or_else(|| raw.to_path_buf())
            };
            Some(std::path::absolute(&full).unwrap_or(full))
        }
    }
}

/// Re-reads the referenced code of `doc` (honouring its line range) and
/// returns the new lines without touching `doc`. Errors say what happened
/// (missing file, not UTF-8, permission) in one sentence.
pub fn reread_code(doc: &Document, base_dir: Option<&Path>) -> Result<Vec<String>, Error> {
    reread_reference(doc, base_dir).map(|(code, _)| code)
}

/// [`reread_code`] with the first line the range starts at.
pub fn reread_reference(
    doc: &Document,
    base_dir: Option<&Path>,
) -> Result<(Vec<String>, u32), Error> {
    let mut copy = doc.clone();
    resolve_code(&mut copy, base_dir)?;
    Ok((copy.code, copy.source.first_line))
}

/// Fills `doc.code` from `doc.code_src`. The result always has at least one line.
pub fn resolve_code(doc: &mut Document, base_dir: Option<&Path>) -> Result<(), Error> {
    resolve_code_inner(doc, base_dir)?;
    if doc.code.is_empty() {
        doc.code.push(String::new());
    }
    Ok(())
}

fn resolve_code_inner(doc: &mut Document, base_dir: Option<&Path>) -> Result<(), Error> {
    match &doc.code_src {
        CodeSource::Embedded(lines) => {
            doc.code = lines.clone();
        }
        CodeSource::File { file, lines } => {
            let raw = Path::new(file);
            let full = if raw.is_absolute() {
                raw.to_path_buf()
            } else {
                base_dir
                    .filter(|d| !d.as_os_str().is_empty())
                    .map(|d| d.join(raw))
                    .unwrap_or_else(|| raw.to_path_buf())
            };
            let text = std::fs::read_to_string(&full).map_err(|e| {
                Error::Invalid(format!(
                    "could not read the referenced code at {} ({e})",
                    full.display()
                ))
            })?;
            // A byte order mark that starts the file is not code, as when
            // the file was first opened ([`Document::from_code`]).
            let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
            let mut all: Vec<String> = text.lines().map(|l| l.to_string()).collect();
            let mut first = 1;
            if let Some(spec) = lines {
                let (a, b) = parse_lines(spec)?;
                let last = all.len() as u32;
                let a = a.clamp(1, last.max(1));
                let b = b.clamp(a, last.max(1));
                all = all[(a - 1) as usize..b as usize].to_vec();
                first = a;
            }
            doc.code = all;
            doc.source.first_line = first;
            if doc.source.path.is_none() {
                doc.source.path = Some(full.to_string_lossy().into_owned());
            }
            if doc.source.language.is_empty() {
                doc.source.language =
                    detect_language(Some(&full.to_string_lossy()), &doc.code).language;
            }
        }
    }
    Ok(())
}

/// `"12-48"`, `"12..48"` or `"7"` → (12, 48).
fn parse_lines(spec: &str) -> Result<(u32, u32), Error> {
    parse_range(spec, "code.lines")
}

/// [`parse_lines`] for any field (`follows.lines`), named in the error.
pub(crate) fn parse_range(spec: &str, field: &str) -> Result<(u32, u32), Error> {
    let s = spec.trim();
    let parts: Vec<&str> = if s.contains("..") {
        s.split("..").collect()
    } else {
        s.split('-').collect()
    };
    let nums: Result<Vec<u32>, _> = parts.iter().map(|p| p.trim().parse::<u32>()).collect();
    match nums
        .map_err(|_| Error::Invalid(format!("invalid line range in {field}: {spec}")))?
        .as_slice()
    {
        [a] => Ok((*a, *a)),
        [a, b] => Ok((*a.min(b), *a.max(b))),
        _ => Err(Error::Invalid(format!(
            "invalid line range in {field}: {spec}"
        ))),
    }
}

/// The folder of a `.tiza` saved at `path`, absolute, symlinks resolved
/// (as the system resolves a `..` from it). A folder that does not exist
/// yet (a save creates it) is its nearest existing ancestor, resolved, and
/// the rest of the path, whose `..` can only mean the folder above.
pub fn document_dir(path: &Path) -> PathBuf {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let abs = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    for base in abs.ancestors() {
        let Ok(mut out) = base.canonicalize() else {
            continue;
        };
        let rest = abs.strip_prefix(base).unwrap_or(Path::new(""));
        for c in rest.components() {
            match c {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                std::path::Component::CurDir => {}
                c => out.push(c.as_os_str()),
            }
        }
        return out;
    }
    abs
}

/// `.` and `..` of an absolute path resolved by the text alone.
fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            c => out.push(c.as_os_str()),
        }
    }
    out
}

/// `file` as a `.tiza` in the folder `to` names the file that one in `from`
/// names `file` (both folders as [`document_dir`] gives them): relative
/// (`../main.rs`), or absolute when the two folders share only the root
/// (another volume). An absolute `file`, or the same folder, keeps it.
pub fn rebase_path(file: &str, from: &Path, to: &Path) -> String {
    if Path::new(file).is_absolute() || from == to {
        return file.to_string();
    }
    let target = normalize(&from.join(file));
    let t: Vec<_> = target.components().collect();
    let d: Vec<_> = to.components().collect();
    let common = t.iter().zip(&d).take_while(|(a, b)| a == b).count();
    if common <= 1 {
        return target.to_string_lossy().into_owned();
    }
    let mut rel = PathBuf::new();
    for _ in common..d.len() {
        rel.push("..");
    }
    for c in &t[common..] {
        rel.push(c.as_os_str());
    }
    rel.to_string_lossy().into_owned()
}

/// The one rewriting of `doc`'s paths for a `.tiza` moving from the folder
/// `from` to `to` ([`rebase_path`]), to apply to the document and its undo
/// history alike. Which link is the main file is decided by names, loosely
/// (a bare `main.rs` names the main file): a link whose new form would
/// cross that line, either way (`../client/main.rs` becoming `main.rs` next
/// to a main file also called `main.rs`), is written absolute instead.
pub fn rebase_fn(doc: &Document, from: &Path, to: &Path) -> impl Fn(&str) -> String + use<> {
    let (from, to) = (from.to_path_buf(), to.to_path_buf());
    let mut moved = doc.clone();
    moved.map_files(&|s| rebase_path(s, &from, &to));
    let links = |d: &Document| -> Vec<String> {
        d.moves
            .iter()
            .filter_map(|m| match &m.action {
                crate::moves::MoveAction::CodeLink(l) => Some(l.file.clone()),
                _ => None,
            })
            .collect()
    };
    let absolute: std::collections::BTreeSet<String> = links(doc)
        .into_iter()
        .zip(links(&moved))
        .filter(|(old, new)| doc.view_key(old).is_none() != moved.view_key(new).is_none())
        .map(|(old, _)| old)
        .collect();
    move |s: &str| {
        if absolute.contains(s) && !Path::new(s).is_absolute() {
            normalize(&from.join(s)).to_string_lossy().into_owned()
        } else {
            rebase_path(s, &from, &to)
        }
    }
}

/// The text [`save_document`] writes for `doc`.
pub fn document_json(doc: &Document) -> Result<String, Error> {
    let mut doc = doc.clone();
    doc.sync_code_src();
    // `follows` is written with embedded code only.
    if !doc.is_embedded() {
        doc.follows = None;
    }
    Ok(serde_json::to_string_pretty(&doc)?)
}

/// Saves atomically (tmp + rename), as editors do.
pub fn save_document(doc: &Document, path: &Path) -> Result<(), Error> {
    let json = document_json(doc)?;
    // The save is an atomic replace (temp file + rename), which would walk
    // over a file the user made read-only: honour the bit instead.
    if let Ok(meta) = std::fs::metadata(path)
        && meta.permissions().readonly()
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("{} is read-only", path.display()),
        )
        .into());
    }
    let tmp = path.with_extension(format!("{EXT}.tmp"));
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&tmp, json.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ranges() {
        let ok = |s: &str| parse_target(s).unwrap();
        assert_eq!(ok("src/main.rs"), (PathBuf::from("src/main.rs"), None));
        assert_eq!(
            ok("src/main.rs:12-48"),
            (PathBuf::from("src/main.rs"), Some((12, 48)))
        );
        assert_eq!(
            ok("src/main.rs:12..48"),
            (PathBuf::from("src/main.rs"), Some((12, 48)))
        );
        assert_eq!(
            ok("src/main.rs:7"),
            (PathBuf::from("src/main.rs"), Some((7, 7)))
        );
        assert_eq!(ok("C:thing"), (PathBuf::from("C:thing"), None));
        assert_eq!(ok("a:b.rs"), (PathBuf::from("a:b.rs"), None));
        assert_eq!(ok("main.rs:"), (PathBuf::from("main.rs:"), None));
        // Text that starts with a digit is a range, never a part of the name.
        let err = |s: &str| parse_target(s).unwrap_err().to_string();
        assert!(err("main.rs:3-x").contains("malformed line range `3-x`"));
        assert!(err("main.rs:1e3").contains("malformed line range `1e3`"));
        assert!(err("main.rs:9-3").contains("range 9-3"));
        assert!(err("main.rs:0-2").contains("start at 1"));
        assert!(err("main.rs:-5").contains("malformed line range `-5`"));
    }

    #[test]
    fn sidecar_name() {
        assert_eq!(
            sidecar_path(Path::new("src/main.rs")),
            PathBuf::from("src/main.rs.tiza")
        );
    }

    #[test]
    fn open_range_and_save_roundtrip() {
        let dir = std::env::temp_dir().join(format!("tiza-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("x.rs");
        std::fs::write(&src, "l1\nl2\nl3\nl4\n").unwrap();
        let o = open(&format!("{}:2-3", src.display())).unwrap();
        assert_eq!(o.document.code, vec!["l2", "l3"]);
        assert_eq!(o.document.source.first_line, 2);
        assert!(o.save_path.to_string_lossy().ends_with("x.rs.2-3.tiza"));
        save_document(&o.document, &o.save_path).unwrap();
        let json = std::fs::read_to_string(&o.save_path).unwrap();
        assert!(
            json.contains("\"file\": \"x.rs\""),
            "the sidecar references the source: {json}"
        );
        assert!(json.contains("\"lines\": \"2-3\""));
        let back = load_document(&o.save_path).unwrap();
        assert_eq!(back, o.document);
        // If the source changes, the referencing document reflects it.
        std::fs::write(&src, "l1\nL2 new\nl3\nl4\n").unwrap();
        let again = load_document(&o.save_path).unwrap();
        assert_eq!(again.code, vec!["L2 new", "l3"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn code_object_absolute_relative_and_embed() {
        let dir = std::env::temp_dir().join(format!("tiza-ref-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let src = dir.join("src/lib.rs");
        std::fs::write(&src, "a\nb\nc\n").unwrap();
        // relative to the .tiza
        let doc_path = dir.join("notes.tiza");
        std::fs::write(
            &doc_path,
            r#"{"schema":1,"source":{"language":""},"code":{"file":"src/lib.rs","lines":"2-3"}}"#,
        )
        .unwrap();
        let d = load_document(&doc_path).unwrap();
        assert_eq!(d.code, vec!["b", "c"]);
        assert_eq!(d.source.first_line, 2);
        assert_eq!(d.source.language, "rs");
        // absolute, no range
        let json = format!(
            r#"{{"schema":1,"source":{{"language":"rs"}},"code":{{"file":"{}"}}}}"#,
            src.display()
        );
        let d = parse_document(&json, None).unwrap();
        assert_eq!(d.code.len(), 3);
        // embedded still works and embed_code freezes
        let mut d = parse_document(
            r#"{"schema":1,"source":{"language":"rs"},"code":["x","y"]}"#,
            None,
        )
        .unwrap();
        assert_eq!(d.code, vec!["x", "y"]);
        d.reference_file("nope.rs", None);
        d.embed_code();
        assert!(d.is_embedded());
        // nonexistent file → clear error
        let err = parse_document(
            r#"{"schema":1,"source":{"language":""},"code":{"file":"/no/existe.rs"}}"#,
            None,
        )
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("could not read the referenced code")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
