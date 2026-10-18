//! Dark and light themes (like Excalidraw), with explicit RGB colors meant to
//! look good on 1080p video. Semantic marks use hues the syntax highlighting
//! does not, and are also told apart by shape (each kind has its own glyph).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::canvas::{CellStyle, Rgb};
use crate::model::{MarkStyle, ThemeName};

// ---- the host terminal's colours (t-119) ----
//
// The canvas carries `Rgb` everywhere. The `terminal` theme needs "the
// terminal's default" and "ANSI colour n" instead, so those are encoded as
// sentinel `Rgb` values that only the TUI decodes (`tiza_tui::ui::rgb`):
// (0xFE, 0xA0, n) is indexed colour n (0-255), (0xFE, 0xA1, 0) is the
// terminal's default. Nothing else in Tiza produces these two prefixes, and
// an export never sees them (`Theme::for_export` replaces them first).

const ANSI_TAG: u8 = 0xFE;
const ANSI_INDEXED: u8 = 0xA0;
const ANSI_DEFAULT: u8 = 0xA1;

/// Indexed terminal colour `n` (0-15 are the user's ANSI palette).
pub const fn ansi(n: u8) -> Rgb {
    Rgb(ANSI_TAG, ANSI_INDEXED, n)
}

/// The terminal's default foreground / background.
pub const fn terminal_default() -> Rgb {
    Rgb(ANSI_TAG, ANSI_DEFAULT, 0)
}

/// What a host colour sentinel stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostColor {
    Default,
    Indexed(u8),
}

/// Decodes a sentinel; `None` for a real colour.
pub fn host_color(c: Rgb) -> Option<HostColor> {
    match c {
        Rgb(ANSI_TAG, ANSI_INDEXED, n) => Some(HostColor::Indexed(n)),
        Rgb(ANSI_TAG, ANSI_DEFAULT, _) => Some(HostColor::Default),
        _ => None,
    }
}

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
    /// Named annotation colours (`orange`, `blue`…) when an override or the
    /// `terminal` theme replaced the built-in hex for this theme.
    pub palette: BTreeMap<String, Rgb>,
    /// Whether the host terminal is dark (from OSC 11); the `terminal` theme
    /// picks its tints with it. `true` when unknown.
    pub host_dark: bool,
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
            palette: BTreeMap::new(),
            host_dark: true,
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
            palette: BTreeMap::new(),
            host_dark: false,
        }
    }

    /// The host terminal's palette (t-119): default background and text,
    /// the roles on the 16 ANSI colours, syntax on the terminal's colours
    /// (bat's `ansi` theme), so Tiza looks like the rest of the terminal.
    /// `host_dark` picks the tint behind highlights and the status bar
    /// (bright black on a dark terminal, white on a light one).
    pub fn terminal(host_dark: bool) -> Self {
        use two_face::theme::EmbeddedThemeName as T;
        let tint = if host_dark { ansi(8) } else { ansi(7) };
        let palette: BTreeMap<String, Rgb> = [
            ("orange", ansi(3)),
            ("blue", ansi(4)),
            ("green", ansi(2)),
            ("purple", ansi(5)),
            ("pink", ansi(13)),
            ("cyan", ansi(6)),
            ("yellow", ansi(11)),
            ("red", ansi(9)),
            ("gray", ansi(8)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        Self {
            name: ThemeName::Terminal,
            bg: terminal_default(),
            fg: terminal_default(),
            muted: ansi(8),
            dimmed: ansi(8),
            accent: ansi(3),
            rail: ansi(8),
            box_line: ansi(3),
            box_bg: terminal_default(),
            arrow: ansi(3),
            note: ansi(6),
            highlight_bg: tint,
            status_bg: tint,
            status_fg: terminal_default(),
            cmd_bg: terminal_default(),
            error: ansi(1),
            syntax: T::Ansi,
            palette,
            host_dark,
        }
    }

    pub fn from_name(name: ThemeName) -> Self {
        match name {
            ThemeName::Dark => Self::dark(),
            ThemeName::Light => Self::light(),
            ThemeName::Terminal => Self::terminal(true),
        }
    }

    /// `t`: dark ⇄ light; from `terminal`, the one that matches the host.
    pub fn toggled(&self) -> Self {
        match self.name {
            ThemeName::Dark => Self::light(),
            ThemeName::Light => Self::dark(),
            ThemeName::Terminal => {
                if self.host_dark {
                    Self::dark()
                } else {
                    Self::light()
                }
            }
        }
    }

    /// Whether this theme draws with the host terminal's colours.
    pub fn is_terminal(&self) -> bool {
        self.name == ThemeName::Terminal
    }

    /// A colour `t` of the way from the background toward `color`: the tint
    /// behind a highlight, a selection or a box. With the host's palette
    /// there is nothing to blend, so stronger tints use the theme's tint
    /// colour and faint ones the plain background.
    pub fn tint(&self, color: Rgb, t: f32) -> Rgb {
        if self.is_terminal() || host_color(color).is_some() || host_color(self.bg).is_some() {
            if t >= 0.2 { self.highlight_bg } else { self.bg }
        } else {
            blend(color, self.bg, t)
        }
    }

    /// A softened ink (the emphasis glyphs, t-096): `ink` at opacity `t`
    /// over the background. With the host's colours the ink stays as it is.
    pub fn tint_ink(&self, ink: Rgb, t: f32) -> Rgb {
        if host_color(ink).is_some() || host_color(self.bg).is_some() {
            ink
        } else {
            blend(ink, self.bg, t)
        }
    }

    /// The theme an export can paint: `terminal` and `reset` tokens have no
    /// value outside the terminal, so they become the dark or light theme's
    /// (by the host's appearance). Returns what was replaced, for a notice.
    pub fn for_export(&self) -> (Theme, Option<String>) {
        if self.is_terminal() {
            let base = if self.host_dark {
                Self::dark()
            } else {
                Self::light()
            };
            return (
                base.clone(),
                Some(format!(
                    "the terminal theme has no palette outside the terminal: exported with {}",
                    if self.host_dark { "dark" } else { "light" }
                )),
            );
        }
        let base = match self.name {
            ThemeName::Light => Self::light(),
            _ => Self::dark(),
        };
        let mut out = self.clone();
        let mut replaced = Vec::new();
        for (name, slot, fallback) in [
            ("bg", &mut out.bg, base.bg),
            ("fg", &mut out.fg, base.fg),
            ("muted", &mut out.muted, base.muted),
            ("dimmed", &mut out.dimmed, base.dimmed),
            ("accent", &mut out.accent, base.accent),
            ("rail", &mut out.rail, base.rail),
            ("box_line", &mut out.box_line, base.box_line),
            ("box_bg", &mut out.box_bg, base.box_bg),
            ("arrow", &mut out.arrow, base.arrow),
            ("note", &mut out.note, base.note),
            ("highlight_bg", &mut out.highlight_bg, base.highlight_bg),
            ("status_bg", &mut out.status_bg, base.status_bg),
            ("status_fg", &mut out.status_fg, base.status_fg),
            ("cmd_bg", &mut out.cmd_bg, base.cmd_bg),
            ("error", &mut out.error, base.error),
        ] {
            if host_color(*slot).is_some() {
                *slot = fallback;
                replaced.push(name);
            }
        }
        out.palette.retain(|_, c| host_color(*c).is_none());
        let notice = (!replaced.is_empty()).then(|| {
            format!(
                "theme tokens set to a terminal colour have no value in an export ({}): the {} theme's are used",
                replaced.join(", "),
                if base.name == ThemeName::Light { "light" } else { "dark" }
            )
        });
        (out, notice)
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
        self.tint(self.mark_color(style), 0.22)
    }

    pub fn base(&self) -> CellStyle {
        CellStyle::new().fg(self.fg).bg(self.bg)
    }
}

/// Minimum contrast of code against the background (WCAG AA for text), so
/// comments and other dim syntax colors survive video compression.
pub const MIN_CODE_CONTRAST: f32 = 4.5;

/// WCAG relative luminance (0 = black, 1 = white).
pub fn luminance(c: Rgb) -> f32 {
    let lin = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.0) + 0.7152 * lin(c.1) + 0.0722 * lin(c.2)
}

/// WCAG contrast ratio between two colors (1..21).
pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (la, lb) = (luminance(a) + 0.05, luminance(b) + 0.05);
    if la > lb { la / lb } else { lb / la }
}

impl Theme {
    /// `fg` pushed toward the theme's text color until it reads at
    /// [`MIN_CODE_CONTRAST`] against the background. Colors that already do
    /// are returned unchanged, so the syntax palette keeps its hues.
    pub fn legible(&self, fg: Rgb) -> Rgb {
        // The host's colours are unknown: nothing to measure.
        if host_color(fg).is_some()
            || host_color(self.bg).is_some()
            || host_color(self.fg).is_some()
        {
            return fg;
        }
        if contrast(fg, self.bg) >= MIN_CODE_CONTRAST {
            return fg;
        }
        let mut t = 0.1;
        while t < 1.0 {
            let c = blend(self.fg, fg, t);
            if contrast(c, self.bg) >= MIN_CODE_CONTRAST {
                return c;
            }
            t += 0.1;
        }
        self.fg
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
        let canonical = match n.as_str() {
            "naranja" | "rust" => "orange",
            "azul" => "blue",
            "verde" => "green",
            "morado" | "violeta" => "purple",
            "rosa" | "magenta" => "pink",
            "cian" | "teal" => "cyan",
            "amarillo" => "yellow",
            "rojo" => "red",
            "grey" | "gris" => "gray",
            other => other,
        };
        if let Some(c) = self.palette.get(canonical) {
            return Some(*c);
        }
        let dark = self.name != ThemeName::Light;
        let pick = |d: u32, l: u32| Some(Rgb::hex(if dark { d } else { l }));
        match n.as_str() {
            "orange" | "naranja" | "rust" | "accent" => Some(self.accent),
            "blue" | "azul" => pick(0x7BB0D5, 0x2F4F9E),
            "green" | "verde" => pick(0x9CC98A, 0x2F6B3A),
            "purple" | "morado" | "violeta" => pick(0xB48CFF, 0x6B3FD6),
            "pink" | "rosa" | "magenta" => pick(0xE85DD0, 0xC0188F),
            "cyan" | "cian" | "teal" => pick(0x3DD6CF, 0x007A7A),
            "yellow" | "amarillo" => pick(0xF2D98D, 0x8A5A00),
            "red" | "rojo" => pick(0xFF4A66, 0xD1203A),
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

// ---- per-token overrides (t-119) ----

/// One value of a `[theme]` table: a colour, or the `dark` / `light`
/// subtable that applies on top when that base theme is active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum OverrideValue {
    /// `#rrggbb`, `#rgb`, `rgb(r, g, b)`, a palette or ANSI name, or
    /// `reset` (the terminal's default colour; exports fall back).
    Color(String),
    /// Overrides that apply only when this base theme is active.
    Table(BTreeMap<String, String>),
}

/// The `[theme]` table of `~/.config/tiza/config.toml`, a `--theme-file`,
/// or a document's `theme_overrides`: Tiza's roles by name (`bg`, `fg`,
/// `muted`, `dimmed`, `accent`, `rail`, `box_line`, `box_bg`, `arrow`,
/// `note`, `highlight_bg`, `status_bg`, `status_fg`, `cmd_bg`, `error`),
/// the named annotation palette (`orange`, `blue`, `green`, `purple`,
/// `pink`, `cyan`, `yellow`, `red`, `gray`), and `dark` / `light`
/// subtables with the same keys.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ThemeOverrides(pub BTreeMap<String, OverrideValue>);

/// Role names an override may set, in the order `Theme` lists them.
pub const ROLES: &[&str] = &[
    "bg",
    "fg",
    "muted",
    "dimmed",
    "accent",
    "rail",
    "box_line",
    "box_bg",
    "arrow",
    "note",
    "highlight_bg",
    "status_bg",
    "status_fg",
    "cmd_bg",
    "error",
];

/// Parses a colour written by a person: `#rrggbb`, `#rgb`, `rgb(r, g, b)`,
/// an ANSI name (`black`, `red`, …, `bright-blue`, `ansi12`), `reset` /
/// `default` (the terminal's own colour), or `none` for the same.
pub fn parse_color(text: &str) -> Result<Rgb, String> {
    let t = text.trim().to_ascii_lowercase();
    if let Some(hex) = t.strip_prefix('#') {
        return match hex.len() {
            6 => u32::from_str_radix(hex, 16)
                .map(Rgb::hex)
                .map_err(|_| format!("{text}: not a hex colour")),
            3 => {
                let v = u32::from_str_radix(hex, 16)
                    .map_err(|_| format!("{text}: not a hex colour"))?;
                let (r, g, b) = ((v >> 8) & 0xF, (v >> 4) & 0xF, v & 0xF);
                Ok(Rgb((r * 17) as u8, (g * 17) as u8, (b * 17) as u8))
            }
            _ => Err(format!("{text}: use #rrggbb or #rgb")),
        };
    }
    if let Some(inner) = t.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() != 3 {
            return Err(format!("{text}: rgb(r, g, b) takes three numbers"));
        }
        let mut v = [0u8; 3];
        for (i, p) in parts.iter().enumerate() {
            v[i] = p
                .parse::<u8>()
                .map_err(|_| format!("{text}: {p} is not a number 0-255"))?;
        }
        return Ok(Rgb(v[0], v[1], v[2]));
    }
    if matches!(t.as_str(), "reset" | "default" | "none") {
        return Ok(terminal_default());
    }
    if let Some(n) = t.strip_prefix("ansi")
        && let Ok(n) = n.parse::<u8>()
    {
        return Ok(ansi(n));
    }
    let names = [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ];
    let (bright, base) = match t
        .strip_prefix("bright-")
        .or_else(|| t.strip_prefix("bright"))
    {
        Some(b) => (true, b.trim_start_matches('-').to_string()),
        None => (false, t.clone()),
    };
    if let Some(i) = names.iter().position(|n| *n == base) {
        return Ok(ansi(i as u8 + if bright { 8 } else { 0 }));
    }
    Err(format!(
        "{text}: use #rrggbb, rgb(r, g, b), an ANSI name (red, bright-blue…), ansiN or reset"
    ))
}

impl Theme {
    /// Applies an override table: the flat tokens, then the subtable of
    /// this theme's name. Returns what it changed and what it ignored, one
    /// line each, for the status line and the CLI.
    pub fn apply(&mut self, ov: &ThemeOverrides) -> Vec<String> {
        let mut notes = Vec::new();
        let mut flat: Vec<(&str, &str)> = Vec::new();
        let mine = match self.name {
            ThemeName::Dark => "dark",
            ThemeName::Light => "light",
            ThemeName::Terminal => "terminal",
        };
        for (k, v) in &ov.0 {
            match v {
                OverrideValue::Color(c) => flat.push((k, c)),
                OverrideValue::Table(t) => {
                    if k == mine {
                        for (k2, v2) in t {
                            flat.push((k2, v2));
                        }
                    } else if !matches!(k.as_str(), "dark" | "light" | "terminal") {
                        notes.push(format!(
                            "theme: unknown table [{k}] (dark, light, terminal)"
                        ));
                    }
                }
            }
        }
        for (k, v) in flat {
            let color = match parse_color(v) {
                Ok(c) => c,
                Err(e) => {
                    notes.push(format!("theme {k}: {e}"));
                    continue;
                }
            };
            let slot = match k {
                "bg" => &mut self.bg,
                "fg" => &mut self.fg,
                "muted" => &mut self.muted,
                "dimmed" => &mut self.dimmed,
                "accent" => &mut self.accent,
                "rail" => &mut self.rail,
                "box_line" => &mut self.box_line,
                "box_bg" => &mut self.box_bg,
                "arrow" => &mut self.arrow,
                "note" => &mut self.note,
                "highlight_bg" => &mut self.highlight_bg,
                "status_bg" => &mut self.status_bg,
                "status_fg" => &mut self.status_fg,
                "cmd_bg" => &mut self.cmd_bg,
                "error" => &mut self.error,
                name if PALETTE.contains(&name) => {
                    self.palette.insert(name.to_string(), color);
                    continue;
                }
                other => {
                    notes.push(format!(
                        "theme: unknown token `{other}` (roles: {}; palette: {})",
                        ROLES.join(", "),
                        PALETTE.join(", ")
                    ));
                    continue;
                }
            };
            *slot = color;
        }
        // Text must still read at 4.5:1 (t-072): push what an override
        // left too faint and say so.
        if host_color(self.bg).is_none() {
            for (name, slot) in [
                ("fg", &mut self.fg),
                ("muted", &mut self.muted),
                ("accent", &mut self.accent),
                ("note", &mut self.note),
                ("error", &mut self.error),
            ] {
                if host_color(*slot).is_some() {
                    continue;
                }
                let bg = self.bg;
                if contrast(*slot, bg) < MIN_CODE_CONTRAST {
                    let before = *slot;
                    let mut t = 0.1;
                    let mut fixed = before;
                    while t < 1.0 {
                        fixed = blend(contrast_anchor(bg), before, t);
                        if contrast(fixed, bg) >= MIN_CODE_CONTRAST {
                            break;
                        }
                        t += 0.1;
                    }
                    *slot = fixed;
                    notes.push(format!(
                        "theme {name}: {} reads at {:.1}:1 on the background, pushed to {} for {MIN_CODE_CONTRAST}:1",
                        hex_of(before),
                        contrast(before, bg),
                        hex_of(fixed)
                    ));
                }
            }
        }
        notes
    }

    /// Built-in → config file → document (t-119): the theme `name` with
    /// both override tables applied, and the notes they produced.
    pub fn resolve(
        name: ThemeName,
        host_dark: bool,
        config: Option<&ThemeOverrides>,
        document: Option<&ThemeOverrides>,
    ) -> (Theme, Vec<String>) {
        let mut th = match name {
            ThemeName::Terminal => Theme::terminal(host_dark),
            other => Theme::from_name(other),
        };
        th.host_dark = host_dark;
        let mut notes = Vec::new();
        if let Some(c) = config {
            notes.extend(th.apply(c));
        }
        if let Some(d) = document {
            notes.extend(th.apply(d));
        }
        (th, notes)
    }
}

/// White or black, whichever contrasts more with `bg`: what a faint token
/// is pushed toward.
fn contrast_anchor(bg: Rgb) -> Rgb {
    if luminance(bg) > 0.5 {
        Rgb(0, 0, 0)
    } else {
        Rgb(255, 255, 255)
    }
}

fn hex_of(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
}

/// `~/.config/tiza/config.toml`: the file as a whole (only `[theme]` for
/// now), plus a free-standing theme file for `--theme-file`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ConfigFile {
    #[serde(default)]
    pub theme: Option<ThemeOverrides>,
}

/// Where the config file is: `$TIZA_CONFIG`, else
/// `$XDG_CONFIG_HOME/tiza/config.toml`, else `~/.config/tiza/config.toml`.
pub fn config_path() -> Option<std::path::PathBuf> {
    if let Ok(p) = std::env::var("TIZA_CONFIG")
        && !p.trim().is_empty()
    {
        return Some(std::path::PathBuf::from(p));
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config"))
        })?;
    Some(base.join("tiza").join("config.toml"))
}

/// Reads the config file when there is one. A file that does not parse is
/// an error the caller shows once; a missing file is `Ok(None)`.
pub fn load_config() -> Result<Option<(ConfigFile, std::path::PathBuf)>, String> {
    let Some(path) = config_path() else {
        return Ok(None);
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let cfg: ConfigFile = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some((cfg, path)))
}

/// Parses a theme file (`--theme-file`, `:theme file PATH`): either a
/// `[theme]` table or the tokens at the top level.
pub fn parse_theme_file(text: &str) -> Result<ThemeOverrides, String> {
    let value: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let table = match value.get("theme") {
        Some(toml::Value::Table(t)) => t.clone(),
        _ => value,
    };
    let mut out = BTreeMap::new();
    for (k, v) in table {
        match v {
            toml::Value::String(s) => {
                out.insert(k, OverrideValue::Color(s));
            }
            toml::Value::Table(t) => {
                let mut sub = BTreeMap::new();
                for (k2, v2) in t {
                    match v2 {
                        toml::Value::String(s) => {
                            sub.insert(k2, s);
                        }
                        other => {
                            return Err(format!(
                                "[{k}] {k2}: expected a colour string, got {}",
                                other.type_str()
                            ));
                        }
                    }
                }
                out.insert(k, OverrideValue::Table(sub));
            }
            other => {
                return Err(format!(
                    "{k}: expected a colour string, got {}",
                    other.type_str()
                ));
            }
        }
    }
    Ok(ThemeOverrides(out))
}

/// Reads and parses a theme file.
pub fn load_theme_file(path: &std::path::Path) -> Result<ThemeOverrides, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_theme_file(&text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse_in_every_written_form() {
        assert_eq!(parse_color("#1e1e2e").unwrap(), Rgb::hex(0x1E1E2E));
        assert_eq!(parse_color("#abc").unwrap(), Rgb(0xAA, 0xBB, 0xCC));
        assert_eq!(parse_color("rgb(1, 2, 3)").unwrap(), Rgb(1, 2, 3));
        assert_eq!(parse_color("reset").unwrap(), terminal_default());
        assert_eq!(parse_color("bright-blue").unwrap(), ansi(12));
        assert_eq!(parse_color("red").unwrap(), ansi(1));
        assert_eq!(parse_color("ansi208").unwrap(), ansi(208));
        assert!(parse_color("#12").is_err() && parse_color("rgb(1,2)").is_err());
        assert!(parse_color("mauve").is_err());
        assert_eq!(host_color(ansi(3)), Some(HostColor::Indexed(3)));
        assert_eq!(host_color(terminal_default()), Some(HostColor::Default));
        assert_eq!(host_color(Rgb(0xFE, 0xA2, 0)), None);
    }

    #[test]
    fn overrides_apply_in_order_and_keep_text_legible() {
        let file = parse_theme_file(
            "accent = \"#a6e3a1\"\nmuted = \"#333333\"\nmauve = \"#cba6f7\"\n[dark]\nnote = \"#f9e2af\"\n[light]\nnote = \"#000000\"\n",
        )
        .unwrap();
        let (th, notes) = Theme::resolve(ThemeName::Dark, true, Some(&file), None);
        assert_eq!(th.accent, Rgb::hex(0xA6E3A1));
        assert_eq!(th.note, Rgb::hex(0xF9E2AF), "the dark subtable applied");
        assert!(
            contrast(th.muted, th.bg) >= MIN_CODE_CONTRAST,
            "muted pushed"
        );
        assert!(
            notes
                .iter()
                .any(|n| n.contains("theme muted:") && n.contains("pushed")),
            "{notes:?}"
        );
        assert!(
            notes.iter().any(|n| n.contains("unknown token `mauve`")),
            "{notes:?}"
        );
        // The document's table wins over the config's.
        let doc = parse_theme_file("accent = \"#ffaa00\"").unwrap();
        let (th, _) = Theme::resolve(ThemeName::Dark, true, Some(&file), Some(&doc));
        assert_eq!(th.accent, Rgb::hex(0xFFAA00));
        // Light picks its own subtable; the flat tokens still apply.
        let (th, notes) = Theme::resolve(ThemeName::Light, false, Some(&file), None);
        assert_eq!(th.note, Rgb::hex(0x000000));
        // A light green on a light background is pushed toward black.
        assert_ne!(th.accent, Rgb::hex(0xA6E3A1));
        assert!(contrast(th.accent, th.bg) >= MIN_CODE_CONTRAST);
        assert!(
            notes.iter().any(|n| n.starts_with("theme accent:")),
            "{notes:?}"
        );
        // A `[theme]` wrapper is accepted too, and a named palette entry
        // changes what `named_color` gives.
        let wrapped = parse_theme_file("[theme]\nblue = \"#0000ff\"").unwrap();
        let (th, _) = Theme::resolve(ThemeName::Dark, true, Some(&wrapped), None);
        assert_eq!(th.named_color("azul"), Some(Rgb::hex(0x0000FF)));
    }

    #[test]
    fn terminal_theme_uses_host_colours_and_exports_fall_back() {
        let th = Theme::terminal(true);
        assert_eq!(host_color(th.bg), Some(HostColor::Default));
        assert_eq!(th.named_color("blue"), Some(ansi(4)));
        assert_eq!(th.legible(ansi(8)), ansi(8));
        assert_eq!(th.tint(ansi(3), 0.22), ansi(8));
        assert_eq!(th.tint(ansi(3), 0.08), terminal_default());
        let (exported, notice) = th.for_export();
        assert_eq!(exported.name, ThemeName::Dark);
        assert!(notice.unwrap().contains("exported with dark"));
        assert_eq!(Theme::terminal(false).toggled().name, ThemeName::Light);
        // A `reset` token in a dark theme: fine in the TUI, replaced in exports.
        let ov = parse_theme_file("bg = \"reset\"").unwrap();
        let (th, _) = Theme::resolve(ThemeName::Dark, true, Some(&ov), None);
        assert_eq!(th.bg, terminal_default());
        let (e, n) = th.for_export();
        assert_eq!(e.bg, Theme::dark().bg);
        assert!(n.unwrap().contains("bg"));
    }

    #[test]
    fn dim_syntax_colors_are_raised_to_aa_contrast() {
        for th in [Theme::dark(), Theme::light()] {
            for c in [Rgb::hex(0x5C6370), Rgb::hex(0xA0A1A7), th.muted, th.fg] {
                let out = th.legible(c);
                assert!(
                    contrast(out, th.bg) >= MIN_CODE_CONTRAST,
                    "{:?} -> {:?}",
                    c,
                    out
                );
            }
            assert_eq!(th.legible(th.fg), th.fg);
            assert!(
                contrast(th.muted, th.bg) >= MIN_CODE_CONTRAST,
                "gutter {:?}",
                th.name
            );
        }
        assert!((contrast(Rgb(0, 0, 0), Rgb(255, 255, 255)) - 21.0).abs() < 0.01);
    }
}
