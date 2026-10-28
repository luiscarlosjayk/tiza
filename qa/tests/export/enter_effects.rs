//! Enter effects (experimental) in the exports: a GIF or a WebP with
//! `--effects` has the frames of each step's effect, and a still or a text
//! export shows the final drawing, the same as without effects.

use tiza_core::anim::{AnimOptions, frames};
use tiza_qa::{fixture, scratch, tiza};

/// The number of frames that `tiza render` says it wrote.
fn frames_written(args: &[&str]) -> usize {
    let out = tiza(args);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "tiza {args:?}: {err}");
    let words: Vec<&str> = err.split_whitespace().collect();
    let i = words
        .iter()
        .position(|w| w.starts_with("frame"))
        .unwrap_or_else(|| panic!("no frame count in {err}"));
    words[i - 1].parse().unwrap()
}

#[test]
fn a_gif_and_a_webp_with_effects_have_the_frames_of_each_effect() {
    let doc_path = fixture("shared/enter_effects/drop.tiza");
    let doc = doc_path.to_str().unwrap();
    // What the core plans for the document with effects: frames of an
    // effect at each of steps 1, 2 and 3.
    let mut d = tiza_core::fs::open(doc).unwrap().document;
    d.effects = Some(tiza_core::effects::Effects::parse("draw,dim").unwrap());
    let planned = frames(&d, &tiza_core::Theme::dark(), &AnimOptions::default());
    let effect_frames = planned.iter().filter(|f| f.effect).count();
    for step in 1..=3 {
        assert!(
            planned.iter().any(|f| f.effect && f.step == step),
            "no effect at step {step}"
        );
    }
    let dir = scratch("effects-gif");
    for ext in ["gif", "webp"] {
        let plain = dir.join(format!("plain.{ext}"));
        let with = dir.join(format!("effects.{ext}"));
        let base = ["render", doc, "--scale", "1", "--out"];
        let n_plain = frames_written(&[&base[..], &[plain.to_str().unwrap()]].concat());
        let n_with = frames_written(
            &[
                &base[..],
                &[with.to_str().unwrap(), "--effects", "draw,dim"],
            ]
            .concat(),
        );
        assert_eq!(n_plain, 4, "{ext}: one frame for each step 0 to 3");
        assert_eq!(n_with, 4 + effect_frames, "{ext}");
    }
}

#[test]
fn a_still_and_a_text_export_show_the_final_drawing() {
    let doc_path = fixture("shared/enter_effects/drop.tiza");
    let doc = doc_path.to_str().unwrap();
    let dir = scratch("effects-still");
    for (ext, step) in [("png", "2"), ("svg", "3"), ("txt", "2"), ("ansi", "3")] {
        let plain = dir.join(format!("plain.{ext}"));
        let with = dir.join(format!("effects.{ext}"));
        for (out, fx) in [(&plain, None), (&with, Some("draw,dim"))] {
            let mut args = vec![
                "render",
                doc,
                "--step",
                step,
                "--out",
                out.to_str().unwrap(),
            ];
            if let Some(fx) = fx {
                args.extend(["--effects", fx]);
            }
            let o = tiza(&args);
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        }
        assert!(
            std::fs::read(&plain).unwrap() == std::fs::read(&with).unwrap(),
            "{ext} at step {step} differs with --effects"
        );
    }
}
