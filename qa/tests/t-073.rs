//! t-073 — Round-trip and backward compatibility of `.tiza` (versioned fixtures).
//!
//! Acceptance criteria:
//! - AC1 Every fixture under `qa/fixtures/compat/` (one per producer version,
//!   never edited) opens with the current code, saves, reopens and compares
//!   equal (document round-trip); the saved file is in the current schema.
//! - AC2 `tiza render FIXTURE` matches the snapshot next to the test, so a
//!   layout change on an old document is a visible diff, not a silent change.
//! - AC3 Old Rust-only kinds (`borrow`, `borrow_mut`, `lifetime`) still resolve
//!   through the alias table; old files carry no new fields and get none on save.
//! - AC4 Bumping `SCHEMA_VERSION` without a fixture for the new version and a
//!   migration in `tiza_core::fs::MIGRATIONS` fails this test; a file with
//!   a schema beyond the current one is refused with `unsupported_schema`.

use tiza_core::fs;
use tiza_core::model::SCHEMA_VERSION;
use tiza_qa::{scratch, tiza, tiza_ok};

fn compat_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/compat")
}

fn fixtures() -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(compat_dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("tiza"))
        .collect();
    v.sort();
    assert!(!v.is_empty());
    v
}

/// Schema version a fixture was written in, from its `v<N>-` prefix.
fn fixture_schema(p: &std::path::Path) -> u32 {
    let name = p.file_name().unwrap().to_str().unwrap();
    name.trim_start_matches('v')
        .split('-')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn ac1_round_trip_every_fixture() {
    for f in fixtures() {
        let dir = scratch(&format!(
            "t073-{}",
            f.file_stem().unwrap().to_str().unwrap()
        ));
        let opened =
            fs::open(f.to_str().unwrap()).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let doc = opened.document;
        assert_eq!(
            doc.schema,
            SCHEMA_VERSION,
            "{}: loaded documents are in the current schema",
            f.display()
        );
        let saved = dir.join("saved.tiza");
        fs::save_document(&doc, &saved).unwrap();
        let back = fs::load_document(&saved).unwrap();
        assert_eq!(
            back,
            doc,
            "{}: save → load is not the identity",
            f.display()
        );
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&saved).unwrap()).unwrap();
        assert_eq!(json["schema"], SCHEMA_VERSION);
        // Saving again is byte-stable.
        let again = dir.join("again.tiza");
        fs::save_document(&back, &again).unwrap();
        assert_eq!(
            std::fs::read_to_string(&saved).unwrap(),
            std::fs::read_to_string(&again).unwrap()
        );
    }
}

#[test]
fn ac2_render_matches_the_snapshot() {
    for f in fixtures() {
        let name = f.file_stem().unwrap().to_str().unwrap().to_string();
        let out = tiza_ok(&[
            "render",
            f.to_str().unwrap(),
            "--width",
            "100",
            "--step",
            "9",
            "--out",
            "-",
        ]);
        insta::assert_snapshot!(format!("render-{name}"), out);
    }
}

#[test]
fn ac3_old_kinds_resolve_and_no_new_fields_are_added() {
    let f = compat_dir().join("v1-0.0.1-rust-kinds.tiza");
    let out = tiza_ok(&[
        "render",
        f.to_str().unwrap(),
        "--width",
        "100",
        "--step",
        "9",
        "--out",
        "-",
    ]);
    assert!(out.contains("╌▶ read"), "borrow → read:\n{out}");
    assert!(out.contains("═▶ write"), "borrow_mut → write:\n{out}");
    assert!(out.contains("┆ scope"), "lifetime → scope:\n{out}");
    assert!(out.contains("╳ drop"), "{out}");
    let dir = scratch("t073-fields");
    let doc = fs::open(f.to_str().unwrap()).unwrap().document;
    let saved = dir.join("s.tiza");
    fs::save_document(&doc, &saved).unwrap();
    let text = std::fs::read_to_string(&saved).unwrap();
    for new_field in [
        "\"stroke\"",
        "\"heads\"",
        "\"route\"",
        "\"border\"",
        "\"corners\"",
        "\"fill\"",
        "\"placement\"",
        "\"style\"",
    ] {
        assert!(
            !text.contains(new_field),
            "{new_field} appeared on an old document:\n{text}"
        );
    }
    assert!(
        text.contains("\"kind\": \"borrow\""),
        "kinds are kept as written (aliases resolve at render time)"
    );
}

#[test]
fn ac4_schema_bumps_need_a_fixture_and_a_migration() {
    let versions: std::collections::BTreeSet<u32> =
        fixtures().iter().map(|p| fixture_schema(p)).collect();
    for v in 1..=SCHEMA_VERSION {
        assert!(
            versions.contains(&v),
            "no compatibility fixture for schema {v}: add qa/fixtures/compat/v{v}-<app>-<what>.tiza"
        );
    }
    assert_eq!(
        fs::MIGRATIONS.len() as u32,
        SCHEMA_VERSION - 1,
        "every schema bump needs a migration in tiza_core::fs::MIGRATIONS"
    );
    let mut froms: Vec<u32> = fs::MIGRATIONS.iter().map(|(from, _)| *from).collect();
    froms.dedup();
    assert_eq!(
        froms,
        (1..SCHEMA_VERSION).collect::<Vec<_>>(),
        "migrations must be contiguous"
    );
    // A newer schema than this binary knows is refused, not guessed.
    let dir = scratch("t073-future");
    let future = dir.join("future.tiza");
    std::fs::write(
        &future,
        format!(
            r#"{{"schema":{},"source":{{"language":"rs"}},"code":["a"],"annotations":[]}}"#,
            SCHEMA_VERSION + 1
        ),
    )
    .unwrap();
    let out = tiza(&["render", future.to_str().unwrap(), "--out", "-"]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("schema"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
