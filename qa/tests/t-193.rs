//! t-193 — A paste turns follow off with a notice.
//!
//! Acceptance criteria:
//! - AC1 A paste while following turns follow off and says so once
//!   (`follow off: the code is pasted, not main.rs; :follow on reloads
//!   main.rs · …`); `follow.status` gives `enabled: false` with the
//!   `reason`, and the bar no longer shows `⟳ following`.
//! - AC2 After the paste, a save in the editor changes nothing on screen and
//!   says nothing about a reload.
//! - AC3 `:follow on` (or `follow.set {enabled: true}`) after the paste loads
//!   the file with the usual reload: re-anchoring, one undo step `reload #N`
//!   and the stale rule for the moves; `u` gives the paste back (and follow
//!   goes off again, saying so: the screen is not the file).
//! - AC4 Undoing the paste does not turn follow back on, and says how
//!   (`undone: paste; follow is off: :follow on reloads main.rs`); the
//!   `reason` only ever describes the code on screen.
//! - `:link` / `:ref` at step 0 over pasted (embedded) code load the file's
//!   code as one undo step, so the screen shows what the saved document will.
//!
//! Corner cases: a paste with follow already off says nothing new; a paste
//! of exactly what the file holds keeps following (nothing to protect); a
//! paste is refused while a move is applied, so a `code.link` view on screen
//! never meets one; a document with moves on line numbers gets the stale
//! rule of the reload when follow comes back.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tiza_core::Document;
use tiza_qa::{Live, fixture, scratch, tiza};

const CODE: &str = "fn main() {\n    let s = String::new();\n    takes(s);\n}\n";
const PASTED: &str = "fn other() {\n    let t = 1;\n    t + 1;\n    t\n}\n";
const NOTICE: &str = "follow off: the code is pasted, not main.rs; :follow on reloads main.rs";

const WAIT: Duration = Duration::from_secs(10);

/// A source file next to its sidecar, opened by reference, following.
fn followed(name: &str, code: &str) -> (Live, PathBuf) {
    let dir = scratch(name);
    let src = std::path::absolute(dir.join("main.rs")).unwrap();
    std::fs::write(&src, code).unwrap();
    let mut doc = Document::from_code(code, "rs", Some(src.display().to_string()));
    doc.reference_file(src.display().to_string(), None);
    let live = Live::open(doc, dir.join("main.rs.tiza"), dir.join("live.sock"));
    live.command("follow on");
    assert_eq!(live.message().unwrap().0, "following main.rs");
    (live, src)
}

fn status(live: &Live) -> Value {
    live.api_ok("follow.status", json!({}))
}

fn code(live: &Live) -> Vec<String> {
    live.with(|app| app.doc.code.clone())
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

fn bar(live: &Live) -> String {
    let screen = live.screen(120, 30);
    screen.lines().last().unwrap_or_default().to_string()
}

fn reloads(live: &Live) -> u64 {
    status(live)["reloads"].as_u64().unwrap_or(0)
}

fn wait_reloads(live: &Live, n: u64) {
    let start = Instant::now();
    while reloads(live) < n {
        assert!(start.elapsed() < WAIT, "timed out waiting for reload #{n}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Quiet time after which "nothing happened" is believed (several times
/// the debounce plus the worst event latency seen on CI), as in t-011.
fn settle() {
    std::thread::sleep(Duration::from_millis(700));
}

fn save(path: &Path, content: &str) {
    let tmp = path.with_extension("rs.tmp");
    std::fs::write(&tmp, content).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

#[test]
fn ac1_ac2_a_paste_turns_follow_off_and_a_save_changes_nothing() {
    let (live, src) = followed("t193-ac1", CODE);
    assert!(bar(&live).contains("⟳ following"), "{}", bar(&live));
    live.with(|app| app.replace_code(PASTED).unwrap());
    let (msg, err) = live.message().unwrap();
    assert!(!err, "{msg}");
    assert_eq!(
        msg,
        format!("{NOTICE} · 5 lines pasted (0 annotations kept)")
    );
    let st = status(&live);
    assert_eq!(st["enabled"], false, "{st}");
    assert_eq!(
        st["reason"],
        "the code is pasted, not main.rs; :follow on reloads main.rs"
    );
    assert_eq!(st["path"], src.display().to_string());
    let b = bar(&live);
    assert!(!b.contains("following") && b.contains(NOTICE), "{b}");
    live.command("follow");
    assert_eq!(
        live.message().unwrap().0,
        "follow off: the code is pasted, not main.rs; :follow on reloads main.rs"
    );

    // A save in the editor: the pasted code stays, nothing is said.
    live.with(|app| app.info("before the save"));
    save(&src, &CODE.replace("takes", "gives"));
    settle();
    assert_eq!(code(&live), lines(PASTED));
    assert_eq!(live.message().unwrap().0, "before the save");
}

#[test]
fn ac3_follow_on_reloads_the_file_and_u_gives_the_paste_back() {
    let (live, src) = followed("t193-ac3", CODE);
    live.command("note 2 the string");
    live.with(|app| app.replace_code(PASTED).unwrap());
    save(&src, &CODE.replace("takes", "gives"));
    live.command("follow on");
    let edited = lines(&CODE.replace("takes", "gives"));
    assert_eq!(code(&live), edited);
    let st = status(&live);
    assert_eq!(st["enabled"], true, "{st}");
    assert!(st.get("reason").is_none(), "{st}");
    assert_eq!(st["reloads"], 1);
    // The note on line 2 was dropped by the paste; the reload is its own
    // undo step even though it changes no annotation.
    assert_eq!(live.with(|app| app.doc.annotations.len()), 0);
    live.with(|app| app.undo());
    assert_eq!(code(&live), lines(PASTED));
    let (msg, _) = live.message().unwrap();
    assert_eq!(
        msg,
        "follow off: the code is the paste again, not main.rs; :follow on reloads main.rs · undone: reload #1"
    );
    assert_eq!(status(&live)["enabled"], false);
    // The saved document holds the paste, not a reference to main.rs.
    live.command("w");
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(src.with_file_name("main.rs.tiza")).unwrap())
            .unwrap();
    assert_eq!(saved["code"][0], "fn other() {", "{saved}");
    // Redo: main.rs again; follow stays as the notice left it.
    live.with(|app| app.redo());
    assert_eq!(code(&live), edited);
    assert_eq!(status(&live)["enabled"], false);
    // Following again from there: further saves reload as usual.
    let r = live.api_ok("follow.set", json!({"enabled": true}));
    assert_eq!(r["enabled"], true, "{r}");
    save(&src, CODE);
    wait_reloads(&live, 1);
    assert_eq!(code(&live), lines(CODE));
}

#[test]
fn ac4_undoing_the_paste_leaves_follow_off() {
    let (live, src) = followed("t193-ac4", CODE);
    live.with(|app| app.replace_code(PASTED).unwrap());
    live.with(|app| app.undo());
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(
        live.message().unwrap().0,
        "undone: paste; follow is off: :follow on reloads main.rs"
    );
    let st = status(&live);
    assert_eq!(st["enabled"], false, "{st}");
    // No paste on screen any more: no reason about one.
    assert!(st.get("reason").is_none(), "{st}");
    live.command("follow");
    assert_eq!(
        live.message().unwrap().0,
        "follow off (:follow on to reload when the file is saved)"
    );
    assert!(!bar(&live).contains("following"));
    save(&src, &CODE.replace("takes", "gives"));
    settle();
    assert_eq!(code(&live), lines(CODE));
    // `:follow on` from there is the usual one.
    live.command("follow on");
    assert_eq!(code(&live), lines(&CODE.replace("takes", "gives")));
}

#[test]
fn corner_a_paste_with_follow_off_says_nothing_new() {
    let (live, _) = followed("t193-off", CODE);
    live.command("follow off");
    live.with(|app| app.replace_code(PASTED).unwrap());
    let (msg, _) = live.message().unwrap();
    assert_eq!(msg, "5 lines pasted (0 annotations kept)");
    assert!(status(&live).get("reason").is_none());
}

#[test]
fn corner_a_paste_of_what_the_file_holds_keeps_following() {
    let (live, src) = followed("t193-same", CODE);
    live.with(|app| app.replace_code(CODE).unwrap());
    let (msg, _) = live.message().unwrap();
    assert!(!msg.contains("follow off"), "{msg}");
    assert_eq!(status(&live)["enabled"], true);
    assert!(!live.with(|app| app.doc.is_embedded()));
    save(&src, &CODE.replace("takes", "gives"));
    wait_reloads(&live, 1);
    assert_eq!(code(&live), lines(&CODE.replace("takes", "gives")));
}

#[test]
fn step4_moves_on_line_numbers_get_the_stale_rule_when_follow_comes_back() {
    let (live, _) = followed("t193-moves", CODE);
    live.api_ok(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "insert", "position": "3",
               "content": "    // before takes"}),
    );
    live.command("step 0");
    // A line on top: the move follows its line to 4.
    let pasted = format!("// header\n{CODE}");
    live.with(move |app| app.replace_code(&pasted).unwrap());
    assert_eq!(status(&live)["enabled"], false);
    let position = |live: &Live| {
        live.with(|app| match &app.doc.moves[0].action {
            tiza_core::moves::MoveAction::CodePatch(p) => p.position.clone(),
            _ => None,
        })
    };
    assert_eq!(position(&live).as_deref(), Some("4"));
    // Back to the file: the header goes, the move follows back to 3 (never
    // applied on another line), and it still matches.
    live.command("follow on");
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(position(&live).as_deref(), Some("3"));
    assert_eq!(live.with(|app| tiza_core::stale::count(&app.doc)), 0);
    live.command("step 1");
    assert_eq!(code(&live)[2], "    // before takes");
}

#[test]
fn rev_redo_of_the_reload_and_two_undos_leave_no_stale_reason() {
    let (live, src) = followed("t193-redo", CODE);
    live.with(|app| app.replace_code(PASTED).unwrap());
    save(&src, &CODE.replace("takes", "gives"));
    live.command("follow on");
    live.with(|app| app.undo());
    assert_eq!(
        status(&live)["reason"],
        "the code is the paste again, not main.rs; :follow on reloads main.rs"
    );
    // `U`: the file again, follow off, and the bar says how to follow.
    live.with(|app| app.redo());
    assert_eq!(code(&live), lines(&CODE.replace("takes", "gives")));
    assert_eq!(
        live.message().unwrap().0,
        "redone: reload #1; follow is off: :follow on reloads main.rs"
    );
    assert!(status(&live).get("reason").is_none());
    // `u` twice: the paste, then the code from before it. main.rs was
    // saved with `gives` since, so that code is not the file's: it stays
    // embedded (a fresh load of the autosave shows the same) and the reason
    // describes it.
    live.with(|app| app.undo());
    live.with(|app| app.undo());
    assert_eq!(code(&live), lines(CODE));
    assert_eq!(
        live.message().unwrap().0,
        "undone: paste; follow is off: the code on screen is not main.rs; :follow on reloads main.rs"
    );
    let st = status(&live);
    assert_eq!(
        st["reason"], "the code on screen is not main.rs; :follow on reloads main.rs",
        "{st}"
    );
    assert_eq!(st["path"], src.display().to_string());
    assert!(live.with(|app| app.doc.is_embedded()));
}

/// The live instance and the autosaved `.tiza`, both rendered as text.
fn live_and_saved(live: &Live, dir: &Path) -> (String, String) {
    live.command("w");
    let sock = dir.join("live.sock");
    let a = tiza(&[
        "render",
        "--socket",
        sock.to_str().unwrap(),
        "--format",
        "txt",
        "--width",
        "80",
    ]);
    let saved = dir.join("main.rs.tiza");
    let b = tiza(&[
        "render",
        saved.to_str().unwrap(),
        "--format",
        "txt",
        "--width",
        "80",
    ]);
    assert!(a.status.success() && b.status.success());
    (
        String::from_utf8_lossy(&a.stdout).into_owned(),
        String::from_utf8_lossy(&b.stdout).into_owned(),
    )
}

#[test]
fn rev_link_or_ref_after_a_paste_shows_the_file_as_the_saved_document_does() {
    let (live, src) = followed("t193-ref", CODE);
    let dir = src.parent().unwrap().to_path_buf();
    let other = dir.join("other.rs");
    std::fs::write(&other, "fn other_file() {}\n").unwrap();
    live.with(|app| app.replace_code(PASTED).unwrap());
    live.command(&format!("ref {}", other.display()));
    let (msg, _) = live.message().unwrap();
    assert!(
        msg.contains("now references") && msg.contains("and shows its code"),
        "{msg}"
    );
    assert_eq!(code(&live), vec!["fn other_file() {}"]);
    let st = status(&live);
    assert_eq!(st["path"], other.display().to_string(), "{st}");
    assert!(st.get("reason").is_none(), "{st}");
    let (a, b) = live_and_saved(&live, &dir);
    assert_eq!(a, b);
    // One undo step: the paste comes back.
    live.with(|app| app.undo());
    assert_eq!(code(&live), lines(PASTED));
    // `:link` (the source file) does the same.
    live.command("link");
    assert_eq!(code(&live), lines(CODE));
    let (a, b) = live_and_saved(&live, &dir);
    assert_eq!(a, b);
}

#[test]
fn rev_an_undo_that_leaves_embedded_code_says_embedded_not_paste() {
    // Code from stdin (embedded), then referenced to a file with the same
    // code, followed; `u` takes back the note and with it the reference.
    let dir = scratch("t193-embedded");
    let src = std::path::absolute(dir.join("main.rs")).unwrap();
    std::fs::write(&src, CODE).unwrap();
    let live = Live::open(
        Document::from_code(CODE, "rs", None),
        dir.join("main.rs.tiza"),
        dir.join("live.sock"),
    );
    live.command("note 2 hello");
    live.command("step 0");
    live.command(&format!("ref {}", src.display()));
    assert_eq!(
        live.message().unwrap().0,
        format!("the .tiza now references {}", src.display())
    );
    live.command("follow on");
    assert_eq!(status(&live)["enabled"], true);
    live.with(|app| app.undo());
    let (msg, _) = live.message().unwrap();
    assert!(
        msg.starts_with("follow off: the code is embedded again, not main.rs; :follow on reloads main.rs · undone: "),
        "{msg}"
    );
    assert_eq!(status(&live)["enabled"], false);
}

#[test]
fn rev_ref_with_moves_applied_is_refused_when_it_would_change_the_code() {
    let dir = scratch("t193-ref-step");
    let moves = "ownership-moves.rs.tiza";
    std::fs::copy(tiza_qa::example(moves), dir.join(moves)).unwrap();
    std::fs::copy(
        fixture("shared/ownership/ownership.rs"),
        dir.join("ownership.rs"),
    )
    .unwrap();
    let path = dir.join("ownership-moves.rs.tiza");
    let doc = tiza_core::fs::load_document(&path).unwrap();
    let live = Live::open(doc, path.clone(), dir.join("live.sock"));
    live.api_ok("step.set", json!({"step": 2}));
    let file = dir.join("ownership.rs");
    live.command(&format!("ref {}", file.display()));
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("while moves are applied (step 2): go back to step 0 first"),
        "{msg}"
    );
    assert!(live.with(|app| app.doc.is_embedded()));
    // The saved file still opens.
    live.command("w");
    tiza_core::fs::load_document(&path).expect("the saved file opens");
    // A file that cannot be read is an error, also on a reference.
    live.api_ok("step.set", json!({"step": 0}));
    live.command(&format!("ref {}", file.display()));
    assert!(!live.with(|app| app.doc.is_embedded()));
    live.command("ref nope.rs");
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains("nope.rs"), "{msg}");
    assert_eq!(
        status(&live)["path"],
        std::path::absolute(&file).unwrap().display().to_string()
    );
}
