//! t-133 — Size and speed budgets: binary < 16 MB, download < 8 MB,
//! `cargo install` time tracked, a benchmark baseline that warns on
//! regressions, and `lto = "fat"` in the release profile.
//!
//! Acceptance criteria:
//! - AC1 `scripts/size-check.sh` builds `--release`, prints the binary size and
//!   the size of the tar.gz the release workflow packs, and fails when the
//!   binary is > 16 MB, the archive > 8 MB, or either grew > 10 % over this
//!   target's baseline in `qa/budgets.json` — unless that file is updated in
//!   the same commit with a new one-line reason.
//! - AC2 `scripts/bench.sh` times fixed scenarios with the release binary
//!   (median of N runs after a warm-up), writes `qa/bench/latest.json`,
//!   compares with `qa/bench/baseline.json` and warns (never fails) on any
//!   scenario > 20 % slower; the baseline moves only with
//!   `--update-baseline --note`.
//! - AC3 `scripts/bench.sh --install` times a cold `cargo install --path
//!   crates/tiza-cli --root <tmp>`; the baseline keeps it as information.
//! - AC4 README: one line with the download size; AGENTS.md: run size-check
//!   and bench before pushing a dependency or an embedded asset.
//! - AC5 The 2026-10-18 numbers are the first baseline.
//! - AC6 `[profile.release]`: `lto = "fat"`, `codegen-units = 1`,
//!   `strip = true`, the default `panic = "unwind"` (no `panic = "abort"`,
//!   discarded by Luis) and no `opt-level = "s"/"z"`.
//!
//! The scripts are exercised here with fake binaries and temporary budget and
//! baseline files (`--bin`, `--budgets`, `--committed`, `--baseline`,
//! `--latest`), so the test never builds a release binary and never rewrites
//! the committed files. The real numbers are on the card.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use tiza_qa::{PyRun, bin, python, scratch};

const MB: u64 = 1_000_000;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read_json(rel: &str) -> Value {
    let p = root().join(rel);
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn host_target() -> String {
    let out = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .expect("rustc -vV");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .expect("host line")
        .to_string()
}

fn script(name: &str) -> PathBuf {
    root().join("scripts").join(name)
}

fn run(name: &str, args: &[&str]) -> PyRun {
    let r = python(&script(name), args, Duration::from_secs(120));
    assert!(!r.timed_out(), "{name} {args:?} timed out:\n{}", r.stdout);
    r
}

fn ok(r: &PyRun) -> bool {
    r.status.is_some_and(|s| s.success())
}

/// A file of `n` bytes: zeros (compresses to nothing) or xorshift noise (does not compress).
fn fake_binary(dir: &Path, name: &str, n: usize, noise: bool) -> PathBuf {
    let mut v = vec![0u8; n];
    if noise {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for b in v.iter_mut() {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *b = x as u8;
        }
    }
    let p = dir.join(name);
    std::fs::write(&p, v).unwrap();
    p
}

fn write_json(dir: &Path, name: &str, v: &Value) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, serde_json::to_string_pretty(v).unwrap()).unwrap();
    p
}

fn budgets(targets: Value) -> Value {
    json!({"limits": {"binary_bytes": 16 * MB, "archive_bytes": 8 * MB, "growth_pct": 10}, "targets": targets})
}

#[test]
fn ac6_release_profile_is_fat_lto_without_abort() {
    let toml = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
    let section: Vec<&str> = toml
        .split("[profile.release]")
        .nth(1)
        .expect("[profile.release] in Cargo.toml")
        .lines()
        .take_while(|l| !l.trim_start().starts_with('['))
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    assert!(section.contains(&r#"lto = "fat""#), "{section:?}");
    assert!(section.contains(&"codegen-units = 1"), "{section:?}");
    assert!(section.contains(&"strip = true"), "{section:?}");
    // Discarded by Luis (2026-10-18): a panic in a socket or MCP thread must
    // fail that request only, never close the presenter's TUI.
    assert!(
        !section.iter().any(|l| l.starts_with("panic")),
        "{section:?}"
    );
    // opt-level "s"/"z" measured 25 % to 2x slower.
    assert!(
        !section.iter().any(|l| l.starts_with("opt-level")),
        "{section:?}"
    );
}

#[test]
fn ac1_ac5_committed_budgets_hold_the_limits_and_a_baseline_per_target() {
    let b = read_json("qa/budgets.json");
    assert_eq!(b["limits"]["binary_bytes"], 16 * MB);
    assert_eq!(b["limits"]["archive_bytes"], 8 * MB);
    assert_eq!(b["limits"]["growth_pct"], 10);
    let targets = b["targets"].as_object().unwrap();
    for t in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        assert!(targets.contains_key(t), "no baseline for {t}");
    }
    for (t, e) in targets {
        let bin = e["binary_bytes"].as_u64().unwrap();
        let arch = e["archive_bytes"].as_u64().unwrap();
        assert!(bin <= 16 * MB && arch <= 8 * MB, "{t}: {e}");
        let reason = e["reason"].as_str().unwrap_or("");
        assert!(
            !reason.trim().is_empty() && !reason.contains('\n'),
            "{t}: {e}"
        );
        assert!(e["measured"].as_str().unwrap().starts_with("2026-"));
    }
}

#[test]
fn ac1_over_the_absolute_budgets_fails_naming_the_budget() {
    let dir = scratch("t133-abs");
    let b = write_json(&dir, "budgets.json", &budgets(json!({})));
    let b = b.to_str().unwrap();

    // Binary over 16 MB (zeros: the archive stays tiny).
    let big = fake_binary(&dir, "big", 17 * MB as usize, false);
    let r = run(
        "size_check.py",
        &[
            "--bin",
            big.to_str().unwrap(),
            "--budgets",
            b,
            "--committed",
            b,
        ],
    );
    assert!(!ok(&r), "{}", r.stdout);
    assert!(
        r.stdout
            .contains("FAIL: binary 17.00 MB is over the 16 MB budget"),
        "{}",
        r.stdout
    );
    assert!(
        r.stdout.contains("binary   17,000,000 bytes"),
        "{}",
        r.stdout
    );

    // Noise does not compress: a 9 MB binary is a > 8 MB download.
    let noisy = fake_binary(&dir, "noisy", 9 * MB as usize, true);
    let r = run(
        "size_check.py",
        &[
            "--bin",
            noisy.to_str().unwrap(),
            "--budgets",
            b,
            "--committed",
            b,
        ],
    );
    assert!(!ok(&r), "{}", r.stdout);
    assert!(r.stdout.contains("is over the 8 MB budget"), "{}", r.stdout);
    assert!(!r.stdout.contains("over the 16 MB budget"), "{}", r.stdout);

    // Under both: OK line with the budgets.
    let small = fake_binary(&dir, "small", 2 * MB as usize, false);
    let r = run(
        "size_check.py",
        &[
            "--bin",
            small.to_str().unwrap(),
            "--budgets",
            b,
            "--committed",
            b,
        ],
    );
    assert!(ok(&r), "{}\n{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("OK (budget 16 MB / 8 MB"), "{}", r.stdout);
}

#[test]
fn ac1_growth_over_ten_percent_needs_the_baseline_moved_with_a_new_reason() {
    let dir = scratch("t133-growth");
    let target = host_target();
    let base = budgets(json!({ target.clone(): {
        "binary_bytes": MB, "archive_bytes": 4 * MB, "commit": "abc1234",
        "measured": "2026-10-18", "reason": "first baseline"
    }}));
    let committed = write_json(&dir, "committed.json", &base);
    let committed = committed.to_str().unwrap();
    let work = write_json(&dir, "budgets.json", &base);
    let work = work.to_str().unwrap();

    // +5 %: within the margin.
    let fine = fake_binary(&dir, "fine", 1_050_000, false);
    let r = run(
        "size_check.py",
        &[
            "--bin",
            fine.to_str().unwrap(),
            "--budgets",
            work,
            "--committed",
            committed,
        ],
    );
    assert!(ok(&r), "{}\n{}", r.stdout, r.stderr);
    assert!(r.stdout.contains("+5.0 % vs baseline"), "{}", r.stdout);

    // +15 % (a fake `include_bytes!`): fails naming the baseline file.
    let grown = fake_binary(&dir, "grown", 1_150_000, false);
    let grown = grown.to_str().unwrap();
    let r = run(
        "size_check.py",
        &["--bin", grown, "--budgets", work, "--committed", committed],
    );
    assert!(!ok(&r), "{}", r.stdout);
    assert!(
        r.stdout
            .contains("FAIL: binary grew 15.0 % over the baseline in qa/budgets.json"),
        "{}",
        r.stdout
    );

    // --update without a reason is refused and writes nothing.
    let r = run(
        "size_check.py",
        &[
            "--bin",
            grown,
            "--budgets",
            work,
            "--committed",
            committed,
            "--update",
        ],
    );
    assert!(!ok(&r));
    assert!(r.stderr.contains("needs --reason"), "{}", r.stderr);

    // Numbers moved by hand with the committed reason: refused.
    let mut hand = base.clone();
    hand["targets"][&target]["binary_bytes"] = json!(1_150_000);
    write_json(&dir, "budgets.json", &hand);
    let r = run(
        "size_check.py",
        &["--bin", grown, "--budgets", work, "--committed", committed],
    );
    assert!(!ok(&r), "{}", r.stdout);
    assert!(
        r.stdout.contains("changed without a new one-line reason"),
        "{}",
        r.stdout
    );

    // --update with a one-line reason: the baseline moves and the check passes.
    write_json(&dir, "budgets.json", &base);
    let r = run(
        "size_check.py",
        &[
            "--bin",
            grown,
            "--budgets",
            work,
            "--committed",
            committed,
            "--update",
            "--reason",
            "test: a 150 kB asset on purpose",
        ],
    );
    assert!(ok(&r), "{}\n{}", r.stdout, r.stderr);
    let moved: Value = serde_json::from_str(&std::fs::read_to_string(work).unwrap()).unwrap();
    assert_eq!(moved["targets"][&target]["binary_bytes"], 1_150_000);
    assert_eq!(
        moved["targets"][&target]["reason"],
        "test: a 150 kB asset on purpose"
    );
    assert_eq!(moved["limits"], base["limits"]);
}

fn bench_baseline(version_ms: f64) -> Value {
    json!({"warn_pct": 20, "targets": { host_target(): {
        "commit": "abc1234", "date": "2026-10-18", "machine": "test",
        "scenarios": {"version": {"median_ms": version_ms}},
        "note": "test baseline"
    }}})
}

#[test]
fn ac2_bench_warns_on_a_slower_scenario_and_never_fails() {
    let dir = scratch("t133-bench");
    let tiza = bin();
    let latest = dir.join("latest.json");
    let common = |baseline: &Path| {
        run(
            "bench.py",
            &[
                "--bin",
                tiza.to_str().unwrap(),
                "--only",
                "version,tui_first_frame",
                "--runs",
                "1",
                "--baseline",
                baseline.to_str().unwrap(),
                "--latest",
                latest.to_str().unwrap(),
            ],
        )
    };

    // The baseline says `--version` took 1 µs: now it is far slower → WARN, exit 0.
    let fast = write_json(&dir, "fast.json", &bench_baseline(0.001));
    let r = common(&fast);
    assert!(ok(&r), "{}\n{}", r.stdout, r.stderr);
    assert!(
        r.stdout.contains("WARN: more than 20 % slower"),
        "{}",
        r.stdout
    );
    assert!(r.stdout.contains("tui_first_frame"), "{}", r.stdout);
    assert!(r.stdout.contains("new scenario"), "{}", r.stdout);
    let l: Value = serde_json::from_str(&std::fs::read_to_string(&latest).unwrap()).unwrap();
    assert_eq!(l["target"], host_target());
    assert!(l["scenarios"]["version"]["median_ms"].as_f64().unwrap() > 0.0);
    assert!(
        l["scenarios"]["tui_first_frame"]["median_ms"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert!(l["scenarios"].get("render_png_step7").is_none());

    // A generous baseline: no warning.
    let slow = write_json(&dir, "slow.json", &bench_baseline(100_000.0));
    let r = common(&slow);
    assert!(ok(&r), "{}\n{}", r.stdout, r.stderr);
    assert!(!r.stdout.contains("WARN"), "{}", r.stdout);
    assert!(
        r.stdout.contains("no scenario over the warning threshold"),
        "{}",
        r.stdout
    );

    // Moving the baseline needs a note.
    let r = run(
        "bench.py",
        &[
            "--bin",
            tiza.to_str().unwrap(),
            "--only",
            "version",
            "--baseline",
            slow.to_str().unwrap(),
            "--latest",
            latest.to_str().unwrap(),
            "--update-baseline",
        ],
    );
    assert!(!ok(&r));
    assert!(r.stderr.contains("needs --note"), "{}", r.stderr);
}

#[test]
fn ac2_ac3_ac5_committed_bench_baseline_covers_every_scenario() {
    let b = read_json("qa/bench/baseline.json");
    assert_eq!(b["warn_pct"], 20);
    // Same shape for every target, no fixed numbers: a baseline moves with
    // `scripts/bench.sh --update-baseline --note …` and must not break this.
    for t in ["x86_64-unknown-linux-gnu", "aarch64-apple-darwin"] {
        let e = &b["targets"][t];
        for s in [
            "version",
            "render_txt_step7",
            "render_png_step7",
            "render_txt_rust_2000_lines",
            "render_txt_typescript_3000_lines",
            "render_gif_example",
            "tui_first_frame",
            "t054_frame_median",
        ] {
            assert!(
                e["scenarios"][s]["median_ms"].as_f64().unwrap_or(0.0) > 0.0,
                "{t} baseline lacks {s}"
            );
        }
        assert!(
            e["cargo_install_cold_s"].as_f64().unwrap_or(0.0) > 0.0,
            "{t} baseline lacks cargo_install_cold_s"
        );
        assert!(
            !e["note"].as_str().unwrap_or("").trim().is_empty(),
            "{t} baseline has no note"
        );
    }
}

#[test]
fn ac4_docs_name_the_download_size_and_the_scripts() {
    let readme = std::fs::read_to_string(root().join("README.md")).unwrap();
    assert!(
        readme
            .lines()
            .any(|l| l.to_lowercase().contains("download size") && l.contains(" MB")),
        "README has no download-size line"
    );
    let agents = std::fs::read_to_string(root().join("AGENTS.md")).unwrap();
    assert!(
        agents.contains("scripts/size-check.sh") && agents.contains("scripts/bench.sh"),
        "AGENTS.md does not mention the size and bench scripts"
    );
    for s in ["size-check.sh", "bench.sh"] {
        let p = script(s);
        let text = std::fs::read_to_string(&p).unwrap();
        assert!(text.starts_with("#!/usr/bin/env bash"), "{s}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert!(mode & 0o111 != 0, "{s} is not executable");
        }
    }
}
