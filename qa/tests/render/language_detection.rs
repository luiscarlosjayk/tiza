//! Language detection by extension, file name, shebang and modeline,
//! with a manual override.
//!
//! Acceptance criteria:
//! - AC1 Opening a file picks the language from the extension when a grammar
//!   exists for it; otherwise from a well-known file name (`Makefile`,
//!   `Dockerfile`, `Cargo.lock`, `.bashrc`…), then a `#!` line, then a Vim or
//!   Emacs modeline in the first or last five lines; else plain text.
//!   A `.tiza` that references a file with an empty `source.language`
//!   detects the same way.
//! - AC2 Code from stdin without `--lang` is detected from its content
//!   (shebang/modeline), not forced to plain text. `--lang` always wins and
//!   warns when no grammar exists for it.
//! - AC3 `:lang` shows the language; `:lang <id>` overrides it (`rust`,
//!   `typescript`… are normalized to `rs`, `ts`…) and a `:lang` without
//!   grammar says so instead of failing; `:lang auto` re-detects and says how.
//! - AC4 Pasting into a document that is still plain text re-detects from the
//!   pasted text; a document with an explicit language keeps it.

use tiza_core::fs;
use tiza_qa::{Live, fixture, tiza};

fn lang_of(rel: &str) -> String {
    fs::open(fixture(rel).to_str().unwrap())
        .unwrap()
        .document
        .source
        .language
}

#[test]
fn extension_file_name_shebang_modeline_default() {
    assert_eq!(lang_of("render/language_detection/Makefile"), "makefile");
    assert_eq!(
        lang_of("render/language_detection/script"),
        "py",
        "shebang, no extension"
    );
    assert_eq!(
        lang_of("render/language_detection/deploy.sh"),
        "sh",
        "the extension beats the shebang"
    );
    assert_eq!(
        lang_of("render/language_detection/notes"),
        "toml",
        "vim modeline on the last line"
    );
    assert_eq!(lang_of("render/language_detection/plain"), "txt");
    assert_eq!(lang_of("follow/reload_on_save/repeated.rs"), "rs");
}

#[test]
fn document_referencing_a_file_detects_from_its_content() {
    let opened = fs::open(
        fixture("render/language_detection/script.ref.tiza")
            .to_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(opened.document.source.language, "py");
    assert_eq!(opened.document.annotations.len(), 1);
}

#[test]
fn stdin_detects_from_content_and_lang_wins() {
    let o = fs::from_stdin("#!/bin/bash\necho hi\n", None).unwrap();
    assert_eq!(o.document.source.language, "sh");
    let o = fs::from_stdin("#!/bin/bash\necho hi\n", Some("py")).unwrap();
    assert_eq!(o.document.source.language, "py");
    let o = fs::from_stdin("plain words\n", None).unwrap();
    assert_eq!(o.document.source.language, "txt");
    // The binary: `--lang` without a grammar warns on stderr but still renders.
    let out = tiza(&[
        "render",
        fixture("render/language_detection/plain").to_str().unwrap(),
        "--lang",
        "klingon",
        "--out",
        "-",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no grammar for `klingon`"), "{err}");
    assert!(String::from_utf8_lossy(&out.stdout).contains("just some words"));
}

#[test]
fn lang_command_shows_overrides_normalizes_and_auto_redetects() {
    let live = Live::from_code("#!/usr/bin/env node\nconsole.log(1)\n", "txt", "lang");
    live.command("lang");
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.starts_with("language: txt"), "{msg}");

    live.command("lang typescript");
    assert_eq!(live.with(|a| a.doc.source.language.clone()), "ts");
    let (msg, _) = live.message().unwrap();
    assert_eq!(msg, "language: ts");

    live.command("lang klingon");
    assert_eq!(live.with(|a| a.doc.source.language.clone()), "klingon");
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.contains("no grammar"), "{msg}");

    live.command("lang auto");
    assert_eq!(live.with(|a| a.doc.source.language.clone()), "js");
    let (msg, _) = live.message().unwrap();
    assert_eq!(msg, "language: js (shebang)");

    // The override is saved in the document and survives a reload.
    live.command("lang rust");
    live.command("w");
    let saved = live.with(|a| a.save_path.clone());
    let reopened = fs::open(saved.to_str().unwrap()).unwrap();
    assert_eq!(reopened.document.source.language, "rs");
}

#[test]
fn paste_redetects_only_plain_text_documents() {
    let live = Live::from_code("plain words\n", "txt", "paste");
    live.with(|a| {
        a.replace_code("#!/usr/bin/env python3\nprint(1)\n")
            .unwrap()
    });
    assert_eq!(live.with(|a| a.doc.source.language.clone()), "py");
    // An explicit language sticks even if the paste carries a shebang.
    live.command("lang rs");
    live.with(|a| a.replace_code("#!/bin/sh\necho\n").unwrap());
    assert_eq!(live.with(|a| a.doc.source.language.clone()), "rs");
    // Pasting text without hints into a plain-text document keeps plain text.
    let live = Live::from_code("plain\n", "txt", "paste2");
    live.with(|a| a.replace_code("more plain\n").unwrap());
    assert_eq!(live.with(|a| a.doc.source.language.clone()), "txt");
}

#[test]
fn shebang_variants_and_modeline_positions() {
    use tiza_core::highlight::{Via, detect_language};
    let d = |lines: &[&str]| {
        let v: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
        detect_language(None, &v)
    };
    // env with flags and a version suffix.
    assert_eq!(d(&["#!/usr/bin/env -S python3.12 -u"]).language, "py");
    assert_eq!(d(&["#! /bin/zsh -f"]).language, "sh");
    // Unknown interpreter: not a detection.
    assert_eq!(d(&["#!/usr/bin/env klingon"]).via, Via::Default);
    // A shebang on line 2 is just a comment.
    assert_eq!(d(&["x", "#!/bin/bash"]).via, Via::Default);
    // Emacs modeline with mode and other variables; Vim modeline with `set`.
    assert_eq!(d(&["# -*- coding: utf-8; mode: ruby -*-"]).language, "rb");
    assert_eq!(
        d(&["x", "x", "x", "x", "// vim: set sw=2 ft=typescript:"]).language,
        "ts"
    );
    // Deep in the middle of the file a modeline is ignored (first/last 5 lines only).
    let mut mid = vec!["x"; 20];
    mid[9] = "# vim: ft=python";
    assert_eq!(d(&mid).via, Via::Default);
    // Extension without grammar falls through to the content.
    let v = vec!["#!/usr/bin/env ruby".to_string()];
    assert_eq!(detect_language(Some("tool.bak"), &v).language, "rb");
    // Empty code and no path: plain text, never a panic.
    assert_eq!(detect_language(None, &[]).language, "txt");
}

#[test]
fn dotfiles_and_dockerfile_variants() {
    use tiza_core::highlight::{Via, detect_language};
    for (name, want) in [
        (".bashrc", "sh"),
        (".zshrc", "sh"),
        (".gitignore", "gitignore"),
        ("Dockerfile.prod", "dockerfile"),
        ("Cargo.lock", "toml"),
        ("CMakeLists.txt", "cmake"),
    ] {
        let d = detect_language(Some(&format!("/x/y/{name}")), &[]);
        assert_eq!(
            (d.language.as_str(), d.via),
            (want, Via::FileName),
            "{name}"
        );
    }
}

#[test]
fn lang_given_a_path_detects_from_it() {
    // The Zed selection task passes `--lang "$ZED_FILE"` (the whole path;
    // Zed allows no shell expansion in tasks): `--lang /repo/Makefile`
    // must not become the language id.
    let out = tiza(&[
        "render",
        fixture("render/language_detection/plain").to_str().unwrap(),
        "--lang",
        "/repo/Makefile",
        "--out",
        "-",
    ]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).is_empty());
    // Through stdin the same path falls back to the content (a shebang).
    use std::io::Write;
    use std::process::Stdio;
    let mut child = tiza_qa::command()
        .args(["--lang", "/repo/run", "render", "-", "--out", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"#!/usr/bin/env python3\nprint(1)\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("print(1)"));
}
