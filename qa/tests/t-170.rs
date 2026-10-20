//! t-170 — MP4/WebM: the last step lasts what the GIF gives it, not twice
//! as long (the concat list repeats the last frame, and ffmpeg 7+ counts
//! that repeat as a whole step; Luis's 9.0.2 made an 8 s last step out of
//! a 4 s one).
//!
//! Acceptance criteria:
//! - AC1 Every step of an MP4/WebM lasts what the GIF gives it (its
//!   `auto_advance_ms`, or `--interval` with the README's rule for the
//!   last frame), measured with the machine's ffmpeg/ffprobe; the total
//!   is the GIF's.
//! - AC2 The fix does not depend on how one ffmpeg version treats the
//!   repeated concat entry: `-t <sum of the durations>` cuts the output
//!   at the GIF's length on every version.
//! - AC3 This file measures the MP4 when ffmpeg is on PATH, and otherwise
//!   skips with a clear notice.
//!
//! The per-step measure decodes the video to 128×96 grey frames at 10 fps
//! and marks a step change wherever two consecutive frames differ by more
//! than 16 levels on some pixel: at that size the encoder's noise is a
//! level or two (also on a keyframe), and a step change is 40 levels or
//! more even when it only adds one note.

use std::path::{Path, PathBuf};
use std::process::Command;

use tiza_qa::{example, fixture, scratch, tiza};

struct Ffmpeg {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

/// ffmpeg and ffprobe from PATH (ffprobe may also sit next to ffmpeg), or
/// `None` with the AC3 notice on stderr.
fn ffmpeg() -> Option<Ffmpeg> {
    let on_path = |name: &str| {
        std::env::var_os("PATH").and_then(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join(name))
                .find(|p| p.is_file())
        })
    };
    let Some(ffmpeg) = on_path("ffmpeg") else {
        eprintln!(
            "t-170: ffmpeg is not on PATH, so the MP4/WebM durations are not measured here \
             (`brew install ffmpeg` / `sudo apt install ffmpeg` and run this test again)"
        );
        return None;
    };
    let Some(ffprobe) = on_path("ffprobe").or_else(|| {
        let next = ffmpeg.with_file_name("ffprobe");
        next.is_file().then_some(next)
    }) else {
        eprintln!(
            "t-170: ffprobe is not on PATH (ffmpeg is, at {}), so the MP4/WebM durations \
             are not measured here",
            ffmpeg.display()
        );
        return None;
    };
    Some(Ffmpeg { ffmpeg, ffprobe })
}

impl Ffmpeg {
    fn has_encoder(&self, name: &str) -> bool {
        let out = Command::new(&self.ffmpeg)
            .args(["-hide_banner", "-encoders"])
            .output()
            .expect("run ffmpeg -encoders");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .any(|l| l.split_whitespace().nth(1) == Some(name))
    }

    /// `format=duration` in seconds.
    fn duration(&self, video: &Path) -> f64 {
        let out = Command::new(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
            ])
            .arg(video)
            .output()
            .expect("run ffprobe");
        assert!(
            out.status.success(),
            "ffprobe {}: {}",
            video.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .expect("ffprobe duration")
    }

    /// How long each step stays on screen, in seconds, from the frames
    /// themselves (see the header), to a tenth of a second.
    fn steps(&self, video: &Path) -> Vec<f64> {
        const W: usize = 128;
        const H: usize = 96;
        let out = Command::new(&self.ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(video)
            .args([
                "-vf",
                "scale=128:96,fps=10",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "gray",
                "-",
            ])
            .output()
            .expect("run ffmpeg");
        assert!(
            out.status.success(),
            "ffmpeg {}: {}",
            video.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        let frames: Vec<&[u8]> = out.stdout.chunks_exact(W * H).collect();
        assert!(
            !frames.is_empty(),
            "no frames decoded from {}",
            video.display()
        );
        let mut steps = Vec::new();
        let mut start = 0;
        for i in 1..frames.len() {
            let changed = frames[i - 1]
                .iter()
                .zip(frames[i])
                .any(|(a, b)| a.abs_diff(*b) > 16);
            if changed {
                steps.push((i - start) as f64 / 10.0);
                start = i;
            }
        }
        steps.push((frames.len() - start) as f64 / 10.0);
        steps
    }
}

/// Frame delays of a GIF, in seconds.
fn gif_delays(gif: &Path) -> Vec<f64> {
    let bytes = std::fs::read(gif).unwrap();
    let mut o = gif::DecodeOptions::new();
    o.set_color_output(gif::ColorOutput::Indexed);
    let mut d = o.read_info(std::io::Cursor::new(bytes)).unwrap();
    let mut delays = Vec::new();
    while let Some(f) = d.read_next_frame().unwrap() {
        delays.push(f.delay as f64 / 100.0);
    }
    delays
}

/// Renders `doc` as a GIF and as `ext` with the same flags, and checks the
/// video's total against the GIF's, its `-t` on the printed command, and
/// every step's length against the GIF's delay. Returns the GIF delays.
fn same_as_the_gif(ff: &Ffmpeg, dir: &Path, doc: &Path, ext: &str, flags: &[&str]) -> Vec<f64> {
    let gif = dir.join(format!("{ext}.gif"));
    let video = dir.join(format!("out.{ext}"));
    let mut printed = String::new();
    for out in [&gif, &video] {
        let mut args = vec![
            "render",
            doc.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ];
        args.extend_from_slice(flags);
        let r = tiza(&args);
        printed = String::from_utf8_lossy(&r.stderr).into_owned();
        assert!(r.status.success(), "tiza {args:?}:\n{printed}");
    }
    let delays = gif_delays(&gif);
    let total: f64 = delays.iter().sum();
    let measured = ff.duration(&video);
    assert!(
        (measured - total).abs() < 0.05,
        "{ext} {flags:?}: ffprobe says {measured} s, the GIF lasts {total} s"
    );
    let steps = ff.steps(&video);
    assert_eq!(
        steps.len(),
        delays.len(),
        "{ext} {flags:?}: steps {steps:?} vs GIF delays {delays:?}"
    );
    for (i, (s, d)) in steps.iter().zip(&delays).enumerate() {
        assert!(
            (s - d).abs() < 0.15,
            "{ext} {flags:?}: step {i} lasts {s} s in the video and {d} s in the GIF \
             (video {steps:?}, GIF {delays:?})"
        );
    }
    // AC2: the sum of the durations goes to ffmpeg as `-t`, so the length
    // is not left to how this ffmpeg treats the repeated concat entry.
    assert!(
        printed.contains(&format!(" -t {total:.3} ")),
        "the printed command should carry `-t {total:.3}`:\n{printed}"
    );
    delays
}

#[test]
fn ac1_ac2_ac3_the_last_step_of_the_example_lasts_four_seconds_not_eight() {
    let Some(ff) = ffmpeg() else { return };
    let dir = scratch("t170-example");
    let ex = example("ownership.rs.tiza");
    // 18 s: seven steps of 2 s and the last one twice as long, as the GIF.
    let delays = same_as_the_gif(&ff, &dir, &ex, "mp4", &[]);
    assert_eq!(delays, vec![2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 2.0, 4.0]);
    // `--interval` and `--skip-zero` change the sum; the video follows.
    let delays = same_as_the_gif(&ff, &dir, &ex, "mp4", &["--interval", "1", "--skip-zero"]);
    assert_eq!(delays, vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0]);
}

#[test]
fn ac1_a_last_step_with_auto_advance_ms_lasts_exactly_that_long() {
    let Some(ff) = ffmpeg() else { return };
    let dir = scratch("t170-timed");
    // Step 3 holds 3 s and the last step 1.5 s (not 2×interval); the
    // other steps keep `--interval`.
    let delays = same_as_the_gif(&ff, &dir, &fixture("t-170/last-timed.tiza"), "mp4", &[]);
    assert_eq!(delays, vec![2.0, 2.0, 2.0, 3.0, 2.0, 2.0, 2.0, 1.5]);
    assert_eq!(delays.iter().sum::<f64>(), 16.5);
}

#[test]
fn corner_a_single_step_document() {
    let Some(ff) = ffmpeg() else { return };
    let dir = scratch("t170-one");
    let doc = fixture("t-170/one-step.tiza");
    // Step 0 and the only annotated step: 2 s + 4 s.
    let delays = same_as_the_gif(&ff, &dir, &doc, "mp4", &[]);
    assert_eq!(delays, vec![2.0, 4.0]);
    // One frame in the whole video: still held for its 4 s, since the
    // repeated concat entry is what makes ffmpeg hold a lone frame at all.
    let delays = same_as_the_gif(&ff, &dir, &doc, "mp4", &["--skip-zero"]);
    assert_eq!(delays, vec![4.0]);
}

#[test]
fn corner_webm_shares_the_frame_list_and_the_fix() {
    let Some(ff) = ffmpeg() else { return };
    if !ff.has_encoder("libvpx-vp9") {
        eprintln!(
            "t-170: this ffmpeg ({}) has no libvpx-vp9 encoder, so the WebM is not measured here",
            ff.ffmpeg.display()
        );
        return;
    }
    let dir = scratch("t170-webm");
    // `--scale 1`: VP9 takes its time on the 2048 px frames, and the
    // timing is the same at half the size.
    let flags = ["--scale", "1"];
    let delays = same_as_the_gif(&ff, &dir, &example("ownership.rs.tiza"), "webm", &flags);
    assert_eq!(delays.iter().sum::<f64>(), 18.0);
    let delays = same_as_the_gif(&ff, &dir, &fixture("t-170/last-timed.tiza"), "webm", &flags);
    assert_eq!(delays.last(), Some(&1.5));
}
