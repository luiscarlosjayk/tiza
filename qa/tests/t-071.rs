//! t-071 — Terminal size matrix with Herdr panes (QA script): the same frame
//! at several real terminal sizes, compared with snapshots.
//!
//! Acceptance criteria:
//! - AC1 `scripts/herdr-matrix.sh` opens Herdr panes at target sizes with
//!   `herdr pane split --ratio` / `herdr pane resize` (80×24, 120×40, 75×50,
//!   60×30 and as large as the base pane allows) and prints each pane's real
//!   size.
//! - AC2 In each pane it runs `tiza examples/ownership.rs.tiza --present
//!   --step 7 --no-save` (plus normal-mode runs with the status bar), waits
//!   for the first frame and saves `herdr pane read --source visible` to
//!   `qa/herdr/<case>-<cols>x<rows>.txt`.
//! - AC3 `--check` diffs the captures against the committed snapshots and
//!   prints the panes that changed; `--update` rewrites them on purpose.
//! - AC4 Closes the panes it created, also on errors; fails with a clear
//!   message if Herdr is not installed or not running.
//! - AC5 Documented in qa/README.md as a QA-round step (not `cargo test`).
//!
//! Herdr 0.9.5 has no `pane wait-output`: the script waits on conditions
//! instead (the instance answering `tiza api status` on its own socket, then
//! two identical `pane read`s). Sizes come from `herdr pane layout` (each
//! pane's `rect.width/height`): every pane is split off at its target size
//! (`--ratio` is the share the parent keeps), and a cell or two of error is
//! fixed with `pane resize` in the direction that GROWS the pane resized —
//! Herdr does not move an edge a pane does not share, so a pane at the right
//! never shrinks with `resize right` (seen in Luis's QA on 2026-10-18, where
//! the first version stayed at 180 columns). These tests drive the whole
//! script with `TIZA_HERDR_FAKE`, which replaces every `herdr` call with a
//! model that has that limitation: panes are real ptys of the modelled size
//! running the real `tiza`, and `pane read` is that instance's live `tiza
//! render`. What only a real Herdr shows (escape sequences through
//! libghostty-vt, its exact rounding) is checked in Luis's QA round with
//! `--update` / `--check`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tiza_qa::{bin, example, scratch};

const CASES: [(&str, &str); 7] = [
    ("vt100", "80x24"),
    ("wide", "120x40"),
    ("zed-half", "75x50"),
    ("narrow", "60x30"),
    ("full", "188x60"),
    ("normal", "120x40"),
    ("normal-narrow", "60x30"),
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

/// Each case's line reports its target size twice: from Herdr's layout and,
/// as a cross-check, from `tiza api status`.
fn assert_sizes(r: &Run) {
    for (case, size) in CASES {
        let line = r
            .stdout
            .lines()
            .find(|l| l.split_whitespace().next() == Some(case))
            .unwrap_or_else(|| panic!("no line for {case}:\n{}\n{}", r.stdout, r.stderr));
        let cols: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(cols[2..4], [size, size], "{case}: {line}\n{}", r.stderr);
    }
}

struct Run {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Runs the script with a hard deadline; output goes to files so a full pipe
/// never stalls it.
fn run(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
    let out = dir.join("stdout");
    let err = dir.join("stderr");
    let mut child = Command::new("bash")
        .arg(script())
        .args(args)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&out).unwrap())
        .stderr(std::fs::File::create(&err).unwrap())
        .spawn()
        .expect("bash");
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
        stdout: std::fs::read_to_string(&out).unwrap_or_default(),
        stderr: std::fs::read_to_string(&err).unwrap_or_default(),
    };
    assert!(
        status.is_some(),
        "herdr-matrix.sh {args:?} timed out\n{}\n{}",
        r.stdout,
        r.stderr
    );
    r
}

/// A fresh Herdr model: one base pane `p0` filling a 200×60 window.
fn fake(dir: &Path) -> PathBuf {
    let f = dir.join("herdr");
    std::fs::create_dir_all(f.join("panes/p0")).unwrap();
    std::fs::write(f.join("window"), "200 60\n").unwrap();
    std::fs::write(f.join("panes/p0/size"), "200 60\n").unwrap();
    f
}

/// Runs the script against `fake` with the test's tiza binary.
fn matrix(dir: &Path, fake: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
    let tiza = bin();
    let snaps = dir.join("snaps");
    let mut all = vec![
        "--tiza",
        tiza.to_str().unwrap(),
        "--snapshots",
        snaps.to_str().unwrap(),
    ];
    all.extend_from_slice(args);
    let mut e = vec![("TIZA_HERDR_FAKE", fake.to_str().unwrap())];
    e.extend_from_slice(env);
    run(dir, &all, &e)
}

/// Every pane the script opened is closed: only the base pane is left, with
/// its whole size back, and no process still runs from the model's panes.
fn assert_cleaned(fake: &Path) {
    let panes: Vec<_> = std::fs::read_dir(fake.join("panes"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(panes, ["p0"], "panes left open");
    assert_eq!(
        std::fs::read_to_string(fake.join("panes/p0/size")).unwrap(),
        "200 60\n",
        "the base pane did not get its size back"
    );
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

fn snapshot_names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| {
            let n = e.unwrap().file_name().into_string().unwrap();
            n.ends_with(".txt").then_some(n)
        })
        .collect();
    v.sort();
    v
}

fn expected_names() -> Vec<String> {
    let mut v: Vec<String> = CASES.iter().map(|(c, s)| format!("{c}-{s}.txt")).collect();
    v.sort();
    v
}

/// A copy of the example (document + source) whose step-1 label is changed.
fn changed_example(dir: &Path) -> PathBuf {
    let d = dir.join("doc");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::copy(example("ownership.rs"), d.join("ownership.rs")).unwrap();
    let doc = std::fs::read_to_string(example("ownership.rs.tiza")).unwrap();
    assert!(doc.contains("s is born"), "the example's label moved");
    let p = d.join("ownership.rs.tiza");
    std::fs::write(&p, doc.replace("s is born", "s is created")).unwrap();
    p
}

/// Test steps 1–3: `--update` opens a pane per case, prints the real sizes,
/// saves `<case>-<cols>x<rows>.txt` and closes every pane; `--check` right
/// after finds no difference; a changed label lists every pane that changed.
#[test]
fn ac1_ac2_ac3_update_check_and_a_changed_label() {
    let dir = scratch("t071-steps");
    let fake = fake(&dir);

    let r = matrix(&dir, &fake, &["--update"], &[]);
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert_sizes(&r);
    assert_eq!(snapshot_names(&dir.join("snaps")), expected_names());
    // Split at the target size: the ratio is what the base keeps (1 − 80/200
    // for vt100, 1 − 24/60 for its rows), so no resize is needed at all.
    let calls = calls(&fake);
    assert!(
        calls
            .iter()
            .any(|c| c.starts_with("pane split p0 --direction right --ratio 0.6000 ")),
        "{calls:#?}"
    );
    assert!(
        calls
            .iter()
            .any(|c| c.contains("--direction down --ratio 0.6000")),
        "{calls:#?}"
    );
    assert!(
        !calls.iter().any(|c| c.starts_with("pane resize")),
        "{calls:#?}"
    );
    let frame = std::fs::read_to_string(dir.join("snaps/vt100-80x24.txt")).unwrap();
    assert!(frame.contains("s is born"), "{frame}");
    // Each case ran the document at step 7, presented or with the status bar.
    let cmds = std::fs::read_to_string(fake.join("log")).unwrap();
    let cmds: Vec<&str> = cmds.lines().collect();
    assert_eq!(cmds.len(), CASES.len(), "{cmds:#?}");
    let presented = cmds.iter().filter(|c| c.contains(" --present ")).count();
    assert_eq!(presented, 5, "{cmds:#?}");
    for c in &cmds {
        assert!(c.contains("ownership.rs.tiza"), "{c}");
        assert!(c.contains("--step 7 --no-save --socket "), "{c}");
    }
    assert_cleaned(&fake);

    let r = matrix(&dir, &fake, &["--check"], &[]);
    assert_eq!(r.code, Some(0), "{}\n{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("no differences"), "{}", r.stdout);
    assert_cleaned(&fake);

    let doc = changed_example(&dir);
    let r = matrix(
        &dir,
        &fake,
        &["--check", "--file", doc.to_str().unwrap()],
        &[],
    );
    assert_eq!(r.code, Some(1), "{}\n{}", r.stdout, r.stderr);
    let changed = r
        .stdout
        .lines()
        .find_map(|l| l.strip_prefix("panes that changed:"))
        .unwrap_or_else(|| panic!("no summary:\n{}", r.stdout));
    let mut changed: Vec<&str> = changed.split_whitespace().collect();
    changed.sort();
    let mut all: Vec<&str> = CASES.iter().map(|(c, _)| *c).collect();
    all.sort();
    assert_eq!(changed, all);
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
    assert_sizes(&r);
    assert_eq!(snapshot_names(&out), expected_names());
    assert!(!dir.join("snaps").exists(), "a plain run wrote snapshots");
    let resizes = resizes(&fake);
    assert!(
        resizes.iter().any(|(p, d)| p == "p0" && d == "right"),
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
    assert_sizes(&r);
    let resizes = resizes(&fake);
    assert!(resizes.iter().any(|(_, d)| d == "left"), "{resizes:#?}");
    assert!(resizes.iter().any(|(_, d)| d == "up"), "{resizes:#?}");
    assert!(
        resizes
            .iter()
            .all(|(p, d)| p != "p0" && (d == "left" || d == "up")),
        "{resizes:#?}"
    );
    assert_cleaned(&fake);
}

/// AC3 corner: a real size that differs from the snapshot's is a change, and
/// a case without a snapshot asks for `--update`.
#[test]
fn ac3_size_change_and_missing_snapshot() {
    let dir = scratch("t071-size");
    let fake = fake(&dir);
    let snaps = dir.join("snaps");
    std::fs::create_dir_all(&snaps).unwrap();
    std::fs::write(snaps.join("vt100-81x24.txt"), "old\n").unwrap();
    let r = matrix(&dir, &fake, &["--check"], &[]);
    assert_eq!(r.code, Some(1), "{}\n{}", r.stdout, r.stderr);
    assert!(
        r.stdout
            .contains("CHANGED vt100: size 81x24.txt → 80x24.txt"),
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

/// AC4: a failure half-way (here `pane read`) still closes every pane the
/// script opened and stops their Tiza instances.
#[test]
fn ac4_failure_closes_the_panes() {
    let dir = scratch("t071-fail");
    let fake = fake(&dir);
    let r = matrix(&dir, &fake, &[], &[("TIZA_HERDR_FAKE_FAIL_READ", "1")]);
    assert_eq!(r.code, Some(2), "{}\n{}", r.stdout, r.stderr);
    assert!(r.stderr.contains("herdr pane read"), "{}", r.stderr);
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
        "not part of `cargo test`",
    ] {
        assert!(readme.contains(needle), "qa/README.md lacks {needle}");
    }
    let ignore = std::fs::read_to_string(root.join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|l| l == "qa/herdr/latest/"));
}
