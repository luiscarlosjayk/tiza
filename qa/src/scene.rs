//! Helpers of the tests that read the scene: the steps, the findings of
//! `check`, where each annotation is and the names of the scenes.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::Live;

/// An annotation as a test compares it: id, step, file, line.
pub type Place = (String, u64, Option<String>, u64);

/// `tiza render SOURCE --all-steps --format txt` (SOURCE: a file, or the
/// live instance with `--socket`): one text per step.
pub fn all_steps(args: &[&str], out: &Path) -> Vec<String> {
    let _ = std::fs::remove_dir_all(out);
    let r = crate::command()
        .arg("render")
        .args(args)
        .args(["--all-steps", "--format", "txt", "--width", "100", "--out"])
        .arg(format!("{}/", out.display()))
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "render {args:?}: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    let mut files: Vec<PathBuf> = std::fs::read_dir(out)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    files
        .iter()
        .map(|f| std::fs::read_to_string(f).unwrap())
        .collect()
}

/// The findings of `check` on the live app.
pub fn findings(live: &Live) -> Vec<Value> {
    live.api_ok("check", json!({}))["findings"]
        .as_array()
        .unwrap()
        .clone()
}

/// A [`Place`] from its parts.
pub fn place(id: &str, step: u64, file: Option<&str>, line: u64) -> Place {
    (id.into(), step, file.map(str::to_string), line)
}

/// The screen row that draws the note `label`.
pub fn row_of(screen: &str, label: &str) -> Option<String> {
    screen
        .lines()
        .find(|l| l.contains(&format!("┆ {label}")))
        .map(str::to_string)
}

/// The number of annotations and the revision.
pub fn state(live: &Live) -> (u64, u64) {
    let s = live.api_ok("status", json!({}));
    (
        s["annotations"].as_u64().unwrap(),
        s["revision"].as_u64().unwrap(),
    )
}

/// The names that `scene.list` gives.
pub fn list(live: &Live) -> Vec<String> {
    live.api_ok("scene.list", json!({}))
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}
