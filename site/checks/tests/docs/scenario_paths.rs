//! Every path that the demo scenarios name exists.

use std::path::PathBuf;

use tiza_site_checks::code_words;

fn root() -> PathBuf {
    tiza_qa::repo_root()
}

fn doc() -> String {
    std::fs::read_to_string(root().join("docs/demos/scenarios.md"))
        .expect("docs/demos/scenarios.md")
}

#[test]
fn every_path_it_names_exists() {
    let text = doc();
    let mut n = 0;
    for (_, word) in code_words(&text) {
        let w = word.trim_matches(|c: char| "()[],:;\"'".contains(c));
        let w = w.split(':').next().unwrap().trim_end_matches('.');
        if w.starts_with("examples/") || w.starts_with("docs/") || w.starts_with("scripts/") {
            n += 1;
            assert!(
                root().join(w).exists(),
                "the catalogue names {w}, which does not exist"
            );
        }
    }
    assert!(n >= 8, "{n} paths checked");
}
