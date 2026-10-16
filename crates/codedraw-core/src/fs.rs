//! Abrir y guardar: archivos fuente (`main.rs`, `main.rs:12-48`) y documentos `.codedraw`.

use std::path::{Path, PathBuf};

use crate::error::Error;
use crate::highlight::language_from_path;
use crate::model::{Document, SCHEMA_VERSION};

pub const EXT: &str = "codedraw";

/// Separa `ruta[:A[-B]]`. Devuelve la ruta y el rango de líneas (1-based, inclusivo).
pub fn parse_target(spec: &str) -> (PathBuf, Option<(u32, u32)>) {
    if let Some((path, range)) = spec.rsplit_once(':')
        && !path.is_empty()
        && !range.is_empty()
        && range.chars().all(|c| c.is_ascii_digit() || c == '-')
    {
        let (a, b): (Option<u32>, Option<u32>) = match range.split_once('-') {
            Some((a, b)) => (a.parse().ok(), b.parse().ok()),
            None => (range.parse().ok(), None),
        };
        if let Some(a) = a {
            let b = b.unwrap_or(a);
            return (PathBuf::from(path), Some((a.min(b), a.max(b))));
        }
    }
    (PathBuf::from(spec), None)
}

/// Ruta del `.codedraw` que acompaña a un archivo fuente (`src/main.rs` → `src/main.rs.codedraw`).
pub fn sidecar_path(source: &Path) -> PathBuf {
    let mut s = source.as_os_str().to_owned();
    s.push(".");
    s.push(EXT);
    PathBuf::from(s)
}

pub fn is_document_path(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()) == Some(EXT)
}

/// Resultado de abrir algo: el documento y dónde se guardará.
#[derive(Debug, Clone)]
pub struct Opened {
    pub document: Document,
    pub save_path: PathBuf,
    /// true si el documento se cargó de un `.codedraw` existente.
    pub existing: bool,
}

/// Abre un `.codedraw` o un archivo fuente (con rango opcional). Si junto al
/// fuente existe su sidecar `.codedraw`, se prefiere este (conserva las anotaciones).
pub fn open(spec: &str) -> Result<Opened, Error> {
    let (path, range) = parse_target(spec);
    if is_document_path(&path) {
        let document = load_document(&path)?;
        return Ok(Opened {
            document,
            save_path: path,
            existing: true,
        });
    }
    let sidecar = sidecar_path(&path);
    if range.is_none() && sidecar.exists() {
        let document = load_document(&sidecar)?;
        return Ok(Opened {
            document,
            save_path: sidecar,
            existing: true,
        });
    }
    let text = std::fs::read_to_string(&path)?;
    let mut document = Document::from_code(
        &text,
        language_from_path(&path.to_string_lossy()),
        Some(path.to_string_lossy().into_owned()),
    );
    if let Some((a, b)) = range {
        let last = document.code.len() as u32;
        let a = a.clamp(1, last.max(1));
        let b = b.clamp(a, last.max(1));
        document.code = document.code[(a - 1) as usize..b as usize].to_vec();
        document.source.first_line = a;
    }
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
    })
}

/// Crea un documento a partir de texto leído por stdin (sin ruta).
pub fn from_stdin_text(text: &str, language: &str) -> Opened {
    let document = Document::from_code(text, language, None);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Opened {
        document,
        save_path: PathBuf::from(format!("codedraw-{stamp}.{EXT}")),
        existing: false,
    }
}

pub fn load_document(path: &Path) -> Result<Document, Error> {
    let text = std::fs::read_to_string(path)?;
    parse_document(&text)
}

pub fn parse_document(text: &str) -> Result<Document, Error> {
    let doc: Document = serde_json::from_str(text)?;
    if doc.schema > SCHEMA_VERSION {
        return Err(Error::UnsupportedSchema {
            found: doc.schema,
            supported: SCHEMA_VERSION,
        });
    }
    Ok(doc)
}

/// Guarda de forma atómica (tmp + rename), como hacen los editores.
pub fn save_document(doc: &Document, path: &Path) -> Result<(), Error> {
    let json = serde_json::to_string_pretty(doc)?;
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
            PathBuf::from("src/main.rs.codedraw")
        );
    }

    #[test]
    fn open_range_and_save_roundtrip() {
        let dir = std::env::temp_dir().join(format!("codedraw-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("x.rs");
        std::fs::write(&src, "l1\nl2\nl3\nl4\n").unwrap();
        let o = open(&format!("{}:2-3", src.display())).unwrap();
        assert_eq!(o.document.code, vec!["l2", "l3"]);
        assert_eq!(o.document.source.first_line, 2);
        assert!(o.save_path.to_string_lossy().ends_with("x.rs.2-3.codedraw"));
        save_document(&o.document, &o.save_path).unwrap();
        let back = load_document(&o.save_path).unwrap();
        assert_eq!(back, o.document);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
