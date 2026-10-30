//! The docs site shows each frame of a page as two panels: the image, then
//! the block that makes it (`site/build.py`). site.js makes them the tabs
//! "Image" and "Source" or "Commands". This check runs build.py on the docs
//! in a copy of `site/`, and reads the pages that it writes: each frame
//! comment of `docs/` gives one frame, with its `<img>` first, then its
//! block as the page has it, the file that it needs and how to make it
//! again: the command opens the frame at the step where it starts, and a
//! `code=` frame does not save, so each frame starts from its code. A
//! frame with no block or no `<img>`, or a `doc=` frame whose block is not
//! its file, stops the build.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use tiza_qa::{repo_root, scratch};

/// A frame comment of a page: its id, its options, its block and its
/// `<img>`, as the page has them.
struct PageFrame {
    page: String,
    id: String,
    opts: BTreeMap<String, String>,
    block: Vec<String>,
    img: String,
}

/// The pages under `docs/`, relative to the repository, in order.
fn doc_pages(root: &Path) -> Vec<String> {
    let mut pages = Vec::new();
    let mut dirs = vec![root.join("docs")];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "md") {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy();
                pages.push(rel.replace('\\', "/"));
            }
        }
    }
    pages.sort();
    pages
}

/// The frames of a page. The two forms: the block, then the `<img>`; or
/// the `<img>`, then the block in a `<details>`.
fn page_frames(page: &str, md: &str) -> Vec<PageFrame> {
    let lines: Vec<&str> = md.split('\n').collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(rest) = line.strip_prefix("<!-- frame id=") else {
            continue;
        };
        let rest = rest
            .strip_suffix(" -->")
            .expect("the comment ends with -->");
        let mut words = rest.split(' ');
        let id = words.next().unwrap().to_string();
        let opts = words
            .map(|w| {
                let (k, v) = w.split_once('=').expect("key=value");
                (k.to_string(), v.to_string())
            })
            .collect();
        let start = (i + 1..lines.len())
            .find(|&j| lines[j].starts_with("```"))
            .unwrap();
        let end = (start + 1..lines.len())
            .find(|&j| lines[j] == "```")
            .unwrap();
        let img = (i + 1..lines.len())
            .find(|&j| lines[j].starts_with("<img "))
            .unwrap();
        out.push(PageFrame {
            page: page.to_string(),
            id,
            opts,
            block: lines[start..=end].iter().map(|l| l.to_string()).collect(),
            img: lines[img].to_string(),
        });
    }
    out
}

/// The value of the attribute `name` in an HTML tag.
fn attr<'a>(tag: &'a str, name: &str) -> &'a str {
    let key = format!(" {name}=\"");
    let at = tag
        .find(&key)
        .unwrap_or_else(|| panic!("no {name} in {tag}"))
        + key.len();
    &tag[at..at + tag[at..].find('"').unwrap()]
}

/// Runs build.py of a copy of `site/` on the repository, and gives the
/// folder of the copy.
fn build_a_copy() -> PathBuf {
    let root = repo_root();
    let site = scratch("frame-tabs").join("site");
    std::fs::create_dir_all(&site).unwrap();
    for f in ["build.py", "nav.json"] {
        std::fs::copy(root.join("site").join(f), site.join(f)).unwrap();
    }
    let out = Command::new("python3")
        .args(["-I", "-B"])
        .arg(site.join("build.py"))
        .args(["--repo", root.to_str().unwrap(), "--ref", "main"])
        .output()
        .expect("run python3");
    assert!(
        out.status.success(),
        "build.py failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    site
}

/// The pages that build.py wrote, by the repository path in their front
/// matter (`source = "docs/…"`).
fn built_pages(site: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut dirs = vec![site.join("content")];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                dirs.push(p);
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            if let Some(src) = text
                .lines()
                .find_map(|l| l.strip_prefix("source = \"")?.strip_suffix('"'))
            {
                out.insert(src.to_string(), text);
            }
        }
    }
    out
}

#[test]
fn each_frame_shows_its_image_then_the_block_that_makes_it() {
    let root = repo_root();
    let pages = doc_pages(&root);
    let before: Vec<String> = pages
        .iter()
        .map(|p| std::fs::read_to_string(root.join(p)).unwrap())
        .collect();
    let site = build_a_copy();
    let built = built_pages(&site);
    let mut frames = 0;
    for (page, md) in pages.iter().zip(&before) {
        for f in page_frames(page, md) {
            frames += 1;
            let at = format!("{} frame {}", f.page, f.id);
            let html = built
                .get(&f.page)
                .unwrap_or_else(|| panic!("{at}: the page is not on the site"));
            let lines: Vec<&str> = html.split('\n').collect();
            let source = if f.opts.contains_key("doc") {
                "Source"
            } else {
                "Commands"
            };
            let open = format!(
                "<div class=\"frame\" id=\"frame-{}\" data-source=\"{source}\">",
                f.id
            );
            let i = lines
                .iter()
                .position(|l| *l == open)
                .unwrap_or_else(|| panic!("{at}: no {open}"));
            // The image first: the same <img>, with the path of the site.
            assert_eq!(
                lines[i + 1],
                "<div class=\"frame-panel frame-image\">",
                "{at}"
            );
            let img = lines[i + 3];
            assert!(img.starts_with("<img "), "{at}: {img}");
            for a in ["width", "alt"] {
                assert_eq!(attr(img, a), attr(&f.img, a), "{at}: {a} of the <img>");
            }
            assert_eq!(
                Path::new(attr(img, "src")).file_name(),
                Path::new(attr(&f.img, "src")).file_name(),
                "{at}: the image"
            );
            assert_eq!(lines[i + 5], "</div>", "{at}");
            // Then the block, as the page has it.
            assert_eq!(
                lines[i + 6],
                "<div class=\"frame-panel frame-source\">",
                "{at}"
            );
            assert!(lines[i + 7].starts_with("<p class=\"frame-file\">"), "{at}");
            let block = &lines[i + 9..i + 9 + f.block.len()];
            assert_eq!(block, f.block.as_slice(), "{at}: the block");
            let how = lines[i + 10 + f.block.len()];
            assert!(how.starts_with("<p class=\"frame-how\">"), "{at}: {how}");
            if let Some(doc) = f.opts.get("doc") {
                let file = std::fs::read_to_string(root.join(doc)).unwrap();
                assert_eq!(
                    file.trim_end_matches('\n'),
                    f.block[1..f.block.len() - 1].join("\n"),
                    "{at}: the block is {doc}"
                );
                let name = Path::new(doc).file_name().unwrap().to_str().unwrap();
                let run = match (f.opts.get("step"), f.opts.get("animate")) {
                    (Some(step), _) => format!("<code>tiza {name} --step {step}</code>"),
                    (None, Some(from)) => {
                        format!("<code>tiza {name} --present --step {from}</code>")
                    }
                    (None, None) => panic!("{at}: no step= and no animate="),
                };
                assert!(how.contains(&run), "{at}: {how}");
                // A link to the document and to the code next to it.
                let json: serde_json::Value = serde_json::from_str(&file).unwrap();
                let dir = Path::new(doc).parent().unwrap();
                for linked in [
                    doc.to_string(),
                    dir.join(json["code"]["file"].as_str().unwrap())
                        .display()
                        .to_string(),
                ] {
                    assert!(
                        how.contains(&format!("/main/{linked}\"")),
                        "{at}: no link to {linked}: {how}"
                    );
                }
            } else {
                let code = &f.opts["code"];
                let (path, lines) = code.rsplit_once(':').unwrap();
                let run = match f.opts.get("files") {
                    // A command reads its file from the folder where Tiza
                    // runs: the code and the file go in one folder.
                    Some(files) => {
                        for file in files.split(',') {
                            assert!(
                                how.contains(&format!("/main/{file}\"")),
                                "{at}: no link to {file}: {how}"
                            );
                        }
                        let name = Path::new(path).file_name().unwrap().to_str().unwrap();
                        format!("<code>tiza {name}:{lines} --no-save</code>")
                    }
                    None => format!("<code>tiza {code} --no-save</code>"),
                };
                assert!(how.contains(&run), "{at}: {how}");
                assert!(
                    how.contains(&format!("/main/{path}\"")),
                    "{at}: no link to {path}: {how}"
                );
            }
        }
    }
    let shown = built
        .values()
        .map(|h| h.matches("<div class=\"frame\" id=\"frame-").count())
        .sum::<usize>();
    assert!(frames > 0, "no frame comment in docs/");
    assert_eq!(
        shown, frames,
        "one frame on the site for each frame comment"
    );
}

#[test]
fn a_frame_that_is_not_whole_stops_the_build() {
    let root = repo_root();
    let site = root.join("site");
    let cases = [
        (
            "the block that makes it is missing",
            "<!-- frame id=x code=examples/ownership.rs:1-3 -->\n\n<img src=\"a.webp\" width=\"1\" alt=\"a\">\n\ntext",
        ),
        (
            "the <img> of the frame is missing",
            "<!-- frame id=x code=examples/ownership.rs:1-3 -->\n```\n:hl 3:takes\n```\n\ntext",
        ),
        (
            "its block is not docs/frames/spot-two.tiza as it is",
            "<!-- frame id=x doc=docs/frames/spot-two.tiza step=1 -->\n```json\n{}\n```\n\n<img src=\"a.webp\" width=\"1\" alt=\"a\">",
        ),
        (
            "a doc= frame needs step=N or animate=0|1",
            "<!-- frame id=x doc=docs/frames/spot-two.tiza -->\n```json\n{}\n```\n\n<img src=\"a.webp\" width=\"1\" alt=\"a\">",
        ),
    ];
    for (error, page) in cases {
        let script = format!(
            "import json, sys\n\
             sys.path.insert(0, {site:?})\n\
             import build\n\
             nav = json.load(open({nav:?}))\n\
             for sec in nav['sections']:\n    sec['entries'] = build.section_pages(sec)\n\
             s = build.Site({root:?}, {site:?}, nav, '0', 'main')\n\
             try:\n    s.frame({page:?}.split('\\n'), 0, 'page.md')\n\
             except build.BuildError as e:\n    print(e)\n    sys.exit(0)\n\
             sys.exit('no error')\n",
            site = site.to_str().unwrap(),
            nav = site.join("nav.json").to_str().unwrap(),
            root = root.to_str().unwrap(),
        );
        let out = Command::new("python3")
            .args(["-I", "-B", "-c", &script])
            .output()
            .expect("run python3");
        let said = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success() && said.trim_end() == format!("page.md:1: frame x: {error}"),
            "{error}: {said}{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
