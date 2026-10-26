//! An image export that would be too large fails with a clear message
//! before it takes the memory: a document whose code alone is over the
//! limit is refused before the layout (PNG, WebP, GIF, `--all-steps` and
//! `export_image`), and `--margin` and `--radius` take only finite numbers
//! in their range. A document just under the limit exports whole. An SVG
//! has its own, higher limit.
//!
//! 20,000 lines are 21 times the limit at the default size; a document of
//! 200,000 lines took 2.1 GB (a still) and 4.7 GB (an animation) before it
//! failed, and now stops at about 44 MB, before the layout.

use std::path::Path;

use serde_json::json;
use tiza_qa::{Live, McpClient, fixture, scratch, tiza};

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

/// Rust code of `n` lines, with no annotations.
fn code(n: usize) -> String {
    (0..n).map(|i| format!("let a{i} = {i};\n")).collect()
}

fn refused(args: &[&str], says: &str) {
    let o = tiza(args);
    let err = String::from_utf8_lossy(&o.stderr);
    assert!(!o.status.success(), "{args:?}: accepted");
    assert!(err.contains(says), "{args:?}: {err}");
}

#[test]
fn a_huge_document_is_refused_before_the_layout() {
    let dir = scratch("huge");
    let big = dir.join("big.rs");
    std::fs::write(&big, code(20_000)).unwrap();
    let says = "20000 lines of code make an image at least";
    for (out, extra) in [
        ("a.png", &["--step", "0"][..]),
        ("a.webp", &["--step", "0"][..]),
        ("a.gif", &[][..]),
        ("b.webp", &[][..]),
        ("steps/", &["--all-steps", "--format", "webp"][..]),
    ] {
        let out = dir.join(out);
        let args = [&["render", p(&big), "--out", p(&out)][..], extra].concat();
        refused(&args, says);
        assert!(!out.exists(), "{} was written", out.display());
    }
}

#[test]
fn a_document_just_under_the_limit_exports_whole() {
    let dir = scratch("under");
    // 380 rows of 21.12 px and two margins of 32 px, at scale 2: 16180 px
    // high, under 16384. 400 rows would be 17024.
    std::fs::write(dir.join("under.rs"), code(380)).unwrap();
    std::fs::write(dir.join("over.rs"), code(400)).unwrap();
    let png = dir.join("under.png");
    // 20 columns: as high, and a small raster in a debug build.
    let o = tiza(&[
        "render",
        p(&dir.join("under.rs")),
        "--step",
        "0",
        "--width",
        "20",
        "--out",
        p(&png),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let bytes = std::fs::read(&png).unwrap();
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    assert!((16_000..=16_384).contains(&height), "{height} px");
    refused(
        &[
            "render",
            p(&dir.join("over.rs")),
            "--step",
            "0",
            "--out",
            p(&dir.join("over.png")),
        ],
        "400 lines of code make an image at least",
    );
}

#[test]
fn margin_and_radius_take_finite_numbers_in_their_range() {
    let dir = scratch("margin-radius");
    let doc = fixture("shared/ownership/ownership.rs.tiza");
    let doc = p(&doc);
    for (flag, out) in [
        ("--margin=inf", "a.webp"),
        ("--margin=NaN", "a.png"),
        ("--margin=-50", "a.gif"),
        ("--margin=1e6", "a.svg"),
    ] {
        let value = flag.split_once('=').unwrap().1;
        let out = dir.join(out);
        refused(
            &["render", doc, "--step", "2", flag, "--out", p(&out)],
            &format!(
                "--margin={}: use a number of cells between 0 and 20",
                value.replace("1e6", "1000000")
            ),
        );
        assert!(!out.exists());
    }
    for flag in ["--radius=NaN", "--radius=inf", "--radius=-1"] {
        let value = flag.split_once('=').unwrap().1;
        refused(
            &[
                "render",
                doc,
                "--step",
                "2",
                flag,
                "--out",
                p(&dir.join("r.png")),
            ],
            &format!("--radius={value}: use a number of px between 0 and 100"),
        );
    }
    // The ends of the ranges work.
    for (margin, radius) in [("0", "0"), ("20", "100")] {
        let out = dir.join(format!("m{margin}.png"));
        let o = tiza(&[
            "render",
            doc,
            "--step",
            "2",
            "--scale",
            "1",
            "--margin",
            margin,
            "--radius",
            radius,
            "--out",
            p(&out),
        ]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
}

#[test]
fn export_image_refuses_a_huge_document_before_its_thread() {
    // 2,000 lines are already 5 times the limit at the default size.
    let live = Live::from_code(&code(2_000), "rs", "huge-mcp");
    let dir = scratch("huge-mcp");
    let mut mcp = McpClient::start(&live.socket);
    for (file, step) in [
        ("a.png", Some(0)),
        ("a.webp", Some(0)),
        ("a.webp", None),
        ("a.gif", None),
    ] {
        let path = dir.join(file);
        let mut args = json!({"path": p(&path)});
        if let Some(s) = step {
            args["step"] = json!(s);
        }
        let (err, text, _) = mcp.call("export_image", args);
        assert!(
            err && text.contains("2000 lines of code make an image at least")
                && text.contains("export a range of lines with `tiza render FILE:A-B`")
                && text.contains("(invalid_argument)"),
            "{file} {step:?}: {text}"
        );
        assert!(!path.exists());
    }
    // An SVG has a higher limit: 2,000 lines at 96 px are over it.
    let path = dir.join("a.svg");
    let (err, text, _) = mcp.call(
        "export_image",
        json!({"path": p(&path), "step": 0, "font_size": 96}),
    );
    assert!(
        err && text.contains("2000 lines of code make an SVG at least")
            && text.contains("(invalid_argument)"),
        "svg: {text}"
    );
    assert!(!path.exists());
}

/// The bound counts the lines that each step shows: a short window of
/// another file (`code.link`) exports, while the huge main file is refused.
#[test]
fn a_short_linked_window_of_a_huge_file_exports() {
    let dir = scratch("huge-link");
    std::fs::write(dir.join("main.rs"), code(20_000)).unwrap();
    std::fs::write(dir.join("small.rs"), code(50)).unwrap();
    let doc = dir.join("link.tiza");
    let tiza_doc = json!({
        "schema": 1,
        "source": {"language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "step": 0,
        "moves": [{"at_step": 1, "action": "code.link", "file": "small.rs", "lines": "1-5"}],
        "annotations": []
    });
    std::fs::write(&doc, tiza_doc.to_string()).unwrap();
    for (args, out) in [
        (&["--step", "1"][..], "s1.png"),
        // An animation from step 1 never lays out step 0, also with
        // `--step 0`, which a recording does not use.
        (&["--skip-zero"][..], "a.gif"),
        (&["--skip-zero", "--step", "0"][..], "b.gif"),
    ] {
        let out = dir.join(out);
        let all = [
            &["render", p(&doc), "--width", "40", "--out", p(&out)][..],
            args,
        ]
        .concat();
        let o = tiza(&all);
        assert!(
            o.status.success(),
            "{all:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(out.exists());
    }
    refused(
        &[
            "render",
            p(&doc),
            "--step",
            "0",
            "--out",
            p(&dir.join("s0.png")),
        ],
        "20000 lines of code make an image at least",
    );
}

/// An SVG holds no pixels: its limit is 8 times higher per side, but the
/// SVG of a huge document is refused too.
#[test]
fn an_svg_has_its_own_higher_limit() {
    let dir = scratch("huge-svg");
    // 6,000 rows: about 126800 px high, under 131072; as a PNG at scale 2,
    // far over 16384. 7,000 rows: about 147900 px.
    std::fs::write(dir.join("six.rs"), code(6_000)).unwrap();
    std::fs::write(dir.join("seven.rs"), code(7_000)).unwrap();
    let svg = dir.join("six.svg");
    let o = tiza(&[
        "render",
        p(&dir.join("six.rs")),
        "--step",
        "0",
        "--width",
        "20",
        "--out",
        p(&svg),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    // Higher than a PNG can be, under the limit of an SVG.
    let text = std::fs::read_to_string(&svg).unwrap();
    let height: u32 = text
        .split_once("height=\"")
        .and_then(|(_, rest)| rest.split('"').next())
        .and_then(|h| h.parse().ok())
        .unwrap();
    assert!((16_384..=131_072).contains(&height), "{height} px");
    refused(
        &[
            "render",
            p(&dir.join("six.rs")),
            "--step",
            "0",
            "--out",
            p(&dir.join("six.png")),
        ],
        "6000 lines of code make an image at least",
    );
    let seven = dir.join("seven.rs");
    for extra in [
        &["--step", "0", "--format", "svg", "--out"][..],
        &["--all-steps", "--format", "svg", "--out"][..],
    ] {
        let out = dir.join("seven-out");
        let args = [&["render", p(&seven)][..], extra, &[p(&out)][..]].concat();
        refused(&args, "7000 lines of code make an SVG at least");
        assert!(!out.exists());
    }
}
