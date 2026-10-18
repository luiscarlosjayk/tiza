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
