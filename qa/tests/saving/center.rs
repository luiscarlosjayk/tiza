//! The field `center` in a `.tiza`, as a person writes it. `true` and
//! `false` load. A `null` or a string does not: the error names the value,
//! and Tiza draws nothing (as for `loop`).

use tiza_qa::{scratch, tiza};

#[test]
fn center_is_true_or_false_and_nothing_else() {
    let dir = scratch("center-field");
    let doc = dir.join("doc.tiza");
    let write = |value: &str| {
        std::fs::write(
            &doc,
            format!(
                r#"{{"schema":2,"source":{{"language":"rs"}},"code":["fn main() {{}}"],"annotations":[],"center":{value}}}"#
            ),
        )
        .unwrap();
    };
    let render = || {
        tiza(&[
            "render",
            doc.to_str().unwrap(),
            "--format",
            "txt",
            "--out",
            "-",
        ])
    };
    for (value, error) in [
        ("null", "invalid type: null"),
        (r#""true""#, r#"invalid type: string "true""#),
    ] {
        write(value);
        let out = render();
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success(), "center: {value}");
        assert!(
            stderr.contains(error) && stderr.contains("expected a boolean"),
            "{stderr}"
        );
        assert!(out.stdout.is_empty(), "center: {value} drew something");
    }
    for value in ["true", "false"] {
        write(value);
        let out = render();
        assert!(
            out.status.success(),
            "center: {value}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(String::from_utf8_lossy(&out.stdout).contains("fn main() {}"));
    }
}
