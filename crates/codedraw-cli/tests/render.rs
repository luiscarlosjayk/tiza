//! Binary smoke test: `render` of the bundled example at several widths and steps.

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
    assert!(text.contains("╭─ takes owns s"));
    assert!(text.contains("━▶ move"));
    assert!(text.contains("╳ drop"));
    for line in text.lines() {
        assert!(
            unicode_width::UnicodeWidthStr::width(line) <= 100,
            "row too wide: {line}"
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
            "row too wide: {line}"
        );
    }
}

#[test]
fn missing_file_exits_one_with_message() {
    let out = bin().args(["render", "/no/existe.rs"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("could not open"));
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
    assert!(String::from_utf8_lossy(&out.stderr).contains("no CodeDraw instance"));
}

#[test]
fn file_reference_with_subdir_and_range_resolves_relative_to_document() {
    let doc = format!(
        "{}/../../examples/stack-impl.codedraw",
        env!("CARGO_MANIFEST_DIR")
    );
    let out = bin()
        .current_dir(std::env::temp_dir())
        .args(["render", &doc, "--width", "100", "--step", "5"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains(" 7 ") && text.contains("impl<T> Stack<T> {"),
        "{text}"
    );
    assert!(!text.contains(" 6 "), "the 7-30 range excludes line 6");
    assert!(text.contains("╭─ impl<T> Stack<T>"));
}
