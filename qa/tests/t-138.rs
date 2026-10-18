//! t-138 — TypeScript highlighting is ~10x slower per line than Rust
//! (3,000-line file: 1.65 s to render).
//!
//! Acceptance criteria:
//! - AC1 Profile the TypeScript render (which grammar contexts / regexes cost
//!   the time) and write the finding in the note. (Board note: the time is
//!   the grammar under fancy-regex — 251 distinct regexes, none above 6 % of
//!   the search time, ~45 % of a 600-line run compiling them, the rest in
//!   fancy-regex's backtracking VM; no repeated work or cache miss in Tiza.)
//! - AC2 If a cause on our side is found, fix it;
//!   render_txt_typescript_3000_lines at least 2x faster, the Rust scenarios
//!   unchanged. (None on our side; the safe mitigation is to highlight long
//!   files in chunks on several threads, which must give exactly the
//!   sequential output — that is what these tests check.)
//! - AC3 If the cost is the grammar itself, record the measurement and the
//!   options (onig, another TS grammar, a cap with plain fallback) for Luis.
//!   (Board note.)
//! - AC4 Update qa/bench/baseline.json with --update-baseline --note in the
//!   same commit.

use tiza_core::highlight::{
    MAX_THREADS, MIN_CHUNK_LINES, Segment, default_threads, highlight, highlight_chunked,
};
use two_face::theme::EmbeddedThemeName;

const THEME: EmbeddedThemeName = EmbeddedThemeName::OneHalfDark;

/// The bench's generated TypeScript (scripts/bench.py `typescript_large`),
/// cut to `n` lines.
fn typescript(n: usize) -> Vec<String> {
    let mut out = vec![
        "import { readFile } from \"node:fs/promises\";".to_string(),
        String::new(),
    ];
    let mut i = 0;
    while out.len() < n {
        out.extend([
            format!("export interface Order{i}<T extends {{ id: string }}> {{"),
            "  readonly id: `order-${number}`;".into(),
            "  items: ReadonlyArray<T>;".into(),
            "  total?: number | null;".into(),
            "}".into(),
            String::new(),
            format!("export class Service{i} implements Disposable {{"),
            format!("  #cache = new Map<string, Order{i}<{{ id: string }}>>();"),
            format!("  constructor(private readonly base: URL, private retries = {}) {{}}", i % 5),
            format!("  async load(path: string): Promise<Order{i}<{{ id: string }}>[]> {{"),
            "    const raw = await readFile(new URL(path, this.base), \"utf8\");".into(),
            "    const parsed: unknown = JSON.parse(raw);".into(),
            format!("    if (!Array.isArray(parsed)) throw new TypeError(`bad ${{path}} (#{i})`);"),
            format!(
                "    return parsed.filter((o): o is Order{i}<{{ id: string }}> => typeof o?.id === \"string\")"
            ),
            format!("      .map((o) => ({{ ...o, total: o.items.reduce((n, _) => n + 1, 0) * {i} }}));"),
            "  }".into(),
            "  [Symbol.dispose](): void { this.#cache.clear(); }".into(),
            "}".into(),
            String::new(),
        ]);
        i += 1;
    }
    out.truncate(n);
    out
}

fn rust(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| match i % 5 {
            0 => format!("/// Item {i}"),
            1 => format!("pub fn f{i}(s: String) -> usize {{"),
            2 => "    let n = s.len(); // \"not a string\"".into(),
            3 => "    n".into(),
            _ => "}".into(),
        })
        .collect()
}

fn sequential(code: &[String], lang: &str) -> Vec<Vec<Segment>> {
    let (out, chunking) = highlight_chunked(code, lang, THEME, 1);
    assert_eq!(chunking.starts, vec![0]);
    out
}

#[test]
fn ac2_chunked_output_is_the_sequential_one_for_ts_tsx_js_and_rust() {
    let ts = typescript(800);
    let rs = rust(1_200);
    for (code, lang) in [(&ts, "ts"), (&ts, "tsx"), (&ts, "js"), (&rs, "rs")] {
        let want = sequential(code, lang);
        for threads in [2, 4] {
            let (got, chunking) = highlight_chunked(code, lang, THEME, threads);
            assert!(chunking.starts.len() > 1, "{lang} x{threads}: not split");
            assert_eq!(got.len(), code.len(), "{lang} x{threads}");
            assert!(
                got == want,
                "{lang} x{threads}: output differs from sequential"
            );
        }
    }
}

#[test]
fn ac2_boundaries_land_at_top_level_on_ordinary_typescript() {
    // The generated file returns to the top level after every `}` + blank
    // line, so no speculative chunk should be thrown away.
    let ts = typescript(1_200);
    let (_, chunking) = highlight_chunked(&ts, "ts", THEME, 4);
    assert_eq!(chunking.redone, 0, "{chunking:?}");
    for &s in &chunking.starts[1..] {
        assert!(
            ts[s - 1].trim().is_empty(),
            "boundary {s} not after a blank line"
        );
        assert!(!ts[s].starts_with(' '), "boundary {s} is indented");
    }
}

#[test]
fn corner_boundary_inside_a_block_comment_is_redone_and_still_exact() {
    // A block comment that spans every boundary, full of blank lines and
    // non-indented lines that look like top level.
    let mut code = vec!["/*".to_string()];
    for i in 0..1_000 {
        code.push(if i % 2 == 0 {
            String::new()
        } else {
            format!("const x{i} = `${{y}}`; // inside the comment")
        });
    }
    code.push("*/".into());
    code.extend(typescript(200));
    let want = sequential(&code, "ts");
    let (got, chunking) = highlight_chunked(&code, "ts", THEME, 4);
    assert!(chunking.redone >= 1, "{chunking:?}");
    assert!(got == want, "output differs from sequential");
}

#[test]
fn corner_template_literal_and_python_string_across_boundaries() {
    let mut ts = vec!["const sql = `".to_string()];
    for i in 0..900 {
        ts.push(if i % 3 == 0 {
            String::new()
        } else {
            format!("SELECT {i} ${{x}}")
        });
    }
    ts.push("`;".into());
    ts.extend(typescript(100));
    assert!(highlight_chunked(&ts, "ts", THEME, 3).0 == sequential(&ts, "ts"));

    let mut py = vec!["DOC = \"\"\"".to_string()];
    for i in 0..900 {
        py.push(if i % 2 == 0 {
            String::new()
        } else {
            format!("def f{i}(): pass")
        });
    }
    py.push("\"\"\"".into());
    py.push("def main():".into());
    py.push("    return DOC".into());
    assert!(highlight_chunked(&py, "py", THEME, 3).0 == sequential(&py, "py"));
}

#[test]
fn corner_minified_single_line_and_tiny_files_stay_on_one_thread() {
    let line: String = (0..400)
        .map(|i| {
            format!("var a{i}=function(b){{return b.map(function(c){{return c*{i}+\"x\"}})}};")
        })
        .collect();
    let one = vec![line];
    for lang in ["js", "ts"] {
        let (got, chunking) = highlight_chunked(&one, lang, THEME, 8);
        assert_eq!(chunking.starts, vec![0], "{lang}");
        let text: String = got[0].iter().map(|s| s.text.as_str()).collect();
        assert_eq!(text, one[0]);
    }
    let (got, chunking) = highlight_chunked(&[], "ts", THEME, 8);
    assert!(got.is_empty());
    assert_eq!(chunking.starts, vec![0]);
    let three = typescript(3);
    assert!(highlight_chunked(&three, "ts", THEME, 8).0 == sequential(&three, "ts"));
}

#[test]
fn corner_plain_text_is_never_split() {
    let code: Vec<String> = (0..2_000).map(|i| format!("line {i}")).collect();
    let (got, chunking) = highlight_chunked(&code, "txt", THEME, 4);
    assert_eq!(chunking.starts, vec![0]);
    assert_eq!(got.len(), 2_000);
}

#[test]
fn ac2_thread_count_follows_the_file_size() {
    assert_eq!(default_threads(0), 1);
    assert_eq!(default_threads(MIN_CHUNK_LINES * 2 - 1), 1);
    let cpus = std::thread::available_parallelism().map_or(1, |n| n.get());
    let big = default_threads(100_000);
    assert!(big <= MAX_THREADS && big <= cpus.max(1), "{big}");
    // `highlight` (what layout and every export call) is the chunked path.
    let ts = typescript(MIN_CHUNK_LINES * 3);
    assert!(highlight(&ts, "ts", THEME) == sequential(&ts, "ts"));
}
