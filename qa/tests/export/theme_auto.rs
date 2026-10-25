//! `tiza render --theme auto` exports the theme of the document.

use tiza_qa::{bin, scratch};

const CODE: &str = "fn main() {\n    println!(\"hi\");\n}\n";

#[test]
fn render_with_theme_auto_exports_the_theme_of_the_document() {
    use tiza_core::{Theme, ThemeName};
    let dir = scratch("render-auto");
    let mut doc = serde_json::to_value(tiza_core::Document::from_code(CODE, "rs", None)).unwrap();
    doc["theme"] = serde_json::json!("light");
    let path = dir.join("d.tiza");
    std::fs::write(&path, doc.to_string()).unwrap();
    let hex = |name| {
        let c = Theme::from_name(name).bg;
        format!("#{:02x}{:02x}{:02x}", c.0, c.1, c.2)
    };
    let (light, dark) = (hex(ThemeName::Light), hex(ThemeName::Dark));
    let out = std::process::Command::new(bin())
        .args([
            "render",
            path.to_str().unwrap(),
            "--format",
            "svg",
            "--no-embed-fonts",
        ])
        .args(["--theme", "auto"])
        .env("TIZA_CONFIG", dir.join("no-config.toml"))
        .output()
        .unwrap();
    let svg = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(svg.contains(&light), "not the light bg {light}");
    assert!(!svg.contains(&dark), "the dark bg {dark}");
    // terminal: dark, and the export says so.
    let out = std::process::Command::new(bin())
        .args([
            "render",
            path.to_str().unwrap(),
            "--format",
            "svg",
            "--no-embed-fonts",
        ])
        .args(["--theme", "terminal"])
        .env("TIZA_CONFIG", dir.join("no-config.toml"))
        .output()
        .unwrap();
    let (svg, err) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{err}");
    assert!(svg.contains(&dark), "not the dark bg {dark}");
    assert!(err.contains("exported with dark"), "{err}");
}
