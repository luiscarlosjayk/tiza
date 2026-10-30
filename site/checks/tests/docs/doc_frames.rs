//! The `doc=` frames of the docs pages are real exports of the current
//! renderer. `scripts/docs_frames.py --only-doc` makes them again, and this
//! check compares them with the committed images: the pixels of a still,
//! and the frames and their times of an animation. Decoded images, not
//! bytes: a lossless encoder can choose another encoding of the same pixels.
//!
//! The `code=` frames type their commands in the TUI on a pseudo-terminal:
//! this check does not make them again.

use std::path::{Component, Path, PathBuf};

use tiza_qa::{bin, repo_root, scratch, webp_frames, webp_still};

/// A `doc=` frame of a page: the page, its id, whether it is an animation,
/// the image (relative to the repository) and the `width` of its `<img>`.
struct DocFrame {
    page: String,
    id: String,
    animated: bool,
    image: PathBuf,
    css_width: u32,
}

/// `dir/rel` with its `..` taken out.
fn join(dir: &Path, rel: &str) -> PathBuf {
    let mut out = PathBuf::new();
    for c in dir.join(rel).components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// The `doc=` frames of every page under docs/.
fn doc_frames() -> Vec<DocFrame> {
    let root = repo_root();
    let mut pages = Vec::new();
    let mut dirs = vec![root.join("docs")];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "md") {
                pages.push(p);
            }
        }
    }
    pages.sort();
    let mut out = Vec::new();
    for p in pages {
        let md = std::fs::read_to_string(&p).unwrap();
        let page = p
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        for line in md.lines() {
            let Some(fields) = line
                .strip_prefix("<!-- frame ")
                .and_then(|l| l.strip_suffix(" -->"))
            else {
                continue;
            };
            let field = |name: &str| {
                fields
                    .split(' ')
                    .find_map(|f| f.strip_prefix(&format!("{name}=")))
            };
            if field("doc").is_none() {
                continue;
            }
            let id = field("id").expect("a frame without id=").to_string();
            let tag = format!("/{id}.webp\" width=\"");
            let at = md
                .find(&tag)
                .unwrap_or_else(|| panic!("{page}: frame {id} has no <img>"));
            let src_start = md[..at].rfind("<img src=\"").unwrap() + "<img src=\"".len();
            let src = &md[src_start..at + tag.len() - "\" width=\"".len()];
            let rest = &md[at + tag.len()..];
            let css_width = rest[..rest.find('"').unwrap()].parse().unwrap();
            out.push(DocFrame {
                image: join(Path::new(&page).parent().unwrap(), src),
                page: page.clone(),
                animated: field("animate").is_some(),
                id,
                css_width,
            });
        }
    }
    out
}

/// The width in pixels of a WebP: the canvas of the extended format
/// (VP8X, which an animation has), or the width of a simple still.
fn pixel_width(bytes: &[u8]) -> u32 {
    if &bytes[12..16] == b"VP8X" {
        (u32::from(bytes[24]) | u32::from(bytes[25]) << 8 | u32::from(bytes[26]) << 16) + 1
    } else {
        webp_still(bytes).1
    }
}

#[test]
fn the_doc_frames_are_fresh_exports_of_the_current_renderer() {
    let frames = doc_frames();
    assert!(!frames.is_empty(), "no doc= frame in docs/");
    let out = scratch("doc-frames");
    let r = std::process::Command::new("python3")
        .arg("-I")
        .arg(repo_root().join("scripts/docs_frames.py"))
        .arg(bin())
        .args(["--only-doc", "--out"])
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "docs_frames.py --only-doc: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    for f in frames {
        let what = format!("{}: frame {} ({})", f.page, f.id, f.image.display());
        let committed =
            std::fs::read(repo_root().join(&f.image)).unwrap_or_else(|e| panic!("{what}: {e}"));
        let fresh = std::fs::read(out.join(&f.image)).unwrap_or_else(|e| panic!("{what}: {e}"));
        let stale = format!("{what} is stale: run scripts/docs-frames.sh {}", f.page);
        if f.animated {
            assert!(webp_frames(&committed) == webp_frames(&fresh), "{stale}");
        } else {
            assert!(webp_still(&committed) == webp_still(&fresh), "{stale}");
        }
        // The script writes half the pixels in the width of the <img>.
        assert_eq!(f.css_width, pixel_width(&committed) / 2, "{stale}");
    }
}
