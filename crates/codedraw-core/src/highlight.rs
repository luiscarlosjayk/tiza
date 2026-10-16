//! Syntax highlighting with syntect + two-face (bat's grammars, including TypeScript and TOML).

use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::parsing::SyntaxSet;
use two_face::theme::{EmbeddedLazyThemeSet, EmbeddedThemeName};

use crate::canvas::Rgb;

fn syntax_set() -> &'static SyntaxSet {
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    SS.get_or_init(two_face::syntax::extra_newlines)
}

fn theme_set() -> &'static EmbeddedLazyThemeSet {
    static TS: OnceLock<EmbeddedLazyThemeSet> = OnceLock::new();
    TS.get_or_init(two_face::theme::extra)
}

/// A fragment of a line with its color.
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub text: String,
    pub fg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
}

/// Known languages, to normalize what the user types (`rust`, `rs`, `ts`…).
pub fn normalize_language(lang: &str) -> String {
    let l = lang.trim().trim_start_matches('.').to_ascii_lowercase();
    match l.as_str() {
        "rust" => "rs".into(),
        "typescript" => "ts".into(),
        "javascript" => "js".into(),
        "python" => "py".into(),
        "golang" => "go".into(),
        "shell" | "bash" | "zsh" => "sh".into(),
        "markdown" => "md".into(),
        "" => "txt".into(),
        _ => l,
    }
}

/// Detects the language from the file extension.
pub fn language_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(normalize_language)
        .unwrap_or_else(|| "txt".into())
}

/// Highlights every line. If the language is unknown, returns plain text.
pub fn highlight(code: &[String], language: &str, theme: EmbeddedThemeName) -> Vec<Vec<Segment>> {
    let ss = syntax_set();
    let syntax = ss
        .find_syntax_by_extension(language)
        .or_else(|| ss.find_syntax_by_token(language))
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let theme = theme_set().get(theme);
    let mut h = HighlightLines::new(syntax, theme);
    code.iter()
        .map(|line| {
            let with_nl = format!("{line}\n");
            match h.highlight_line(&with_nl, ss) {
                Ok(ranges) => ranges
                    .into_iter()
                    .map(|(st, s)| Segment {
                        text: s.trim_end_matches('\n').to_string(),
                        fg: Some(Rgb(st.foreground.r, st.foreground.g, st.foreground.b)),
                        bold: st
                            .font_style
                            .contains(syntect::highlighting::FontStyle::BOLD),
                        italic: st
                            .font_style
                            .contains(syntect::highlighting::FontStyle::ITALIC),
                    })
                    .filter(|s| !s.text.is_empty())
                    .collect(),
                Err(_) => vec![Segment {
                    text: line.clone(),
                    fg: None,
                    bold: false,
                    italic: false,
                }],
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_keywords_get_a_color() {
        let out = highlight(
            &["fn main() {}".to_string()],
            "rs",
            EmbeddedThemeName::OneHalfDark,
        );
        assert_eq!(out.len(), 1);
        let joined: String = out[0].iter().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "fn main() {}");
        assert!(out[0].iter().any(|s| s.fg.is_some()));
    }

    #[test]
    fn typescript_and_toml_exist() {
        let ss = syntax_set();
        assert!(ss.find_syntax_by_extension("ts").is_some());
        assert!(ss.find_syntax_by_extension("toml").is_some());
    }

    #[test]
    fn unknown_language_is_plain() {
        let out = highlight(&["hola".to_string()], "zzz", EmbeddedThemeName::OneHalfDark);
        assert_eq!(
            out[0].iter().map(|s| s.text.as_str()).collect::<String>(),
            "hola"
        );
    }
}
