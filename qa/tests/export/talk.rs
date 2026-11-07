//! `tiza render` of a talk (a master `.tiza`): a .gif, .webp, .mp4 or
//! .webm plays the scenes that show in a row, each exactly as its own
//! export; `--scene` exports one scene as `tiza render SCENE` does.
//!
//! Acceptance criteria:
//! - AC1 The frames of a talk are the frames of each scene alone, in
//!   order, with the same flags; a skipped scene is left out.
//! - AC2 Each scene keeps its own theme, also the background around the
//!   cells, in a GIF, a WebP and a video.
//! - AC3 Every frame has the size of the widest and the tallest scene; a
//!   smaller scene is at the top left.
//! - AC4 The last step stays twice as long only at the end of the talk,
//!   and a step that the document times keeps its own time.
//! - AC5 A still, a text export, `--all-steps`, a .cast and a .tape of a
//!   whole talk are refused with what to give; `--scene` gives the export
//!   of that scene, byte for byte.
//!
//! The fixture is `qa/fixtures/export/talk/`: a typed scene with
//! effects (dark), a light scene with `center`, `loop` and a timed last
//! step, a tall scene with its code in `src/`, and a skipped scene whose
//! file is not there, as the last entry.

use std::path::{Path, PathBuf};

use tiza_qa::{fixture, scratch};

/// Runs `tiza ARGS`: (success, stdout, stderr).
fn run(args: &[&str]) -> (bool, Vec<u8>, String) {
    let out = tiza_qa::command().args(args).output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(!err.contains("panicked"), "tiza {args:?}:\n{err}");
    (out.status.success(), out.stdout, err)
}

/// Runs `tiza ARGS`, which must succeed: its stderr.
fn run_ok(args: &[&str]) -> String {
    let (ok, _, err) = run(args);
    assert!(ok, "tiza {args:?} failed:\n{err}");
    err
}

/// The talk and its scenes, as files of the fixture.
fn talk() -> PathBuf {
    fixture("export/talk/talk.tiza")
}

fn scene(name: &str) -> PathBuf {
    fixture(&format!("export/talk/scenes/{name}.tiza"))
}

/// The scenes of `talk.tiza` that show, in order.
const SHOWN: [&str; 3] = ["typed", "light", "tall"];

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// Exports `file` to `out` with `flags`.
fn export(file: &Path, out: &Path, flags: &[&str]) -> Vec<u8> {
    let mut args = vec!["render", s(file), "--out", s(out)];
    args.extend_from_slice(flags);
    run_ok(&args);
    std::fs::read(out).unwrap()
}

/// A frame of an image: its RGBA pixels, its size and how long it stays.
struct Rgba {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Rgba {
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        self.pixels[i..i + 4].try_into().unwrap()
    }
}

/// The rectangle a GIF frame draws: left, top, width, height.
type Rect = (u16, u16, u16, u16);

/// The frames of a GIF, composed as a player shows them (each frame is
/// drawn on the one before: Tiza writes `Keep`), with the rectangle each
/// frame draws and its delay in centiseconds.
fn gif_frames(bytes: &[u8]) -> Vec<(Rgba, Rect, u16)> {
    let mut o = gif::DecodeOptions::new();
    o.set_color_output(gif::ColorOutput::RGBA);
    let mut d = o.read_info(std::io::Cursor::new(bytes)).unwrap();
    let (w, h) = (u32::from(d.width()), u32::from(d.height()));
    let mut screen = vec![0u8; (w * h * 4) as usize];
    let mut out = Vec::new();
    while let Some(f) = d.read_next_frame().unwrap() {
        assert_eq!(f.dispose, gif::DisposalMethod::Keep);
        for row in 0..u32::from(f.height) {
            let from = (row * u32::from(f.width) * 4) as usize;
            let to = (((u32::from(f.top) + row) * w + u32::from(f.left)) * 4) as usize;
            let n = (u32::from(f.width) * 4) as usize;
            screen[to..to + n].copy_from_slice(&f.buffer[from..from + n]);
        }
        out.push((
            Rgba {
                width: w,
                height: h,
                pixels: screen.clone(),
            },
            (f.left, f.top, f.width, f.height),
            f.delay,
        ));
    }
    out
}

/// The frames of an animated WebP, composed, with their durations in ms.
fn webp_frames(bytes: &[u8]) -> Vec<(Rgba, u32)> {
    // The canvas of an animation: its VP8X chunk (the size less one).
    assert_eq!(&bytes[12..16], b"VP8X");
    let u24 = |b: &[u8]| u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16;
    let (w, h) = (u24(&bytes[24..]) + 1, u24(&bytes[27..]) + 1);
    tiza_qa::webp_frames(bytes)
        .into_iter()
        .map(|(pixels, ms)| {
            (
                Rgba {
                    width: w,
                    height: h,
                    pixels,
                },
                ms,
            )
        })
        .collect()
}

/// `true` when the top left `small.width × small.height` pixels of `big`
/// are `small`, and every other pixel of `big` is `rest`.
fn top_left_is(big: &Rgba, small: &Rgba, rest: [u8; 4]) -> bool {
    (0..big.height).all(|y| {
        (0..big.width).all(|x| {
            let want = if x < small.width && y < small.height {
                small.at(x, y)
            } else {
                rest
            };
            big.at(x, y) == want
        })
    })
}

/// The flags of the comparisons: no rounded corner, so that the edge of a
/// scene alone is the background as in the larger frame of the talk.
const SAME: [&str; 8] = [
    "--width",
    "70",
    "--scale",
    "1",
    "--radius",
    "0",
    "--interval",
    "1",
];

/// Checks that the WebP of the talk is the WebP of each scene alone, in a
/// row, with `flags`. The last step of a scene that is not the last one
/// stays half as long as alone (alone, it is the end of its loop), unless
/// its document times it (`timed`).
fn talk_is_its_scenes(dir: &Path, flags: &[&str], timed: &[&str]) {
    let all: Vec<&str> = SAME.iter().copied().chain(flags.iter().copied()).collect();
    let whole = webp_frames(&export(&talk(), &dir.join("talk.webp"), &all));
    let mut want: Vec<(Rgba, u32)> = Vec::new();
    for (k, name) in SHOWN.iter().enumerate() {
        let mut alone = webp_frames(&export(
            &scene(name),
            &dir.join(format!("{name}.webp")),
            &all,
        ));
        if k + 1 < SHOWN.len() && !timed.contains(name) {
            alone.last_mut().unwrap().1 /= 2;
        }
        want.extend(alone);
    }
    assert_eq!(
        whole.iter().map(|f| f.1).collect::<Vec<_>>(),
        want.iter().map(|f| f.1).collect::<Vec<_>>(),
        "{flags:?}: the durations of the frames"
    );
    for (i, ((got, _), (alone, _))) in whole.iter().zip(&want).enumerate() {
        // Past the scene: the background of its theme, as at its own edge.
        let rest = alone.at(alone.width - 1, alone.height - 1);
        assert!(
            top_left_is(got, alone, rest),
            "{flags:?}: frame {i} of the talk is not its scene alone at the top left"
        );
    }
}

#[test]
fn a_talk_plays_each_scene_that_shows_as_its_own_export_in_a_row() {
    let dir = scratch("talk-export-row");
    // The light scene times its last step: it keeps its time in the talk.
    talk_is_its_scenes(&dir, &[], &["light"]);
    // The flags go to every scene; without step 0 the first annotated step
    // of each scene shows as the scene opens, with no effect.
    talk_is_its_scenes(
        &dir,
        &["--skip-zero", "--no-numbers", "--margin-cols", "20"],
        &["light"],
    );
}

/// The PNG frames a video is made of, from an ffmpeg that copies them.
fn video_frames(dir: &Path, file: &Path, flags: &[&str]) -> (Vec<Rgba>, f64, String) {
    let frames = dir.join("frames");
    let _ = std::fs::remove_dir_all(&frames);
    std::fs::create_dir_all(&frames).unwrap();
    let fake = dir.join("ffmpeg-copies");
    std::fs::write(
        &fake,
        "#!/bin/sh\nprev=\nfor a; do\n  [ \"$prev\" = -i ] && list=$a\n  prev=$a\n  last=$a\ndone\ncp \"$(dirname \"$list\")\"/* \"$FRAMES/\"\necho video > \"$last\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let out = dir.join("talk.mp4");
    let mut args = vec!["render", s(file), "--out", s(&out)];
    args.extend_from_slice(flags);
    let r = tiza_qa::command()
        .args(&args)
        .env("TIZA_FFMPEG", &fake)
        .env("FRAMES", &frames)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&r.stderr).into_owned();
    assert!(r.status.success(), "{err}");
    let list = std::fs::read_to_string(frames.join("frames.txt")).unwrap();
    let seconds: f64 = list
        .lines()
        .filter_map(|l| l.strip_prefix("duration "))
        .map(|d| d.parse::<f64>().unwrap())
        .sum();
    let mut names: Vec<String> = std::fs::read_dir(&frames)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".png"))
        .collect();
    names.sort();
    let pngs = names
        .iter()
        .map(|n| {
            let d = png::Decoder::new(std::io::BufReader::new(
                std::fs::File::open(frames.join(n)).unwrap(),
            ));
            let mut r = d.read_info().unwrap();
            let mut buf = vec![0; r.output_buffer_size().unwrap()];
            let info = r.next_frame(&mut buf).unwrap();
            assert_eq!(info.color_type, png::ColorType::Rgba, "{n}");
            buf.truncate(info.buffer_size());
            Rgba {
                width: info.width,
                height: info.height,
                pixels: buf,
            }
        })
        .collect();
    (pngs, seconds, err)
}

/// The number of frames of the GIF of `file` alone, from its `wrote` line.
fn frames_alone(dir: &Path, file: &Path) -> usize {
    let err = run_ok(&[
        "render",
        s(file),
        "--out",
        s(&dir.join("alone.gif")),
        "--scale",
        "1",
        "--interval",
        "1",
    ]);
    let n = err.split(" frame").next().unwrap();
    n.rsplit(' ').next().unwrap().parse().unwrap()
}

#[test]
fn each_scene_keeps_its_own_theme_around_its_cells_in_every_format() {
    let dir = scratch("talk-export-themes");
    let flags = ["--scale", "1", "--radius", "0", "--interval", "1"];
    // The background of each theme, from a scene alone.
    let bg = |name: &str| {
        let f = gif_frames(&export(
            &scene(name),
            &dir.join(format!("{name}.gif")),
            &flags,
        ));
        f[0].0.at(1, 1)
    };
    let (dark, light) = (bg("typed"), bg("light"));
    assert_ne!(dark, light);
    let first = frames_alone(&dir, &scene("typed"));
    // GIF: the first frame of the light scene is drawn whole, its margin
    // too; a frame inside a scene is the rectangle that changed.
    let gif = gif_frames(&export(&talk(), &dir.join("talk.gif"), &flags));
    let (w, h) = (gif[0].0.width, gif[0].0.height);
    assert_eq!(gif[0].0.at(1, 1), dark);
    assert_eq!(
        gif[first].0.at(1, 1),
        light,
        "the top margin of the light scene"
    );
    assert_eq!(gif[first].0.at(w - 2, h - 2), light);
    assert_eq!(gif[first].1, (0, 0, w as u16, h as u16));
    assert!(
        gif[first + 1].1 != (0, 0, w as u16, h as u16),
        "a frame inside a scene draws only what changed"
    );
    // WebP.
    let webp = webp_frames(&export(&talk(), &dir.join("talk.webp"), &flags));
    assert_eq!(webp[0].0.at(1, 1), dark);
    assert!(webp.iter().any(|f| f.0.at(1, 1) == light));
    // A video: every PNG frame has one size, and has the theme of its
    // scene; its length is the length of the GIF.
    let (pngs, seconds, err) = video_frames(&dir, &talk(), &flags);
    assert!(
        pngs.iter()
            .all(|p| (p.width, p.height) == (pngs[0].width, pngs[0].height)),
        "one frame size in the video"
    );
    assert_eq!(pngs[0].at(1, 1), dark);
    assert_eq!(pngs[first].at(1, 1), light);
    let gif_cs: u64 = gif.iter().map(|f| u64::from(f.2)).sum();
    assert!((seconds - gif_cs as f64 / 100.0).abs() < 0.001, "{seconds}");
    assert!(
        err.contains("3 scenes") && err.contains("tallest: scene 3"),
        "{err}"
    );
    // `--theme light`: every scene is light.
    let all_light = gif_frames(&export(
        &talk(),
        &dir.join("light.gif"),
        &[&flags[..], &["--theme", "light"]].concat(),
    ));
    assert_eq!(all_light[0].0.at(1, 1), light);
}

#[test]
fn a_talk_frame_has_the_size_of_its_widest_and_its_tallest_scene() {
    let dir = scratch("talk-export-size");
    let flags = ["--scale", "1", "--interval", "1"];
    let size = |name: &str| {
        let f = gif_frames(&export(
            &scene(name),
            &dir.join(format!("{name}.gif")),
            &flags,
        ));
        (f[0].0.width, f[0].0.height)
    };
    let sizes: Vec<(u32, u32)> = SHOWN.iter().map(|n| size(n)).collect();
    let want = (
        sizes.iter().map(|s| s.0).max().unwrap(),
        sizes.iter().map(|s| s.1).max().unwrap(),
    );
    // The first scene is not the tallest.
    assert!(sizes[0].1 < want.1);
    let out = dir.join("talk.gif");
    let err = run_ok(&["render", s(&talk()), "--out", s(&out), "--scale", "1"]);
    let gif = gif_frames(&std::fs::read(&out).unwrap());
    assert!(gif.iter().all(|f| (f.0.width, f.0.height) == want));
    assert!(
        err.contains(&format!("{}×{} px", want.0, want.1))
            && err.contains("tallest: scene 3 (tall)"),
        "{err}"
    );
}

/// The delays of a GIF, in centiseconds.
fn delays(file: &Path, out: &Path, extra: &[&str]) -> Vec<u16> {
    let mut flags = vec!["--scale", "1", "--interval", "1"];
    flags.extend_from_slice(extra);
    gif_frames(&export(file, out, &flags))
        .into_iter()
        .map(|f| f.2)
        .collect()
}

#[test]
fn the_last_step_holds_twice_only_at_the_end_of_the_talk() {
    let dir = scratch("talk-export-hold");
    let alone: Vec<Vec<u16>> = SHOWN
        .iter()
        .map(|n| delays(&scene(n), &dir.join(format!("{n}.gif")), &[]))
        .collect();
    // Alone, each scene ends with its last step twice as long (the light
    // scene times its last step: 1.5 s).
    assert_eq!(*alone[0].last().unwrap(), 200);
    assert_eq!(*alone[1].last().unwrap(), 150);
    assert_eq!(*alone[2].last().unwrap(), 200);
    // In the talk (whose last entry is a skipped scene): the typed scene
    // hands over after one interval, the light one after its own time,
    // and the tall one, the last that shows, holds twice.
    let whole = delays(&talk(), &dir.join("talk.gif"), &[]);
    let (a, b) = (alone[0].len(), alone[1].len());
    assert_eq!(whole.len(), a + b + alone[2].len());
    assert_eq!(whole[a - 1], 100);
    assert_eq!(whole[a + b - 1], 150);
    assert_eq!(*whole.last().unwrap(), 200);
    // A talk that ends with a timed step: that step keeps its own time.
    let timed_end = fixture("export/talk/timed-end.tiza");
    let end = delays(&timed_end, &dir.join("end.gif"), &[]);
    assert_eq!(end[a - 1], 100);
    assert_eq!(*end.last().unwrap(), 150);
    // `--scene` exports the scene alone, with its own end.
    for (spec, name) in [("2", "light"), ("light", "light"), ("3", "tall")] {
        let flags = ["--scale", "1", "--interval", "1", "--scene", spec];
        let one = export(&talk(), &dir.join("one.gif"), &flags);
        let alone = export(&scene(name), &dir.join("alone.gif"), &flags[..4]);
        assert!(one == alone, "--scene {spec}: not the GIF of {name} alone");
    }
}

/// The bytes of an export, with the `timestamp` of the header of a
/// `.cast` taken out: it is the time of the export, in seconds, so two
/// exports of the same scene a second apart differ only there.
fn without_cast_time(bytes: Vec<u8>) -> Vec<u8> {
    let text = String::from_utf8_lossy(&bytes);
    let Some((head, rest)) = text.split_once('\n') else {
        return bytes;
    };
    match serde_json::from_str::<serde_json::Value>(head) {
        Ok(mut h) if h.get("version").is_some() && h.get("timestamp").is_some() => {
            h.as_object_mut().unwrap().remove("timestamp");
            format!("{h}\n{rest}").into_bytes()
        }
        _ => bytes,
    }
}

#[test]
fn what_a_whole_talk_cannot_be_is_refused_with_what_to_give() {
    let dir = scratch("talk-export-refused");
    let t = talk();
    let t = s(&t);
    let refused = |args: &[&str], want: &str| {
        let (ok, _, err) = run(args);
        assert!(!ok && err.contains(want), "tiza {args:?}:\n{err}");
    };
    let still = "give --scene ID --step K";
    refused(&["render", t], still);
    refused(&["render", t, "--out", s(&dir.join("x.png"))], still);
    refused(&["render", t, "--out", s(&dir.join("x.svg"))], still);
    refused(
        &["render", t, "--out", s(&dir.join("x.webp")), "--step", "1"],
        still,
    );
    refused(
        &["render", t, "--out", s(&dir.join("x.cast"))],
        "a .cast of a talk is not made yet",
    );
    refused(
        &["render", t, "--out", s(&dir.join("x.tape"))],
        "a .tape of a talk is not made yet",
    );
    // A tape opens the file it is given in the TUI: the master would play
    // the whole talk.
    refused(
        &["render", t, "--scene", "1", "--out", s(&dir.join("x.tape"))],
        "give the scene file",
    );
    refused(
        &[
            "render",
            t,
            "--all-steps",
            "--out",
            &format!("{}/", s(&dir.join("all"))),
        ],
        "--all-steps writes the steps of one document",
    );
    refused(
        &["render", t, "--out", "-", "--format", "gif"],
        "needs a file",
    );
    let lang = "--lang does not work with a talk";
    refused(
        &["render", t, "--lang", "rs", "--out", s(&dir.join("x.gif"))],
        lang,
    );
    refused(
        &["--lang", "rs", "render", t, "--out", s(&dir.join("x.gif"))],
        lang,
    );
    let theme = dir.join("theme.toml");
    std::fs::write(&theme, "").unwrap();
    refused(
        &[
            "render",
            t,
            "--theme-file",
            s(&theme),
            "--out",
            s(&dir.join("x.gif")),
        ],
        "--theme-file does not work with a talk",
    );
    let one = scene("light");
    refused(
        &[
            "render",
            s(&one),
            "--scene",
            "1",
            "--out",
            s(&dir.join("x.gif")),
        ],
        "--scene opens a scene of a talk",
    );
    refused(
        &["render", t, "--scene", "4", "--out", s(&dir.join("x.gif"))],
        "scene 4 is skipped",
    );
    // Its file is not there: no command to export it anyway.
    refused(
        &["render", t, "--scene", "4", "--out", s(&dir.join("x.gif"))],
        "its file",
    );
    // A master on stdin has no folder for its scenes.
    let r = tiza_qa::command()
        .args(["render", "-", "--out", s(&dir.join("x.gif"))])
        .stdin(std::fs::File::open(talk()).unwrap())
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&r.stderr);
    assert!(
        !r.status.success() && err.contains("give the path of the master"),
        "{err}"
    );
    // A still, a text export and a .cast of one scene: that scene alone,
    // byte for byte; the `--scene` before `render` too.
    for (out, extra) in [
        ("x.png", &["--step", "1"][..]),
        ("x.txt", &[][..]),
        ("x.cast", &[][..]),
    ] {
        let mut a = vec!["render", t, "--scene", "light", "--out"];
        let p = dir.join(format!("talk-{out}"));
        a.push(s(&p));
        a.extend_from_slice(extra);
        run_ok(&a);
        let q = dir.join(format!("alone-{out}"));
        let mut b = vec!["render", s(&one), "--out", s(&q)];
        b.extend_from_slice(extra);
        run_ok(&b);
        assert!(
            without_cast_time(std::fs::read(&p).unwrap())
                == without_cast_time(std::fs::read(&q).unwrap()),
            "{out}: --scene is not the scene alone"
        );
    }
    let p = dir.join("before.txt");
    run_ok(&["--scene", "2", "render", t, "--out", s(&p)]);
    assert!(std::fs::read(&p).unwrap() == std::fs::read(dir.join("alone-x.txt")).unwrap());
}

#[test]
fn the_talk_take_lends_each_scene_with_its_theme_at_the_size_of_the_talk() {
    use tiza_core::anim::{AnimOptions, Take, TalkPart, TalkTake};
    use tiza_core::model::ThemeName;
    use tiza_core::theme::Theme;
    let theme = |name| Theme::resolve(name, true, None, None).0;
    let opts = |hold_last| AnimOptions {
        width: 70,
        hold_last,
        ..AnimOptions::default()
    };
    let docs: Vec<_> = SHOWN
        .iter()
        .map(|n| tiza_core::fs::load_document(&scene(n)).unwrap())
        .collect();
    let themes = [ThemeName::Dark, ThemeName::Light, ThemeName::Dark];
    let takes: Vec<Take> = docs
        .iter()
        .zip(themes)
        .enumerate()
        .map(|(k, (d, t))| Take::record(d, &theme(t), &opts(k == 2)))
        .collect();
    let talk = TalkTake::new(
        takes
            .iter()
            .zip(themes)
            .enumerate()
            .map(|(k, (take, t))| TalkPart {
                position: k + 1,
                name: None,
                take: take.clone(),
                theme: theme(t),
            })
            .collect(),
    );
    let (w, h) = talk.size();
    assert_eq!(w, takes.iter().map(|t| t.size().0).max().unwrap());
    assert_eq!(h, takes.iter().map(|t| t.size().1).max().unwrap());
    // In the other order too: the height is not the one of the first
    // scene. (Every scene is as wide as the width of the export.)
    let reversed = TalkTake::new(talk.parts().iter().rev().cloned().collect());
    assert_eq!(reversed.size(), (w, h));
    let first = |t: &TalkTake| t.parts()[0].take.size();
    assert!(first(&talk).1 < h || first(&reversed).1 < h);
    // Each scene's frames in order, padded with its own background.
    let mut want = Vec::new();
    for (k, take) in takes.iter().enumerate() {
        take.each(|f| -> Result<(), ()> {
            want.push((k, f.canvas.clone(), f.delay_ms));
            Ok(())
        })
        .unwrap();
    }
    let mut i = 0;
    talk.each(|t| -> Result<(), ()> {
        let (k, alone, delay) = &want[i];
        assert_eq!((t.part, t.frame.delay_ms), (*k, *delay), "frame {i}");
        assert_eq!(t.theme.bg, theme(themes[*k]).bg, "frame {i}");
        assert_eq!((t.frame.canvas.width, t.frame.canvas.height), (w, h));
        let base = alone.get(alone.width - 1, alone.height - 1).cloned();
        for y in 0..h {
            for x in 0..w {
                let want = alone.get(x, y).cloned().or(base.clone());
                assert_eq!(
                    t.frame.canvas.get(x, y).cloned(),
                    want,
                    "frame {i} at {x},{y}"
                );
            }
        }
        i += 1;
        Ok(())
    })
    .unwrap();
    assert_eq!(i, want.len());
}

/// The same cells twice, the first time with the dark theme and the second
/// time, as a new part, with the light one.
struct SameCellsTwoThemes {
    canvas: tiza_core::canvas::Canvas,
    themes: [tiza_core::theme::Theme; 2],
}

impl tiza_export::Frames for SameCellsTwoThemes {
    fn each(
        &self,
        f: &mut dyn FnMut(&tiza_core::canvas::Canvas, u64) -> Result<(), tiza_export::Error>,
    ) -> Result<(), tiza_export::Error> {
        f(&self.canvas, 100)?;
        f(&self.canvas, 100)
    }

    fn each_themed(
        &self,
        _theme: &tiza_core::theme::Theme,
        f: &mut tiza_export::ThemedFrame<'_>,
    ) -> Result<(), tiza_export::Error> {
        for (part, theme) in self.themes.iter().enumerate() {
            f(&self.canvas, 100, theme, part)?;
        }
        Ok(())
    }
}

#[test]
fn a_gif_draws_a_new_scene_with_its_theme_when_its_cells_are_the_same() {
    use tiza_core::canvas::{Canvas, CellStyle};
    use tiza_core::model::ThemeName;
    use tiza_core::theme::Theme;
    let theme = |name| Theme::resolve(name, true, None, None).0;
    // Cells with no color of their own: the theme gives the color of
    // their text.
    let mut canvas = Canvas::new(14, 3);
    canvas.put_str(1, 1, "same cells", CellStyle::default());
    let two = SameCellsTwoThemes {
        canvas: canvas.clone(),
        themes: [theme(ThemeName::Dark), theme(ThemeName::Light)],
    };
    let o = tiza_export::GifOptions::default();
    let (bytes, _) = tiza_export::to_gif_from(&two, &theme(ThemeName::Dark), &o).unwrap();
    let frames = gif_frames(&bytes);
    let alone = |name| {
        let (b, _) = tiza_export::to_gif(&[(canvas.clone(), 100)], &theme(name), &o).unwrap();
        gif_frames(&b).remove(0).0
    };
    let (dark, light) = (alone(ThemeName::Dark), alone(ThemeName::Light));
    assert!(dark.pixels != light.pixels);
    assert!(
        frames[0].0.pixels == dark.pixels,
        "the first frame is not dark"
    );
    assert!(
        frames[1].0.pixels == light.pixels,
        "the second frame does not have the theme of its part"
    );
}

#[test]
fn one_scene_needs_only_its_own_file_and_each_scene_says_what_it_says_alone() {
    let dir = scratch("talk-export-one-scene");
    tiza_qa::copy_dir(&fixture("export/talk"), &dir);
    // A box past the end of the code of the typed scene: alone, its open
    // notice names it.
    let typed = dir.join("scenes/typed.tiza");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&typed).unwrap()).unwrap();
    doc["annotations"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!(
            {"id": "far", "step": 1, "type": "box", "from_line": 40, "to_line": 41}
        ));
    std::fs::write(&typed, doc.to_string()).unwrap();
    let (ok, _, alone) = run(&["render", s(&typed), "--out", s(&dir.join("a.gif"))]);
    assert!(ok, "{alone}");
    let notice = alone
        .lines()
        .find(|l| l.contains("far"))
        .and_then(|l| l.strip_prefix("tiza: "))
        .unwrap_or_else(|| panic!("no notice alone:\n{alone}"));
    let (ok, _, err) = run(&["render", s(&talk_in(&dir)), "--out", s(&dir.join("t.gif"))]);
    assert!(ok, "{err}");
    assert!(
        err.contains(&format!("tiza: scene 1 (typed): {notice}\n")),
        "{err}"
    );
    // A master whose third scene is not there: the whole talk names it,
    // and `--scene` exports a scene that is there.
    let master = dir.join("broken.tiza");
    std::fs::write(
        &master,
        serde_json::json!({"schema": 2, "talk": {"scenes":
            ["scenes/typed.tiza", "scenes/light.tiza", "scenes/none.tiza"]}})
        .to_string(),
    )
    .unwrap();
    let (ok, _, err) = run(&["render", s(&master), "--out", s(&dir.join("b.gif"))]);
    assert!(!ok && err.contains("scene 3"), "{err}");
    let one = dir.join("one.gif");
    let flags = ["--scale", "1", "--interval", "1"];
    let got = export(&master, &one, &[&flags[..], &["--scene", "light"]].concat());
    let want = export(&dir.join("scenes/light.tiza"), &dir.join("l.gif"), &flags);
    assert!(got == want, "--scene light is not the scene alone");
    // A scene that is a master: refused as in the talk.
    std::fs::write(
        &master,
        serde_json::json!({"schema": 2, "talk": {"scenes":
            ["scenes/typed.tiza", "talk.tiza"]}})
        .to_string(),
    )
    .unwrap();
    let (ok, _, err) = run(&[
        "render",
        s(&master),
        "--scene",
        "2",
        "--out",
        s(&dir.join("n.gif")),
    ]);
    assert!(
        !ok && err.contains("is a talk: a talk plays documents, not other talks"),
        "{err}"
    );
}

/// `talk.tiza` in a copy of the fixture.
fn talk_in(dir: &Path) -> PathBuf {
    dir.join("talk.tiza")
}
