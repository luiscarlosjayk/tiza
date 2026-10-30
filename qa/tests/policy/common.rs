//! Helpers for more than one module of the policy area.

// --- The Markdown of the docs --------------------------------------------

/// The fence that opens or closes a fenced block on `line`: three or more
/// backticks or tildes after the indent, as `FENCE_RE` of `site/build.py`
/// reads it.
fn fence_of(line: &str) -> Option<String> {
    let t = line.trim_start();
    let c = t.chars().next().filter(|&c| c == '`' || c == '~')?;
    let len = t.chars().take_while(|&x| x == c).count();
    (len >= 3).then(|| c.to_string().repeat(len))
}

/// The words of the code spans of a line: a run of backticks up to the
/// next run of the same length.
fn span_words(line: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        let ticks = rest.len() - rest.trim_start_matches('`').len();
        let len = if ticks > 0 {
            ticks
        } else {
            rest.find('`').unwrap_or(rest.len())
        };
        parts.push(&rest[..len]);
        rest = &rest[len..];
    }
    let mut words = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        if parts[i].starts_with('`')
            && let Some(j) = (i + 1..parts.len()).find(|&j| parts[j] == parts[i])
        {
            words.extend(
                parts[i + 1..j]
                    .concat()
                    .split_whitespace()
                    .map(str::to_string),
            );
            i = j + 1;
            continue;
        }
        i += 1;
    }
    words
}

/// The words of the code of a page, with their line (from 1): the words of
/// each code span and of each fenced block. A fence closes as `site/build.py`
/// reads it: the same character, at least as many, alone on its line. The
/// words are as they are: the caller takes off the punctuation.
pub fn code_words(md: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut fence: Option<String> = None;
    for (i, line) in md.lines().enumerate() {
        let found = fence_of(line);
        if let Some(open) = &fence {
            let closes = found.as_ref().is_some_and(|f| {
                f.starts_with(&open[..1]) && f.len() >= open.len() && line.trim() == f
            });
            if closes {
                fence = None;
            } else {
                out.extend(line.split_whitespace().map(|w| (i + 1, w.to_string())));
            }
            continue;
        }
        if found.is_some() {
            fence = found;
            continue;
        }
        out.extend(span_words(line).into_iter().map(|w| (i + 1, w)));
    }
    out
}

const FRAME_START: &str = "<!-- readme-images:frame ";
const FRAME_END: &str = "<!-- /readme-images:frame -->";

/// A text frame of a page, between the markers that
/// `scripts/readme-images.sh` reads.
pub struct Frame {
    /// The line of the first marker, from 1.
    pub line: usize,
    /// The indent of the first marker: each line of the frame that is not
    /// blank has it.
    pub indent: String,
    pub doc: String,
    pub step: u32,
    pub width: u32,
    /// The lines between the two markers.
    pub body: Vec<String>,
}

/// The text frames of `md`, the page `page`. A marker without a field, or
/// without its end marker, fails with the page and the line.
pub fn frames(page: &str, md: &str) -> Vec<Frame> {
    let lines: Vec<&str> = md.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start();
        let Some(fields) = t.strip_prefix(FRAME_START) else {
            i += 1;
            continue;
        };
        let n = i + 1;
        let fields = fields
            .strip_suffix(" -->")
            .unwrap_or_else(|| panic!("{page}:{n}: the frame marker does not end with -->"));
        let field = |name: &str| -> String {
            fields
                .split_whitespace()
                .find_map(|f| f.strip_prefix(name)?.strip_prefix('='))
                .unwrap_or_else(|| panic!("{page}:{n}: the frame marker has no {name}="))
                .to_string()
        };
        let number = |name: &str| -> u32 {
            field(name)
                .parse()
                .unwrap_or_else(|e| panic!("{page}:{n}: {name}= of the frame marker: {e}"))
        };
        let end = (i + 1..lines.len())
            .find(|&j| {
                lines[j].trim() == FRAME_END || lines[j].trim_start().starts_with(FRAME_START)
            })
            .filter(|&j| lines[j].trim() == FRAME_END)
            .unwrap_or_else(|| panic!("{page}:{n}: the frame marker has no {FRAME_END} after it"));
        out.push(Frame {
            line: n,
            indent: lines[i][..lines[i].len() - t.len()].to_string(),
            doc: field("doc"),
            step: number("step"),
            width: number("width"),
            body: lines[i + 1..end].iter().map(|l| l.to_string()).collect(),
        });
        i = end + 1;
    }
    out
}
