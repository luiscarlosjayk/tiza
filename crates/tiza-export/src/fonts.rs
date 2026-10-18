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
/// character (t-130: the SVG embeds only the faces a frame uses).
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

/// Face `face` (an index in [`FACES`]) cut down to the characters in `chars`
/// plus every glyph the browser can reach from them (t-137): the GSUB closure
/// of the features a browser applies by default (`calt` draws JetBrains
/// Mono's ligatures, `ccmp`/`locl`, …) and the GPOS `mark`/`mkmk` anchors of
/// combining marks, with `cmap`, `glyf`/`loca`, `hmtx`, the hinting tables and
/// the vertical metrics kept consistent. Done in memory for the SVG only; the
/// font files in the binary stay complete and the PNG path never sees this.
///
/// `None` when the subsetter refuses the font: the caller then embeds the
/// whole face, so a subsetting bug costs bytes, never glyphs.
pub fn subset(face: usize, chars: &std::collections::BTreeSet<char>) -> Option<Vec<u8>> {
    use skrifa::raw::collections::IntSet;
    use skrifa::raw::types::{NameId, Tag};

    let font = skrifa::FontRef::new(FACES.get(face)?.data).ok()?;
    let unicodes: IntSet<u32> = chars.iter().map(|&c| c as u32).collect();
    let mut scripts = IntSet::<Tag>::empty();
    scripts.invert(); // every script
    let features: IntSet<Tag> = skera::DEFAULT_LAYOUT_FEATURES.iter().copied().collect();
    // hb-subset's default name records (family, style, version, PostScript
    // name) plus the OFL license text and URL (13, 14), US English.
    let names: IntSet<NameId> = (0..=6).chain([13, 14]).map(NameId::new).collect();
    let languages: IntSet<u16> = [0x0409].into_iter().collect();
    let plan = skera::Plan::new(
        &IntSet::empty(),
        &unicodes,
        &font,
        skera::SubsetFlags::default(), // keeps the hinting: same pixels
        &IntSet::empty(),
        &scripts,
        &features,
        &names,
        &languages,
    );
    skera::subset_font(&font, &plan).ok()
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

    fn glyphs(data: &[u8]) -> u16 {
        use skrifa::raw::TableProvider as _;
        skrifa::FontRef::new(data)
            .unwrap()
            .maxp()
            .unwrap()
            .num_glyphs()
    }

    #[test]
    fn subset_keeps_the_characters_and_what_gsub_reaches_from_them() {
        use skrifa::MetadataProvider as _;
        let chars: std::collections::BTreeSet<char> = "->".chars().collect();
        let sub = subset(0, &chars).expect("JetBrains Mono subsets");
        let f = skrifa::FontRef::new(&sub).unwrap();
        let cmap = f.charmap();
        assert!(cmap.map('-').is_some() && cmap.map('>').is_some());
        assert!(cmap.map('a').is_none(), "only the characters asked for");
        // .notdef, '-', '>' and the `calt` ligature pieces of `->` (and of
        // the other `-`/`>` sequences): more glyphs than characters.
        let n = glyphs(&sub);
        assert!(n > 3, "{n} glyphs: the GSUB closure is missing");
        assert!(n < glyphs(FACES[0].data) / 4, "{n} glyphs");
        assert!(sub.len() < FACES[0].data.len() / 4);
    }

    #[test]
    fn subset_never_touches_the_embedded_files() {
        let before: Vec<usize> = FACES.iter().map(|f| f.data.len()).collect();
        let chars = "fn main() ★".chars().collect();
        for i in 0..FACES.len() {
            assert!(subset(i, &chars).is_some(), "face {i}");
        }
        assert_eq!(
            before,
            FACES.iter().map(|f| f.data.len()).collect::<Vec<_>>()
        );
        assert_eq!(glyphs(FACES[0].data), 1743, "JetBrains Mono Regular, whole");
    }
}
