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

/// `tiza check --json FILE` run from `cwd`: exit code, errors, warnings
/// and the output.
fn check_from(file: &Path, cwd: &Path) -> (i32, u64, u64, String) {
    let out = std::process::Command::new(bin())
        .args(["check", "--json"])
        .arg(file)
        .current_dir(cwd)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
    let n = |x: &Value| {
        x.as_u64()
            .or(x.as_array().map(|a| a.len() as u64))
            .unwrap_or(u64::MAX)
    };
    (
        out.status.code().unwrap_or(-1),
        n(&v["errors"]),
        n(&v["warnings"]),
        text,
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
    let (rc, errors, warnings, out) = check_from(copy, Path::new("/"));
    assert!(
        rc == 0 && errors == 0 && warnings == 0,
        "check {}: {out}",
        copy.display()
    );
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
    struct Gone(PathBuf);
    impl Drop for Gone {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../target"));
    let far = target.join(format!("t221-abs-{}", std::process::id()));
    std::fs::create_dir_all(&far).unwrap();
    let far = Gone(far.canonicalize().unwrap());
    if !far.0.starts_with("/Users") {
        // Only the root in common with the scratch folders is what this
        // checks; elsewhere there is nothing to check.
        return;
    }
    let copy = far.0.join("far.tiza");
    live.command(&format!("w {}", copy.display()));
    let file = saved(&copy)["code"]["file"].as_str().unwrap().to_string();
    assert!(Path::new(&file).is_absolute(), "{file}");
    assert!(file.ends_with("/proj/main.rs"), "{file}");
    assert_opens(&copy);
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

// ---------- Review ----------

#[test]
fn rev_a_copy_into_a_folder_that_does_not_exist_yet_opens() {
    let (root, tiza) = project("t221-newdir");
    let live = open(&tiza);
    // `:w ../newdir/copy.tiza` typed in `proj`.
    let copy = root.join("proj/../newdir/copy.tiza");
    live.command(&format!("w {}", copy.display()));
    assert!(!message(&live).1, "{:?}", message(&live));
    assert_eq!(saved(&copy)["code"]["file"], "../proj/main.rs");
    assert_opens(&copy);
    let copy = root.join("proj/new/deeper/c.tiza");
    live.command(&format!("wq {}", copy.display()));
    assert_eq!(saved(&copy)["code"]["file"], "../../main.rs");
    assert_opens(&copy);
}

#[test]
fn rev_a_link_back_to_the_main_file_stays_the_main_file() {
    let root = scratch("t221-back");
    let proj = root.join("proj");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join("main.rs"), MAIN).unwrap();
    let tiza = proj.join("main.rs.tiza");
    // Pasted code over main.rs, and a link back to it at step 2.
    std::fs::write(
        &tiza,
        json!({
            "schema": 1, "source": {"path": "main.rs", "language": "rs", "first_line": 1},
            "code": ["fn pasted() {}", "fn more() {}"], "follows": {"file": "main.rs"}, "step": 1,
            "moves": [{"at_step": 1, "action": "code.link", "file": "main.rs"}],
            "annotations": [{"id": "a2", "step": 1, "type": "box", "from_line": 1, "to_line": 2, "file": "main.rs", "label": "back"}]
        })
        .to_string(),
    )
    .unwrap();
    let live = open(&tiza);
    let copy = root.join("else/c.tiza");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    live.command(&format!("w {}", copy.display()));
    let doc = tiza_core::fs::load_document(&copy).unwrap();
    assert_eq!(doc.code, vec!["fn pasted() {}", "fn more() {}"]);
    let (rc, errors, warnings, out) = check_from(&copy, Path::new("/"));
    assert!(rc == 0 && errors == 0 && warnings == 0, "{out}");
    // The same after `:saveas`, live.
    live.command(&format!("saveas {}", root.join("else/s.tiza").display()));
    assert_eq!(
        live.with(|a| a.doc.code.clone()),
        vec!["fn pasted() {}", "fn more() {}"]
    );
    let r = live.api_ok("check", json!({}));
    assert_eq!(
        (r["errors"].clone(), r["warnings"].clone()),
        (json!(0), json!(0)),
        "{r}"
    );
}

#[test]
fn rev_a_linked_file_with_the_main_files_name_stays_a_view() {
    let root = scratch("t221-twins");
    for (f, text) in [("server/main.rs", MAIN), ("client/main.rs", OTHER)] {
        let p = root.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    let tiza = root.join("server/main.rs.tiza");
    std::fs::write(
        &tiza,
        json!({
            "schema": 1, "source": {"path": "main.rs", "language": "rs", "first_line": 1},
            "code": {"file": "main.rs"}, "step": 1,
            "moves": [{"at_step": 1, "action": "code.link", "file": "../client/main.rs"}],
            "annotations": [{"id": "c", "step": 1, "type": "box", "from_line": 1, "to_line": 2, "file": "../client/main.rs", "label": "client"}]
        })
        .to_string(),
    )
    .unwrap();
    let live = open(&tiza);
    let copy = root.join("client/copy.tiza");
    live.command(&format!("w {}", copy.display()));
    let v = saved(&copy);
    // `main.rs` would name the main file (a bare name does): the link is
    // written absolute instead, and so are the annotations naming it.
    let link = v["moves"][0]["file"].as_str().unwrap();
    assert!(
        link.starts_with('/') && link.ends_with("/client/main.rs"),
        "{v}"
    );
    assert_eq!(v["annotations"][0]["file"].as_str(), Some(link), "{v}");
    assert_eq!(v["source"]["path"], "../server/main.rs", "{v}");
    let doc = tiza_core::fs::load_document(&copy).unwrap();
    assert_eq!(
        doc.code[0], "fn other() -> u32 {",
        "step 1 shows the client"
    );
    let (rc, errors, warnings, out) = check_from(&copy, Path::new("/"));
    assert!(rc == 0 && errors == 0 && warnings == 0, "{out}");
}

#[test]
fn rev_link_alone_after_saveas_links_the_same_file() {
    let (root, tiza) = project("t221-link");
    let live = open(&tiza);
    live.command("step 0");
    live.command("embed");
    let target = root.join("else/s.tiza");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    // A file of the same name next to the new place must not be taken.
    std::fs::write(root.join("else/main.rs"), "fn unrelated() {}\n").unwrap();
    live.command(&format!("saveas {}", target.display()));
    live.command("link");
    assert!(!message(&live).1, "{:?}", message(&live));
    assert_eq!(
        live.with(|a| a.doc.base().to_vec()),
        MAIN.lines().map(str::to_string).collect::<Vec<_>>()
    );
    assert_eq!(saved(&target)["code"]["file"], "../proj/main.rs");
}

const INI: &str = "[server]\nhost = localhost\nport = 8080\n[db]\nuser = admin\n";

#[test]
fn rev_wq_path_over_stale_moves_refuses_before_writing() {
    let root = scratch("t221-stale");
    std::fs::write(root.join("config.ini"), INI).unwrap();
    let tiza = root.join("config.ini.tiza");
    std::fs::write(
        &tiza,
        json!({
            "schema": 1, "source": {"language": "ini"}, "code": {"file": "config.ini"}, "step": 0,
            "moves": [{"at_step": 1, "action": "code.patch", "op": "replace", "position": "2", "content": "host = a"}]
        })
        .to_string(),
    )
    .unwrap();
    let live = open(&tiza);
    live.with(|a| a.replace_code(&INI.replace("localhost", "h")).unwrap());
    let copy = root.join("c.tiza");
    live.command(&format!("wq {}", copy.display()));
    let (msg, err) = message(&live);
    assert!(err && msg.contains("stale move"), "{msg}");
    assert!(!live.with(|a| a.quit));
    assert!(!copy.exists(), "no copy before the quit is possible");
    live.command(&format!("wq! {}", copy.display()));
    assert!(live.with(|a| a.quit), "{:?}", message(&live));
    assert!(copy.exists());
}

#[test]
fn rev_saveas_with_autosave_off_says_what_saves() {
    let (root, tiza) = project("t221-off");
    let live = open(&tiza);
    live.command("autosave off");
    let target = root.join("s.tiza");
    live.command(&format!("saveas {}", target.display()));
    let (msg, err) = message(&live);
    assert!(!err && msg.contains(":w saves there"), "{msg}");
    live.command("note 1 later");
    assert!(!std::fs::read_to_string(&target).unwrap().contains("later"));
}

#[test]
fn rev_a_folder_is_not_a_file_name() {
    let (root, tiza) = project("t221-dir");
    let live = open(&tiza);
    let dir = root.join("d");
    std::fs::create_dir_all(&dir).unwrap();
    for target in [dir.display().to_string(), format!("{}/", dir.display())] {
        for cmd in ["w", "w!", "saveas!"] {
            live.command(&format!("{cmd} {target}"));
            let (msg, err) = message(&live);
            assert!(err && msg.contains("is a folder"), "{cmd} {target}: {msg}");
        }
    }
    assert_eq!(live.with(|a| a.save_path.clone()), tiza);
}
