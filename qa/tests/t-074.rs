//! t-074 — Input fuzzing (`.tiza` parser, `:` commands, socket NDJSON)
//! and the `use-dev-tty` smoke test.
//!
//! Acceptance criteria:
//! - AC1 `fs::parse_document` never panics: random bytes, random JSON, and
//!   structured mutations of a valid document all return `Ok` or `Err`.
//!   Whatever parses also lays out at any width without panicking.
//! - AC2 `App::run_command` never panics on arbitrary command lines (random
//!   bytes, every command name with random arguments, giant numbers, odd
//!   Unicode); after each line the app still answers `status`.
//! - AC3 The socket survives garbage: random lines (invalid JSON, invalid
//!   UTF-8, huge lines, JSON that is not a request, unknown methods, params of
//!   the wrong type) get an error reply or a closed connection, and the server
//!   keeps serving the next client.
//! - AC4 `tiza --smoke` draws one frame, polls the input once and exits 0
//!   with the terminal restored; with stdin piped it reads keys from /dev/tty.
//!   CI runs it under `script` on Linux and macOS (`.github/workflows/ci.yml`).
//!
//! proptest rather than cargo-fuzz: it runs on stable, in `cargo test`, on
//! both CI runners, and keeps regressions in `*.proptest-regressions`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

use proptest::prelude::*;
use serde_json::{Value, json};
use tiza_core::fs;
use tiza_core::layout::{LayoutOptions, layout};
use tiza_core::{Document, Theme};
use tiza_qa::{Live, fixture, scratch};
use tiza_tui::app::App;

fn layouts_without_panic(doc: &Document) {
    let theme = Theme::from_name(tiza_core::ThemeName::Dark);
    for width in [1usize, 20, 61, 100, 300] {
        let l = layout(
            doc,
            &theme,
            &LayoutOptions {
                width,
                step: doc.step,
                focus: None,
                line_numbers: true,
                tab_width: 4,
                typing: false,
                redact: true,
                stable: true,
                margin_cap: None,
            },
        );
        let _ = l.canvas.to_plain_text();
        let _ = l.canvas.to_ansi();
    }
}

/// Arbitrary JSON values, shallow but with every scalar type.
fn json_strategy() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| json!(n)),
        any::<f64>()
            .prop_filter("finite", |f| f.is_finite())
            .prop_map(|f| json!(f)),
        "\\PC{0,12}".prop_map(Value::String),
    ];
    leaf.prop_recursive(3, 24, 6, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
            prop::collection::hash_map("[a-z_]{1,10}", inner, 0..6)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}

/// A valid document with one random field replaced by a random value.
fn mutated_document() -> impl Strategy<Value = String> {
    let base = std::fs::read_to_string(fixture("t-050/export.tiza")).unwrap();
    let base: Value = serde_json::from_str(&base).unwrap();
    let keys = [
        "schema",
        "title",
        "source",
        "code",
        "styles",
        "step",
        "theme",
        "padding",
        "annotations",
        "nope",
    ];
    (0..keys.len(), json_strategy(), any::<u8>()).prop_map(move |(k, v, ann)| {
        let mut d = base.clone();
        if ann % 3 == 0 {
            // Mutate one annotation's field instead of a top-level one.
            if let Some(a) = d["annotations"].as_array_mut()
                && let n = a.len()
                && n > 0
                && let Some(first) = a.get_mut((ann as usize / 3) % n)
                && let Some(obj) = first.as_object_mut()
            {
                let names: Vec<String> = obj.keys().cloned().collect();
                let name = names[k % names.len()].clone();
                obj.insert(name, v);
                return d.to_string();
            }
        }
        d[keys[k]] = v;
        d.to_string()
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]

    #[test]
    fn ac1_parser_never_panics_on_bytes(bytes in prop::collection::vec(any::<u8>(), 0..200)) {
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(d) = fs::parse_document(&text, None) {
            layouts_without_panic(&d);
        }
    }

    #[test]
    fn ac1_parser_never_panics_on_json(v in json_strategy()) {
        if let Ok(d) = fs::parse_document(&v.to_string(), None) {
            layouts_without_panic(&d);
        }
    }

    #[test]
    fn ac1_parser_never_panics_on_mutations(text in mutated_document()) {
        if let Ok(d) = fs::parse_document(&text, None) {
            layouts_without_panic(&d);
            // And what parsed also round-trips.
            let again = serde_json::to_string(&d).unwrap();
            let d2 = fs::parse_document(&again, None).unwrap();
            prop_assert_eq!(d2.annotations.len(), d.annotations.len());
        }
    }
}

/// Commands that touch the file system or the process, excluded from the
/// random walk (they are covered by their own tickets).
const EXCLUDED: &[&str] = &[
    "w", "write", "wq", "q", "quit", "export", "link", "embed", "follow",
];

fn command_strategy() -> impl Strategy<Value = String> {
    let names: Vec<&'static str> = App::COMMANDS
        .iter()
        .copied()
        .filter(|c| !EXCLUDED.contains(c))
        .collect();
    let arg = prop_oneof![
        "[0-9]{1,3}",
        "[0-9]{1,3}-[0-9]{1,3}",
        Just("99999999999999999999".to_string()),
        Just("-1".to_string()),
        Just(".".to_string()),
        "[0-9]{1,2}:[a-z(){};= ]{1,8}",
        "[a-z]{1,8}=[a-z0-9,#]{0,8}",
        Just("a1".to_string()),
        "\\PC{0,10}",
        Just("🔥".to_string()),
        Just("\u{202e}".to_string()),
        Just("\t\t".to_string()),
    ];
    (
        prop::sample::select(names),
        prop::collection::vec(arg, 0..5),
    )
        .prop_map(|(name, args)| format!("{name} {}", args.join(" ")))
}

fn fresh_app(name: &str) -> App {
    let doc = fs::open(fixture("t-050/export.tiza").to_str().unwrap())
        .unwrap()
        .document;
    let dir = scratch(name);
    let mut app = App::new(doc, dir.join("doc.tiza"));
    app.autosave = false;
    app.size = (100, 40);
    app
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn ac2_commands_never_panic(cmds in prop::collection::vec(command_strategy(), 1..6)) {
        let mut app = fresh_app("t074-cmds");
        for c in &cmds {
            app.run_command(c);
            let st = app.status();
            prop_assert!(st.lines >= 1);
            let _ = app.layout(100);
        }
    }

    #[test]
    fn ac2_raw_lines_never_panic(line in "\\PC{0,40}") {
        let first = line.split_whitespace().next().unwrap_or("");
        prop_assume!(!EXCLUDED.contains(&first));
        let mut app = fresh_app("t074-raw");
        app.run_command(&line);
        let _ = app.layout(80);
    }
}

#[test]
fn ac3_socket_survives_garbage() {
    let live = Live::from_code("fn a() {}\nfn b() {}\n", "rs", "t074-sock");
    let garbage: Vec<Vec<u8>> = vec![
        b"".to_vec(),
        b"   ".to_vec(),
        b"not json".to_vec(),
        b"{".to_vec(),
        b"[]".to_vec(),
        b"42".to_vec(),
        b"\"status\"".to_vec(),
        b"{\"id\":1}".to_vec(),
        b"{\"method\":\"status\"}".to_vec(),
        b"{\"id\":1,\"method\":42}".to_vec(),
        b"{\"id\":1,\"method\":\"no.such.method\"}".to_vec(),
        b"{\"id\":1,\"method\":\"annotation.add\",\"params\":[]}".to_vec(),
        b"{\"id\":1,\"method\":\"annotation.add\",\"params\":{\"type\":\"note\",\"line\":\"x\"}}".to_vec(),
        b"{\"id\":1,\"method\":\"annotation.add\",\"params\":{\"type\":\"box\",\"from_line\":-1,\"to_line\":99999999999}}".to_vec(),
        b"{\"id\":1,\"method\":\"step.set\",\"params\":{\"step\":1e308}}".to_vec(),
        b"{\"id\":{\"nested\":[1,2]},\"method\":\"status\"}".to_vec(),
        vec![0xff, 0xfe, b'{', b'}'],
        format!("{{\"id\":1,\"method\":\"{}\"}}", "x".repeat(200_000)).into_bytes(),
        b"{\"id\":1,\"method\":\"annotation.add\",\"params\":{\"type\":\"note\",\"line\":1,\"text\":\"\\u0000\\u202e\"}}".to_vec(),
    ];
    for g in &garbage {
        let mut s = UnixStream::connect(&live.socket).unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        s.write_all(g).unwrap();
        s.write_all(b"\n").unwrap();
        s.flush().unwrap();
        let mut reader = BufReader::new(s.try_clone().unwrap());
        let mut reply = String::new();
        // Either an error/result line or a closed connection; never a hang.
        let _ = reader.read_line(&mut reply);
        if !reply.trim().is_empty() {
            let v: Value =
                serde_json::from_str(&reply).unwrap_or_else(|_| panic!("not JSON: {reply}"));
            assert!(
                v.get("result").is_some() || v.get("error").is_some(),
                "{reply}"
            );
        }
        drop(reader);
        // The server still answers a well-formed request.
        let st = live.api_ok("status", json!({}));
        assert_eq!(st["lines"], json!(2));
    }
    assert_eq!(
        live.with(|a| a.doc.annotations.len()),
        1,
        "the one valid note stays"
    );
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 120, ..ProptestConfig::default() })]

    #[test]
    fn ac3_random_requests_get_a_reply(method in "[a-z.]{0,16}", params in json_strategy()) {
        let live = Live::from_code("fn a() {}\n", "rs", "t074-rand");
        let resp = live.api("status", json!({}));
        prop_assert!(resp.error.is_none());
        let r = live.api(&method, params);
        prop_assert!(r.result.is_some() || r.error.is_some());
        let st = live.api_ok("status", json!({}));
        prop_assert_eq!(st["lines"].as_u64(), Some(1));
    }
}

#[test]
fn ac4_smoke_flag_draws_one_frame_and_exits_cleanly() {
    // A pty via Python (always on the runners): stdin is a pipe, so keys come
    // from /dev/tty. The frame must contain the code and the exit code be 0.
    let dir = scratch("t074-smoke");
    let stdout = tiza_qa::pty_script(
        &dir,
        "smoke.py",
        r#"
binary, source = sys.argv[1], sys.argv[2]
s = Pty(["bash", "-c", f"cat {source} | {binary} --smoke --no-socket; echo EXIT=$?"], 40, 120)
s.wait_for(lambda out: b"EXIT=" in out, 30, "EXIT=")
s.finish(10)
out = s.out
text = plain(out)
print("ALTSCREEN_IN" if b"\x1b[?1049h" in out else "no-altscreen")
print("ALTSCREEN_OUT" if b"\x1b[?1049l" in out else "no-restore")
print("CODE" if "impl" in text else "no-code")
print(text[text.find("EXIT="):].strip())
"#,
        &[
            tiza_qa::bin().to_str().unwrap(),
            fixture("t-011/repeated.rs").to_str().unwrap(),
        ],
    );
    assert!(stdout.contains("ALTSCREEN_IN"), "{stdout}");
    assert!(
        stdout.contains("ALTSCREEN_OUT"),
        "terminal restored: {stdout}"
    );
    assert!(stdout.contains("CODE"), "{stdout}");
    assert!(stdout.contains("EXIT=0"), "{stdout}");
    // `--smoke` never writes a sidecar next to the source.
    assert!(!fixture("t-011/repeated.rs.tiza").exists());
}
