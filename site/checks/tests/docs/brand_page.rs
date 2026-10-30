//! The page of the brand files, `docs/brand/README.md`, names each file of
//! `docs/brand/` and the two colors of the logo.

use tiza_qa::{repo_file, repository_files};

#[test]
fn the_brand_page_names_each_file_and_the_colors_of_the_logo() {
    let about = repo_file("docs/brand/README.md");
    let files: Vec<String> = repository_files()
        .into_iter()
        .filter_map(|f| f.strip_prefix("docs/brand/").map(str::to_string))
        .filter(|f| !f.contains('/') && f != "README.md")
        .collect();
    assert!(files.len() >= 5, "{files:?}");
    for f in &files {
        assert!(
            about.contains(&format!("`{f}`")),
            "docs/brand/README.md does not name {f}"
        );
    }
    let svg = repo_file("docs/brand/logo.svg");
    for color in ["#22272b", "#f3f0e7"] {
        assert!(
            svg.contains(color) && about.contains(color),
            "{color}: in logo.svg {}, on the page {}",
            svg.contains(color),
            about.contains(color)
        );
    }
}
