//! The install command of the README is the one of `install.sh`, before
//! `cargo install tiza`, and the README and the install page do not give
//! the variable that only the tests of `install.sh` use.

use tiza_qa::repo_file;

#[test]
fn the_readme_gives_the_command_first() {
    let command =
        "curl -fsSL https://raw.githubusercontent.com/luiscarlosjayk/tiza/main/install.sh | sh";
    let script = repo_file("install.sh");
    assert!(
        script.contains("SCRIPT_URL=https://raw.githubusercontent.com/$REPO/main/install.sh\n")
    );
    let readme = repo_file("README.md");
    let install = readme
        .split("\n## Installation\n")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .expect("an Installation section");
    let first = install
        .find(&format!("```sh\n{command}\n```"))
        .expect("the command");
    let at = install
        .find("cargo install tiza")
        .expect("cargo install tiza");
    assert!(first < at, "the command comes before cargo install tiza");
    // The install page gives the options of the script; the variable that
    // only the tests use is in neither.
    let page = repo_file("docs/guides/install.md");
    for text in [install, page.as_str()] {
        assert!(!text.contains("TIZA_DOWNLOAD_BASE"), "for tests only");
    }
}
