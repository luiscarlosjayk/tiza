//! Scenario 12 of `docs/demos/scenarios.md` gives the patch of the steps
//! that the MCP test of the scenario runs
//! (`qa/fixtures/mcp/agent_scenarios/scenario-12-patch.json`).

use serde_json::Value;
use tiza_qa::markdown::scene_patch_json;
use tiza_qa::{fixture, repo_file};

/// The numbered scenario sections (`## 1. Title` …), as (title, body).
fn scenarios() -> Vec<(String, String)> {
    let text = repo_file("docs/demos/scenarios.md");
    let mut out = Vec::new();
    let mut parts = text.split("\n## ").skip(1);
    for part in parts.by_ref() {
        let (title, body) = part.split_once('\n').unwrap_or((part, ""));
        let numbered = title
            .split_once(". ")
            .is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()));
        if numbered {
            out.push((title.to_string(), body.to_string()));
        }
    }
    out
}

#[test]
fn scenario_12_gives_the_patch_that_the_mcp_test_runs() {
    let s = scenarios();
    let (title, body) = &s[11];
    assert!(title.starts_with("12. "), "the 12th scenario is {title:?}");
    let page = scene_patch_json(body).expect("scenario 12 patches the steps");
    let path = fixture("mcp/agent_scenarios/scenario-12-patch.json");
    let tested: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        page,
        tested,
        "scenario 12 of docs/demos/scenarios.md gives another patch than {}: copy the one of the page there, and run the MCP test",
        path.display()
    );
}
