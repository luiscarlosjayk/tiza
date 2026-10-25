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
    // A config that does not parse: the export goes on, and says so.
    std::fs::write(&cfg, "[theme\n").unwrap();
    let (svg, reply) = export(&mut mcp, "b.svg");
    assert!(!svg.contains("#123456"));
    let warning = reply["warning"].as_str().unwrap_or_default();
    assert!(warning.contains("config ignored"), "{reply}");
}
