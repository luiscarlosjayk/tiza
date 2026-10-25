//! t-046 — Catalogue of demo scenarios: ways to use Tiza with agents
//! (codebase tours, agent-to-agent explanations, reviews), as ideas for
//! recordings.
//!
//! Acceptance criteria:
//! - AC1 `docs/demos/scenarios.md` (English) with 10-15 scenarios, each with
//!   who it is for, the setup (a sample under `examples/`, a real demo), the
//!   interaction script, what the audience sees, the features it exercises
//!   and recording notes.
//! - AC2 At least the two scenarios Luis named: a codebase tour while an
//!   agent draws on a few files; one agent explaining code to another for
//!   review or knowledge sharing.
//! - AC3 One scenario fully worked as a runnable example: a multi-file
//!   `.tiza` with steps and links (`examples/tally-tour.tiza` over
//!   `examples/tally/`) plus the exact prompt list.
//! - AC4 A closing list of what each scenario needs that Tiza does not do yet.
//!   After Luis's rework (2026-10-18) it names only what will be built
//!   (t-166), and no scenario depends on a discarded feature: no
//!   arrows between files or two files at once, no author metadata, no
//!   resource notifications, no diff import, no `--no-save` (the scenarios work on a copy and keep the sidecar the
//!   TUI autosaves).
//! - AC5 No marketing or channel tasks.
//!
//! The document is prose, so these tests pin what can drift: its shape, the
//! paths it names, and the worked example — `tiza check` clean at the width
//! it is recorded at, each step showing its file, and the MCP tool calls the
//! prompt list leads to rebuilding exactly the frames of the shipped `.tiza`.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn doc() -> String {
    std::fs::read_to_string(root().join("docs/demos/scenarios.md"))
        .expect("docs/demos/scenarios.md")
}

#[test]
fn ac1_every_path_it_names_exists() {
    let text = doc();
    let mut n = 0;
    for chunk in text.split('`').skip(1).step_by(2) {
        for word in chunk.split_whitespace() {
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
    }
    assert!(n >= 8, "{n} paths checked");
}
