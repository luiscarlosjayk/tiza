//! `docs/reference/config.md` against the code: its key tables list the keys
//! that Tiza reads, no more and no fewer, and its example file and its
//! `tiza config` output are what the binary takes and prints.

use std::collections::BTreeSet;
use std::path::Path;

use tiza_core::config::{DEFAULT_KEYS, STYLE_FIELDS};
use tiza_qa::{scratch, tiza_with_config};

fn page() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::fs::read_to_string(root.join("docs/reference/config.md")).unwrap()
}

/// The text under an H2 of the page, up to the next H2.
fn section<'a>(md: &'a str, heading: &str) -> &'a str {
    let start = md
        .find(&format!("\n{heading}\n"))
        .unwrap_or_else(|| panic!("the page has no heading {heading:?}"));
    let rest = &md[start + heading.len() + 2..];
    rest.find("\n## ").map_or(rest, |end| &rest[..end])
}

/// The first cell of each row of the first table of a section, without
/// its backticks.
fn first_column(text: &str) -> BTreeSet<String> {
    text.lines()
        .skip_while(|l| !l.starts_with('|'))
        .take_while(|l| l.starts_with('|'))
        .skip(2)
        .map(|l| {
            l.split('|')
                .nth(1)
                .unwrap()
                .trim()
                .trim_matches('`')
                .to_string()
        })
        .collect()
}

/// The first code block of a section with this info string.
fn block(text: &str, info: &str) -> String {
    let start = text
        .find(&format!("```{info}\n"))
        .unwrap_or_else(|| panic!("no ```{info} block in:\n{text}"));
    let rest = &text[start + info.len() + 4..];
    rest[..rest.find("```").unwrap()].to_string()
}

/// `tiza config` with the example file of the page: (exit code, stdout,
/// stderr).
fn config_of_the_example(md: &str) -> (i32, String, String) {
    let dir = scratch("config-page-example");
    std::fs::write(
        dir.join("config.toml"),
        block(section(md, "## An example"), "toml"),
    )
    .unwrap();
    tiza_with_config(&dir, "config.toml", &["config"])
}

#[test]
fn the_key_tables_list_the_keys_that_tiza_reads() {
    let md = page();
    let mut wrong = Vec::new();
    for (heading, keys) in [
        ("## `[defaults]`", DEFAULT_KEYS),
        ("## `[styles]`", STYLE_FIELDS),
    ] {
        let on_page = first_column(section(&md, heading));
        let in_code: BTreeSet<String> = keys.iter().map(|k| k.to_string()).collect();
        for key in in_code.difference(&on_page) {
            wrong.push(format!(
                "{heading}: Tiza reads {key}, and the table does not list it"
            ));
        }
        for key in on_page.difference(&in_code) {
            wrong.push(format!(
                "{heading}: the table lists {key}, and Tiza does not read it"
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn the_example_file_gives_no_warnings_and_sets_every_default() {
    let (code, out, err) = config_of_the_example(&page());
    assert_eq!(code, 0, "{out}{err}");
    assert!(err.is_empty(), "stderr: {err}");
    assert!(
        out.lines().any(|l| l == "warnings: none"),
        "the example of the page gives warnings:\n{out}"
    );
    for key in DEFAULT_KEYS {
        let line = out
            .lines()
            .find(|l| l.split_whitespace().next() == Some(key))
            .unwrap_or_else(|| panic!("`tiza config` lists no {key}:\n{out}"));
        assert!(
            line.ends_with(" config"),
            "the example of the page does not set {key}: {line:?}"
        );
    }
}

/// The output on the page is a run of the binary with the example file:
/// every line after `file:`, whose path depends on the computer.
#[test]
fn the_tiza_config_output_on_the_page_is_what_tiza_prints() {
    let md = page();
    let (_, out, _) = config_of_the_example(&md);
    let shown = block(section(&md, "## `tiza config`"), "text");
    let after_file = |text: &str| -> Vec<String> {
        text.lines()
            .skip_while(|l| !l.starts_with("file: "))
            .skip(1)
            .map(str::to_string)
            .collect()
    };
    assert_eq!(
        after_file(&shown),
        after_file(&out),
        "run `tiza config` with the example file and copy its output to the page"
    );
}
