//! The links of SECURITY.md, CONTRIBUTING.md, the README and the release
//! notes go to files of the repository and to this repository, and the
//! reports go where they must.

use tiza_qa::markdown::link_targets;
use tiza_qa::{repo_file, repo_root};

/// The repository of the workspace (`repository` in Cargo.toml).
const REPO: &str = env!("CARGO_PKG_REPOSITORY");

/// The links of SECURITY.md, CONTRIBUTING.md, the README and the release
/// notes: a relative one names a file of the repository, an absolute one
/// this repository, and the reports go where Luis decided (2026-10-24):
/// Discussions, the bug form, the private vulnerability report.
#[test]
fn the_reporting_links_resolve_and_go_to_the_right_place() {
    let security = repo_file("SECURITY.md");
    assert!(security.contains(&format!("<{REPO}/security/advisories/new>")));
    for name in ["SECURITY.md", "CONTRIBUTING.md"] {
        for t in link_targets(&repo_file(name)) {
            if t.starts_with("https://") {
                assert!(t.starts_with(REPO), "{name}: {t}");
            } else {
                let file = t.split('#').next().unwrap();
                assert!(repo_root().join(file).is_file(), "{name}: {t}");
            }
        }
    }
    let readme = repo_file("README.md");
    for link in [
        "(CONTRIBUTING.md)".to_string(),
        "(SECURITY.md)".to_string(),
        format!("({REPO}/discussions)"),
    ] {
        assert!(readme.contains(&link), "the README does not link {link}");
    }
    let notes = repo_file("docs/release-notes/v0.1.0.md");
    for (name, md) in [("the release notes", &notes), ("the README", &readme)] {
        assert!(md.contains(&format!("({REPO}/discussions)")), "{name}");
        assert!(
            md.contains(&format!("({REPO}/issues/new/choose)")),
            "{name}"
        );
        assert!(
            !md.contains(&format!("({REPO}/issues)")),
            "{name} sends people to Issues without the form"
        );
    }
}
