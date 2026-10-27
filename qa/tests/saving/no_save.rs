//! With `--no-save` and changes, the bar said `unsaved (:w)`: it invited
//! `:w`, which writes over the file opened, the one `--no-save` is there to
//! keep as it is (an example annotated without touching it).
//!
//! Acceptance criteria:
//! - AC1 With `--no-save` and changes, the bar says
//!   `unsaved (:saveas PATH · :q! discards)`, or `unsaved (:saveas · :q!)`
//!   when it is narrow, at 80 and 120 columns.
//! - AC2 With `:autosave off` on the user's own file the hint keeps
//!   `(:w)`; read-only keeps its own text.
//! - AC3 `:q` refused with unsaved changes in no-save mode names the same
//!   two ways out.
//! - AC4 The help, the README section «Saving» and the CHANGELOG say it.
//! - AC5 With no changes, the chip on the bar says why nothing saves:
//!   `no-save` with `--no-save` (the file opened is kept as it is),
//!   `autosave off` after `:autosave off`, `read-only` when the file cannot
//!   be written. It fits at 80, 100 and 120 columns next to a warning
//!   badge.
//! - AC6 `:saveas PATH` says `saved as PATH (autosave on)`, or
//!   `(autosave off)`: at 100 columns, next to `! 3 labels cut`, the
//!   message shows whole with a path such as `/Users/…/sub/mine.tiza`.
//!
//! Corner cases: after `:saveas PATH` the document saves there, and the
//! hint is `(:w)` again; a narrow bar still names both commands; a
//! foreign `scene put` leaves changes (`unsaved (:w)`), and after the `:w`
//! that writes it over the file opened the chip is `autosave off`.

use tiza_qa::{Live, scratch};

const CODE: &str = "fn main() {\n    let a = 1;\n}\n";

/// An instance as `tiza FILE --no-save` opens it, with one change.
fn no_save_with_a_change(name: &str) -> Live {
    let live = Live::from_code(CODE, "rs", name);
    live.with(|a| {
        a.autosave = false;
        a.no_save = true;
    });
    live.command("note 2 hi");
    live
}

fn bar(live: &Live, cols: u16) -> String {
    live.screen(cols, 30).lines().last().unwrap().to_string()
}

#[test]
fn the_bar_names_saveas_and_q_bang_at_120_and_80() {
    let live = no_save_with_a_change("bar");
    let b120 = bar(&live, 120);
    assert!(
        b120.contains("unsaved (:saveas PATH · :q! discards)"),
        "{b120}"
    );
    assert!(!b120.contains("(:w)"), "{b120}");
    // At 80 the bar may need the short form: both commands stay.
    let b80 = bar(&live, 80);
    assert!(
        b80.contains("unsaved (:saveas PATH · :q! discards)")
            || b80.contains("unsaved (:saveas · :q!)"),
        "{b80}"
    );
}

#[test]
fn a_narrow_bar_keeps_both_commands() {
    let live = no_save_with_a_change("narrow");
    let b = bar(&live, 50);
    assert!(b.contains("unsaved (:saveas · :q!)"), "{b}");
    assert!(b.chars().count() <= 50, "{b}");
}

#[test]
fn autosave_off_on_your_own_file_keeps_w_and_read_only_its_text() {
    let live = Live::from_code(CODE, "rs", "own");
    live.command("autosave off");
    live.command("note 2 hi");
    let b = bar(&live, 120);
    assert!(b.contains("unsaved (:w)"), "{b}");
    assert!(!b.contains(":saveas"), "{b}");
    // A sidecar that cannot be written keeps its own words.
    live.with(|a| a.save_error = Some("read-only".into()));
    let b = bar(&live, 120);
    assert!(b.contains("read-only • unsaved (:w)"), "{b}");
}

#[test]
fn q_refused_names_saveas_and_q_bang() {
    let live = no_save_with_a_change("quit");
    live.command("q");
    assert!(!live.with(|a| a.quit));
    let (msg, err) = live.message().unwrap();
    assert!(err, "{msg}");
    assert_eq!(
        msg,
        "unsaved changes (--no-save): :saveas PATH saves them elsewhere, :q! discards them"
    );
    live.command("q!");
    assert!(live.with(|a| a.quit));
}

#[test]
fn after_saveas_every_change_saves_to_the_new_file() {
    let live = no_save_with_a_change("saveas");
    let dir = scratch("saveas-to");
    let to = dir.join("mine.tiza");
    live.command(&format!("saveas {}", to.display()));
    assert!(to.exists(), "{:?}", live.message());
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.ends_with("mine.tiza (autosave on)"), "{msg}");
    assert!(live.with(|a| a.autosave && !a.no_save));
    let b = bar(&live, 120);
    assert!(
        !b.contains("unsaved") && !b.contains("no-save"),
        "saved: {b}"
    );
    // A new note reaches the file with no `:w`.
    live.command("note 1 again");
    let saved = std::fs::read_to_string(&to).unwrap();
    assert!(saved.contains("again"), "{saved}");
    let b = bar(&live, 120);
    assert!(!b.contains("unsaved"), "{b}");
}

#[test]
fn w_path_writes_a_copy_and_stays_in_no_save() {
    let live = no_save_with_a_change("w-copy");
    let to = scratch("w-copy-to").join("copy.tiza");
    live.command(&format!("w {}", to.display()));
    assert!(to.exists(), "{:?}", live.message());
    assert!(live.with(|a| !a.autosave && a.no_save));
    let b = bar(&live, 120);
    assert!(b.contains("unsaved (:saveas"), "{b}");
}

#[test]
fn autosave_off_stays_off_after_saveas() {
    // `:autosave off` was the user's choice: `:saveas` keeps it.
    let live = Live::from_code(CODE, "rs", "own-saveas");
    live.command("autosave off");
    live.command("note 2 hi");
    let to = scratch("own-saveas-to").join("mine.tiza");
    live.command(&format!("saveas {}", to.display()));
    assert!(live.with(|a| !a.autosave));
    let (msg, _) = live.message().unwrap();
    assert!(msg.ends_with("mine.tiza (autosave off)"), "{msg}");
    assert!(bar(&live, 120).contains("  autosave off"));
}

#[test]
fn autosave_on_ends_no_save() {
    let live = no_save_with_a_change("autosave-on");
    live.command("autosave on");
    assert!(!live.with(|a| a.no_save));
    assert!(!live.with(|a| a.dirty));
}

#[test]
fn the_help_and_the_changelog_say_it() {
    let help: Vec<String> = tiza_tui::keys::HELP
        .iter()
        .flat_map(|(_, rows)| rows.iter().map(|r| r.key_text()))
        .collect();
    assert!(
        help.iter().any(|t| t
            == "With --no-save, :saveas PATH saves to another file, and every change saves there after it."),
        "{help:?}"
    );
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |f: &str| std::fs::read_to_string(root.join(f)).unwrap();
    assert!(read("CHANGELOG.md").contains("unsaved (:saveas PATH · :q! discards)"));
}

#[test]
fn read_only_then_saveas_saves_every_change_there_already() {
    // A sidecar that cannot be written: autosave stays on, so after a
    // `:saveas` that works every change goes to the new file.
    let dir = scratch("read-only");
    let path = dir.join("doc.tiza");
    let live = Live::open(
        tiza_core::Document::from_code(CODE, "rs", None),
        path.clone(),
        dir.join("s.sock"),
    );
    live.command("note 2 first");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    live.command("note 1 second");
    assert!(live.with(|a| a.save_error.is_some()));
    let to = scratch("read-only-to").join("mine.tiza");
    live.command(&format!("saveas {}", to.display()));
    live.command("note 3 third");
    let saved = std::fs::read_to_string(&to).unwrap();
    assert!(saved.contains("third"), "{saved}");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
}

/// The copy of `examples/ownership.rs.tiza` in `qa/fixtures/tui/bar_warnings/` (labels
/// cut at 100 columns, code of 64 columns), with its code, in a folder of
/// its own, with autosave on: the user's own file.
fn ownership(name: &str) -> Live {
    let dir = scratch(name);
    for f in ["ownership.rs.tiza", "ownership.rs"] {
        std::fs::copy(tiza_qa::fixture("tui/bar_warnings").join(f), dir.join(f)).unwrap();
    }
    let doc = tiza_core::fs::open(dir.join("ownership.rs.tiza").to_str().unwrap())
        .unwrap()
        .document;
    Live::open(doc, dir.join("ownership.rs.tiza"), dir.join("s.sock"))
}

/// A path with a common shape once the bar shortens it,
/// `/Users/…/sub/mine.tiza`: under the target folder, not the system's
/// temporary folder (`/private/var/…` on macOS).
fn usual_path(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("sub").join("mine.tiza")
}

#[test]
fn the_chip_says_why_nothing_saves() {
    // `--no-save`: the file opened is kept as it is.
    let live = Live::from_code(CODE, "rs", "chip-no-save");
    live.with(|a| {
        a.autosave = false;
        a.no_save = true;
    });
    let b = bar(&live, 120);
    assert!(
        b.contains("  no-save") && !b.contains("autosave off"),
        "{b}"
    );
    // `:autosave off`: the user's choice.
    let live = Live::from_code(CODE, "rs", "chip-off");
    live.command("autosave off");
    live.with(|a| a.message = None);
    let b = bar(&live, 120);
    assert!(
        b.contains("  autosave off") && !b.contains("no-save"),
        "{b}"
    );
    // `--no-save` and then `:autosave off`: the file opened is still kept.
    let live = Live::from_code(CODE, "rs", "chip-both");
    live.with(|a| {
        a.autosave = false;
        a.no_save = true;
    });
    live.command("autosave off");
    live.with(|a| a.message = None);
    assert!(bar(&live, 120).contains("  no-save"));
    // A file that cannot be written: as before.
    let live = Live::from_code(CODE, "rs", "chip-read-only");
    live.with(|a| a.save_error = Some("read-only".into()));
    let b = bar(&live, 120);
    assert!(b.contains("  read-only") && !b.contains("no-save"), "{b}");
}

#[test]
fn a_foreign_put_leaves_changes_and_after_w_the_chip_is_autosave_off() {
    let live = ownership("chip-put");
    let other =
        tiza_core::fs::load_document(&tiza_qa::fixture("shared/export/export.tiza")).unwrap();
    live.api_ok("scene.put", serde_json::json!({"document": other}));
    assert!(!live.with(|a| a.autosave));
    live.with(|a| a.message = None);
    let b = bar(&live, 120);
    assert!(b.contains("•  unsaved (:w)"), "{b}");
    // `:w` writes it over the file opened, on purpose: nothing is kept as
    // it was, and autosave stays off until `:autosave on`.
    live.command("w");
    live.with(|a| a.message = None);
    let b = bar(&live, 120);
    assert!(
        b.contains("  autosave off") && !b.contains("no-save"),
        "{b}"
    );
    live.command("autosave on");
    let b = bar(&live, 120);
    assert!(!b.contains("autosave off") && !b.contains("no-save"), "{b}");
}

#[test]
fn the_chip_and_the_badge_fit_at_80_100_and_120_columns() {
    let live = ownership("chip-widths");
    live.command("step 7");
    live.command("autosave off");
    live.with(|a| a.message = None);
    for (cols, badge) in [
        (80, Some("  ! code needs 83 cols")),
        (100, Some("  ! 3 labels cut")),
        (120, None),
    ] {
        let b = bar(&live, cols);
        assert!(b.chars().count() <= cols as usize, "{cols}: {b}");
        assert!(b.contains("  autosave off"), "{cols}: {b}");
        if let Some(badge) = badge {
            assert!(b.contains(badge), "{cols}: {b}");
        }
        assert!(b.contains("? help"), "{cols}: {b}");
    }
}

#[test]
fn with_no_changes_the_chip_gives_way_and_unsaved_stays() {
    use crossterm::event::KeyCode;
    let live = Live::from_code(CODE, "rs", "chip-gives-way");
    live.command("autosave off");
    live.with(|a| a.message = None);
    // A selection: its keys take the room of the chip.
    live.keys(&[KeyCode::Char('v'), KeyCode::Char('l')]);
    let b = bar(&live, 80);
    assert!(!b.contains("autosave off"), "{b}");
    assert!(
        b.ends_with("h highlight · m mark · n note · b box · a arrow"),
        "{b}"
    );
    // The chip is back with the next key.
    live.keys(&[KeyCode::Esc]);
    live.with(|a| a.message = None);
    assert!(bar(&live, 80).contains("  autosave off"));
    // With changes, `unsaved (:w)` stays next to the keys.
    live.command("note 2 hi");
    live.with(|a| a.message = None);
    live.keys(&[KeyCode::Char('v'), KeyCode::Char('l')]);
    let b = bar(&live, 80);
    assert!(b.contains("•  unsaved (:w)"), "{b}");
}

#[test]
fn the_saveas_message_shows_whole_at_100_columns_next_to_the_badge() {
    // `--no-save`: after `:saveas` autosave is on.
    let live = ownership("msg-on");
    live.with(|a| {
        a.autosave = false;
        a.no_save = true;
    });
    live.command("step 7");
    live.command("note 2 hi");
    let to = usual_path("msg-on");
    live.command(&format!("saveas {}", to.display()));
    assert!(to.exists(), "{:?}", live.message());
    let b = bar(&live, 100);
    assert!(b.contains("  ! 3 labels cut"), "{b}");
    assert!(
        b.trim_end().ends_with("/sub/mine.tiza (autosave on)"),
        "{b}"
    );
    assert!(b.contains(" saved as /"), "{b}");
    // `:autosave off`, the user's: after `:saveas` it stays off, and the
    // chip shows too.
    let live = ownership("msg-off");
    live.command("step 7");
    live.command("autosave off");
    let to = usual_path("msg-off");
    live.command(&format!("saveas {}", to.display()));
    assert!(to.exists(), "{:?}", live.message());
    let b = bar(&live, 100);
    assert!(b.contains("  autosave off  ! 3 labels cut"), "{b}");
    assert!(
        b.trim_end().ends_with("/sub/mine.tiza (autosave off)"),
        "{b}"
    );
    assert!(b.contains(" saved as /"), "{b}");
}

#[test]
fn the_changelog_names_the_chips_and_the_message() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |f: &str| std::fs::read_to_string(root.join(f)).unwrap();
    let changelog = read("CHANGELOG.md");
    assert!(changelog.contains("It shows `no-save` with `--no-save`"));
    assert!(changelog.contains("It shows `autosave off` after `:autosave off`"));
    assert!(changelog.contains("`saved as PATH (autosave on)`, or `(autosave off)`"));
    assert!(!changelog.contains("changes now save there"));
}

// A count of one says it in the singular: `1 line pasted (1 annotation
// kept)`, not `1 lines pasted (1 annotations kept)`.

fn paste(live: &Live, text: &str) -> String {
    let text = text.to_string();
    live.with(move |app| app.replace_code(&text)).unwrap();
    live.message().unwrap().0
}

#[test]
fn plural_a_paste_of_one_line_and_of_two_lines() {
    let live = Live::from_code(CODE, "rs", "paste-one");
    live.command("note 2 hi");
    assert_eq!(
        paste(&live, "    let a = 1;\n"),
        "1 line pasted (1 annotation kept)"
    );
    let live = Live::from_code(CODE, "rs", "paste-two");
    live.command("note 2 hi");
    live.command("note 3 bye");
    assert_eq!(
        paste(&live, "    let a = 1;\n}\n"),
        "2 lines pasted (2 annotations kept)"
    );
    let live = Live::from_code(CODE, "rs", "paste-none");
    assert_eq!(
        paste(&live, "fn a() {}\nfn b() {}\n"),
        "2 lines pasted (0 annotations kept)"
    );
    // One annotation dropped, as before.
    let live = Live::from_code(CODE, "rs", "paste-dropped");
    live.command("note 2 hi");
    assert_eq!(
        paste(&live, "fn b() {}\n"),
        "1 line pasted; 1 annotation dropped (their text is gone; u undoes)"
    );
}

#[test]
fn plural_a_range_past_a_file_of_one_line_and_of_two_lines() {
    let dir = scratch("range");
    for (name, code, says) in [
        (
            "one.rs",
            "fn main() {}\n",
            "the file has 1 line: range 3-5 clamped to 1-1",
        ),
        (
            "two.rs",
            "fn a() {}\nfn b() {}\n",
            "the file has 2 lines: range 3-5 clamped to 2-2",
        ),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, code).unwrap();
        let opened = tiza_core::fs::open(&format!("{}:3-5", path.display())).unwrap();
        assert_eq!(opened.notice.as_deref(), Some(says));
    }
}

#[test]
fn plural_a_move_past_the_end_of_one_line_and_of_two_lines() {
    let dir = scratch("stale");
    std::fs::write(dir.join("main.rs"), "a\nb\nc\nd\n").unwrap();
    let doc = serde_json::json!({
        "schema": 1,
        "source": {"language": "rust"},
        "code": {"file": "main.rs"},
        "step": 0,
        "moves": [{"at_step": 1, "action": "code.patch", "op": "delete", "position": "4"}],
        "annotations": [],
    });
    let path = dir.join("main.rs.tiza");
    std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap()).unwrap();
    let d = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(d, path, dir.join("s.sock"));
    live.with(|a| a.autosave = false);
    let reason = |live: &Live| {
        live.api_ok("move.list", serde_json::json!({}))["moves"][0]["stale_reason"].clone()
    };
    paste(&live, "a\n");
    assert_eq!(reason(&live), "line 4 is past the end of the code (1 line)");
    paste(&live, "a\nb\n");
    assert_eq!(
        reason(&live),
        "line 4 is past the end of the code (2 lines)"
    );
}

#[test]
fn plural_a_gif_of_one_frame_and_of_two_frames() {
    let dir = scratch("gif");
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    // Code with no steps: step 0 alone, or nothing but the hold with
    // `--skip-zero`.
    for (extra, says) in [(None, "2 frames, "), (Some("--skip-zero"), "1 frame, ")] {
        let out = dir.join("demo.gif");
        let mut cmd = tiza_qa::command();
        cmd.arg("render")
            .arg(dir.join("main.rs"))
            .arg("--out")
            .arg(&out);
        if let Some(extra) = extra {
            cmd.arg(extra);
        }
        let r = cmd.output().unwrap();
        let err = String::from_utf8_lossy(&r.stderr);
        assert!(r.status.success(), "{err}");
        assert!(err.contains(says), "{err}");
    }
}
