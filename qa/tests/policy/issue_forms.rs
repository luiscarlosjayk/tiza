//! The issue and discussion forms are valid YAML and ask for what a report
//! needs.

use std::path::PathBuf;
use std::process::Command;

const REPO: &str = "https://github.com/luiscarlosjayk/tiza";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

// --- How to help, and security (AC7) ----------------------------------

/// A YAML file of the repository as JSON, through PyYAML or the Ruby of
/// the system (macOS and the GitHub runners have it); `None` when neither
/// is there.
fn yaml(rel: &str) -> Option<serde_json::Value> {
    let path = root().join(rel);
    let tries: [(&str, &[&str]); 2] = [
        (
            "python3",
            &[
                "-c",
                "import sys, json, yaml; print(json.dumps(yaml.safe_load(open(sys.argv[1]))))",
            ],
        ),
        (
            "ruby",
            &[
                "-ryaml",
                "-rjson",
                "-e",
                "puts JSON.generate(YAML.safe_load(File.read(ARGV[0])))",
            ],
        ),
    ];
    for (tool, args) in tries {
        let Ok(out) = Command::new(tool).args(args).arg(&path).output() else {
            continue;
        };
        if out.status.success() {
            return Some(serde_json::from_slice(&out.stdout).unwrap());
        }
        let err = String::from_utf8_lossy(&out.stderr);
        // A parser that is there and refuses the file is a failure.
        assert!(
            err.contains("No module named") || err.contains("cannot load such file"),
            "{rel} is not valid YAML ({tool}): {err}"
        );
    }
    None
}

#[test]
fn the_issue_and_discussion_forms_are_valid_and_complete() {
    let files = [
        ".github/ISSUE_TEMPLATE/bug.yml",
        ".github/ISSUE_TEMPLATE/config.yml",
        ".github/DISCUSSION_TEMPLATE/ideas.yml",
        ".github/DISCUSSION_TEMPLATE/feedback.yml",
    ];
    for f in files {
        assert!(root().join(f).is_file(), "{f} is missing");
    }
    // As text, always: no blank issues, and the ways out.
    let config = read(".github/ISSUE_TEMPLATE/config.yml");
    assert!(config.contains("blank_issues_enabled: false"));
    assert!(config.contains(&format!("{REPO}/discussions")));
    assert!(config.contains(&format!("{REPO}/security/advisories/new")));
    let Some(bug) = yaml(files[0]) else {
        eprintln!("issue_forms: no PyYAML and no Ruby, the forms are not parsed");
        return;
    };
    let ids = |form: &serde_json::Value| -> Vec<String> {
        form["body"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|b| b["id"].as_str().map(str::to_string))
            .collect()
    };
    assert_eq!(
        ids(&bug),
        [
            "confirm",
            "current-behavior",
            "expected-behavior",
            "reproduction",
            "effect",
            "version",
            "os",
            "terminal",
            "panics-log"
        ]
    );
    let body = bug["body"].as_array().unwrap();
    let kinds = ["markdown", "checkboxes", "textarea", "input"];
    for b in body {
        assert!(kinds.contains(&b["type"].as_str().unwrap()), "{b}");
        if b["type"] != "markdown" {
            assert!(b["attributes"]["label"].is_string(), "{b}");
        }
    }
    // Both boxes are required; everything but panics.log too.
    let boxes = body.iter().find(|b| b["id"] == "confirm").unwrap();
    let options = boxes["attributes"]["options"].as_array().unwrap();
    assert_eq!(options.len(), 2);
    assert!(options.iter().all(|o| o["required"] == true));
    for b in body.iter().filter(|b| b["type"] != "markdown") {
        let id = b["id"].as_str().unwrap();
        if id != "confirm" {
            assert_eq!(b["validations"]["required"], id != "panics-log", "{id}");
        }
    }
    let intro = body[0]["attributes"]["value"].as_str().unwrap();
    assert!(intro.contains("not for ideas, feature requests, feedback or questions"));
    let config = yaml(files[1]).unwrap();
    assert_eq!(config["blank_issues_enabled"], false);
    for link in config["contact_links"].as_array().unwrap() {
        assert!(link["url"].as_str().unwrap().starts_with(REPO), "{link}");
        assert!(
            link["name"].is_string() && link["about"].is_string(),
            "{link}"
        );
    }
    for (f, want) in [
        (files[2], ["problem", "change", "why"]),
        (files[3], ["use", "feedback", "version"]),
    ] {
        let form = yaml(f).unwrap();
        assert!(form["title"].is_string(), "{f}");
        assert_eq!(ids(&form), want, "{f}");
    }
}
