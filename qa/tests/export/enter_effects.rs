//! Enter effects (experimental) in the exports: a GIF or a WebP has the
//! frames of the effect of each step that has one, and a still or a text
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

/// A copy of `drop.tiza` with no `effect`, as `dir/drop.tiza`.
fn plain_copy(dir: &std::path::Path) -> std::path::PathBuf {
    let text = std::fs::read_to_string(fixture("shared/enter_effects/drop.tiza")).unwrap();
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    for a in doc["annotations"].as_array_mut().unwrap() {
        a.as_object_mut().unwrap().remove("effect");
    }
    let path = dir.join("drop.tiza");
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    path
}

#[test]
fn a_gif_and_a_webp_with_effects_have_the_frames_of_each_effect() {
    let doc_path = fixture("shared/enter_effects/drop.tiza");
    let doc = doc_path.to_str().unwrap();
    // What the core plans for the document: frames of an effect at each of
    // steps 1 to 4 (the dim of the spotlight fades out at step 4).
    let d = tiza_core::fs::open(doc).unwrap().document;
    let planned = frames(&d, &tiza_core::Theme::dark(), &AnimOptions::default());
    let effect_frames = planned.iter().filter(|f| f.effect).count();
    for step in 1..=4 {
        assert!(
            planned.iter().any(|f| f.effect && f.step == step),
            "no effect at step {step}"
        );
    }
    let dir = scratch("effects-gif");
    let plain_doc = plain_copy(&dir);
    for ext in ["gif", "webp"] {
        let plain = dir.join(format!("plain.{ext}"));
        let with = dir.join(format!("effects.{ext}"));
        let n_plain = frames_written(&[
            "render",
            plain_doc.to_str().unwrap(),
            "--scale",
            "1",
            "--out",
            plain.to_str().unwrap(),
        ]);
        let n_with = frames_written(&[
            "render",
            doc,
            "--scale",
            "1",
            "--out",
            with.to_str().unwrap(),
        ]);
        assert_eq!(n_plain, 5, "{ext}: one frame for each step 0 to 4");
        assert_eq!(n_with, 5 + effect_frames, "{ext}");
    }
}

#[test]
fn a_still_and_a_text_export_show_the_final_drawing() {
    let doc_path = fixture("shared/enter_effects/drop.tiza");
    let doc = doc_path.to_str().unwrap();
    let dir = scratch("effects-still");
    let plain_dir = dir.join("plain");
    std::fs::create_dir_all(&plain_dir).unwrap();
    let plain_doc = plain_copy(&plain_dir);
    for (ext, step) in [("png", "2"), ("svg", "3"), ("txt", "2"), ("ansi", "3")] {
        let plain = dir.join(format!("plain.{ext}"));
        let with = dir.join(format!("effects.{ext}"));
        for (out, source) in [(&plain, plain_doc.to_str().unwrap()), (&with, doc)] {
            let o = tiza(&[
                "render",
                source,
                "--step",
                step,
                "--out",
                out.to_str().unwrap(),
            ]);
            assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        }
        assert!(
            std::fs::read(&plain).unwrap() == std::fs::read(&with).unwrap(),
            "{ext} at step {step} differs with effects"
        );
    }
}

#[test]
fn a_step_without_an_effect_adds_no_frames() {
    // Only the callout of step 3 has an effect.
    let d = tiza_core::fs::open(
        fixture("shared/enter_effects/drop-mixed.tiza")
            .to_str()
            .unwrap(),
    )
    .unwrap()
    .document;
    let planned = frames(&d, &tiza_core::Theme::dark(), &AnimOptions::default());
    let steps: std::collections::BTreeSet<u32> = planned
        .iter()
        .filter(|f| f.effect)
        .map(|f| f.step)
        .collect();
    assert_eq!(steps, [3].into(), "the steps with frames of an effect");
}
