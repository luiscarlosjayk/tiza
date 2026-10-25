//! A line range opens its own `.tiza` from an earlier session.

use serde_json::json;
use tiza_qa::Live;

/// The number of annotations and the revision.
fn state(live: &Live) -> (u64, u64) {
    let s = live.api_ok("status", json!({}));
    (
        s["annotations"].as_u64().unwrap(),
        s["revision"].as_u64().unwrap(),
    )
}

#[test]
fn a_range_opens_its_document_from_an_earlier_session() {
    use tiza_core::fs;
    let dir = tiza_qa::scratch("range");
    let file = dir.join("three.txt");
    std::fs::write(&file, "one\ntwo\nthree\n").unwrap();
    let spec = |r: &str| format!("{}:{r}", file.display());
    // Session 1: a note on lines 2-3, autosaved.
    let o = fs::open(&spec("2-3")).unwrap();
    assert!(!o.existing);
    let save = dir.join("three.txt.2-3.tiza");
    assert_eq!(o.save_path, save);
    {
        let live = Live::open(o.document, o.save_path, dir.join("a.sock"));
        live.api_ok(
            "annotation.add",
            json!({"type": "note", "line": 2, "text": "kept"}),
        );
    }
    // Session 2, the same range and one that clamps to it: the note is there.
    for (n, r) in ["2-3", "2..9"].iter().enumerate() {
        let o = fs::open(&spec(r)).unwrap();
        assert!(o.existing, "{r}");
        assert_eq!(o.save_path, save, "{r}");
        assert!(
            o.notice
                .as_deref()
                .unwrap_or("")
                .contains("opened the existing"),
            "{r}: {:?}",
            o.notice
        );
        let live = Live::open(o.document, o.save_path, dir.join(format!("b{n}.sock")));
        assert_eq!(state(&live).0, 1, "{r}");
    }
    // `tiza render` shows only the lines of a range, without its document.
    let out = std::process::Command::new(tiza_qa::bin())
        .args(["render", &spec("2-3"), "--format", "txt"])
        .output()
        .unwrap();
    let (text, err) = (
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(out.status.success(), "{err}");
    assert!(text.contains("two") && !text.contains("kept"), "{text}");
    assert!(!err.contains("opened the existing"), "{err}");
    // Another range is another document.
    let o = fs::open(&spec("1-2")).unwrap();
    assert!(!o.existing && o.document.annotations.is_empty());
    assert_eq!(o.save_path, dir.join("three.txt.1-2.tiza"));
    // A document there that does not load: an error, and the file stays.
    std::fs::write(&save, "not json").unwrap();
    assert!(fs::open(&spec("2-3")).is_err());
    assert_eq!(std::fs::read_to_string(&save).unwrap(), "not json");
}
