//! Every test starts the `tiza` binary with an empty config, so the config
//! file of whoever runs the tests never changes a result: through
//! `tiza_qa::command()` (or a helper that uses it) in `qa/tests`, and with
//! `TIZA_CONFIG` and `XDG_CONFIG_HOME` set in a crate's own tests.
//! `qa/tests/env/developer_config.rs` checks the helpers with a config in
//! each place that Tiza looks.

use std::path::{Path, PathBuf};

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// The lines of a file of `qa/tests` that start the binary directly.
fn direct_starts(text: &str) -> Vec<usize> {
    // Built here, so this file does not name them.
    let direct: Vec<String> = [
        "bin()",
        "&bin()",
        "bin",
        "&bin",
        "tiza_qa::bin()",
        "&tiza_qa::bin()",
    ]
    .iter()
    .map(|b| format!("Command::new({b})"))
    .collect();
    text.lines()
        .enumerate()
        .filter(|(_, line)| direct.iter().any(|d| line.contains(d.as_str())))
        .map(|(i, _)| i + 1)
        .collect()
}

/// Whether a file of a crate's tests starts the binary without an empty
/// config.
fn starts_with_a_config(text: &str) -> bool {
    text.contains(&format!("CARGO_BIN_EXE_{}", "tiza"))
        && !(text.contains("\"TIZA_CONFIG\"") && text.contains("\"XDG_CONFIG_HOME\""))
}

#[test]
fn every_qa_test_starts_tiza_through_tiza_qa_command() {
    let mut files = Vec::new();
    rs_files(&root().join("qa/tests"), &mut files);
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        for n in direct_starts(&text) {
            found.push(format!(
                "{}:{n}",
                file.strip_prefix(root()).unwrap().display()
            ));
        }
    }
    assert!(
        found.is_empty(),
        "start tiza with tiza_qa::command(), not with Command::new: {found:?}"
    );
}

#[test]
fn a_crate_test_that_starts_tiza_gives_it_an_empty_config() {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root().join("crates")).unwrap() {
        let tests = entry.unwrap().path().join("tests");
        if tests.is_dir() {
            rs_files(&tests, &mut files);
        }
    }
    let found: Vec<String> = files
        .iter()
        .filter(|f| starts_with_a_config(&std::fs::read_to_string(f).unwrap()))
        .map(|f| f.strip_prefix(root()).unwrap().display().to_string())
        .collect();
    assert!(
        found.is_empty(),
        "these tests start tiza without TIZA_CONFIG and XDG_CONFIG_HOME: {found:?}"
    );
}

#[test]
fn the_checks_find_a_test_that_starts_tiza_with_a_config() {
    let qa_test = format!(
        "let b = tiza_qa::bin();\nlet out = Command::new({}).arg(\"render\");\n\
         let out = Command::new({}).output();\nlet ok = tiza_qa::command();\n",
        "&b".replace('b', "bin"),
        "tiza_qa::bin()"
    );
    assert_eq!(direct_starts(&qa_test), [2, 3]);
    let crate_test = format!("Command::new(env!(\"CARGO_BIN_EXE_{}\"))", "tiza");
    assert!(starts_with_a_config(&crate_test));
    let isolated = format!("{crate_test}.env(\"TIZA_CONFIG\", f).env(\"XDG_CONFIG_HOME\", d)");
    assert!(!starts_with_a_config(&isolated));
}
