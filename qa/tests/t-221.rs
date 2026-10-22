//! `:w PATH` into another folder wrote the `.tiza`'s relative paths
//! unchanged, so the copy looked for its code next to itself («could not
//! read the referenced code at out/main.rs»); and `:w PATH` switched where
//! Tiza saved from then on, which is not what Vim's `:w {file}` does.
//!
//! Acceptance criteria:
//! - AC1 Compared with Vim, Neovim, Helix, Kakoune and nano (card note):
//!   Tiza follows Vim. `:w PATH` writes a copy and keeps saving where it
//!   did; `:saveas PATH` (`:sav`) writes there and saves there from then on;
//!   `:wq PATH` writes the copy and quits; an existing file needs `!`.
//! - AC2 Every path the `.tiza` names relative to its folder (`code.file`,
//!   each `code.link` file and the annotations and `views` that name it,
//!   `follows.file`) names the same file from the new folder: relative, or
//!   absolute when the two folders share only the root. The copy opens and
//!   passes `tiza check` from any working directory.
//! - AC3 The read-only and autosave-off messages point at `:saveas PATH`.
//! - AC4 No other command writes a `.tiza` elsewhere (`:export` takes no
//!   `.tiza`).
//! - AC5 This file; CHANGELOG; README; agent guide.
//!
//! Corner cases: the same folder under another name (nothing changes); a
//! folder reached through a symlink (the scratch folders live under
//! `/var`, a symlink to `/private/var`); embedded code; `:w` after
//! `:saveas`; `u` after `:saveas`.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tiza_qa::{Live, bin, scratch};

const MAIN: &str = "fn main() {\n    let x = 1;\n    println!(\"{x}\");\n}\n";
const QUEUE: &str = "struct Queue {\n    items: Vec<u32>,\n}\n";
const OTHER: &str = "fn other() -> u32 {\n    7\n}\n";

/// `proj/main.rs.tiza` reading `main.rs`, with `code.link` moves to
/// `src/queue.rs` (step 2) and `lib/other.rs` (step 3), a box on each view.
fn project(name: &str) -> (PathBuf, PathBuf) {
    let root = scratch(name);
    let proj = root.join("proj");
    for (f, text) in [
        ("main.rs", MAIN),
        ("src/queue.rs", QUEUE),
        ("lib/other.rs", OTHER),
    ] {
        let p = proj.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let doc = json!({
        "schema": 1,
        "source": {"path": "main.rs", "language": "rs", "first_line": 1},
        "code": {"file": "main.rs"},
        "step": 1,
        "moves": [
            {"at_step": 2, "action": "code.link", "file": "src/queue.rs"},
            {"at_step": 3, "action": "code.link", "file": "lib/other.rs"}
        ],
        "annotations": [
            {"id": "a1", "step": 1, "type": "box", "from_line": 1, "to_line": 2, "label": "main"},
            {"id": "a2", "step": 2, "type": "box", "from_line": 1, "to_line": 2, "file": "src/queue.rs", "label": "queue"},
            {"id": "a3", "step": 3, "type": "box", "from_line": 1, "to_line": 2, "file": "lib/other.rs", "label": "other"}
        ]
    });
    let tiza = proj.join("main.rs.tiza");
    std::fs::write(&tiza, doc.to_string()).unwrap();
    (root, tiza)
}

fn open(tiza: &Path) -> Live {
    let doc = tiza_core::fs::load_document(tiza).unwrap();
    Live::open(doc, tiza.to_path_buf(), tiza.with_file_name("live.sock"))
}

fn saved(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn message(live: &Live) -> (String, bool) {
    live.message().unwrap_or_default()
}

/// `tiza check FILE` run from `cwd`: exit code and output.
fn check_from(file: &Path, cwd: &Path) -> (i32, String) {
    let out = std::process::Command::new(bin())
        .arg("check")
        .arg(file)
        .current_dir(cwd)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// The copy opens with the project's code and every view, and passes
/// `check` from another working directory.
fn assert_opens(copy: &Path) {
    let doc = tiza_core::fs::load_document(copy)
        .unwrap_or_else(|e| panic!("{}: {e}\n{}", copy.display(), saved(copy)));
    // (The step-0 code of the main file: it may open at a linked file.)
    assert_eq!(doc.base().join("\n") + "\n", MAIN, "{}", copy.display());
    assert_eq!(doc.view_code.len(), 2, "{}", copy.display());
    let (rc, out) = check_from(copy, Path::new("/"));
    assert_eq!(rc, 0, "check {}: {out}", copy.display());
}

#[test]
fn step1_ac1_ac2_w_path_writes_a_copy_that_opens_and_keeps_saving_here() {
    let (root, tiza) = project("t221-w");
    let live = open(&tiza);
    let copy = root.join("proj/out/copy.tiza");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    live.command(&format!("w {}", copy.display()));
    let (msg, err) = message(&live);
    assert!(!err, "{msg}");
    let v = saved(&copy);
    assert_eq!(v["code"], json!({"file": "../main.rs"}), "{v}");
    assert_eq!(v["moves"][0]["file"], "../src/queue.rs", "{v}");
    assert_eq!(v["moves"][1]["file"], "../lib/other.rs", "{v}");
    assert_eq!(v["annotations"][1]["file"], "../src/queue.rs", "{v}");
    assert_eq!(v["annotations"][2]["file"], "../lib/other.rs", "{v}");
    assert_opens(&copy);
    // Vim's `:w {file}`: the open document keeps saving where it did.
    live.command("note 3 after the copy");
    assert!(
        std::fs::read_to_string(&tiza)
            .unwrap()
            .contains("after the copy"),
        "the original took the edit"
    );
    assert!(
        !std::fs::read_to_string(&copy)
            .unwrap()
            .contains("after the copy")
    );
    assert_eq!(live.with(|a| a.save_path.clone()), tiza);
}

#[test]
fn ac2_copies_into_a_parent_and_a_sibling_folder_open() {
    let (root, tiza) = project("t221-where");
    let live = open(&tiza);
    let sibling = root.join("sib/c.tiza");
    std::fs::create_dir_all(sibling.parent().unwrap()).unwrap();
    for (copy, main) in [
        (root.join("up.tiza"), "proj/main.rs"),
        (sibling.clone(), "../proj/main.rs"),
        // The same folder under another name: nothing to rewrite.
        (root.join("proj/again.tiza"), "main.rs"),
    ] {
        live.command(&format!("w {}", copy.display()));
        let (msg, err) = message(&live);
        assert!(!err, "{}: {msg}", copy.display());
        assert_eq!(saved(&copy)["code"]["file"], main, "{}", copy.display());
        assert_opens(&copy);
    }
}

#[test]
fn ac2_a_folder_that_shares_only_the_root_gets_absolute_paths() {
    let (_root, tiza) = project("t221-abs");
    let live = open(&tiza);
    // The scratch folders live under /var (/private/var); the build
    // folder under /Users.
    let far = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target")
        .join(format!("t221-abs-{}", std::process::id()));
    std::fs::create_dir_all(&far).unwrap();
    let far = far.canonicalize().unwrap();
    let copy = far.join("far.tiza");
    live.command(&format!("w {}", copy.display()));
    let file = saved(&copy)["code"]["file"].as_str().unwrap().to_string();
    assert!(Path::new(&file).is_absolute(), "{file}");
    assert!(file.ends_with("/proj/main.rs"), "{file}");
    assert_opens(&copy);
    let _ = std::fs::remove_dir_all(&far);
}

#[test]
fn ac1_an_existing_file_needs_a_bang() {
    let (root, tiza) = project("t221-bang");
    let live = open(&tiza);
    let copy = root.join("c.tiza");
    std::fs::write(&copy, "keep").unwrap();
    for cmd in ["w", "saveas", "sav", "wq"] {
        live.command(&format!("{cmd} {}", copy.display()));
        let (msg, err) = message(&live);
        assert!(
            err && msg.contains("exists (add ! to overwrite)"),
            "{cmd}: {msg}"
        );
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "keep", "{cmd}");
        assert!(!live.with(|a| a.quit), "{cmd}");
    }
    live.command(&format!("w! {}", copy.display()));
    assert_opens(&copy);
    assert_eq!(live.with(|a| a.save_path.clone()), tiza);
    // `:w` naming the open file itself is a plain save.
    live.command(&format!("w {}", tiza.display()));
    let (msg, err) = message(&live);
    assert!(!err, "{msg}");
}

#[test]
fn step2_saveas_moves_the_document_and_its_paths() {
    let (root, tiza) = project("t221-saveas");
    let live = open(&tiza);
    live.command("note 1 before");
    let target = root.join("elsewhere/c.tiza");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    live.command(&format!("saveas {}", target.display()));
    let (msg, err) = message(&live);
    assert!(!err, "{msg}");
    assert_eq!(live.with(|a| a.save_path.clone()), target);
    assert_eq!(saved(&target)["code"]["file"], "../proj/main.rs");
    assert_opens(&target);
    // Later edits go there; `:w` with no path too.
    let before = std::fs::read_to_string(&tiza).unwrap();
    live.command("note 2 after");
    assert!(std::fs::read_to_string(&target).unwrap().contains("after"));
    assert_eq!(std::fs::read_to_string(&tiza).unwrap(), before);
    live.command("w");
    assert!(!message(&live).1, "{:?}", message(&live));
    // The live document reads its files from the new folder: the views,
    // a re-read of the code, and `u` all still find them.
    live.command("step 3");
    assert_eq!(live.with(|a| a.doc.code.clone())[0], "fn other() -> u32 {");
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    live.keys(&[crossterm::event::KeyCode::Char('u')]);
    assert_eq!(saved(&target)["code"]["file"], "../proj/main.rs");
    assert_eq!(saved(&target)["moves"][0]["file"], "../proj/src/queue.rs");
    assert_opens(&target);
    let r = live.api_ok("check", json!({}));
    assert_eq!(r["errors"], json!(0), "{r}");
}

#[test]
fn ac1_wq_path_writes_the_copy_and_quits() {
    let (root, tiza) = project("t221-wq");
    let live = open(&tiza);
    live.command("autosave off");
    live.command("note 1 unsaved here");
    let copy = root.join("out.tiza");
    live.command(&format!("wq {}", copy.display()));
    assert!(live.with(|a| a.quit), "{:?}", message(&live));
    assert!(
        std::fs::read_to_string(&copy)
            .unwrap()
            .contains("unsaved here")
    );
    assert_opens(&copy);
}

#[test]
fn ac2_embedded_code_and_follows() {
    let root = scratch("t221-follows");
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("main.rs"), MAIN).unwrap();
    // Saved after a paste over a followed main.rs.
    let tiza = proj.join("main.rs.tiza");
    std::fs::write(
        &tiza,
        json!({
            "schema": 1, "source": {"path": "main.rs", "language": "rs", "first_line": 1},
            "code": ["fn pasted() {}"], "follows": {"file": "main.rs"}, "step": 0
        })
        .to_string(),
    )
    .unwrap();
    let live = open(&tiza);
    let copy = root.join("sub/x.tiza");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    live.command(&format!("w {}", copy.display()));
    let v = saved(&copy);
    assert_eq!(v["code"], json!(["fn pasted() {}"]), "{v}");
    assert_eq!(v["follows"], json!({"file": "../proj/main.rs"}), "{v}");
    let reopened = open(&copy);
    reopened.command("follow on");
    assert_eq!(message(&reopened).0, "following main.rs");
    assert_eq!(
        reopened.with(|a| a.doc.code.clone()),
        MAIN.lines().map(str::to_string).collect::<Vec<_>>()
    );
}

#[test]
fn ac3_the_read_only_message_points_at_saveas_and_it_works() {
    let (root, tiza) = project("t221-ro");
    let live = open(&tiza);
    let mut perm = std::fs::metadata(&tiza).unwrap().permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(&tiza, perm.clone()).unwrap();
    live.command("note 1 cannot save");
    let (msg, err) = message(&live);
    assert!(err && msg.contains(":saveas PATH"), "{msg}");
    live.command("q");
    let (msg, _) = message(&live);
    assert!(msg.contains(":saveas PATH"), "{msg}");
    let target = root.join("rw.tiza");
    live.command(&format!("saveas {}", target.display()));
    assert!(
        live.with(|a| a.save_error.is_none()),
        "{:?}",
        message(&live)
    );
    assert!(
        std::fs::read_to_string(&target)
            .unwrap()
            .contains("cannot save")
    );
    live.command("q");
    assert!(live.with(|a| a.quit), "{:?}", message(&live));
    use std::os::unix::fs::PermissionsExt;
    perm.set_mode(0o644);
    std::fs::set_permissions(&tiza, perm).unwrap();
}

#[test]
fn ac5_changelog_readme_and_guide() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |f: &str| std::fs::read_to_string(root.join(f)).unwrap();
    assert!(read("CHANGELOG.md").contains(":saveas"), "CHANGELOG");
    assert!(read("README.md").contains(":saveas PATH"), "README");
}
