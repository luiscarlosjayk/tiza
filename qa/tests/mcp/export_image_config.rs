//! The MCP `export_image` applies the config file, and a broken one does not
//! stop it.

use tiza_qa::scratch;

const CODE: &str = "fn main() {\n    println!(\"hi\");\n}\n";

#[test]
fn export_image_uses_the_config_file_and_a_broken_one_does_not_stop_it() {
    let dir = scratch("export-config");
    let cfg = dir.join("config.toml");
    std::fs::write(&cfg, "[theme]\nbg = \"#123456\"\n").unwrap();
    let live = tiza_qa::Live::from_code(CODE, "rs", "export-live");
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
    // A config that does not parse: the export goes on, and says where.
    std::fs::write(&cfg, "[theme\n").unwrap();
    let (svg, reply) = export(&mut mcp, "b.svg");
    assert!(!svg.contains("#123456"));
    let warning = reply["warning"].as_str().unwrap_or_default();
    assert!(warning.contains("config.toml:1:"), "{reply}");
}

#[test]
fn export_image_draws_the_gutter_that_the_screen_shows() {
    let dir = scratch("export-gutter");
    let export = |live: &tiza_qa::Live, name: &str| {
        let mut mcp = tiza_qa::McpClient::start(&live.socket);
        let out = dir.join(name);
        let (err, text, _) = mcp.call(
            "export_image",
            serde_json::json!({"path": out.to_str().unwrap()}),
        );
        assert!(!err, "{text}");
        // The client's `$TIZA_CONFIG` names no file: «no config», with no
        // warning in the reply.
        let reply: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(reply.get("warning").is_none(), "{reply}");
        std::fs::read_to_string(&out).unwrap()
    };
    // The instance's config file turns the gutter off, and the document
    // does not set it: the screen shows no gutter, and the export neither.
    let by_config = tiza_qa::Live::from_code(CODE, "rs", "gutter-config");
    by_config.with(|a| a.config_defaults.numbers = Some(false));
    let by_document = tiza_qa::Live::from_code(CODE, "rs", "gutter-document");
    by_document.with(|a| a.doc.numbers = Some(false));
    let plain = tiza_qa::Live::from_code(CODE, "rs", "gutter-none");
    let config_svg = export(&by_config, "config.svg");
    assert_eq!(config_svg, export(&by_document, "document.svg"));
    assert_ne!(config_svg, export(&plain, "plain.svg"));
}
