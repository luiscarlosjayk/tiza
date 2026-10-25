//! Theme bugs that the check of the themes page found.
//!
//! Acceptance criteria:
//! - AC1 With `--no-save`, `--theme-file` does not write the `.tiza`
//!   document at start: no new file, and a file that is there keeps its
//!   bytes. Without `--no-save` it writes the tokens at start, as before.
//! - AC2 A `--theme-file` on the command line is not an edit that `u`
//!   takes back, as for `--theme`: after `u` the tokens stay.
//! - AC3 The `[dark]`, `[light]` or `[terminal]` subtable of the theme in
//!   use wins over the flat tokens, whatever the names of the keys.
//! - AC4 A text token that reads under 4.5:1 on the background moves toward
//!   black or white, whichever contrasts more, until it reads at 4.5:1:
//!   the notice that says so is always true.
//! - AC5 A value that is not a color string leaves out only that token, in
//!   the config file and in a theme file, with a note. The config file
//!   names a key other than `theme`. A document with such a value does not
//!   open, and the error says what a token must be. A theme file with no
//!   color token to apply (empty, or no value is a string) is refused and
//!   changes nothing: the tokens of the document stay.
//!   open, and the error says what a token must be.
//! - AC6 MCP `export_image` applies the config file, as `tiza render`
//!   does. A config file that does not parse is said in the reply, and the
//!   export goes on.
//! - AC7 `tiza render --theme auto` exports the theme of the document (else
//!   `dark`), and `--theme terminal` exports `dark` and says so, as the help
//!   says.
//!
//! Corner cases: `:theme file PATH` keeps its undo entry; `--theme-file`
//! still says how many tokens it applied.

use tiza_qa::{bin, scratch};

const CODE: &str = "fn main() {\n    println!(\"hi\");\n}\n";

#[test]
fn ac6_export_image_uses_the_config_file_and_a_broken_one_does_not_stop_it() {
    let dir = scratch("t256-export-config");
    let cfg = dir.join("config.toml");
    std::fs::write(&cfg, "[theme]\nbg = \"#123456\"\n").unwrap();
    let live = tiza_qa::Live::from_code(CODE, "rs", "t256-export-live");
    let mut mcp =
        tiza_qa::McpClient::start_with_env(&live.socket, &[("TIZA_CONFIG", cfg.to_str().unwrap())]);
    let export = |mcp: &mut tiza_qa::McpClient, name: &str| {
        let out = dir.join(name);
        let (err, text, _) = mcp.call(
            "export_image",
            serde_json::json!({"path": out.to_str().unwrap()}),
        );
        assert!(!err, "{text}");
        let reply: serde_json::Value = serde_json::from_str(&text).unwrap();
        (std::fs::read_to_string(&out).unwrap(), reply)
    };
    let (svg, reply) = export(&mut mcp, "a.svg");
    assert!(svg.contains("#123456"), "the config bg did not apply");
    assert!(reply.get("warning").is_none(), "{reply}");
    // A config that does not parse: the export goes on, and says so.
    std::fs::write(&cfg, "[theme\n").unwrap();
    let (svg, reply) = export(&mut mcp, "b.svg");
    assert!(!svg.contains("#123456"));
    let warning = reply["warning"].as_str().unwrap_or_default();
    assert!(warning.contains("config ignored"), "{reply}");
}

#[test]
fn ac7_render_with_theme_auto_exports_the_theme_of_the_document() {
    use tiza_core::{Theme, ThemeName};
    let dir = scratch("t256-render-auto");
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
