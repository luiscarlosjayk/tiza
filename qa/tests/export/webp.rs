//! Lossless WebP export. A `.webp` follows the GIF and the PNG of the same
//! surface: with one step (`--step`, `--all-steps`, `step` of
//! `export_image`, `:export`) it is a still with the pixels of the PNG of
//! that step; without one, it is an animation of every step with the
//! frames and the times of the GIF.

use std::io::Cursor;
use std::path::Path;

use serde_json::json;
use tiza_qa::{Live, McpClient, bin, fixture, scratch, tiza, webp_frames, webp_still};

fn doc() -> String {
    fixture("shared/ownership/ownership.rs.tiza")
        .to_str()
        .unwrap()
        .to_string()
}

fn p(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `tiza render ARGS`, which must succeed: what it said on stderr.
fn render(args: &[&str]) -> String {
    let o = tiza(args);
    let said = String::from_utf8_lossy(&o.stderr).into_owned();
    assert!(o.status.success(), "{args:?}: {said}");
    said
}

/// RGBA pixels and size of a PNG export.
fn png_rgba(bytes: &[u8]) -> (Vec<u8>, u32, u32) {
    let mut r = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    let mut px = vec![0; r.output_buffer_size().unwrap()];
    let info = r.next_frame(&mut px).unwrap();
    assert_eq!(
        (info.color_type, info.bit_depth),
        (png::ColorType::Rgba, png::BitDepth::Eight)
    );
    px.truncate(info.buffer_size());
    (px, info.width, info.height)
}

fn decoder(bytes: &[u8]) -> image_webp::WebPDecoder<Cursor<&[u8]>> {
    image_webp::WebPDecoder::new(Cursor::new(bytes)).unwrap()
}

/// The frame times of a GIF, in milliseconds.
fn gif_times(bytes: &[u8]) -> Vec<u32> {
    let mut d = gif::DecodeOptions::new()
        .read_info(Cursor::new(bytes))
        .unwrap();
    let mut times = Vec::new();
    while let Some(f) = d.read_next_frame().unwrap() {
        times.push(u32::from(f.delay) * 10);
    }
    times
}

#[test]
fn a_webp_of_one_step_has_the_pixels_of_the_png() {
    let dir = scratch("webp-still");
    for step in ["0", "3"] {
        let png = dir.join(format!("s{step}.png"));
        let webp = dir.join(format!("s{step}.webp"));
        for out in [&png, &webp] {
            render(&[
                "render",
                &doc(),
                "--step",
                step,
                "--width",
                "80",
                "--out",
                p(out),
            ]);
        }
        assert!(
            webp_still(&read(&webp)) == png_rgba(&read(&png)),
            "step {step}: the WebP is not the PNG"
        );
    }
    // The same bytes each time, also through a pipe.
    let piped = tiza(&[
        "render",
        &doc(),
        "--step",
        "3",
        "--width",
        "80",
        "--format",
        "webp",
        "--out",
        "-",
    ]);
    assert!(piped.status.success());
    assert!(piped.stdout == read(&dir.join("s3.webp")));
}

#[test]
fn a_webp_of_every_step_has_the_frames_and_times_of_the_gif() {
    let dir = scratch("webp-anim");
    let (gif, webp, steps) = (dir.join("a.gif"), dir.join("a.webp"), dir.join("steps"));
    let take = ["--width", "80", "--scale", "1", "--interval", "0.5"];
    render(&[&["render", &doc(), "--out", p(&gif)][..], &take].concat());
    let said = render(&[&["render", &doc(), "--out", p(&webp)][..], &take].concat());
    render(&[
        "render",
        &doc(),
        "--all-steps",
        "--width",
        "80",
        "--scale",
        "1",
        "--out",
        p(&steps),
    ]);
    // One GIF frame for each step (this document types nothing). A frame
    // equal to the one before it only makes that one last longer.
    let mut pngs: Vec<_> = std::fs::read_dir(&steps)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    pngs.sort();
    let times = gif_times(&read(&gif));
    assert_eq!(times.len(), pngs.len());
    let mut expected: Vec<(Vec<u8>, u32)> = Vec::new();
    for (png, t) in pngs.iter().zip(times) {
        let px = png_rgba(&read(png)).0;
        match expected.last_mut() {
            Some((last, lasts)) if *last == px => *lasts += t,
            _ => expected.push((px, t)),
        }
    }
    let got = webp_frames(&read(&webp));
    assert_eq!(got.len(), expected.len());
    for (i, ((px, t), (want, want_t))) in got.iter().zip(&expected).enumerate() {
        assert_eq!(t, want_t, "frame {i}: its time");
        assert!(px == want, "frame {i}: its pixels");
    }
    assert!(
        said.contains(" frames, ") && said.ends_with("loops forever\n"),
        "{said}"
    );
    assert_eq!(
        decoder(&read(&webp)).loop_count(),
        image_webp::LoopCount::Forever
    );
    // `--loop`, as in the GIF.
    let twice = dir.join("twice.webp");
    let said = render(
        &[
            &["render", &doc(), "--loop", "2", "--out", p(&twice)][..],
            &take,
        ]
        .concat(),
    );
    assert!(said.contains("plays 2×"), "{said}");
    assert_eq!(
        decoder(&read(&twice)).loop_count(),
        image_webp::LoopCount::Times(2.try_into().unwrap())
    );
}

/// Where a GIF is every step, a WebP is an animation; where a PNG is one
/// step, a WebP is a still of it: `tiza render` of a file and of the live
/// instance, `--all-steps`, and `export_image`.
#[test]
fn a_webp_is_a_still_where_a_png_is_and_animated_where_a_gif_is() {
    let dir = scratch("webp-surfaces");
    let animated = |path: &Path| decoder(&read(path)).is_animated();
    let same_image = |webp: &Path, png: &Path| webp_still(&read(webp)) == png_rgba(&read(png));
    // A file.
    let (a, s, s_png) = (dir.join("a.webp"), dir.join("s.webp"), dir.join("s.png"));
    render(&[
        "render",
        &doc(),
        "--width",
        "80",
        "--scale",
        "1",
        "--out",
        p(&a),
    ]);
    render(&[
        "render",
        &doc(),
        "--width",
        "80",
        "--scale",
        "1",
        "--step",
        "2",
        "--out",
        p(&s),
    ]);
    render(&[
        "render",
        &doc(),
        "--width",
        "80",
        "--scale",
        "1",
        "--step",
        "2",
        "--out",
        p(&s_png),
    ]);
    assert!(animated(&a) && !animated(&s) && same_image(&s, &s_png));
    // --all-steps: a still for each step, each one the PNG of that step.
    let (webps, pngs) = (dir.join("webps"), dir.join("pngs"));
    let said_webp = render(&[
        "render",
        &doc(),
        "--width",
        "80",
        "--scale",
        "1",
        "--all-steps",
        "--format",
        "webp",
        "--out",
        p(&webps),
    ]);
    let said_png = render(&[
        "render",
        &doc(),
        "--width",
        "80",
        "--scale",
        "1",
        "--all-steps",
        "--out",
        p(&pngs),
    ]);
    // The summary gives the size of the images, as for PNG files.
    let size = |said: &str| said.split_once(" (").map(|(_, s)| s.to_string());
    assert!(said_webp.contains(" px each)"), "{said_webp}");
    assert_eq!(size(&said_webp), size(&said_png));
    for step in 0..=7 {
        let name = format!("step-{step:02}");
        assert!(
            same_image(
                &webps.join(format!("{name}.webp")),
                &pngs.join(format!("{name}.png"))
            ),
            "{name}"
        );
    }
    // The live instance, from the shell and over MCP.
    let document = tiza_core::fs::open(&doc()).unwrap().document;
    let live = Live::open(document, dir.join("doc.tiza"), dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": 2}));
    let sock = p(&live.socket).to_string();
    let (la, ls, ls_png) = (dir.join("la.webp"), dir.join("ls.webp"), dir.join("ls.png"));
    for (out, step) in [(&la, None), (&ls, Some("2")), (&ls_png, Some("2"))] {
        let mut args = vec![
            "--socket",
            &sock,
            "render",
            "--width",
            "80",
            "--scale",
            "1",
            "--out",
            p(out),
        ];
        args.extend(step.map(|s| ["--step", s]).iter().flatten());
        let o = tiza(&args);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
    assert!(animated(&la) && !animated(&ls) && same_image(&ls, &ls_png));
    let mut mcp = McpClient::start(&live.socket);
    let (ma, ms, ms_png) = (dir.join("ma.webp"), dir.join("ms.webp"), dir.join("ms.png"));
    let (err, every, _) = mcp.call(
        "export_image",
        json!({"path": p(&ma), "width": 80, "scale": 1}),
    );
    assert!(!err, "{every}");
    assert!(
        every.contains("\"format\": \"webp\"")
            && every.contains("\"frames\": 8")
            && every.contains("\"steps\": \"0-7\""),
        "{every}"
    );
    for path in [&ms, &ms_png] {
        let (err, one, _) = mcp.call(
            "export_image",
            json!({"path": p(path), "width": 80, "scale": 1, "step": 2}),
        );
        assert!(!err && one.contains("\"step\": 2"), "{one}");
    }
    assert!(animated(&ma) && !animated(&ms) && same_image(&ms, &ms_png));
    // export_image and `tiza render` draw the same animation.
    assert!(webp_frames(&read(&ma)) == webp_frames(&read(&la)));
}

/// `:export x.webp` in the real TUI: a still of the step, the pixels of
/// `:export x.png`.
#[test]
fn the_tui_exports_a_webp_still() {
    let dir = scratch("webp-tui");
    let body = r##"
import json
binary, doc, out = sys.argv[1], sys.argv[2], sys.argv[3]
os.chdir(out)
env = dict(os.environ, TERM="xterm-256color", TIZA_NO_OSC="1")
env.pop("TIZA_TEST_HOOK", None)
s = Pty([binary, "--socket", "s.sock", doc], 30, 120, env=env)

def bar():
    r = subprocess.run([binary, "--socket", "s.sock", "api", "follow.status", "{}"],
                       capture_output=True, text=True, timeout=10)
    return json.loads(r.stdout).get("bar")

def run(cmd, name):
    s.send(("\x1b:" + cmd + "\r").encode())
    end = time.monotonic() + 10
    while time.monotonic() < end:
        b = bar()
        if b and name in b and b.split()[0] in ("exported", "could"):
            return b
        time.sleep(0.05)
    return bar()

if s.wait_for(lambda o: b"\x1b[?1049h" in o and os.path.exists("s.sock"), 10, "first frame"):
    for name, f in [("PNG", "shot.png"), ("WEBP", "shot.webp")]:
        print(name, json.dumps(run("export " + f + " step=2", f)), flush=True)
    s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10))
"##;
    // A copy: the TUI must not touch the fixture.
    for f in ["ownership.rs.tiza", "ownership.rs"] {
        std::fs::copy(fixture(&format!("shared/ownership/{f}")), dir.join(f)).unwrap();
    }
    let copy = dir.join("ownership.rs.tiza");
    let out = tiza_qa::pty_script(&dir, "export.py", body, &[p(&bin()), p(&copy), p(&dir)]);
    for (name, file) in [("PNG", "shot.png"), ("WEBP", "shot.webp")] {
        assert!(
            out.lines().any(
                |l| l.starts_with(&format!("{name} \"exported step 2 to ")) && l.contains(file)
            ),
            "{name}\n{out}"
        );
    }
    let (webp, png) = (read(&dir.join("shot.webp")), read(&dir.join("shot.png")));
    assert!(
        webp_still(&webp) == png_rgba(&png),
        "the WebP is not the PNG"
    );
}

#[test]
fn a_webp_is_refused_where_a_png_or_a_gif_is() {
    let dir = scratch("webp-refused");
    let refused = |args: &[&str], says: &str| {
        let o = tiza(args);
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(!o.status.success() && err.contains(says), "{args:?}: {err}");
    };
    let out = p(&dir.join("x.webp")).to_string();
    let d = doc();
    for step in [&["--step", "1"][..], &[][..]] {
        for (flag, value, says) in [
            (
                "--width",
                "99999",
                "width=99999: use a number between 20 and 400",
            ),
            ("--scale", "99", "scale=99: use a number between 0.25 and 8"),
            (
                "--font-size",
                "999",
                "font-size=999: use a number between 4 and 96",
            ),
        ] {
            let args = [&["render", &d, "--out", &out, flag, value][..], step].concat();
            refused(&args, says);
        }
    }
    assert!(!dir.join("x.webp").exists());
    // An animation needs a file; a still goes through a pipe.
    refused(
        &["render", &doc(), "--format", "webp", "--out", "-"],
        "an animated .webp needs a file: pass --out demo.webp, or --step N for a still",
    );
    // --all-steps takes a folder.
    refused(
        &[
            "render",
            &doc(),
            "--all-steps",
            "--out",
            p(&dir.join("demo.webp")),
        ],
        "Pass a folder, for example --out steps/ --format webp",
    );
    // A second run into the same folder removes the WebP steps that it
    // does not write again.
    let steps = dir.join("steps");
    let short = dir.join("short.rs");
    std::fs::write(&short, "fn main() {}\n").unwrap();
    render(&[
        "render",
        &doc(),
        "--all-steps",
        "--format",
        "webp",
        "--out",
        p(&steps),
    ]);
    let said = render(&[
        "render",
        p(&short),
        "--all-steps",
        "--format",
        "webp",
        "--out",
        p(&steps),
    ]);
    assert!(
        said.contains("removed 7 files from an earlier run"),
        "{said}"
    );
    let left: Vec<_> = std::fs::read_dir(&steps)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left, ["step-00.webp"]);
}
