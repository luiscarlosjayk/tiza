//! The help of `tiza render` names every format that `--out` writes.

use std::process::Command;

use tiza_qa::bin;

// --- The help of `tiza render` (docs review, 2026-10-24) ---------------

#[test]
fn render_help_names_every_format_that_out_writes() {
    let out = Command::new(bin())
        .args(["render", "--help"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let help = String::from_utf8_lossy(&out.stdout);
    let summary = help.lines().next().unwrap();
    // Each extension `--out` takes, and the name the summary gives it.
    let formats = [
        (".txt", "text"),
        (".md", "Markdown"),
        (".ansi", "ANSI"),
        (".svg", "SVG"),
        (".png", "PNG"),
        (".webp", "WebP"),
        (".gif", "GIF"),
        (".mp4", "MP4"),
        (".webm", "WebM"),
        (".cast", "asciicast"),
        (".tape", "VHS"),
    ];
    let listed = help
        .lines()
        .find(|l| l.contains("The extension picks the format"))
        .expect("the --out help lists the extensions");
    let count = listed.matches("`.").count();
    assert_eq!(count, formats.len(), "{listed}");
    for (ext, name) in formats {
        assert!(
            listed.contains(&format!("`{ext}`")),
            "--out does not list {ext}"
        );
        assert!(
            summary.contains(name),
            "the summary does not name {name}: {summary}"
        );
    }
}
