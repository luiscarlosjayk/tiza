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
//! - Review of 43ac5fd: the video times each frame as the GIF does (10 ms
//!   unit, 20 ms floor), so typed frames add up to the same length, and
//!   `--interval 0` still writes a video stream.
//!
//! The per-step measure decodes the video to 128×96 grey frames at 10 fps
//! and marks a step change wherever two consecutive frames differ by more
//! than 16 levels on some pixel: at that size the encoder's noise is a
//! level or two (also on a keyframe), and a step change is 40 levels or
//! more even when it only adds one note.

use std::path::{Path, PathBuf};
use std::process::Command;

use tiza_qa::{fixture, scratch, tiza};

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

    /// One `format=` entry of ffprobe, as text.
    fn format_entry(&self, video: &Path, entry: &str) -> String {
        let out = Command::new(&self.ffprobe)
            .args(["-v", "error", "-show_entries"])
            .arg(format!("format={entry}"))
            .args(["-of", "csv=p=0"])
            .arg(video)
            .output()
            .expect("run ffprobe");
        assert!(
            out.status.success(),
            "ffprobe {}: {}",
            video.display(),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// `format=duration` in seconds.
    fn duration(&self, video: &Path) -> f64 {
        let text = self.format_entry(video, "duration");
        text.parse()
            .unwrap_or_else(|_| panic!("ffprobe duration of {}: {text:?}", video.display()))
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
        let (frames, rest) = out.stdout.as_chunks::<{ W * H }>();
        assert!(
            rest.is_empty(),
            "raw video from {} came out truncated: {} bytes left over after {} whole frames of {} bytes ({W}x{H} gray)",
            video.display(),
            rest.len(),
            frames.len(),
            W * H
        );
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
                .zip(&frames[i])
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
/// video's total against the GIF's and its `-t` on the printed command
/// (AC2: the sum of the durations goes to ffmpeg, so the length is not left
/// to how this ffmpeg treats the repeated concat entry). Returns the GIF
/// delays and the video's path.
fn total_as_the_gif(
    ff: &Ffmpeg,
    dir: &Path,
    doc: &Path,
    ext: &str,
    flags: &[&str],
) -> (Vec<f64>, PathBuf) {
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
    assert_eq!(
        ff.format_entry(&video, "nb_streams"),
        "1",
        "{ext} {flags:?}: the file should hold one video stream"
    );
    let measured = ff.duration(&video);
    // Within one 30 fps frame: a sum that is not a multiple of 1/30 s
    // (typed frames) ends on the last whole frame before it.
    assert!(
        (measured - total).abs() < 0.05,
        "{ext} {flags:?}: ffprobe says {measured} s, the GIF lasts {total} s"
    );
    assert!(
        printed.contains(&format!(" -t {total:.3} ")),
        "the printed command should carry `-t {total:.3}`:\n{printed}"
    );
    (delays, video)
}

/// `total_as_the_gif`, plus every step's length against the GIF's delay.
fn same_as_the_gif(ff: &Ffmpeg, dir: &Path, doc: &Path, ext: &str, flags: &[&str]) -> Vec<f64> {
    let (delays, video) = total_as_the_gif(ff, dir, doc, ext, flags);
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
    delays
}

#[test]
fn ac1_ac2_ac3_the_last_step_of_the_example_lasts_four_seconds_not_eight() {
    let Some(ff) = ffmpeg() else { return };
    let dir = scratch("t170-example");
    let ex = fixture("shared/ownership/ownership.rs.tiza");
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
fn review_typed_frames_are_timed_as_the_gif_times_them() {
    let Some(ff) = ffmpeg() else { return };
    let dir = scratch("t170-typed");
    // A move typed at 25 ms per character: the GIF rounds each typed frame
    // up to its 10 ms unit (30 ms), and the video uses the same delays, so
    // both add up to the same length (the typed frames are too short for
    // the 10 fps step measure, so only the total is compared here).
    let (delays, _) = total_as_the_gif(
        &ff,
        &dir,
        &fixture("t-170/typed.tiza"),
        "mp4",
        &["--scale", "1"],
    );
    let typed: Vec<&f64> = delays.iter().filter(|d| **d < 1.0).collect();
    assert!(
        typed.len() >= 10 && typed.iter().all(|d| (**d - 0.03).abs() < 1e-9),
        "typed frames at 25 ms should be 30 ms in the GIF: {delays:?}"
    );
    assert_eq!(delays.first(), Some(&2.0));
    assert_eq!(delays.last(), Some(&4.0));
}

#[test]
fn review_interval_zero_still_writes_a_video_stream() {
    let Some(ff) = ffmpeg() else { return };
    let dir = scratch("t170-zero");
    // `--interval 0`: the GIF gives each step its 20 ms floor; the video
    // used to get `-t 0.000` and come out as a file with no stream at all.
    let doc = fixture("t-170/one-step.tiza");
    let (delays, video) =
        total_as_the_gif(&ff, &dir, &doc, "mp4", &["--interval", "0", "--scale", "1"]);
    assert_eq!(delays, vec![0.02, 0.02]);
    assert!(ff.duration(&video) > 0.0);
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
    let delays = same_as_the_gif(
        &ff,
        &dir,
        &fixture("shared/ownership/ownership.rs.tiza"),
        "webm",
        &flags,
    );
    assert_eq!(delays.iter().sum::<f64>(), 18.0);
    let delays = same_as_the_gif(&ff, &dir, &fixture("t-170/last-timed.tiza"), "webm", &flags);
    assert_eq!(delays.last(), Some(&1.5));
}
