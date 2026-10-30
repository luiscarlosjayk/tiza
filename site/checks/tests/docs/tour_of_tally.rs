//! The tour of tally, `docs/demos/tour-of-tally.md`, against the document
//! that its prompts build, `examples/tally-tour.tiza`: a text frame for
//! each step, the names of the steps, and the paths that the page names.

use serde_json::Value;
use tiza_qa::markdown::scene_patch_json;
use tiza_qa::{repo_file, repo_root, tiza_ok};

use tiza_site_checks::{code_words, frames};

const PAGE: &str = "docs/demos/tour-of-tally.md";
const DOC: &str = "examples/tally-tour.tiza";

#[test]
fn the_page_has_a_frame_for_each_step_of_the_tour() {
    let doc = repo_root().join(DOC);
    let check: Value = serde_json::from_str(&tiza_ok(&[
        "check",
        doc.to_str().unwrap(),
        "--width",
        "130",
        "--json",
    ]))
    .unwrap();
    let max = check["max_step"]
        .as_u64()
        .expect("max_step in tiza check --json") as u32;
    let mut steps: Vec<u32> = frames(PAGE, &repo_file(PAGE))
        .iter()
        .filter(|f| f.doc == DOC)
        .map(|f| f.step)
        .collect();
    steps.sort();
    assert_eq!(
        steps,
        (1..=max).collect::<Vec<u32>>(),
        "{PAGE} must have one frame of {DOC} for each step from 1 to {max}. Add or remove a \
         marker, then run scripts/readme-images.sh --text"
    );
}

#[test]
fn the_step_names_on_the_page_are_those_of_the_document() {
    let patch = scene_patch_json(&repo_file(PAGE)).expect("the page names the steps");
    let doc: Value = serde_json::from_str(&repo_file(DOC)).unwrap();
    assert_eq!(
        patch["steps"], doc["steps"],
        "the `tiza scene patch` command of {PAGE} gives other steps than {DOC}"
    );
}

/// The tutorial copies `examples/tally` to `/tmp/tally`, and Tiza saves
/// the tour of `/tmp/tally/src/main.rs` as `/tmp/tally/src/main.rs.tiza`.
#[test]
fn every_path_the_page_names_exists() {
    let md = repo_file(PAGE);
    // The code in a frame is what Tiza draws, not a path of the page.
    let in_frame: Vec<(usize, usize)> = frames(PAGE, &md)
        .iter()
        .map(|f| (f.line, f.line + f.body.len() + 1))
        .collect();
    let mut n = 0;
    let mut wrong = Vec::new();
    for (line, word) in code_words(&md) {
        if in_frame.iter().any(|&(a, b)| (a..=b).contains(&line)) {
            continue;
        }
        let w = word.trim_matches(|c: char| "()[],:;\"'".contains(c));
        let w = w.split(':').next().unwrap().trim_end_matches('.');
        let (path, copy) = if w == "/tmp/tally" {
            ("examples/tally".to_string(), true)
        } else if let Some(rest) = w.strip_prefix("/tmp/tally/") {
            (format!("examples/tally/{rest}"), true)
        } else if w.starts_with("examples/") || w.starts_with("docs/") || w.starts_with("scripts/")
        {
            (w.to_string(), false)
        } else {
            continue;
        };
        n += 1;
        let exists = repo_root().join(&path).exists()
            || (copy
                && path
                    .strip_suffix(".tiza")
                    .is_some_and(|code| repo_root().join(code).exists()));
        if !exists {
            wrong.push(format!("{PAGE}:{line}: {w} does not exist"));
        }
    }
    assert!(n >= 8, "{n} paths checked on {PAGE}");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
