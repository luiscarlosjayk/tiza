//! Humo del binario: `render` del ejemplo incluido, en varios anchos y pasos.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_codedraw"))
}

fn example() -> String {
    format!(
        "{}/../../examples/ownership.rs.codedraw",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn renders_example_at_full_step() {
    let out = bin()
        .args(["render", &example(), "--width", "100", "--step", "5"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("╭─ takes es dueña de s"));
    assert!(text.contains("━▶ move"));
    assert!(text.contains("╳ drop"));
    for line in text.lines() {
        assert!(
            unicode_width::UnicodeWidthStr::width(line) <= 100,
            "fila demasiado ancha: {line}"
        );
    }
}

#[test]
fn step_one_hides_later_annotations() {
    let out = bin()
        .args(["render", &example(), "--width", "100", "--step", "1"])
        .output()
        .unwrap();
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains("╳ drop"));
    assert!(text.contains("move"));
}

#[test]
fn narrow_width_never_overflows() {
    let out = bin()
        .args(["render", &example(), "--width", "60", "--format", "txt"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    for line in text.lines() {
        assert!(
            unicode_width::UnicodeWidthStr::width(line) <= 60,
            "fila demasiado ancha: {line}"
        );
    }
}

#[test]
fn missing_file_exits_one_with_message() {
    let out = bin().args(["render", "/no/existe.rs"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no se pudo abrir"));
}

#[test]
fn scene_without_instance_explains() {
    let out = bin()
        .args([
            "scene",
            "status",
            "--socket",
            "/tmp/codedraw-definitely-not-here.sock",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no hay una instancia"));
}
