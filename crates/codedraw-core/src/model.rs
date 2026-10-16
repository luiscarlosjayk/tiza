//! Modelo del documento: código fuente + anotaciones ancladas al texto.
//!
//! Las anclas viven en coordenadas del código (línea absoluta 1-based y
//! offset de grafema 0-based), nunca en celdas de pantalla. La conversión a
//! celdas ocurre solo al renderizar (ver `layout`).

use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::error::Error;

/// Versión del esquema del archivo `.codedraw`.
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Document {
    pub schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub source: Source,
    /// Cómo se guarda el código en el archivo: embebido (array de líneas) o
    /// referencia a un archivo (`{"file": "src/main.rs", "lines": "12-48"}`).
    #[serde(rename = "code")]
    pub code_src: CodeSource,
    /// Líneas de código en memoria, sin el salto de línea final. Se rellenan
    /// al cargar (ver `fs::load_document`) y nunca se serializan directamente.
    #[serde(skip)]
    pub code: Vec<String>,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    /// Paso actual de la presentación. 0 = solo el código, sin anotaciones;
    /// las anotaciones van en pasos ≥ 1.
    #[serde(default = "one")]
    pub step: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeName>,
}

/// Origen del código dentro de un `.codedraw`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum CodeSource {
    /// Copia del código dentro del documento: portable, pero congelada.
    Embedded(Vec<String>),
    /// Referencia a un archivo existente. `file` puede ser absoluta o relativa al
    /// directorio del `.codedraw`; `lines` es un rango opcional `"12-48"` o `"7"`.
    File {
        file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lines: Option<String>,
    },
}

impl Default for CodeSource {
    fn default() -> Self {
        CodeSource::Embedded(Vec::new())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Source {
    /// Ruta del archivo original, si lo hubo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Identificador de lenguaje (extensión o nombre): `rs`, `ts`, `py`…
    #[serde(default)]
    pub language: String,
    /// Número de la primera línea mostrada (permite abrir `main.rs:12-48`).
    #[serde(default = "one")]
    pub first_line: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    Dark,
    Light,
}

/// Un punto en el código: línea absoluta (1-based) y grafema (0-based).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Anchor {
    pub line: u32,
    pub col: u32,
}

/// Un rango dentro de UNA línea, en grafemas, fin exclusivo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TextSpan {
    pub line: u32,
    pub start: u32,
    pub end: u32,
}

/// Marcas semánticas para explicar Rust (cada una con color, glifo y leyenda fijos).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MarkKind {
    Move,
    Borrow,
    BorrowMut,
    Drop,
    Lifetime,
}

impl MarkKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "move" | "m" => Self::Move,
            "borrow" | "ref" | "&" | "b" => Self::Borrow,
            "mut" | "borrow_mut" | "borrowmut" | "&mut" => Self::BorrowMut,
            "drop" | "d" => Self::Drop,
            "lifetime" | "life" | "'a" | "l" => Self::Lifetime,
            _ => return None,
        })
    }

    /// Leyenda corta que se muestra junto a la marca.
    pub fn legend(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Borrow => "borrow &",
            Self::BorrowMut => "borrow &mut",
            Self::Drop => "drop",
            Self::Lifetime => "lifetime",
        }
    }

    /// Glifo que acompaña a la leyenda (la forma distingue la marca sin depender del color).
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Move => "━▶",
            Self::Borrow => "╌▶",
            Self::BorrowMut => "═▶",
            Self::Drop => "╳",
            Self::Lifetime => "┆",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Shape {
    /// Resalta un rango de texto.
    Highlight {
        span: TextSpan,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Resalte con significado (move, borrow…).
    Mark {
        kind: MarkKind,
        span: TextSpan,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Caja alrededor de un rango de líneas.
    Box {
        from_line: u32,
        to_line: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Flecha de un punto del código a otro.
    Arrow {
        from: Anchor,
        to: Anchor,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Nota de texto anclada a una línea.
    Note { line: u32, text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Annotation {
    /// Id público corto y estable (`a1`, `a2`…), elegido por el cliente o asignado.
    pub id: String,
    /// Paso en el que aparece (1-based).
    #[serde(default = "one")]
    pub step: u32,
    /// Color elegido por quien explica: nombre de la paleta (`blue`, `verde`…) o `#rrggbb`.
    /// Sin color, las flechas rotan por una paleta y el resto usa el color del tema.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(flatten)]
    pub shape: Shape,
}

fn one() -> u32 {
    1
}

impl Document {
    pub fn from_code(code: &str, language: impl Into<String>, path: Option<String>) -> Self {
        let lines: Vec<String> = code.lines().map(|l| l.to_string()).collect();
        Self {
            schema: SCHEMA_VERSION,
            title: None,
            source: Source {
                path,
                language: language.into(),
                first_line: 1,
            },
            code_src: CodeSource::Embedded(lines.clone()),
            code: lines,
            annotations: Vec::new(),
            step: 1,
            theme: None,
        }
    }

    /// Cambia el origen a referencia de archivo (el código en memoria no cambia).
    pub fn reference_file(&mut self, file: impl Into<String>, lines: Option<(u32, u32)>) {
        self.code_src = CodeSource::File {
            file: file.into(),
            lines: lines.map(|(a, b)| {
                if a == b {
                    a.to_string()
                } else {
                    format!("{a}-{b}")
                }
            }),
        };
    }

    /// Congela el código dentro del documento (deja de depender del archivo).
    pub fn embed_code(&mut self) {
        self.code_src = CodeSource::Embedded(self.code.clone());
    }

    pub fn is_embedded(&self) -> bool {
        matches!(self.code_src, CodeSource::Embedded(_))
    }

    /// Deja `code_src` coherente con `code` antes de serializar.
    pub fn sync_code_src(&mut self) {
        if let CodeSource::Embedded(lines) = &mut self.code_src {
            *lines = self.code.clone();
        }
    }

    /// Última línea absoluta del documento.
    pub fn last_line(&self) -> u32 {
        self.source.first_line + self.code.len().saturating_sub(1) as u32
    }

    pub fn has_line(&self, line: u32) -> bool {
        line >= self.source.first_line && line <= self.last_line()
    }

    /// Texto de una línea absoluta.
    pub fn line_text(&self, line: u32) -> Option<&str> {
        if !self.has_line(line) {
            return None;
        }
        self.code
            .get((line - self.source.first_line) as usize)
            .map(|s| s.as_str())
    }

    /// Número de grafemas de una línea.
    pub fn line_len(&self, line: u32) -> u32 {
        self.line_text(line)
            .map(|t| t.graphemes(true).count() as u32)
            .unwrap_or(0)
    }

    /// Mayor paso usado por alguna anotación (mínimo 1).
    pub fn max_step(&self) -> u32 {
        self.annotations
            .iter()
            .map(|a| a.step)
            .max()
            .unwrap_or(1)
            .max(1)
    }

    pub fn find_annotation(&self, id: &str) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.id == id)
    }

    /// Siguiente id libre con el prefijo `a` (a1, a2, …).
    pub fn next_id(&self) -> String {
        let mut n = self.annotations.len() as u32 + 1;
        loop {
            let id = format!("a{n}");
            if self.find_annotation(&id).is_none() {
                return id;
            }
            n += 1;
        }
    }

    /// Valida que una forma apunte a líneas/columnas existentes.
    pub fn validate_shape(&self, shape: &Shape) -> Result<(), Error> {
        let check_line = |l: u32| {
            if self.has_line(l) {
                Ok(())
            } else {
                Err(Error::LineOutOfRange {
                    line: l,
                    first: self.source.first_line,
                    last: self.last_line(),
                })
            }
        };
        let check_span = |s: &TextSpan| {
            check_line(s.line)?;
            let len = self.line_len(s.line);
            if s.start >= s.end || s.end > len {
                return Err(Error::SpanOutOfRange {
                    line: s.line,
                    start: s.start,
                    end: s.end,
                    len,
                });
            }
            Ok(())
        };
        let check_anchor = |a: &Anchor| {
            check_line(a.line)?;
            let len = self.line_len(a.line);
            if a.col > len {
                return Err(Error::SpanOutOfRange {
                    line: a.line,
                    start: a.col,
                    end: a.col,
                    len,
                });
            }
            Ok(())
        };
        match shape {
            Shape::Highlight { span, .. } | Shape::Mark { span, .. } => check_span(span),
            Shape::Box {
                from_line, to_line, ..
            } => {
                check_line(*from_line)?;
                check_line(*to_line)?;
                if from_line > to_line {
                    return Err(Error::Invalid(format!(
                        "la caja empieza en {from_line} y termina antes, en {to_line}"
                    )));
                }
                Ok(())
            }
            Shape::Arrow { from, to, .. } => {
                check_anchor(from)?;
                check_anchor(to)
            }
            Shape::Note { line, text } => {
                check_line(*line)?;
                if text.trim().is_empty() {
                    return Err(Error::Invalid("la nota está vacía".into()));
                }
                Ok(())
            }
        }
    }

    /// Añade una anotación. Si `id` viene vacío se asigna uno nuevo. Devuelve el id.
    pub fn add(&mut self, mut ann: Annotation) -> Result<String, Error> {
        self.validate_shape(&ann.shape)?;
        if ann.id.trim().is_empty() {
            ann.id = self.next_id();
        } else if self.find_annotation(&ann.id).is_some() {
            return Err(Error::DuplicateId(ann.id));
        }
        if ann.step == 0 {
            ann.step = 1;
        }
        let id = ann.id.clone();
        self.annotations.push(ann);
        Ok(id)
    }

    pub fn update(&mut self, ann: Annotation) -> Result<(), Error> {
        self.validate_shape(&ann.shape)?;
        let slot = self
            .annotations
            .iter_mut()
            .find(|a| a.id == ann.id)
            .ok_or_else(|| Error::NotFound(ann.id.clone()))?;
        *slot = ann;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<Annotation, Error> {
        let pos = self
            .annotations
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| Error::NotFound(id.to_string()))?;
        Ok(self.annotations.remove(pos))
    }

    /// Busca `text` en el código y devuelve el span de la `occurrence`-ésima
    /// aparición (1-based). Si `near_line` viene, busca primero en esa línea.
    pub fn find_text(
        &self,
        text: &str,
        occurrence: u32,
        near_line: Option<u32>,
    ) -> Option<TextSpan> {
        if text.is_empty() {
            return None;
        }
        let occurrence = occurrence.max(1);
        let mut seen = 0;
        let search_line = |line: u32, seen: &mut u32| -> Option<TextSpan> {
            let t = self.line_text(line)?;
            let mut from = 0;
            while let Some(pos) = t[from..].find(text) {
                let byte_start = from + pos;
                *seen += 1;
                if *seen == occurrence {
                    let start = grapheme_index(t, byte_start);
                    let end = grapheme_index(t, byte_start + text.len());
                    return Some(TextSpan { line, start, end });
                }
                from = byte_start + text.len().max(1);
            }
            None
        };
        if let Some(l) = near_line {
            if let Some(s) = search_line(l, &mut seen) {
                return Some(s);
            }
            seen = 0;
        }
        for line in self.source.first_line..=self.last_line() {
            if let Some(s) = search_line(line, &mut seen) {
                return Some(s);
            }
        }
        None
    }
}

/// Convierte un offset en bytes de `s` al índice de grafema correspondiente.
pub fn grapheme_index(s: &str, byte_offset: usize) -> u32 {
    s.grapheme_indices(true)
        .take_while(|(b, _)| *b < byte_offset)
        .count() as u32
}

/// Devuelve el slice de `s` entre los grafemas `start..end`.
pub fn grapheme_slice(s: &str, start: u32, end: u32) -> &str {
    let mut it = s.grapheme_indices(true);
    let b0 = it
        .by_ref()
        .nth(start as usize)
        .map(|(b, _)| b)
        .unwrap_or(s.len());
    let b1 = if end > start {
        s.grapheme_indices(true)
            .nth(end as usize)
            .map(|(b, _)| b)
            .unwrap_or(s.len())
    } else {
        b0
    };
    &s[b0..b1]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> Document {
        Document::from_code(
            "fn main() {\n    let s = String::from(\"hola\");\n    takes(s);\n}\n",
            "rs",
            None,
        )
    }

    #[test]
    fn find_text_returns_grapheme_span() {
        let d = doc();
        let s = d.find_text("takes(s)", 1, None).unwrap();
        assert_eq!(
            s,
            TextSpan {
                line: 3,
                start: 4,
                end: 12
            }
        );
        assert_eq!(
            grapheme_slice(d.line_text(3).unwrap(), s.start, s.end),
            "takes(s)"
        );
    }

    #[test]
    fn find_text_second_occurrence() {
        let d = doc();
        // En la línea 2, "o" aparece en `from` (1) y en `hola` (2).
        let s = d.find_text("o", 2, Some(2)).unwrap();
        assert_eq!(s.line, 2);
        assert_eq!(s.start, 27);
        // Si no hay suficientes ocurrencias en la línea pedida, se busca en todo el documento.
        let s = d.find_text("s", 2, Some(2)).unwrap();
        assert_eq!(s.line, 3);
    }

    #[test]
    fn add_validates_and_assigns_ids() {
        let mut d = doc();
        let id = d
            .add(Annotation {
                id: String::new(),
                step: 1,
                color: None,
                shape: Shape::Note {
                    line: 2,
                    text: "aquí nace s".into(),
                },
            })
            .unwrap();
        assert_eq!(id, "a1");
        let err = d
            .add(Annotation {
                id: String::new(),
                step: 1,
                color: None,
                shape: Shape::Note {
                    line: 99,
                    text: "x".into(),
                },
            })
            .unwrap_err();
        assert!(matches!(err, Error::LineOutOfRange { line: 99, .. }));
    }

    #[test]
    fn roundtrip_json() {
        let mut d = doc();
        d.add(Annotation {
            id: "a1".into(),
            step: 2,
            color: None,
            shape: Shape::Arrow {
                from: Anchor { line: 2, col: 8 },
                to: Anchor { line: 3, col: 10 },
                label: Some("move".into()),
            },
        })
        .unwrap();
        let json = serde_json::to_string_pretty(&d).unwrap();
        let mut back: Document = serde_json::from_str(&json).unwrap();
        if let CodeSource::Embedded(lines) = &back.code_src {
            back.code = lines.clone();
        }
        assert_eq!(d, back);
        assert!(json.contains("\"type\": \"arrow\""));
    }

    #[test]
    fn unicode_graphemes() {
        let d = Document::from_code("let año = \"ñandú\"; // 🦀 crab\n", "rs", None);
        let s = d.find_text("🦀", 1, None).unwrap();
        assert_eq!(
            grapheme_slice(d.line_text(1).unwrap(), s.start, s.end),
            "🦀"
        );
        assert_eq!(s.end - s.start, 1);
    }
}
