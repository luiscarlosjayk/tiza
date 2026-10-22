//! Fonts shipped inside the binary.

/// Font family used for every cell.
pub const FAMILY: &str = "JetBrains Mono";
/// Fallback family for symbols the main font lacks (★ ✎ ⇄ …).
pub const SYMBOLS_FAMILY: &str = "Tiza Symbols";

/// Advance width of one cell, in em (JetBrains Mono: 600/1000).
pub const CELL_WIDTH_EM: f32 = 0.6;
/// Height of one cell, in em (ascender 1020 + descender 300, per 1000 upem).
pub const CELL_HEIGHT_EM: f32 = 1.32;
/// Baseline offset from the top of a cell, in em.
pub const ASCENT_EM: f32 = 1.02;

pub struct Face {
    pub family: &'static str,
    pub weight: u16,
    pub italic: bool,
    pub data: &'static [u8],
}

pub const FACES: &[Face] = &[
    Face {
        family: FAMILY,
        weight: 400,
        italic: false,
        data: include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    },
    Face {
        family: FAMILY,
        weight: 700,
        italic: false,
        data: include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    },
    Face {
        family: FAMILY,
        weight: 400,
        italic: true,
        data: include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf"),
    },
    Face {
        family: FAMILY,
        weight: 700,
        italic: true,
        data: include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
    },
    Face {
        family: SYMBOLS_FAMILY,
        weight: 400,
        italic: false,
        data: include_bytes!("../assets/fonts/TizaSymbols.ttf"),
    },
];

/// Index in [`FACES`] of the JetBrains Mono face for a text style.
pub fn mono_face(bold: bool, italic: bool) -> usize {
    match (bold, italic) {
        (false, false) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (true, true) => 3,
    }
}

/// Index in [`FACES`] of the symbols fallback.
pub const SYMBOLS_FACE: usize = 4;

/// Character maps of [`FACES`], in the same order, to ask which face draws a
/// character (the SVG embeds only the faces a frame uses).
pub struct Coverage {
    maps: Vec<Option<skrifa::charmap::Charmap<'static>>>,
}

impl Coverage {
    pub fn new() -> Self {
        use skrifa::MetadataProvider as _;
        Self {
            maps: FACES
                .iter()
                .map(|f| skrifa::FontRef::new(f.data).ok().map(|r| r.charmap()))
                .collect(),
        }
    }

    /// Whether face `face` (an index in [`FACES`]) has a glyph for `ch`.
    pub fn has(&self, face: usize, ch: char) -> bool {
        self.maps
            .get(face)
            .and_then(Option::as_ref)
            .and_then(|m| m.map(ch))
            .is_some_and(|g| g.to_u32() != 0)
    }
}

impl Default for Coverage {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn face_indexes_match_the_table() {
        for (bold, italic) in [(false, false), (true, false), (false, true), (true, true)] {
            let f = &FACES[mono_face(bold, italic)];
            assert_eq!(f.family, FAMILY);
            assert_eq!(f.weight == 700, bold);
            assert_eq!(f.italic, italic);
        }
        assert_eq!(FACES[SYMBOLS_FACE].family, SYMBOLS_FAMILY);
    }

    #[test]
    fn coverage_tells_the_symbols_the_mono_lacks() {
        let c = Coverage::new();
        assert!(c.has(0, 'a') && c.has(0, '─') && c.has(3, 'a'));
        assert!(!c.has(0, '★') && c.has(SYMBOLS_FACE, '★'));
    }
}
