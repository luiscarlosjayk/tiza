//! Each text frame of the docs is what `tiza render` draws now, and at the
//! width of the frame it cuts no label and no code. `scripts/readme-images.sh`
//! writes the frames between their markers.

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;
use tiza_qa::{repo_file, repo_root, repository_files, tiza, tiza_ok};

use crate::common::frames;

/// The code that a document shows: its file and the file of each
/// `code.link` move, from the folder of the document.
fn code_of(doc: &Path) -> String {
    let d: Value = serde_json::from_str(&std::fs::read_to_string(doc).unwrap()).unwrap();
    let dir = doc.parent().unwrap();
    let mut code = d["code"]["text"].as_str().unwrap_or_default().to_string();
    let linked = d["moves"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["action"] == "code.link")
        .filter_map(|m| m["file"].as_str());
    for file in d["code"]["file"].as_str().into_iter().chain(linked) {
        code += &std::fs::read_to_string(dir.join(file))
            .unwrap_or_else(|e| panic!("{}: {file}: {e}", doc.display()));
    }
    code
}

/// The test reads the pages of `docs/` that git tracks, as the script does.
#[test]
fn every_text_frame_is_what_tiza_renders_at_its_width() {
    let mut checked = 0;
    let mut checked_docs = HashSet::new();
    let mut wrong = Vec::new();
    let pages = repository_files()
        .into_iter()
        .filter(|f| f.starts_with("docs/") && f.ends_with(".md"));
    for page in pages {
        for f in frames(&page, &repo_file(&page)) {
            checked += 1;
            let at = format!("{page}:{}: the frame of {} step {}", f.line, f.doc, f.step);
            let doc = repo_root().join(&f.doc);
            let (doc_arg, step, width) = (
                doc.to_str().unwrap(),
                f.step.to_string(),
                f.width.to_string(),
            );
            let fresh = tiza_ok(&[
                "render", doc_arg, "--step", &step, "--width", &width, "--format", "md", "--out",
                "-",
            ]);
            let want: Vec<String> = fresh
                .trim_end_matches('\n')
                .split('\n')
                .map(|l| {
                    if l.is_empty() {
                        String::new()
                    } else {
                        format!("{}{l}", f.indent)
                    }
                })
                .collect();
            if f.body != want {
                wrong.push(format!(
                    "{at} at width {width} is stale: run scripts/readme-images.sh --text, then \
                     check the text around it (in the tour: the line numbers in the prompts of \
                     steps 4 to 8)"
                ));
                continue;
            }
            // `tiza check` reads every step: once for each document and width.
            if checked_docs.insert((f.doc.clone(), f.width)) {
                let check = tiza(&["check", doc_arg, "--width", &width, "--json"]);
                let report: Value = serde_json::from_slice(&check.stdout)
                    .unwrap_or_else(|e| panic!("{at}: tiza check --json: {e}"));
                if report["errors"] != 0 || report["warnings"] != 0 {
                    wrong.push(format!(
                        "{page}:{}: tiza check finds labels of {} that it cuts or does not \
                         draw at width {width}: {}",
                        f.line, f.doc, report["findings"]
                    ));
                }
            }
            if f.body.iter().any(|l| l.contains('…')) && !code_of(&doc).contains('…') {
                wrong.push(format!(
                    "{at}: the frame has a `…` that the code does not have: width {width} cuts \
                     a line or a label"
                ));
            }
        }
    }
    assert!(checked > 0, "no text frame in the pages of docs/");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
