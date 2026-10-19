//! t-071 — Terminal size matrix with Herdr panes (QA script): the same frame
//! at several real terminal sizes, compared with snapshots.
//!
//! Acceptance criteria:
//! - AC1 `scripts/herdr-matrix.sh` opens Herdr panes whose TERMINAL (the size
//!   Tiza sees) is the target, with `herdr pane split --ratio` / `herdr pane
//!   resize` (80×24, 120×40, 75×50, 60×30 and as large as the base pane
//!   allows), and prints each one's size as Tiza sees it and Herdr's rect.
//! - AC2 In each pane it runs `tiza examples/ownership.rs.tiza --present
//!   --step 7 --no-save` (plus normal-mode runs with the status bar), waits
//!   for the first frame and saves `herdr pane read --source visible` to
//!   `qa/herdr/<case>-<cols>x<rows>.txt`, named by the size Tiza sees.
//! - AC3 `--check` diffs the captures against the committed snapshots and
//!   prints the panes that changed; `--update` rewrites them on purpose.
//! - AC4 Works in a tab of its own (`herdr tab create`) that it closes at the
//!   end, also on errors and Ctrl-C; `--base PANE` splits that pane instead
//!   and closes every pane it opened; fails with a clear message if Herdr is
//!   not installed or not running.
//! - AC5 Documented in qa/README.md as a QA-round step (not `cargo test`).
//!
//! t-158 adds the ansi side:
//! - AC1 Each case also saves `<case>-<cols>x<rows>.ansi` from `herdr pane
//!   read --source visible --format ansi`.
//! - AC2 `--check` diffs it too and says whether a change is text or only
//!   style; it compares normalised SGR (text per row and runs of one style),
//!   not bytes, since Herdr may write the same style differently between
//!   versions.
//! - AC3 The fake mode simulates it (`tiza render --format ansi`, CRLF line
//!   ends as Herdr prints them, and `TIZA_HERDR_FAKE_SGR=other` for the same
//!   styles in other bytes) and this file covers it.
//!
//! Herdr's `pane layout` rect counts the pane's frame (Luis's QA on
//! 2026-10-18 17:55: Tiza always saw 3 columns and 2 rows less than the rect,
//! so the b9f34fa version tested 77×22 instead of 80×24). The script measures
//! that frame on a probe pane (rect − `tiza api status`) and splits each pane
//! at target + frame. These tests drive the whole script with
//! `TIZA_HERDR_FAKE`, a model of Herdr 0.9.5: tabs (`tab create`/`tab
//! close`), rects = terminal + frame (3×2 by default, configurable), a pane at
//! the right that never shrinks with `resize right`; panes are real ptys of
//! the terminal size running the real `tiza`, and `pane read` is that
//! instance's live `tiza render`. What only a real Herdr shows (escape
//! sequences through libghostty-vt, its exact rounding, its JSON) is checked
//! in Luis's QA round with `--update` / `--check`.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tiza_qa::{bin, example, scratch};

/// Case and the terminal size Tiza must see (`full` is the 200×60 tab less
/// the 12 columns left to the base and the 3×2 frame).
const CASES: [(&str, (u32, u32)); 7] = [
    ("vt100", (80, 24)),
    ("wide", (120, 40)),
    ("zed-half", (75, 50)),
    ("narrow", (60, 30)),
    ("full", (185, 58)),
    ("normal", (120, 40)),
    ("normal-narrow", (60, 30)),
];

fn script() -> PathBuf {
    root().join("scripts/herdr-matrix.sh")
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Every `herdr` call the script made against the model, in order.
fn calls(fake: &Path) -> Vec<String> {
    std::fs::read_to_string(fake.join("calls"))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The `pane resize` calls as (pane, direction).
fn resizes(fake: &Path) -> Vec<(String, String)> {
    calls(fake)
        .iter()
        .filter(|c| c.starts_with("pane resize"))
        .map(|c| {
            let w: Vec<&str> = c.split_whitespace().collect();
            let arg = |k: &str| {
                let i = w.iter().position(|x| *x == k).expect(k);
                w[i + 1].to_owned()
            };
            (arg("--pane"), arg("--direction"))
        })
        .collect()
}

fn wxh((c, r): (u32, u32)) -> String {
    format!("{c}x{r}")
}

/// Each case's line: target, the size Tiza sees (the target) and Herdr's
/// rect, which is that plus the frame (`extra` gives a case more, for a pane
/// whose frame differs from the measured one).
fn assert_sizes(r: &Run, frame: (u32, u32), extra: impl Fn(&str) -> (u32, u32)) {
    for (case, (c, rows)) in CASES {
        let line = r
            .stdout
            .lines()
            .find(|l| l.split_whitespace().next() == Some(case))
            .unwrap_or_else(|| panic!("no line for {case}:\n{}\n{}", r.stdout, r.stderr));
        let cols: Vec<&str> = line.split_whitespace().collect();
        let (ec, er) = extra(case);
        let herdr = wxh((c + frame.0 + ec, rows + frame.1 + er));
        assert_eq!(
            cols[2..4],
            [wxh((c, rows)), herdr],
            "{case}: {line}\n{}",
            r.stderr
        );
        let file = format!("{case}-{}.txt", wxh((c, rows)));
        assert!(cols[4].ends_with(&file), "{case}: {line}");
    }
}

fn no_extra(_: &str) -> (u32, u32) {
    (0, 0)
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn spawn(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Child {
    Command::new("bash")
        .arg(script())
        .args(args)
        .envs(env.iter().copied())
        .env_remove("HERDR_WORKSPACE_ID")
        .env_remove("HERDR_PANE_ID")
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(dir.join("stdout")).unwrap())
        .stderr(std::fs::File::create(dir.join("stderr")).unwrap())
        .spawn()
        .expect("bash")
}

/// Waits for `child` with a hard deadline; output went to files so a full
/// pipe never stalls it.
fn finish(dir: &Path, mut child: Child, what: &str) -> Run {
    let deadline = Instant::now() + Duration::from_secs(180);
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break Some(s);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::park_timeout(Duration::from_millis(50));
    };
    let r = Run {
        code: status.and_then(|s| s.code()),
        stdout: std::fs::read_to_string(dir.join("stdout")).unwrap_or_default(),
        stderr: std::fs::read_to_string(dir.join("stderr")).unwrap_or_default(),
    };
    assert!(
        status.is_some(),
        "herdr-matrix.sh {what} timed out\n{}\n{}",
        r.stdout,
        r.stderr
    );
    r
}

fn run(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
    let child = spawn(dir, args, env);
    finish(dir, child, &format!("{args:?}"))
}

/// A fresh Herdr model: the user's pane `p0` fills a 200×60 tab `w1:t1`.
fn fake(dir: &Path) -> PathBuf {
    let f = dir.join("herdr");
    std::fs::create_dir_all(f.join("panes/p0")).unwrap();
    std::fs::write(f.join("window"), "200 60\n").unwrap();
    std::fs::write(f.join("panes/p0/size"), "200 60\n").unwrap();
    f
}

fn matrix_args(dir: &Path, tiza: &str, args: &[&str]) -> Vec<String> {
    let snaps = dir.join("snaps");
    let mut all = vec![
        "--tiza".to_owned(),
        tiza.to_owned(),
        "--snapshots".to_owned(),
        snaps.to_str().unwrap().to_owned(),
    ];
    all.extend(args.iter().map(|a| (*a).to_owned()));
    all
}

/// Runs the script against `fake` with the test's tiza binary.
fn matrix(dir: &Path, fake: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
    let tiza = bin();
    let all = matrix_args(dir, tiza.to_str().unwrap(), args);
    let all: Vec<&str> = all.iter().map(String::as_str).collect();
    let mut e = vec![("TIZA_HERDR_FAKE", fake.to_str().unwrap())];
    e.extend_from_slice(env);
    run(dir, &all, &e)
}

/// The script's own tab was created in the current pane's workspace, with
/// the repository as cwd, and closed as the last thing it did.
fn assert_tab_opened_and_closed(fake: &Path) {
    let calls = calls(fake);
    let create = format!(
        "tab create --workspace w1 --cwd {} --label tiza-matrix",
        root().display()
    );
    assert_eq!(
        calls.iter().filter(|c| **c == create).count(),
        1,
        "{calls:#?}"
    );
    assert_eq!(calls.last().map(String::as_str), Some("tab close w1:t2"));
    // The user's pane is never split, resized or run in.
    assert!(
        !calls
            .iter()
            .any(|c| c.starts_with("pane split p0 ") || c.contains("--pane p0 ")),
        "{calls:#?}"
    );
}

/// Every pane and tab the script opened is closed: only the user's pane is
/// left, with its whole size, and no process still runs from the model.
fn assert_cleaned(fake: &Path) {
    let panes: Vec<_> = std::fs::read_dir(fake.join("panes"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(panes, ["p0"], "panes left open");
    assert_eq!(
        std::fs::read_to_string(fake.join("panes/p0/size")).unwrap(),
        "200 60\n",
        "the user's pane did not keep (or get back) its size"
    );
    if let Ok(tabs) = std::fs::read_dir(fake.join("tabs")) {
        let tabs: Vec<_> = tabs.map(|e| e.unwrap().file_name()).collect();
        assert!(tabs.is_empty(), "tabs left open: {tabs:?}");
    }
    let left = Command::new("pgrep")
        .args(["-f", fake.to_str().unwrap()])
        .output()
        .expect("pgrep");
    assert!(
        String::from_utf8_lossy(&left.stdout).trim().is_empty(),
        "processes left running: {}",
        String::from_utf8_lossy(&left.stdout)
    );
}

/// The files in `dir` with extension `ext` (`txt` or `ansi`), sorted.
fn snapshot_names(dir: &Path, ext: &str) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().into_string().unwrap();
            n.ends_with(&format!(".{ext}")).then_some(n)
        })
        .collect();
    v.sort();
    v
}

fn expected_names(ext: &str) -> Vec<String> {
    let mut v: Vec<String> = CASES
        .iter()
        .map(|(c, s)| format!("{c}-{}.{ext}", wxh(*s)))
        .collect();
    v.sort();
    v
}

/// A copy of the example (document + source) in `dir/sub` with `from`
/// replaced by `to` in the document.
fn changed_copy(dir: &Path, sub: &str, from: &str, to: &str) -> PathBuf {
    let d = dir.join(sub);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::copy(example("ownership.rs"), d.join("ownership.rs")).unwrap();
    let doc = std::fs::read_to_string(example("ownership.rs.tiza")).unwrap();
    assert!(doc.contains(from), "the example has no {from}");
    let p = d.join("ownership.rs.tiza");
    std::fs::write(&p, doc.replace(from, to)).unwrap();
    p
}

/// A copy of the example whose step-1 label is changed.
fn changed_example(dir: &Path) -> PathBuf {
    changed_copy(dir, "doc", "s is born", "s is created")
}

/// The case names in the `panes that changed:` summary, sorted.
fn changed_cases(r: &Run) -> Vec<String> {
    let line = r
        .stdout
        .lines()
        .find_map(|l| l.strip_prefix("panes that changed:"))
        .unwrap_or_else(|| panic!("no summary:\n{}", r.stdout));
    let mut v: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
    v.sort();
    v
}

fn all_cases() -> Vec<String> {
    let mut v: Vec<String> = CASES.iter().map(|(c, _)| (*c).to_owned()).collect();
    v.sort();
    v
}

/// Test steps 1–3: `--update` works in a new tab, measures Herdr's frame,
/// gives every case a pane whose terminal is the target, saves
/// `<case>-<cols>x<rows>.txt` with that size and closes the tab; `--check`
/// right after finds no difference; a changed label lists every pane.
#[test]
fn ac1_ac2_ac3_update_check_and_a_changed_label() {
    let dir = scratch("t071-steps");
    let fake = fake(&dir);

    let r = matrix(&dir, &fake, &["--update"], &[]);
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("herdr frame: 3x2 cells"), "{}", r.stdout);
    assert_sizes(&r, (3, 2), no_extra);
    assert_eq!(
        snapshot_names(&dir.join("snaps"), "txt"),
        expected_names("txt")
    );
    // t-158 AC1: the ansi capture next to each text one, CRLF as Herdr's.
    assert_eq!(
        snapshot_names(&dir.join("snaps"), "ansi"),
        expected_names("ansi")
    );
    let ansi = std::fs::read_to_string(dir.join("snaps/vt100-80x24.ansi")).unwrap();
    assert!(ansi.contains("\x1b[") && ansi.contains("\r\n"), "{ansi:?}");
    assert!(ansi.contains("s is born"), "{ansi:?}");
    // The probe pane, then each case split off the tab's root pane p1 at
    // target + frame: 1 − 83/200 for vt100's columns, 1 − 26/60 for its rows,
    // so no resize is needed at all.
    let all_calls = calls(&fake);
    for split in [
        "pane split p1 --direction right --ratio 0.5 ",
        "pane split p1 --direction right --ratio 0.5850 ",
    ] {
        assert!(
            all_calls.iter().any(|c| c.starts_with(split)),
            "{all_calls:#?}"
        );
    }
    assert!(
        all_calls
            .iter()
            .any(|c| c.contains("--direction down --ratio 0.5667")),
        "{all_calls:#?}"
    );
    assert!(
        !all_calls.iter().any(|c| c.starts_with("pane resize")),
        "{all_calls:#?}"
    );
    let frame = std::fs::read_to_string(dir.join("snaps/vt100-80x24.txt")).unwrap();
    assert!(frame.contains("s is born"), "{frame}");
    // The probe, then each case at step 7, presented or with the status bar.
    let cmds = std::fs::read_to_string(fake.join("log")).unwrap();
    let cmds: Vec<&str> = cmds.lines().collect();
    assert_eq!(cmds.len(), CASES.len() + 1, "{cmds:#?}");
    assert!(cmds[0].contains("/frame.sock"), "{}", cmds[0]);
    let cmds = &cmds[1..];
    let presented = cmds.iter().filter(|c| c.contains(" --present ")).count();
    assert_eq!(presented, 5, "{cmds:#?}");
    for c in cmds {
        assert!(c.contains("ownership.rs.tiza"), "{c}");
        assert!(c.contains("--step 7 --no-save --socket "), "{c}");
    }
    assert_tab_opened_and_closed(&fake);
    assert_cleaned(&fake);

    std::fs::remove_file(fake.join("calls")).unwrap();
    let r = matrix(&dir, &fake, &["--check"], &[]);
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("no differences"), "{}", r.stdout);
    assert!(calls(&fake).contains(&"tab close w1:t3".to_owned()));
    assert_cleaned(&fake);

    let doc = changed_example(&dir);
    let r = matrix(
        &dir,
        &fake,
        &["--check", "--file", doc.to_str().unwrap()],
        &[],
    );
    assert_eq!(r.code, Some(1), "{}\n{}", r.stdout, r.stderr);
    assert_eq!(changed_cases(&r), all_cases());
    assert!(
        r.stdout.contains("CHANGED vt100: text differs ("),
        "{}",
        r.stdout
    );
    assert!(!r.stdout.contains("style only"), "{}", r.stdout);
    assert!(
        r.stdout.contains("+    │        · s is created"),
        "{}",
        r.stdout
    );
    // --check never rewrites the snapshots.
    let frame = std::fs::read_to_string(dir.join("snaps/vt100-80x24.txt")).unwrap();
    assert!(frame.contains("s is born"));
    assert_cleaned(&fake);
}

/// AC1: the frame is measured, not assumed — with another frame (1×4) Tiza
/// still sees the targets and Herdr's rects grow by 1×4.
#[test]
fn ac1_the_frame_is_measured() {
    let dir = scratch("t071-frame");
    let fake = fake(&dir);
    let r = matrix(&dir, &fake, &[], &[("TIZA_HERDR_FAKE_FRAME", "1 4")]);
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("herdr frame: 1x4 cells"), "{}", r.stdout);
    // `full` keeps its 188 columns of rect: 187 for Tiza, and 56 rows.
    for (case, (c, rows)) in CASES {
        let want = if case == "full" { (187, 56) } else { (c, rows) };
        let line = r
            .stdout
            .lines()
            .find(|l| l.split_whitespace().next() == Some(case))
            .unwrap();
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(
            cols[2..4],
            [wxh(want), wxh((want.0 + 1, want.1 + 4))],
            "{line}"
        );
    }
    assert_cleaned(&fake);
}

/// AC1: a pane whose frame is not the measured one (here every pane split
/// off downwards has a frame of 3×3) is resized by the difference once Tiza
/// reports its size, so Tiza still sees the target.
#[test]
fn ac1_a_pane_with_another_frame_is_resized() {
    let dir = scratch("t071-frame-below");
    let fake = fake(&dir);
    let r = matrix(&dir, &fake, &[], &[("TIZA_HERDR_FAKE_FRAME_BELOW", "3 3")]);
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("herdr frame: 3x2 cells"), "{}", r.stdout);
    assert_sizes(&r, (3, 2), |case| (0, u32::from(case != "full")));
    assert!(
        r.stderr.contains("resizing the pane by the difference"),
        "{}",
        r.stderr
    );
    assert!(
        resizes(&fake).iter().all(|(_, d)| d == "up"),
        "{:#?}",
        resizes(&fake)
    );
    assert_cleaned(&fake);
}

/// AC1: when a split misses by a couple of cells (here the model gives every
/// new pane 2 cells too many), the pane is shrunk by growing its NEIGHBOUR
/// towards it (`resize right|down` on the pane left of / above it), never by
/// `resize right|down` on the pane itself, which Herdr ignores; the plain
/// run (no flag) only writes `--out`.
#[test]
fn ac1_too_big_is_shrunk_by_growing_the_neighbour() {
    let dir = scratch("t071-skew-plus");
    let fake = fake(&dir);
    let out = dir.join("out");
    let r = matrix(
        &dir,
        &fake,
        &["--out", out.to_str().unwrap()],
        &[("TIZA_HERDR_FAKE_SKEW", "2")],
    );
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert_sizes(&r, (3, 2), no_extra);
    assert_eq!(snapshot_names(&out, "txt"), expected_names("txt"));
    assert_eq!(snapshot_names(&out, "ansi"), expected_names("ansi"));
    assert!(!dir.join("snaps").exists(), "a plain run wrote snapshots");
    let resizes = resizes(&fake);
    assert!(
        resizes.iter().any(|(p, d)| p == "p1" && d == "right"),
        "{resizes:#?}"
    );
    assert!(resizes.iter().any(|(_, d)| d == "down"), "{resizes:#?}");
    assert!(
        resizes.iter().all(|(_, d)| d == "right" || d == "down"),
        "{resizes:#?}"
    );
    assert_cleaned(&fake);
}

/// AC1: a split 2 cells short grows the pane itself towards its neighbour
/// (`resize left|up`), and never touches the base pane.
#[test]
fn ac1_too_small_grows_the_pane_itself() {
    let dir = scratch("t071-skew-minus");
    let fake = fake(&dir);
    let out = dir.join("out");
    let r = matrix(
        &dir,
        &fake,
        &["--out", out.to_str().unwrap()],
        &[("TIZA_HERDR_FAKE_SKEW", "-2")],
    );
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert_sizes(&r, (3, 2), no_extra);
    let resizes = resizes(&fake);
    assert!(resizes.iter().any(|(_, d)| d == "left"), "{resizes:#?}");
    assert!(resizes.iter().any(|(_, d)| d == "up"), "{resizes:#?}");
    assert!(
        resizes
            .iter()
            .all(|(p, d)| p != "p1" && (d == "left" || d == "up")),
        "{resizes:#?}"
    );
    assert_cleaned(&fake);
}

/// AC4: `--base PANE` opens no tab and splits that pane; it gets its whole
/// size back at the end.
#[test]
fn ac4_base_splits_the_given_pane() {
    let dir = scratch("t071-base");
    let fake = fake(&dir);
    let out = dir.join("out");
    let r = matrix(
        &dir,
        &fake,
        &["--base", "p0", "--out", out.to_str().unwrap()],
        &[],
    );
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert_sizes(&r, (3, 2), no_extra);
    let calls = calls(&fake);
    assert!(!calls.iter().any(|c| c.starts_with("tab ")), "{calls:#?}");
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("pane split p0 --direction right --ratio 0.5850 ")),
        "{calls:#?}"
    );
    assert_cleaned(&fake);
}

/// AC4: a `tab create` whose answer names neither the tab nor its root pane
/// (Herdr's exact JSON is not known here): the root pane is found in `pane
/// list`, the tab id in its `pane layout`, and the tab is still closed.
#[test]
fn ac4_tab_found_without_its_answer() {
    let dir = scratch("t071-tab-quiet");
    let fake = fake(&dir);
    let r = matrix(&dir, &fake, &[], &[("TIZA_HERDR_FAKE_TAB_OUTPUT", "none")]);
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert!(
        r.stdout.contains("new tab w1:t2 (root pane p1)"),
        "{}",
        r.stdout
    );
    assert_sizes(&r, (3, 2), no_extra);
    assert_tab_opened_and_closed(&fake);
    assert_cleaned(&fake);
}

/// AC3 corner: a size that differs from the snapshot's is a change, and a
/// case without a snapshot asks for `--update`.
#[test]
fn ac3_size_change_and_missing_snapshot() {
    let dir = scratch("t071-size");
    let fake = fake(&dir);
    let snaps = dir.join("snaps");
    std::fs::create_dir_all(&snaps).unwrap();
    std::fs::write(snaps.join("vt100-77x22.txt"), "old\n").unwrap();
    let r = matrix(&dir, &fake, &["--check"], &[]);
    assert_eq!(r.code, Some(1), "{}\n{}", r.stdout, r.stderr);
    assert!(
        r.stdout
            .contains("CHANGED vt100: size 77x22.txt → 80x24.txt"),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("CHANGED wide: no snapshot in"),
        "{}",
        r.stdout
    );
    assert_cleaned(&fake);
}

/// AC4: a failure half-way (here `pane read`) still closes the tab, or with
/// `--base` every pane the script opened, and stops their Tiza instances.
#[test]
fn ac4_failure_closes_the_tab_and_the_panes() {
    let dir = scratch("t071-fail");
    let fake = fake(&dir);
    let fail = [("TIZA_HERDR_FAKE_FAIL_READ", "1")];
    let r = matrix(&dir, &fake, &[], &fail);
    assert_eq!(r.code, Some(2), "{}\n{}", r.stdout, r.stderr);
    assert!(r.stderr.contains("herdr pane read"), "{}", r.stderr);
    assert_tab_opened_and_closed(&fake);
    assert_cleaned(&fake);

    let r = matrix(&dir, &fake, &["--base", "p0"], &fail);
    assert_eq!(r.code, Some(2), "{}\n{}", r.stdout, r.stderr);
    assert_cleaned(&fake);
}

/// AC4: Ctrl-C half-way (SIGINT while a case's Tiza runs) closes the tab.
#[test]
fn ac4_ctrl_c_closes_the_tab() {
    let dir = scratch("t071-int");
    let fake = fake(&dir);
    let tiza = bin();
    let args = matrix_args(&dir, tiza.to_str().unwrap(), &[]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let child = spawn(&dir, &args, &[("TIZA_HERDR_FAKE", fake.to_str().unwrap())]);
    // Wait until the probe and the first case have started Tiza.
    let deadline = Instant::now() + Duration::from_secs(60);
    while std::fs::read_to_string(fake.join("log"))
        .unwrap_or_default()
        .lines()
        .count()
        < 2
    {
        assert!(Instant::now() < deadline, "no case started");
        std::thread::park_timeout(Duration::from_millis(50));
    }
    let ok = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("kill");
    assert!(ok.success());
    let r = finish(&dir, child, "after SIGINT");
    assert_eq!(r.code, Some(130), "{}\n{}", r.stdout, r.stderr);
    assert!(
        calls(&fake).contains(&"tab close w1:t2".to_owned()),
        "{:#?}",
        calls(&fake)
    );
    assert_cleaned(&fake);
}

/// AC4: no `herdr` on PATH, or a Herdr that does not answer, end with a
/// message that says so before anything is opened.
#[test]
fn ac4_herdr_missing_or_not_running() {
    let dir = scratch("t071-missing");
    let path = "/usr/bin:/bin";
    let has_herdr = path.split(':').any(|d| Path::new(d).join("herdr").exists());
    if !has_herdr {
        let r = run(&dir, &[], &[("PATH", path)]);
        assert_eq!(r.code, Some(2), "{}\n{}", r.stdout, r.stderr);
        assert!(r.stderr.contains("herdr is not installed"), "{}", r.stderr);
    }

    let dead = dir.join("dead");
    std::fs::create_dir_all(&dead).unwrap();
    let r = run(&dir, &[], &[("TIZA_HERDR_FAKE", dead.to_str().unwrap())]);
    assert_eq!(r.code, Some(2), "{}\n{}", r.stdout, r.stderr);
    assert!(r.stderr.contains("Herdr is not running"), "{}", r.stderr);

    let r = run(&dir, &["--bogus"], &[]);
    assert_eq!(r.code, Some(2));
    assert!(
        r.stderr.contains("unknown argument: --bogus"),
        "{}",
        r.stderr
    );
}

/// t-158 test steps 1–2: `--update` reads every pane's visible ansi and
/// replaces an `.ansi` of another size; a `--check` whose ansi capture
/// writes the same styles in other SGR bytes finds no difference; a copy
/// with only a colour changed is reported as `style only` in every pane,
/// never as text; other text in the `.ansi` alone is a text change; a
/// missing `.ansi` snapshot asks for `--update`.
#[test]
fn t158_style_only_change_and_other_sgr_bytes() {
    let dir = scratch("t158-ansi");
    let fake = fake(&dir);
    std::fs::create_dir_all(dir.join("snaps")).unwrap();
    std::fs::write(dir.join("snaps/vt100-77x22.ansi"), "old\n").unwrap();
    let r = matrix(&dir, &fake, &["--update"], &[]);
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert_eq!(
        snapshot_names(&dir.join("snaps"), "ansi"),
        expected_names("ansi")
    );
    let reads = calls(&fake)
        .into_iter()
        .filter(|c| c.starts_with("pane read ") && c.ends_with(" --source visible --format ansi"))
        .count();
    assert!(reads >= 2 * CASES.len(), "{:#?}", calls(&fake));

    // Corner case: normalised SGR, not bytes.
    let r = matrix(
        &dir,
        &fake,
        &["--check"],
        &[("TIZA_HERDR_FAKE_SGR", "other")],
    );
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("no differences"), "{}", r.stdout);
    let snap = std::fs::read(dir.join("snaps/vt100-80x24.ansi")).unwrap();
    let other = std::fs::read(dir.join("snaps/latest/vt100-80x24.ansi")).unwrap();
    assert_ne!(snap, other, "the fake wrote the same bytes");
    assert_cleaned(&fake);

    // Step 2: only a colour changes (orange annotations drawn red).
    let doc = changed_copy(&dir, "doc-red", "\"orange\"", "\"red\"");
    let r = matrix(
        &dir,
        &fake,
        &["--check", "--file", doc.to_str().unwrap()],
        &[],
    );
    assert_eq!(r.code, Some(1), "{}\n{}", r.stdout, r.stderr);
    assert_eq!(changed_cases(&r), all_cases());
    for case in all_cases() {
        assert!(
            r.stdout.contains(&format!("CHANGED {case}: style only (")),
            "{case}:\n{}",
            r.stdout
        );
    }
    assert!(!r.stdout.contains("text differs"), "{}", r.stdout);
    // The diff names the runs: the old colour out, the new one in.
    assert!(
        r.stdout
            .lines()
            .any(|l| l.starts_with("-S ") && l.contains("fg=#")),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout
            .lines()
            .any(|l| l.starts_with("+S ") && l.contains("fg=#")),
        "{}",
        r.stdout
    );
    // --check never rewrites the snapshots.
    assert_eq!(
        std::fs::read(dir.join("snaps/vt100-80x24.ansi")).unwrap(),
        snap
    );
    assert_cleaned(&fake);

    // The same .txt but other text in the .ansi is a text change.
    let ansi = String::from_utf8(snap.clone()).unwrap();
    std::fs::write(
        dir.join("snaps/vt100-80x24.ansi"),
        ansi.replace("s is born", "s is torn"),
    )
    .unwrap();
    let r = matrix(&dir, &fake, &["--check"], &[]);
    assert_eq!(r.code, Some(1), "{}\n{}", r.stdout, r.stderr);
    assert!(
        r.stdout
            .contains("CHANGED vt100: text differs in the ansi capture (1 rows)"),
        "{}",
        r.stdout
    );
    assert!(r.stdout.contains("s is torn"), "{}", r.stdout);
    assert_eq!(changed_cases(&r), ["vt100"]);
    assert_cleaned(&fake);

    // Snapshots from before t-158 have no .ansi: that case asks for --update.
    std::fs::remove_file(dir.join("snaps/vt100-80x24.ansi")).unwrap();
    let r = matrix(&dir, &fake, &["--check"], &[]);
    assert_eq!(r.code, Some(1), "{}\n{}", r.stdout, r.stderr);
    assert!(
        r.stdout.contains("CHANGED vt100: no vt100-80x24.ansi in"),
        "{}",
        r.stdout
    );
    assert_eq!(changed_cases(&r), ["vt100"]);
    assert_cleaned(&fake);
}

/// `--normalize FILE` prints what `--check` compares, without Herdr.
fn normalize(dir: &Path, name: &str, bytes: &str) -> String {
    let f = dir.join(name);
    std::fs::write(&f, bytes).unwrap();
    let r = run(dir, &["--normalize", f.to_str().unwrap()], &[]);
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    r.stdout
}

/// t-158 corner case: the same styles in other SGR bytes normalise alike
/// (`0m`/`m`, `;`/`:` colours, combined or separate sequences, palette
/// colours 16-255 as RGB, CRLF); a stray SGR code and a change of colour or
/// attribute do not; other escape sequences are dropped.
#[test]
fn t158_normalised_sgr() {
    let dir = scratch("t158-normalize");
    let a = normalize(
        &dir,
        "a",
        "\x1b[0m\x1b[31mAB\x1b[0m  \n\x1b[1m\x1b[38;2;255;0;0mC\x1b[22mD\x1b[0m\n",
    );
    assert_eq!(
        a,
        "T 1|AB\nT 2|CD\nS 1:1-2 fg=idx1\nS 2:1-1 fg=#ff0000 bold\nS 2:2-2 fg=#ff0000\n"
    );
    let same = [
        "\x1b[31mAB\x1b[m  \r\n\x1b[1;38:2::255:0:0mC\x1b[22mD\x1b[m",
        "\x1b[38;5;1mA\x1b[38:5:1mB\x1b[39m  \r\n\x1b[38;5;196;1mC\x1b[0;38;2;255;0;0mD\x1b]8;;x\x1b\\\x1b[2K\r\n\r\n",
        // Non-SGR escapes and C0 controls are dropped whole (`\e(B` as
        // `tput sgr0` writes it, a private CSI ending in m, a DCS string, an
        // unterminated OSC, a tab); blanks keep only their background.
        "\x1b[31mA\x1b(B\x1b[31mB\x1b[1m \x1b[m\x1b[?25l \x1b[>4;2m\r\n\x1bP1$r0m\x1b\\\x1b[1;38;2;255;0;0mC\x1b[22mD\x1b[m\t\x1b]8;;x",
    ];
    for (i, s) in same.iter().enumerate() {
        let n = normalize(&dir, &format!("same{i}"), s);
        // `\x1b[31m` and `38;5;1` are the same palette colour.
        assert_eq!(n, a, "{s:?}");
    }
    let differ = [
        (
            "stray",
            "\x1b[31mAB\x1b[0m\n\x1b[1;38;2;255;0;0;73mC\x1b[22mD",
        ),
        (
            "colour",
            "\x1b[32mAB\x1b[0m\n\x1b[1;38;2;255;0;0mC\x1b[22mD",
        ),
        (
            "italic",
            "\x1b[31mAB\x1b[0m\n\x1b[3;38;2;255;0;0mC\x1b[23mD",
        ),
    ];
    for (name, s) in differ {
        let n = normalize(&dir, name, s);
        assert_ne!(n, a, "{name}");
        assert_eq!(
            n.lines()
                .filter(|l| l.starts_with("T "))
                .collect::<Vec<_>>(),
            ["T 1|AB", "T 2|CD"],
            "{name}: the text is the same"
        );
    }
    // Underline styles, wide characters (two cells), colon colours with an
    // ITU T.416 colour space id, and what a blank cell keeps.
    let n = normalize(
        &dir,
        "forms",
        "\x1b[31m漢x\x1b[4:3mU\x1b[4:0;38:2:0:255:0:0:9:9mR\x1b[48:2::0:0:255;58:5:3;21mB\x1b[0;7m \x1b[0;1;48;5;196m \x1b[0;1m ",
    );
    assert_eq!(
        n,
        "T 1|漢xURB\nS 1:1-3 fg=idx1\nS 1:4-4 fg=idx1 underline-curly\nS 1:5-5 fg=#ff0000\n\
         S 1:6-6 bg=#0000ff fg=#ff0000 ul=idx3 underline-double\nS 1:7-7 reverse\nS 1:8-8 bg=#ff0000\n"
    );
    let r = run(&dir, &["--normalize", "/nonexistent.ansi"], &[]);
    assert_eq!(r.code, Some(2));
    assert!(r.stderr.contains("file not found"), "{}", r.stderr);
}

/// AC5: qa/README.md documents the script as a QA-round step, and the
/// captures of a plain run stay out of git.
#[test]
fn ac5_documented() {
    let root = root();
    let readme = std::fs::read_to_string(root.join("qa/README.md")).unwrap();
    for needle in [
        "scripts/herdr-matrix.sh",
        "--check",
        "--update",
        "--base",
        "herdr tab create",
        "frame",
        "not part of `cargo test`",
        ".ansi",
        "--format ansi",
        "style only",
        "--normalize",
    ] {
        assert!(readme.contains(needle), "qa/README.md lacks {needle}");
    }
    let ignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|l| l == "qa/herdr/latest/"));
}
