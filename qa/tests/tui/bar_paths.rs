//! Bar messages were cut at their end: a long path in `:w`, `:saveas` or
//! `:export` lost its file name, and the long `L:text` not-found error lost
//! its last sentence at 120 columns.
//!
//! Acceptance criteria:
//! - AC1 A path in a bar message is cut in the middle with … and keeps the
//!   last folder and the file name (`/Users/…/nuevo/x.tiza`), at 80 and
//!   120 columns.
//! - AC2 The not-found message shows whole at 120 columns with the usual
//!   left segments.
//! - AC3 Tests at 80 and 120; the CHANGELOG says it.
//!
//! Corner cases: a file name longer than the room is cut at its start and
//! keeps its extension; wide characters in the path.

use tiza_qa::{Live, scratch};
use tiza_tui::ui::fit_message;

fn bar(live: &Live, cols: u16) -> String {
    live.screen(cols, 30).lines().last().unwrap().to_string()
}

/// A folder with a long path, under the scratch folder.
fn deep(name: &str) -> std::path::PathBuf {
    scratch(name).join("a/very/long/folder/name/that/goes/on/nuevo")
}

#[test]
fn a_long_path_keeps_its_last_folder_and_file_name() {
    for cols in [80u16, 120] {
        let live = Live::from_code("fn main() {}\n", "rs", &format!("w-{cols}"));
        let to = deep(&format!("w-{cols}-to")).join("x.tiza");
        live.command(&format!("w {}", to.display()));
        assert!(to.exists(), "{:?}", live.message());
        let b = bar(&live, cols);
        assert!(b.contains("…/nuevo/x.tiza"), "{cols}: {b}");
        assert!(b.contains("wrote a copy to /"), "{cols}: {b}");
    }
}

#[test]
fn saveas_and_export_keep_the_file_name_too() {
    let live = Live::from_code("fn main() {\n    let a = 1;\n}\n", "rs", "saveas");
    live.with(|a| a.autosave = false);
    let to = deep("saveas-to").join("y.tiza");
    live.command(&format!("saveas {}", to.display()));
    let b = bar(&live, 80);
    assert!(b.contains("…/nuevo/y.tiza"), "{b}");
    let shot = deep("export-to").join("shot.txt");
    live.command(&format!("export {}", shot.display()));
    assert!(shot.exists(), "{:?}", live.message());
    // At 120 the last folder and the name; at 80, beside `! small terminal`,
    // the name keeps its end at least.
    let b = bar(&live, 120);
    assert!(b.contains("…/nuevo/shot.txt"), "{b}");
    let b = bar(&live, 80);
    assert!(b.contains("…") && b.contains("shot.txt"), "{b}");
}

#[test]
fn the_not_found_message_is_whole_at_120() {
    let dir = scratch("nowhere");
    let examples = tiza_qa::example("");
    std::fs::copy(
        examples.join("stack-and-queue.tiza"),
        dir.join("stack-and-queue.tiza"),
    )
    .unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    for f in ["stack.rs", "queue.rs"] {
        std::fs::copy(examples.join("src").join(f), dir.join("src").join(f)).unwrap();
    }
    let doc = tiza_core::fs::load_document(&dir.join("stack-and-queue.tiza")).unwrap();
    let live = Live::open(doc, dir.join("stack-and-queue.tiza"), dir.join("s.sock"));
    live.with(|a| a.autosave = false);
    live.command("step 3");
    live.command("hl 22:signature");
    let want =
        "«signature» is not in the code shown (src/queue.rs:10-36). L:text skips notes and labels.";
    assert_eq!(live.message().unwrap().0, want);
    let b = bar(&live, 120);
    assert!(b.ends_with(want), "{b}");
}

#[test]
fn a_file_name_longer_than_the_room_keeps_its_extension() {
    let m = "wrote a copy to /Users/me/notes/a-really-very-long-file-name-for-a-bar.tiza (main.rs.tiza is still the file open)";
    let cut = fit_message(m, 40);
    assert!(cut.starts_with("wrote a copy to …"), "{cut}");
    assert!(cut.ends_with(".tiza"), "{cut}");
    assert!(unicode_cells(&cut) <= 40, "{cut}");
}

#[test]
fn wide_characters_in_the_path() {
    let m = "wrote a copy to /Users/me/文書/プロジェクト/とても長いフォルダ/名前/x.tiza (main.rs.tiza is still the file open)";
    let cut = fit_message(m, 60);
    assert!(cut.contains("…/名前/x.tiza"), "{cut}");
    assert!(unicode_cells(&cut) <= 60, "{cut}");
}

#[test]
fn a_short_path_and_a_message_without_one_are_as_before() {
    let m = "saved to main.rs.tiza";
    assert_eq!(fit_message(m, 80), m);
    let long = "unknown kind «nope»: try :mark warn 3:zz or :style nope glyph=G and more words";
    assert_eq!(fit_message(long, 20), "unknown kind «nope»…");
}

/// Terminal cells: a wide character takes two.
fn unicode_cells(s: &str) -> usize {
    s.chars()
        .map(|c| {
            if (c as u32) >= 0x1100 && !c.is_ascii() && c != '…' && c != '«' && c != '»' {
                2
            } else {
                1
            }
        })
        .sum()
}

#[test]
fn a_shortened_path_cut_again_has_one_ellipsis() {
    let m = "wrote a copy to /Users/me/a/very/long/folder/nuevo/x.tiza (main.rs.tiza is still the file open)";
    let cut = fit_message(m, 33);
    assert!(!cut.contains("……"), "{cut}");
    assert!(cut.ends_with("/nuevo/x.tiza"), "{cut}");
}
