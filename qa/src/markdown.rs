//! Readers for Markdown: for the checks of the docs against the code
//! (`site/checks`), and for the tests of the README that a crate packages.

/// Each number N that a line of `md` gives as `before` N `after`: the line
/// (from 1) and N. The words around the number find it; they are not a
/// sentence that a test checks.
pub fn cited(md: &str, before: &str, after: &str) -> Vec<(usize, u64)> {
    assert!(!before.is_empty(), "give the words before the number");
    let mut out = Vec::new();
    for (i, line) in md.lines().enumerate() {
        let mut from = 0;
        while let Some(k) = line[from..].find(before) {
            let start = from + k + before.len();
            let digits = line[start..].bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 && line[start + digits..].starts_with(after) {
                out.push((i + 1, line[start..start + digits].parse().unwrap()));
            }
            from = start;
        }
    }
    out
}

/// An H2 of a page and the text under it.
pub struct Section<'a> {
    /// The line of the heading, from 1.
    pub line: usize,
    /// The text after the heading, up to the next H2.
    pub text: &'a str,
}

impl Section<'_> {
    /// The line of the page for a line of [`Section::text`] (from 1).
    pub fn page_line(&self, n: usize) -> usize {
        self.line + n
    }
}

/// The section of `md` under the line `heading` (`## Title`), or `None`
/// when the page has no such line.
pub fn section<'a>(md: &'a str, heading: &str) -> Option<Section<'a>> {
    let start = md.find(&format!("\n{heading}\n"))? + 1;
    let line = md[..start].lines().count() + 1;
    let rest = &md[start + heading.len() + 1..];
    let text = rest.find("\n## ").map_or(rest, |end| &rest[..end]);
    Some(Section { line, text })
}

/// The rows of the first table of `text`, without the header and the
/// `|---|` row: the line of each one in `text` (from 1), and its cells as
/// they are, spaces too.
pub fn table_rows(text: &str) -> Vec<(usize, Vec<String>)> {
    text.lines()
        .enumerate()
        .skip_while(|(_, l)| !l.starts_with('|'))
        .take_while(|(_, l)| l.starts_with('|'))
        .skip(2)
        .map(|(i, l)| {
            let inner = l[1..].trim_end();
            let inner = inner.strip_suffix('|').unwrap_or(inner);
            (i + 1, inner.split('|').map(str::to_string).collect())
        })
        .collect()
}

/// The JSON of the first `tiza scene patch '…'` command of `text`, or
/// `None` when the text has no such command.
pub fn scene_patch_json(text: &str) -> Option<serde_json::Value> {
    const COMMAND: &str = "tiza scene patch '";
    let start = text.find(COMMAND)? + COMMAND.len();
    let len = text[start..]
        .find('\'')
        .expect("the quote that closes the JSON of `tiza scene patch`");
    Some(
        serde_json::from_str(&text[start..start + len])
            .unwrap_or_else(|e| panic!("the JSON of `tiza scene patch` is not valid: {e}")),
    )
}

/// Every link and image target of a Markdown text: `](target)`.
pub fn link_targets(md: &str) -> Vec<String> {
    md.split("](")
        .skip(1)
        .map(|rest| rest.split(')').next().unwrap_or("").to_string())
        .collect()
}
