//! Syntax highlighting with syntect + two-face (bat's grammars, including TypeScript and TOML).

use std::sync::{Arc, Mutex, OnceLock};

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
        "ruby" => "rb".into(),
        "perl" => "pl".into(),
        "yaml" => "yml".into(),
        "plain" | "text" | "plaintext" => "txt".into(),
        "shell" | "bash" | "zsh" => "sh".into(),
        "markdown" => "md".into(),
        "" => "txt".into(),
        _ => l,
    }
}

/// Where a detected language came from, for the `:lang` message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// The file extension (`main.rs`).
    Extension,
    /// A well-known file name without a useful extension (`Makefile`, `Dockerfile`).
    FileName,
    /// A `#!` line (`#!/usr/bin/env python3`).
    Shebang,
    /// A Vim or Emacs modeline (`# vim: ft=python`, `-*- mode: ruby -*-`).
    Modeline,
    /// Nothing matched: plain text.
    Default,
}

impl Via {
    pub fn label(self) -> &'static str {
        match self {
            Via::Extension => "file extension",
            Via::FileName => "file name",
            Via::Shebang => "shebang",
            Via::Modeline => "modeline",
            Via::Default => "default: plain text",
        }
    }
}

/// A detected language and how it was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub language: String,
    pub via: Via,
}

/// Whether syntect/two-face has a grammar for this language id (`rs`, `py`, `toml`…).
pub fn has_grammar(language: &str) -> bool {
    let ss = syntax_set();
    language != "txt"
        && (ss.find_syntax_by_extension(language).is_some()
            || ss.find_syntax_by_token(language).is_some())
}

/// Detects the language from the path and, failing that, the code itself.
///
/// Order: the file extension when a grammar exists for it (so a `.sh` file
/// with a Python shebang stays shell), well-known file names (`Makefile`,
/// `Dockerfile`, `Cargo.lock`, `.bashrc`…), a `#!` line, then a Vim or
/// Emacs modeline in the first or last five lines. Otherwise plain text.
pub fn detect_language(path: Option<&str>, code: &[String]) -> Detection {
    if let Some(p) = path {
        if let Some(ext) = std::path::Path::new(p)
            .extension()
            .and_then(|e| e.to_str())
            .map(normalize_language)
            .filter(|l| has_grammar(l))
        {
            return Detection {
                language: ext,
                via: Via::Extension,
            };
        }
        if let Some(name) = std::path::Path::new(p)
            .file_name()
            .and_then(|f| f.to_str())
            .and_then(language_from_file_name)
        {
            return Detection {
                language: name.into(),
                via: Via::FileName,
            };
        }
    }
    if let Some(d) = language_from_content(code) {
        return d;
    }
    Detection {
        language: "txt".into(),
        via: Via::Default,
    }
}

/// Detects the language from the file extension (or a well-known file name).
pub fn language_from_path(path: &str) -> String {
    detect_language(Some(path), &[]).language
}

/// Well-known file names whose extension says nothing (or is missing).
fn language_from_file_name(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    Some(match lower.as_str() {
        "makefile" | "gnumakefile" | "makefile.am" | "makefile.in" => "makefile",
        "dockerfile" | "containerfile" => "dockerfile",
        "cargo.lock" | "pipfile" | "poetry.lock" | "uv.lock" => "toml",
        "justfile" => "makefile",
        "rakefile" | "gemfile" | "gemfile.lock" | "guardfile" | "vagrantfile" => "rb",
        "cmakelists.txt" => "cmake",
        ".bashrc" | ".bash_profile" | ".bash_aliases" | ".profile" | ".zshrc" | ".zshenv"
        | ".zprofile" | "profile" => "sh",
        ".gitignore" | ".gitattributes" | ".dockerignore" | ".npmignore" => "gitignore",
        ".editorconfig" | ".gitconfig" | ".gitmodules" => "ini",
        ".babelrc" | ".eslintrc" | ".prettierrc" => "json",
        _ => return lower.starts_with("dockerfile.").then_some("dockerfile"),
    })
}

/// Shebang or modeline in the code, if any.
pub fn language_from_content(code: &[String]) -> Option<Detection> {
    if let Some(first) = code.first()
        && let Some(lang) = language_from_shebang(first)
    {
        return Some(Detection {
            language: lang.into(),
            via: Via::Shebang,
        });
    }
    let head = code.iter().take(5);
    let tail = code.iter().rev().take(5);
    head.chain(tail)
        .find_map(|l| language_from_modeline(l))
        .map(|language| Detection {
            language,
            via: Via::Modeline,
        })
}

/// `#!/usr/bin/env python3`, `#!/bin/bash`, `#!/usr/bin/env -S deno run` …
fn language_from_shebang(line: &str) -> Option<&'static str> {
    let rest = line.strip_prefix("#!")?.trim();
    let mut words = rest.split_whitespace().peekable();
    let mut interpreter = words.next()?;
    if interpreter.ends_with("/env") || interpreter == "env" {
        // Skip env's own flags (`-S`, `-i`, `VAR=value`).
        interpreter = words
            .by_ref()
            .find(|w| !w.starts_with('-') && !w.contains('='))?;
    }
    let base = interpreter.rsplit('/').next().unwrap_or(interpreter);
    // `python3.12` → `python`, `node22` → `node`.
    let name = base.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    Some(match name {
        "python" | "pypy" => "py",
        "node" | "nodejs" | "bun" => "js",
        "deno" | "ts-node" | "tsx" => "ts",
        "sh" | "bash" | "zsh" | "dash" | "ksh" | "ash" => "sh",
        "fish" => "fish",
        "ruby" => "rb",
        "perl" => "pl",
        "php" => "php",
        "lua" | "luajit" => "lua",
        "rust-script" | "cargo-script" | "cargo" => "rs",
        "rscript" => "r",
        "osascript" => "applescript",
        "awk" | "gawk" | "mawk" => "awk",
        "make" => "makefile",
        _ => return None,
    })
}

/// Vim (`vim: set ft=python:`, `vi: filetype=rb`) and Emacs
/// (`-*- mode: python -*-`, `-*- python -*-`) modelines.
fn language_from_modeline(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    if let Some(i) = lower.find("-*-")
        && let Some(j) = lower[i + 3..].find("-*-")
    {
        let inner = lower[i + 3..i + 3 + j].trim();
        let mode = inner
            .split(';')
            .map(str::trim)
            .find_map(|part| part.strip_prefix("mode:").map(str::trim))
            .unwrap_or(inner);
        let mode = mode.split_whitespace().next()?;
        return (!mode.is_empty() && !mode.contains(':')).then(|| normalize_language(mode));
    }
    for key in ["vim:", "vi:", "ex:"] {
        if let Some(i) = lower.find(key) {
            let rest = &lower[i + key.len()..];
            for token in rest
                .split(|c: char| c.is_whitespace() || c == ':')
                .filter(|t| !t.is_empty())
            {
                let t = token.strip_prefix("set ").unwrap_or(token);
                if let Some(v) = t
                    .strip_prefix("filetype=")
                    .or_else(|| t.strip_prefix("ft="))
                    .or_else(|| t.strip_prefix("syntax=").or_else(|| t.strip_prefix("syn=")))
                {
                    return Some(normalize_language(v));
                }
            }
        }
    }
    None
}

/// Highlighted lines shared between frames (see `highlight_cached`).
pub type Highlighted = Arc<Vec<Vec<Segment>>>;

/// `highlight` with a small memo: the TUI lays out the same code on every
/// frame and syntect costs ~50 µs per line, so re-highlighting 2,000 lines
/// each frame would eat the whole budget. Keyed by a hash of (code, language,
/// theme); the last four results are kept (TUI + MCP bridge + exports).
/// A syntect colour as a canvas colour. bat's terminal themes (`Ansi`,
/// `Base16`) encode the host palette in the alpha channel: `a == 0` means
/// indexed colour `r` and `a == 1` the terminal's default foreground
/// (t-119); real colours have `a == 255`.
fn syntect_color(c: syntect::highlighting::Color) -> Rgb {
    match c.a {
        0 => crate::theme::ansi(c.r),
        1 => crate::theme::terminal_default(),
        _ => Rgb(c.r, c.g, c.b),
    }
}

pub fn highlight_cached(code: &[String], language: &str, theme: EmbeddedThemeName) -> Highlighted {
    use std::hash::{Hash, Hasher};
    static CACHE: Mutex<Vec<(u64, Highlighted)>> = Mutex::new(Vec::new());
    let mut h = std::collections::hash_map::DefaultHasher::new();
    code.hash(&mut h);
    language.hash(&mut h);
    (theme as u32).hash(&mut h);
    code.len().hash(&mut h);
    let key = h.finish();
    if let Ok(cache) = CACHE.lock()
        && let Some((_, v)) = cache.iter().find(|(k, _)| *k == key)
    {
        return v.clone();
    }
    let fresh = Arc::new(highlight(code, language, theme));
    if let Ok(mut cache) = CACHE.lock() {
        cache.retain(|(k, _)| *k != key);
        cache.insert(0, (key, fresh.clone()));
        cache.truncate(4);
    }
    fresh
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
                        fg: Some(syntect_color(st.foreground)),
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
    fn every_file_name_and_shebang_target_has_a_grammar() {
        for name in [
            "Makefile",
            "Dockerfile",
            "Cargo.lock",
            "Justfile",
            "Rakefile",
            "CMakeLists.txt",
            ".bashrc",
            ".zshrc",
            ".gitignore",
            ".editorconfig",
            ".babelrc",
            "Dockerfile.dev",
        ] {
            let d = detect_language(Some(name), &[]);
            assert_eq!(d.via, Via::FileName, "{name}");
            assert!(has_grammar(&d.language), "{name} → {}", d.language);
        }
        for line in [
            "#!/usr/bin/env python3",
            "#!/bin/bash",
            "#!/usr/bin/env -S deno run -A",
            "#!/usr/bin/env node",
            "#!/usr/bin/ruby",
            "#!/usr/bin/perl -w",
            "#!/usr/bin/env lua",
            "#!/usr/bin/env rust-script",
            "#!/usr/bin/env fish",
            "#!/usr/bin/awk -f",
        ] {
            let d = detect_language(None, &[line.to_string()]);
            assert_eq!(d.via, Via::Shebang, "{line}");
            assert!(has_grammar(&d.language), "{line} → {}", d.language);
        }
    }

    #[test]
    fn extension_beats_shebang_and_unknown_extension_falls_through() {
        let py = vec!["#!/usr/bin/env python3".to_string()];
        assert_eq!(detect_language(Some("run.sh"), &py).language, "sh");
        let d = detect_language(Some("tool.bak"), &py);
        assert_eq!((d.language.as_str(), d.via), ("py", Via::Shebang));
        let d = detect_language(Some("notes"), &["hello".into()]);
        assert_eq!((d.language.as_str(), d.via), ("txt", Via::Default));
    }

    #[test]
    fn modelines_vim_and_emacs() {
        let cases = [
            ("# vim: set ft=python ts=4:", "py"),
            ("// vim:ft=rust", "rs"),
            ("# vi: syntax=ruby", "rb"),
            ("# -*- mode: python; coding: utf-8 -*-", "py"),
            ("; -*- lisp -*-", "lisp"),
        ];
        for (line, want) in cases {
            let code = vec!["x".to_string(), line.to_string()];
            let d = detect_language(None, &code);
            assert_eq!(d.language, want, "{line}");
            assert_eq!(d.via, Via::Modeline, "{line}");
        }
        // A shebang only counts on the first line.
        let d = detect_language(None, &["x".into(), "#!/usr/bin/env python3".into()]);
        assert_eq!(d.language, "txt");
        // A modeline also counts at the end, never in the middle.
        let mut long: Vec<String> = (0..30).map(|i| format!("line {i}")).collect();
        long.push("# vim: ft=toml".into());
        assert_eq!(detect_language(None, &long).language, "toml");
        let mut mid: Vec<String> = (0..30).map(|i| format!("line {i}")).collect();
        mid[12] = "# vim: ft=toml".into();
        assert_eq!(detect_language(None, &mid).language, "txt");
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
