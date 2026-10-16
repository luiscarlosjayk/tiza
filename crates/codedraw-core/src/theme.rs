//! Dark and light themes (like Excalidraw), with explicit RGB colors meant to
//! look good on 1080p video. Semantic marks use hues the syntax highlighting
//! does not, and are also told apart by shape (each kind has its own glyph).

use crate::canvas::{CellStyle, Rgb};
use crate::model::{MarkStyle, ThemeName};

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: ThemeName,
    pub bg: Rgb,
    pub fg: Rgb,
    /// Secondary text (line numbers, legends).
    pub muted: Rgb,
    /// Dimmed code outside the focus.
    pub dimmed: Rgb,
    pub accent: Rgb,
    pub rail: Rgb,
    pub box_line: Rgb,
    pub box_bg: Rgb,
    pub arrow: Rgb,
    pub note: Rgb,
    pub highlight_bg: Rgb,
    pub status_bg: Rgb,
    pub status_fg: Rgb,
    pub cmd_bg: Rgb,
    pub error: Rgb,
    /// Matching two-face syntax theme.
    pub syntax: two_face::theme::EmbeddedThemeName,
}

impl Theme {
    pub fn dark() -> Self {
        use two_face::theme::EmbeddedThemeName as T;
        Self {
            name: ThemeName::Dark,
            bg: Rgb::hex(0x15171B),
            fg: Rgb::hex(0xE8E6E1),
            muted: Rgb::hex(0x8A919E),
            dimmed: Rgb::hex(0x5A6170),
            accent: Rgb::hex(0xF08B5B),
            rail: Rgb::hex(0x3E444F),
            box_line: Rgb::hex(0xF08B5B),
            box_bg: Rgb::hex(0x1E2128),
            arrow: Rgb::hex(0xF08B5B),
            note: Rgb::hex(0xE2C27A),
            highlight_bg: Rgb::hex(0x3A2A22),
            status_bg: Rgb::hex(0x272B33),
            status_fg: Rgb::hex(0xE8E6E1),
            cmd_bg: Rgb::hex(0x1D2026),
            error: Rgb::hex(0xF47C6F),
            syntax: T::OneHalfDark,
        }
    }

    pub fn light() -> Self {
        use two_face::theme::EmbeddedThemeName as T;
        Self {
            name: ThemeName::Light,
            bg: Rgb::hex(0xF7F5EF),
            fg: Rgb::hex(0x1F2328),
            muted: Rgb::hex(0x6B7079),
            dimmed: Rgb::hex(0xA3A7AE),
            accent: Rgb::hex(0xB7410E),
            rail: Rgb::hex(0xCFCBC0),
            box_line: Rgb::hex(0xB7410E),
            box_bg: Rgb::hex(0xEFECE4),
            arrow: Rgb::hex(0xB7410E),
            note: Rgb::hex(0x7A5A00),
            highlight_bg: Rgb::hex(0xF3DED2),
            status_bg: Rgb::hex(0xE8E6DF),
            status_fg: Rgb::hex(0x1F2328),
            cmd_bg: Rgb::hex(0xFBFAF7),
            error: Rgb::hex(0xB42318),
            syntax: T::OneHalfLight,
        }
    }

    pub fn from_name(name: ThemeName) -> Self {
        match name {
            ThemeName::Dark => Self::dark(),
            ThemeName::Light => Self::light(),
        }
    }

    pub fn toggled(&self) -> Self {
        match self.name {
            ThemeName::Dark => Self::light(),
            ThemeName::Light => Self::dark(),
        }
    }

    /// Color of a resolved mark style (falls back to the accent).
    pub fn mark_color(&self, style: &MarkStyle) -> Rgb {
        style
            .color
            .as_deref()
            .and_then(|c| self.named_color(c))
            .unwrap_or(self.accent)
    }

    /// Faint background for a mark's span: the mark color blended with the background.
    /// Subtle background for a mark's span: the mark color blended into the background.
    pub fn mark_bg(&self, style: &MarkStyle) -> Rgb {
        blend(self.mark_color(style), self.bg, 0.22)
    }

    pub fn base(&self) -> CellStyle {
        CellStyle::new().fg(self.fg).bg(self.bg)
    }
}

/// Blends `a` over `b` with opacity `t` (0..1).
pub fn blend(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let f = |x: u8, y: u8| {
        ((x as f32) * t + (y as f32) * (1.0 - t))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgb(f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

/// Color names accepted by `color` on an annotation (with a hex per theme).
/// `#rrggbb` is accepted too.
pub const PALETTE: &[&str] = &[
    "orange", "blue", "green", "purple", "pink", "cyan", "yellow", "red", "gray",
];

impl Theme {
    /// Resolves an annotation color: palette name, Spanish alias or `#rrggbb`.
    pub fn named_color(&self, name: &str) -> Option<Rgb> {
        let n = name.trim().to_ascii_lowercase();
        if let Some(hex) = n.strip_prefix('#')
            && hex.len() == 6
            && let Ok(v) = u32::from_str_radix(hex, 16)
        {
            return Some(Rgb::hex(v));
        }
        let dark = self.name == ThemeName::Dark;
        let pick = |d: u32, l: u32| Some(Rgb::hex(if dark { d } else { l }));
        match n.as_str() {
            "orange" | "naranja" | "rust" | "accent" => Some(self.accent),
            "blue" | "azul" => pick(0x7BB0D5, 0x2F4F9E),
            "green" | "verde" => pick(0x9CC98A, 0x2F6B3A),
            "purple" | "morado" | "violeta" => pick(0xB48CFF, 0x6B3FD6),
            "pink" | "rosa" | "magenta" => pick(0xE85DD0, 0xC0188F),
            "cyan" | "cian" | "teal" => pick(0x3DD6CF, 0x007A7A),
            "yellow" | "amarillo" => pick(0xF2D98D, 0x8A5A00),
            "red" | "rojo" => pick(0xFF3D5A, 0xD1203A),
            "gray" | "grey" | "gris" => Some(self.muted),
            _ => None,
        }
    }

    /// Palette that arrows without an explicit color rotate through, so they can be told apart.
    pub fn auto_palette(&self) -> Vec<Rgb> {
        [
            "orange", "blue", "green", "purple", "cyan", "pink", "yellow",
        ]
        .iter()
        .filter_map(|n| self.named_color(n))
        .collect()
    }
}
