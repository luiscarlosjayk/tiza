//! Texts and messages that misled a user or an agent (found while the
//! documentation pages were checked against the code).
//!
//! Acceptance criteria:
//! - AC1 The review footer says what `a` and `d` do (`a accept all`,
//!   `d reject all`). The footer test of t-239 checks it.
//! - AC2 `annotation.update` with `text` says what to send: `label` for the
//!   text of a note or the title of a callout, a new `shape` for the text
//!   of a callout. The MCP tool, which has no `shape`, says to remove the
//!   callout and add it again, and names no field that it does not take.
//! - AC3 An argument that an MCP tool refuses itself (border, corners,
//!   emphasis, stroke, route, a head, placement, the path, theme and scale
//!   of export_image) ends with `(invalid_argument)`, as a refusal of the
//!   instance does, so that an agent can tell the cases apart.
//! - AC4 `tiza render --all-steps` with an `--out` that ends in the
//!   extension of a format (`answer.md`, `demo.gif`) and is not a folder is
//!   refused, and nothing is written. A folder works as before.

use serde_json::json;
use tiza_qa::{Live, McpClient};

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n}\n";

#[test]
fn ac3_every_refusal_of_the_mcp_tools_has_its_code() {
    let live = Live::from_code(CODE, "rs", "t257-codes");
    let mut mcp = McpClient::start(&live.socket);
    let out = tiza_qa::scratch("t257-codes-out");
    let line = |n: u32| json!({"line": n});
    let cases = [
        (
            "add_box",
            json!({"from_line": 1, "to_line": 2, "border": "zigzag"}),
        ),
        (
            "add_box",
            json!({"from_line": 1, "to_line": 2, "corners": "zigzag"}),
        ),
        ("highlight", json!({"at": line(2), "emphasis": "zigzag"})),
        (
            "add_arrow",
            json!({"from": line(1), "to": line(2), "stroke": "zigzag"}),
        ),
        (
            "add_arrow",
            json!({"from": line(1), "to": line(2), "route": "zigzag"}),
        ),
        (
            "add_arrow",
            json!({"from": line(1), "to": line(2), "head_end": "zigzag"}),
        ),
        (
            "add_callout",
            json!({"from_line": 1, "text": "t", "placement": "zigzag"}),
        ),
        (
            "export_image",
            json!({"path": out.join("a.bmp").to_str().unwrap()}),
        ),
        (
            "export_image",
            json!({"path": out.join("a.png").to_str().unwrap(), "theme": "zigzag"}),
        ),
        (
            "export_image",
            json!({"path": out.join("a.png").to_str().unwrap(), "scale": 99}),
        ),
    ];
    for (tool, args) in cases {
        let (err, text, _) = mcp.call(tool, args.clone());
        assert!(err, "{tool} {args}: accepted: {text}");
        assert!(
            text.ends_with("(invalid_argument)"),
            "{tool} {args}: {text}"
        );
    }
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0);
}

#[test]
fn ac4_all_steps_refuses_a_file_name_for_out() {
    let dir = tiza_qa::scratch("t257-all-steps");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let run = |out: &str, format: Option<&str>| {
        let mut c = std::process::Command::new(tiza_qa::bin());
        c.args(["render", "main.rs", "--all-steps", "--out", out])
            .current_dir(&dir)
            .env("TIZA_CONFIG", dir.join("no-config.toml"));
        if let Some(f) = format {
            c.args(["--format", f]);
        }
        c.output().unwrap()
    };
    let render = |out: &str| run(out, None);
    for (out, format, says) in [
        (
            "answer.md",
            None,
            "is a file name. Pass a folder, for example --out steps/ --format md",
        ),
        ("shot.PNG", None, "--format png"),
        ("demo.gif", None, "already holds every step"),
        // A --format given is the one to suggest.
        ("shots.png", Some("svg"), "--format svg"),
        (
            "talk.mp4",
            Some("svg"),
            "is a file name. Pass a folder, for example --out steps/ --format svg",
        ),
    ] {
        let o = run(out, format);
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(!o.status.success(), "{out}: accepted");
        assert!(err.contains(says), "{out}: {err}");
        assert!(!dir.join(out).exists(), "{out} was made");
    }
    let o = render("steps/");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(dir.join("steps/step-00.png").is_file());
    // A slash at the end asks for a folder, whatever the name.
    let o = render("answer.md/");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(dir.join("answer.md/step-00.png").is_file());
    // A folder with such a name that is there already still works.
    std::fs::create_dir(dir.join("old.md")).unwrap();
    let o = render("old.md");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}
