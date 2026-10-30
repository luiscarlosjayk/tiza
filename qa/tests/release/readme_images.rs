//! Rendering an example for the README images leaves the files of the
//! example as they are. `readme_images` of the checks of the docs
//! (`site/checks`) checks the images.

use tiza_qa::{example, scratch, tiza_ok};

const WIDTH: &str = "92";

fn render(args: &[&str], out: &std::path::Path) {
    let ex = example("ownership.rs.tiza");
    let mut a = vec![
        "render",
        ex.to_str().unwrap(),
        "--width",
        WIDTH,
        "--out",
        out.to_str().unwrap(),
    ];
    a.extend_from_slice(args);
    tiza_ok(&a);
}

#[test]
fn the_examples_are_untouched_by_rendering() {
    // `tiza render` only reads the document: the example is the same bytes
    // after an export (the autosave belongs to the TUI).
    let ex = example("ownership.rs.tiza");
    let before = std::fs::read(&ex).unwrap();
    render(&["--step", "3"], &scratch("ex").join("x.png"));
    assert_eq!(std::fs::read(&ex).unwrap(), before);
}
