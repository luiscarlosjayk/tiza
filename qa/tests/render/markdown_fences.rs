//! A fenced code block of a Markdown file indented by 1 to 3 spaces got no
//! colors of its language, and the rest of the file lost its colors too:
//! the grammar knew a fence only at column 0, and the `` ``` `` of an
//! indented fence started a code span. CommonMark allows a fence indented
//! by up to 3 spaces; 4 spaces, or a tab, is an indented code block.
//!
//! The oracle is the same file with its fences at column 0: every line
//! that is the same in both files gets the same colors. Each file is a
//! `.md` in a scratch folder, drawn by `tiza render --format ansi`.

use tiza_qa::{scratch, tiza_ok};

/// A Markdown file with a fence of rust: `open` and `close` go before its
/// two fence lines (`fence` is `` ``` `` or `~~~`). After it, Markdown and
/// a fence at column 0, to see that the rest of the file keeps its colors.
fn doc(open: &str, close: &str, fence: &str) -> String {
    format!(
        "# Fences\n\nSome *text*.\n\n{open}{fence}rust\nfn main() {{\n    let x = 1;\n}}\n{close}{fence}\n\n\
         After the fence, **bold** and `code`.\n\n```rust\nlet z = 3;\n```\n"
    )
}

/// The rows of `tiza render FILE --format ansi` for `text` as a `.md`.
fn ansi_rows(name: &str, text: &str) -> Vec<String> {
    let path = scratch(name).join("fence.md");
    std::fs::write(&path, text).unwrap();
    let out = tiza_ok(&[
        "render",
        path.to_str().unwrap(),
        "--width",
        "80",
        "--format",
        "ansi",
    ]);
    out.lines().map(str::to_string).collect()
}

/// The rows of the two files that differ, for the lines whose text is the
/// same in both (1-based line numbers).
fn different_rows(name: &str, a: &str, b: &str) -> Vec<usize> {
    let (ra, rb) = (
        ansi_rows(&format!("{name}-a"), a),
        ansi_rows(&format!("{name}-b"), b),
    );
    assert_eq!(ra.len(), rb.len(), "{name}: not the same number of rows");
    a.lines()
        .zip(b.lines())
        .enumerate()
        .filter(|(_, (la, lb))| la == lb)
        .filter(|&(i, _)| ra[i] != rb[i])
        .map(|(i, _)| i + 1)
        .collect()
}

/// The line numbers of what comes after the fence.
const AFTER: [usize; 5] = [11, 12, 13, 14, 15];

#[test]
fn a_fence_indented_by_one_to_three_spaces_gets_the_colors_of_its_language() {
    for fence in ["```", "~~~"] {
        let flat = doc("", "", fence);
        for n in 1..=3 {
            let pad = " ".repeat(n);
            let differ = different_rows(&format!("pad{n}"), &doc(&pad, &pad, fence), &flat);
            assert!(
                differ.is_empty(),
                "a {fence} fence after {n} spaces: lines {differ:?} have other colors than at column 0"
            );
        }
    }
}

#[test]
fn a_closing_fence_indented_by_one_to_three_spaces_closes_the_block() {
    let flat = doc("", "", "```");
    for (open, close) in [("", "   "), ("  ", ""), (" ", "  ")] {
        let differ = different_rows("close", &doc(open, close, "```"), &flat);
        assert!(
            differ.is_empty(),
            "open after {} and close after {} spaces: lines {differ:?} differ",
            open.len(),
            close.len()
        );
    }
}

/// The color (`R;G;B`) of the first run of `row` whose text, trimmed, is
/// `word`.
fn color_of(row: &str, word: &str) -> Option<String> {
    // Each run: `ESC[38;2;R;G;Bm`, maybe `ESC[48;2;R;G;Bm`, the text, then
    // the next `ESC`.
    row.split("\x1b[38;2;").skip(1).find_map(|run| {
        let (rgb, mut rest) = run.split_once('m')?;
        if let Some(bg) = rest.strip_prefix("\x1b[48;2;") {
            rest = bg.split_once('m')?.1;
        }
        let text = rest.split('\x1b').next().unwrap_or("");
        (text.trim() == word).then(|| rgb.to_string())
    })
}

/// `doc` with each line of the block (its two fences and its code) after
/// `pad`: an indented code block when `pad` is 4 spaces or a tab.
fn indented(pad: &str) -> String {
    doc("", "", "```")
        .lines()
        .enumerate()
        .map(|(i, l)| {
            if (5..=9).contains(&(i + 1)) {
                format!("{pad}{l}")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn four_spaces_or_a_tab_are_not_a_fence_and_the_rest_keeps_its_colors() {
    let flat = doc("", "", "```");
    let keyword = color_of(&ansi_rows("keyword", &flat)[13], "let").expect("a `let` on line 14");
    for (name, pad) in [("four", "    "), ("tab", "\t")] {
        let text = indented(pad);
        let rows = ansi_rows(name, &text);
        // Not a fence: no run of its code has the color of a keyword.
        assert!(
            !rows[6].contains(&format!("\x1b[38;2;{keyword}m")),
            "{name}: a fence:\n{}",
            rows[6]
        );
        // After it, the rest of the file is as at column 0.
        let differ = different_rows(name, &text, &flat);
        assert!(
            !AFTER.iter().any(|l| differ.contains(l)),
            "{name}: the rest of the file has other colors on lines {differ:?}"
        );
    }
}

#[test]
fn a_fence_in_a_list_item_keeps_the_colors_of_its_language() {
    // The fence of the item is 2 spaces in: the grammar already knew it,
    // and the fence goes to syntect without its indent now. After the
    // item, a fence at column 0: its `let` has the color of the keyword.
    let text = "- a step:\n\n  ```rust\n  let x = 1;\n  ```\n\n```rust\nlet z = 3;\n```\n";
    let rows = ansi_rows("item", text);
    let keyword = color_of(&rows[7], "let").expect("a `let` on line 8");
    assert_eq!(
        color_of(&rows[3], "let").as_deref(),
        Some(keyword.as_str()),
        "the `let` of the fence in the item:\n{}",
        rows[3]
    );
}
