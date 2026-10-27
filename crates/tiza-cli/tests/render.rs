//! Binary smoke test: `render` of the bundled example at several widths and steps.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

/// The binary with an empty config file and an empty config folder, so the
/// config of whoever runs the tests changes nothing.
fn bin() -> Command {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    let dir = DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("tiza-cli-tests-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("config")).unwrap();
        std::fs::write(dir.join("config.toml"), "").unwrap();
        dir
    });
    let mut c = Command::new(env!("CARGO_BIN_EXE_tiza"));
    c.env("TIZA_CONFIG", dir.join("config.toml"))
        .env("XDG_CONFIG_HOME", dir.join("config"));
    c
}

fn example() -> String {
    format!(
        "{}/../../examples/ownership.rs.tiza",
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
            "/tmp/tiza-definitely-not-here.sock",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no Tiza instance"));
}

#[test]
fn file_reference_with_subdir_and_range_resolves_relative_to_document() {
    let doc = format!(
        "{}/../../examples/stack-impl.tiza",
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

#[test]
fn exports_svg_and_png_by_extension() {
    let dir = std::env::temp_dir().join(format!("tiza-export-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let svg = dir.join("step3.svg");
    let out = bin()
        .args([
            "render",
            &example(),
            "--out",
            svg.to_str().unwrap(),
            "--step",
            "3",
            "--no-embed-fonts",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(&svg).unwrap();
    assert!(text.starts_with("<svg "));
    assert!(text.contains("takes owns s"), "box label is in the SVG");
    assert!(!text.contains("@font-face"));

    let png = dir.join("step3.png");
    let out = bin()
        .args([
            "render",
            &example(),
            "--out",
            png.to_str().unwrap(),
            "--step",
            "3",
            "--scale",
            "1",
            "--theme",
            "light",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = std::fs::read(&png).unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    // Width = 100 cols × 9.6 px + 2 × margin (1.5 × 21.12 px, rounded) = 960 + 64.
    let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    assert_eq!(w, 1024);

    // PNG bytes never go to a terminal; to a pipe they do.
    let out = bin()
        .args(["render", &example(), "--format", "png", "--out", "-"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(&out.stdout[..4], b"\x89PNG");
    std::fs::remove_dir_all(&dir).ok();
}
