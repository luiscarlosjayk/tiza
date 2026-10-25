//! `--all-steps` refuses a file name for `--out`.

const CODE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n}\n";

#[test]
fn all_steps_refuses_a_file_name_for_out() {
    let dir = tiza_qa::scratch("all-steps");
    std::fs::write(dir.join("main.rs"), CODE).unwrap();
    let run = |out: &str, format: Option<&str>| {
        let mut c = std::process::Command::new(tiza_qa::bin());
        c.args(["render", "main.rs", "--all-steps", "--out", out])
            .current_dir(&dir)
            .env("TIZA_CONFIG", dir.join("no-config.toml"));
        if let Some(f) = format {
            c.args(["--format", f]);
        }
        c.output().unwrap()
    };
    let render = |out: &str| run(out, None);
    for (out, format, says) in [
        (
            "answer.md",
            None,
            "is a file name. Pass a folder, for example --out steps/ --format md",
        ),
        ("shot.PNG", None, "--format png"),
        ("demo.gif", None, "already holds every step"),
        // A --format given is the one to suggest.
        ("shots.png", Some("svg"), "--format svg"),
        (
            "talk.mp4",
            Some("svg"),
            "is a file name. Pass a folder, for example --out steps/ --format svg",
        ),
    ] {
        let o = run(out, format);
        let err = String::from_utf8_lossy(&o.stderr);
        assert!(!o.status.success(), "{out}: accepted");
        assert!(err.contains(says), "{out}: {err}");
        assert!(!dir.join(out).exists(), "{out} was made");
    }
    let o = render("steps/");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(dir.join("steps/step-00.png").is_file());
    // A slash at the end asks for a folder, whatever the name.
    let o = render("answer.md/");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(dir.join("answer.md/step-00.png").is_file());
    // A folder with such a name that is there already still works.
    std::fs::create_dir(dir.join("old.md")).unwrap();
    let o = render("old.md");
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}
