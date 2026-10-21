//! Open and save: source files (`main.rs`, `main.rs:12-48`) and `.tiza` documents.

use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::highlight::detect_language;
use crate::model::{CodeSource, Document, SCHEMA_VERSION};

pub const EXT: &str = "tiza";

/// Splits `path[:A[-B]]`. Returns the path and the line range (1-based, inclusive).
pub fn parse_target(spec: &str) -> (PathBuf, Option<(u32, u32)>) {
    match parse_target_checked(spec) {
        Ok(t) => t,
        Err(_) => (PathBuf::from(spec), None),
    }
}

/// Like [`parse_target`], but `A-B` with `A > B` or `A == 0` is an error
/// instead of being silently reordered. Accepts `A-B`, `A..B` and `A`.
pub fn parse_target_checked(spec: &str) -> Result<(PathBuf, Option<(u32, u32)>), Error> {
    if let Some((path, range)) = spec.rsplit_once(':')
        && !path.is_empty()
        && !range.is_empty()
        && range
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == '.')
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
/// One line naming the annotations whose lines fall outside the code (they
/// are kept in the file but cannot be drawn), or `None`.
pub fn outside_notice(doc: &Document) -> Option<String> {
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
    let (path, range) = parse_target_checked(spec)?;
    if is_document_path(&path) {
        let document = load_document(&path)?;
        let notice = outside_notice(&document);
        return Ok(Opened {
            document,
            save_path: path,
            existing: true,
            notice,
        });
    }
    let sidecar = sidecar_path(&path);
    if range.is_none() && sidecar.exists() {
        let document = load_document(&sidecar)?;
        let notice = outside_notice(&document);
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
    let range = range.map(|(a, b)| {
        let last = document.code.len() as u32;
        if a > last || b > last {
            notice = Some(format!(
                "the file has {last} lines: range {a}-{b} clamped to {}-{}",
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
        let notice = outside_notice(&document);
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

/// Parses a `.tiza` and resolves its code. `base_dir` is the document's
/// directory, against which relative `code.file` paths are resolved.
pub fn parse_document(text: &str, base_dir: Option<&Path>) -> Result<Document, Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut raw: serde_json::Value = serde_json::from_str(text)?;
    let found = raw.get("schema").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
    if found > SCHEMA_VERSION {
        return Err(Error::UnsupportedSchema {
            found,
            supported: SCHEMA_VERSION,
        });
    }
    // Older files are migrated step by step before being read as the current schema.
    for (from, migrate) in MIGRATIONS {
        if found <= *from {
            migrate(&mut raw);
        }
    }
    let mut doc: Document = serde_json::from_value(raw)?;
    doc.schema = SCHEMA_VERSION;
    resolve_code(&mut doc, base_dir)?;
    // A linked file that cannot be read is named by the document's own
    // `code.link` (t-166).
    load_views(&mut doc, base_dir)?;
    doc.normalize_loaded()?;
    Ok(doc)
}

/// Readies the document a live instance handed out (`scene.get`) for an
/// export, as [`parse_document`] readies a `.tiza` (t-166): its main code
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

/// Loads the code of every file the document's `code.link` moves name
/// (t-105): from the embedded `views` when present, else from disk next to
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
        let code = read_view_file(&file, base_dir)
            .map_err(|e| Error::Invalid(format!("move #{}, step {at_step}: {e}", i + 1)))?;
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
/// entry here and a fixture under `qa/fixtures/compat/` (t-073 checks both).
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
    let s = spec.trim();
    let parts: Vec<&str> = if s.contains("..") {
        s.split("..").collect()
    } else {
        s.split('-').collect()
    };
    let nums: Result<Vec<u32>, _> = parts.iter().map(|p| p.trim().parse::<u32>()).collect();
    match nums
        .map_err(|_| Error::Invalid(format!("invalid line range in code.lines: {spec}")))?
        .as_slice()
    {
        [a] => Ok((*a, *a)),
        [a, b] => Ok((*a.min(b), *a.max(b))),
        _ => Err(Error::Invalid(format!(
            "invalid line range in code.lines: {spec}"
        ))),
    }
}

/// The text [`save_document`] writes for `doc`.
pub fn document_json(doc: &Document) -> Result<String, Error> {
    let mut doc = doc.clone();
    doc.sync_code_src();
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
        assert_eq!(
            parse_target("src/main.rs"),
            (PathBuf::from("src/main.rs"), None)
        );
        assert_eq!(
            parse_target("src/main.rs:12-48"),
            (PathBuf::from("src/main.rs"), Some((12, 48)))
        );
        assert_eq!(
            parse_target("src/main.rs:7"),
            (PathBuf::from("src/main.rs"), Some((7, 7)))
        );
        assert_eq!(parse_target("C:thing"), (PathBuf::from("C:thing"), None));
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
