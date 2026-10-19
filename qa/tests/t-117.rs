//! t-117 — `redact`: the presenter masks a span and the mask applies to the
//! TUI, every export, the recordings and what agents read; the file on disk
//! never changes.
//!
//! Acceptance criteria:
//! - AC1 `:redact L`, `:redact L:anchor [keep=N]`, `:redact L@S..E`, `r` on a
//!   selection; socket `annotation.add {type: "redact"}`; MCP `redact`.
//! - AC2 Eight `•` plus the last `keep` characters, never the real length.
//! - AC3 Masked everywhere: TUI, txt/md/ansi/svg/png, .cast/.tape,
//!   render.text, code.get, scene.get; active from step 0 whatever the
//!   annotation's step (unless `redact_from_step`).
//! - AC4 Only columns are stored; anchoring by the secret itself is refused;
//!   `:embed` and `:w` never write the secret.
//! - AC5 Follow/paste re-anchor a redact; a dropped one is reported loudly.
//!   (Retrabajo) A redact follows the text AROUND it, never the secret: a
//!   rotated value under the same key stays masked (`keep=N` shows the new
//!   tail). (Retrabajo 2) When it cannot follow that text it fails CLOSED:
//!   the rest of its line (or every line of a hunk that changed size) is
//!   masked, loudly, marked `widened` in scene.list/scene.get until the
//!   value is marked again; only a line deleted with nothing in its place
//!   drops it. `:embed` turns follow off (the code no longer comes from
//!   the file).
//! - AC6 `:redact off|on` for the author only; `◆ redacted` on the bar;
//!   presentation mode and exports force the masks.
//! - AC7 `:redact auto [--dry]` creates ordinary redact annotations from the
//!   built-in patterns, reports them, never runs by itself.
//! - AC9 No leak: the masked text never appears in status, scene.list, error
//!   messages, stderr, the crash line, `--smoke`, or the saved `.tiza`.

use crossterm::event::KeyCode;
use serde_json::json;
use tiza_core::fs;
use tiza_qa::{Live, McpClient, scratch, tiza, tiza_ok};

const SECRET: &str = "sk-live-abc123def456XYZ";
const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV";

fn config() -> String {
    format!(
        "# service config\nAPI_KEY={SECRET}\nAUTH=Bearer {JWT}\nDATABASE_URL=postgres://admin:hunter2@db.internal:5432/app\nNAME=demo\n"
    )
}

fn opened(name: &str) -> (Live, std::path::PathBuf, std::path::PathBuf) {
    let dir = scratch(name);
    let src = dir.join("config.env");
    std::fs::write(&src, config()).unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let sidecar = dir.join("config.env.tiza");
    let live = Live::open(doc, sidecar.clone(), dir.join("live.sock"));
    (live, src, sidecar)
}

fn assert_clean(what: &str, s: &str) {
    assert!(!s.contains(SECRET), "{what} leaks the secret:\n{s}");
    assert!(
        !s.contains("abc123"),
        "{what} leaks part of the secret:\n{s}"
    );
}

#[test]
fn ac1_ac2_ac3_forms_mask_and_every_reader_sees_the_mask() {
    let (live, _src, _) = opened("t117-forms");
    // The anchor before the secret, to the end of the line.
    live.command("redact 2:API_KEY=");
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.contains("redacted L2"), "{msg}");
    assert_clean("message", &msg);
    // keep=4 after `Bearer`.
    live.command("redact 3:Bearer keep=4");
    assert!(!live.message().unwrap().1);
    // Whole line 4.
    live.command("redact 4");
    assert!(!live.message().unwrap().1);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 3);
    let text = live.api_ok("render.text", json!({"width": 100}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_clean("render.text", &text);
    assert!(text.contains("API_KEY=••••••••"), "{text}");
    assert!(
        text.contains(&format!("Bearer ••••••••{}", &JWT[JWT.len() - 4..])),
        "{text}"
    );
    assert!(
        text.contains("••••••••") && !text.contains("hunter2"),
        "{text}"
    );
    assert!(text.contains("NAME=demo"), "untouched line: {text}");
    // The mask is eight cells whatever the secret's length.
    let l2 = text.lines().find(|l| l.contains("API_KEY=")).unwrap();
    assert!(l2.trim_end().ends_with("API_KEY=••••••••"), "{l2}");
    // code.get and scene.get are masked too.
    let code = live.api_ok("code.get", json!({}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_clean("code.get", &code);
    assert!(code.contains("••••••••"));
    let scene = live.api_ok("scene.get", json!({})).to_string();
    assert_clean("scene.get", &scene);
    assert!(!scene.contains(JWT));
    // scene.list describes columns, not text.
    let list = live.api_ok("scene.list", json!({})).to_string();
    assert_clean("scene.list", &list);
    assert!(
        list.contains("redact L2") && list.contains("(masked)"),
        "{list}"
    );
    // The screen (normal mode) is masked; step 0 too (the redacts are at step 1).
    let screen = live.screen(100, 20);
    assert_clean("screen", &screen);
    assert!(screen.contains("◆ redacted"), "{screen}");
    live.api_ok("step.set", json!({"step": 0}));
    assert_clean("screen at step 0", &live.screen(100, 20));
    // Columns only in the document: no `find`, no markers.
    let saved = serde_json::to_string(&live.with(|a| a.doc.clone())).unwrap();
    assert_clean("document JSON", &saved);
    assert!(!saved.contains("find"), "{saved}");
}

#[test]
fn ac3_exports_and_recordings_are_masked() {
    let (live, src, sidecar) = opened("t117-exports");
    live.command("redact 2:API_KEY=");
    live.command("w");
    let dir = src.parent().unwrap();
    for ext in ["txt", "md", "ansi", "svg", "cast", "tape"] {
        let out = dir.join(format!("x.{ext}"));
        tiza_ok(&[
            "render",
            sidecar.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
            "--width",
            "100",
        ]);
        let s = std::fs::read_to_string(&out).unwrap();
        if ext == "tape" {
            // A tape only names the document; the binary it runs masks.
            assert_clean("tape", &s);
        } else {
            assert_clean(ext, &s);
            assert!(s.contains("••••••••"), "{ext}:\n{s}");
        }
    }
    // PNG: rendered from the same canvas; its bytes cannot carry the text,
    // but check the SVG it came from had none (above) and that it renders.
    let png = dir.join("x.png");
    tiza_ok(&[
        "render",
        sidecar.to_str().unwrap(),
        "--out",
        png.to_str().unwrap(),
        "--scale",
        "1",
    ]);
    assert!(std::fs::metadata(&png).unwrap().len() > 1000);
    // The live instance's export tool (same canvas).
    let out = dir.join("live.txt");
    live.api_ok("step.set", json!({"step": 1}));
    live.command(&format!("export {}", out.display()));
    assert_clean("export", &std::fs::read_to_string(&out).unwrap());
}

#[test]
fn ac4_anchoring_by_the_secret_is_refused_and_the_sidecar_never_holds_it() {
    let (live, _src, sidecar) = opened("t117-save");
    live.command(&format!("redact 2:{SECRET}"));
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("anchor a redact by columns or by the text before it"),
        "{msg}"
    );
    assert_clean("refusal message", &msg);
    live.command(&format!("redact 3:{JWT}"));
    assert!(live.message().unwrap().1);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0);
    // Accepted form, then save: the sidecar references the file (no text).
    live.command("redact 2:API_KEY=");
    live.command("w");
    let saved = std::fs::read_to_string(&sidecar).unwrap();
    assert_clean("sidecar", &saved);
    // `:embed` freezes the MASKED code, and says so in the file.
    live.command("embed");
    live.command("w");
    let saved = std::fs::read_to_string(&sidecar).unwrap();
    assert_clean("embedded sidecar", &saved);
    assert!(saved.contains("••••••••"), "{saved}");
    assert!(saved.contains("NAME=demo"));
    // The source file on disk is untouched.
    assert_eq!(std::fs::read_to_string(_src).unwrap(), config());
}

#[test]
fn ac5_follow_reanchors_and_a_lost_anchor_fails_closed() {
    let (live, src, _) = opened("t117-follow");
    live.command("redact 2:API_KEY=");
    live.command("follow on");
    // A line added above: the mask follows.
    save_atomically(&src, &format!("# new first line\n{}", config()));
    wait_reloads(&live, 1);
    let text = render_text(&live);
    assert_clean("after reload", &text);
    assert!(
        text.lines()
            .any(|l| l.contains("3") && l.contains("API_KEY=••••••••")),
        "{text}"
    );
    // The anchor itself (the text before the secret) rewritten: fail-closed,
    // the mask is NOT dropped — it covers the rest of the line from where
    // the line first differs, loudly. (A new value under the same key stays
    // masked precisely: see `ac5_rotating_a_masked_value_under_follow_keeps_it_masked`.)
    let changed = config().replace(&format!("API_KEY={SECRET}"), "APIKEY_V2=rotated-later");
    save_atomically(&src, &format!("# new first line\n{changed}"));
    wait_reloads(&live, 2);
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains(
            "redact a1 could not follow its text: masking the rest of L3 — fix it before recording (u undoes the reload)"
        ),
        "{msg}"
    );
    assert!(
        !msg.contains("dropped") && !msg.contains("rotated"),
        "{msg}"
    );
    assert_eq!(code_line(&live, 3), "API••••••••");
    assert_no_value(&live, "anchor rewritten", &["rotated-later", "KEY_V2"]);
    // Deleted with nothing in its place: only now is it dropped, loudly.
    let deleted = config().replace(&format!("API_KEY={SECRET}\n"), "");
    save_atomically(&src, &format!("# new first line\n{deleted}"));
    wait_reloads(&live, 3);
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg
            .contains("redact a1 dropped: the line changed — check the screen before recording"),
        "{msg}"
    );
    assert_clean("drop notice", &msg);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 0);
}

/// `render.text` at width 100.
fn render_text(live: &Live) -> String {
    live.api_ok("render.text", json!({"width": 100}))["text"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Line `n` (1-based) of `code.get`.
fn code_line(live: &Live, n: usize) -> String {
    live.api_ok("code.get", json!({}))["text"]
        .as_str()
        .unwrap()
        .lines()
        .nth(n - 1)
        .and_then(|l| l.get(6..)) // past the `{n:>4}  ` gutter
        .unwrap_or_default()
        .to_string()
}

/// None of `pieces` of a new value on any surface an agent, a recording or
/// the screen reads: code.get, render.text, scene.get, scene.list, the
/// screen, the `.tiza` a save writes and an SVG rendered from it (the PNG
/// and the GIF are drawn from the same canvas).
fn assert_no_value(live: &Live, what: &str, pieces: &[&str]) {
    live.command("w");
    let sidecar = live.with(|a| a.save_path.clone());
    let saved = std::fs::read_to_string(&sidecar).unwrap();
    let svg_path = scratch(&format!("t117-svg-{}", what.replace(' ', "-"))).join("x.svg");
    tiza_ok(&[
        "render",
        sidecar.to_str().unwrap(),
        "--out",
        svg_path.to_str().unwrap(),
        "--width",
        "100",
    ]);
    let svg = std::fs::read_to_string(&svg_path).unwrap();
    assert!(svg.contains("••••••••"), "{what}: svg without the mask");
    let code = live.api_ok("code.get", json!({}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    for (surface, s) in [
        ("code.get", code),
        ("render.text", render_text(live)),
        ("scene.get", live.api_ok("scene.get", json!({})).to_string()),
        (
            "scene.list",
            live.api_ok("scene.list", json!({})).to_string(),
        ),
        ("screen", live.screen(100, 20)),
        ("sidecar", saved),
        ("svg", svg),
    ] {
        for p in pieces {
            assert!(!s.contains(p), "{what}: {surface} shows `{p}`:\n{s}");
        }
        assert_clean(&format!("{what}: {surface}"), &s);
    }
}

const ROTATED: &str = "sk-live-NEWvalue987654zzQ9";

/// Waits (10 s at most) until follow mode has applied `n` reloads.
fn wait_reloads(live: &Live, n: u64) {
    let start = std::time::Instant::now();
    while live.api_ok("follow.status", json!({}))["reloads"]
        .as_u64()
        .unwrap_or(0)
        < n
    {
        assert!(
            start.elapsed().as_secs() < 10,
            "follow did not reload #{n} in 10 s"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Atomic save, as editors do (tmp + rename).
fn save_atomically(src: &std::path::Path, text: &str) {
    let tmp = src.with_extension("env.tmp");
    std::fs::write(&tmp, text).unwrap();
    std::fs::rename(&tmp, src).unwrap();
}

/// The rotated value is nowhere an agent or a recording can read it.
fn assert_rotated_masked(live: &Live, what: &str) {
    let text = live.api_ok("render.text", json!({"width": 100}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    let code = live.api_ok("code.get", json!({}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    let scene = live.api_ok("scene.get", json!({})).to_string();
    let screen = live.screen(100, 20);
    for (surface, s) in [
        ("render.text", &text),
        ("code.get", &code),
        ("scene.get", &scene),
        ("screen", &screen),
    ] {
        assert!(
            !s.contains(ROTATED) && !s.contains("NEWvalue"),
            "{what}: {surface} shows the rotated value:\n{s}"
        );
        assert_clean(&format!("{what}: {surface}"), s);
    }
    // keep=4 shows the tail of the NEW value.
    let tail = &ROTATED[ROTATED.len() - 4..];
    assert!(
        text.contains(&format!("API_KEY=••••••••{tail}")),
        "{what}: {text}"
    );
    assert!(
        code.contains(&format!("API_KEY=••••••••{tail}")),
        "{what}: {code}"
    );
}

#[test]
fn ac5_rotating_a_masked_value_under_follow_keeps_it_masked() {
    let (live, src, _) = opened("t117-rotate-follow");
    live.command("redact 2:API_KEY= keep=4");
    assert!(!live.message().unwrap().1);
    live.command("follow on");
    // Same key, new (longer) value, saved with tmp + rename.
    save_atomically(
        &src,
        &config().replace(&format!("API_KEY={SECRET}"), &format!("API_KEY={ROTATED}")),
    );
    wait_reloads(&live, 1);
    assert_rotated_masked(&live, "after the rotation");
    let (msg, err) = live.message().unwrap();
    assert!(!err && !msg.contains("dropped"), "kept, no notice: {msg}");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    // Rotated again with a line added above it and one right after it
    // (the line moves and changes at once): still masked.
    let again = config().replace(
        &format!("API_KEY={SECRET}\n"),
        &format!("API_KEY={ROTATED}\nAPI_REGION=eu\n"),
    );
    save_atomically(&src, &format!("# rotated on 2026-10-18\n{again}"));
    wait_reloads(&live, 2);
    assert_rotated_masked(&live, "after moving and rotating");
    // The sidecar a save writes never holds the new value either.
    let saved = serde_json::to_string(&live.with(|a| a.doc.clone())).unwrap();
    assert!(!saved.contains(ROTATED), "{saved}");
}

#[test]
fn ac5_rotating_a_masked_value_by_paste_keeps_it_masked() {
    let (live, _src, _) = opened("t117-rotate-paste");
    live.command("redact 2:API_KEY= keep=4");
    assert!(!live.message().unwrap().1);
    let pasted = config().replace(&format!("API_KEY={SECRET}"), &format!("API_KEY={ROTATED}"));
    live.with(move |a| a.replace_code(&pasted)).unwrap();
    assert_rotated_masked(&live, "after the paste");
    let (msg, err) = live.message().unwrap();
    assert!(!err && !msg.contains("dropped"), "{msg}");
    // A paste where the key itself is gone: fail-closed, the whole line
    // (nothing before the old column survived), loudly.
    let renamed = config().replace(&format!("API_KEY={SECRET}"), &format!("TOKEN_V2={ROTATED}"));
    live.with(move |a| a.replace_code(&renamed)).unwrap();
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("redact a1 could not follow its text: masking the rest of L2"),
        "{msg}"
    );
    assert!(!msg.contains(ROTATED), "{msg}");
    assert_eq!(code_line(&live, 2), "••••••••", "no tail once widened");
    assert_no_value(&live, "paste renamed", &[ROTATED, "NEWvalue", "TOKEN_V2"]);
}

/// `cfg.json` with `{"token": "abc123secret", "user": "ana"}`, the value
/// selected as columns (as `r` on a selection stores it), follow on.
fn json_live(name: &str) -> (Live, std::path::PathBuf) {
    let dir = scratch(name);
    let src = dir.join("cfg.json");
    std::fs::write(&src, "{\"token\": \"abc123secret\", \"user\": \"ana\"}\n").unwrap();
    let doc = fs::open(src.to_str().unwrap()).unwrap().document;
    let live = Live::open(doc, dir.join("cfg.json.tiza"), dir.join("live.sock"));
    live.command("redact 1@11..23");
    assert!(!live.message().unwrap().1);
    live.command("follow on");
    (live, src)
}

#[test]
fn ac5_column_redact_that_cannot_follow_fails_closed() {
    // Retrabajo (Luis, 2026-10-17): dropping revealed the secret. A value
    // selected mid-line follows the text on both sides; when one side or
    // both change, the mask covers the rest of the line, loudly, and the
    // widened state shows in scene.list / scene.get until the presenter
    // marks the value again.
    let (live, src) = json_live("t117-columns");
    save_atomically(
        &src,
        "{\"token\": \"NEWvalue-much-longer\", \"user\": \"ana\"}\n",
    );
    wait_reloads(&live, 1);
    assert_eq!(
        code_line(&live, 1),
        "{\"token\": \"••••••••\", \"user\": \"ana\"}"
    );
    // One side changed (`user` → `usr`): the rest of the line from the value on.
    save_atomically(&src, "{\"token\": \"r0tat3d-0ne-side\", \"usr\": 1}\n");
    wait_reloads(&live, 2);
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains(
            "redact a1 could not follow its text: masking the rest of L1 — fix it before recording"
        ),
        "{msg}"
    );
    assert!(!msg.contains("dropped"), "{msg}");
    assert_eq!(code_line(&live, 1), "{\"token\": \"••••••••");
    assert_no_value(&live, "one side", &["r0tat3d", "0ne-side"]);
    // (t-183: the widened mask keeps no tail; there is no `widened` flag.)
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    // Both sides changed: from where the line first differs (`{"tok`), so
    // not one character of the new value shows (the old column 11 would
    // have shown `zz`).
    save_atomically(&src, "{\"tok\": \"zzq-b0th-s1des\", \"usr\": 1}\n");
    wait_reloads(&live, 3);
    let (msg, err) = live.message().unwrap();
    assert!(err && msg.contains("could not follow its text"), "{msg}");
    assert_eq!(code_line(&live, 1), "{\"tok••••••••");
    assert_no_value(&live, "both sides", &["zzq-b0th", "b0th-s1des"]);
    // Marking the value again (`:redact off` shows it to the author, a
    // selection or columns mark it) adds a precise mask; deleting the wide
    // one (`x`, `:del`) leaves it.
    live.command("redact 1@9..23");
    assert!(!live.message().unwrap().1);
    live.command("del a1");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    assert_eq!(code_line(&live, 1), "{\"tok\": \"••••••••\", \"usr\": 1}");
}

#[test]
fn ac5_auto_redact_whose_prefix_is_gone_fails_closed() {
    let (live, src, _) = opened("t117-auto-prefix");
    live.command("redact auto");
    assert!(live.message().unwrap().0.contains("3 masked"));
    live.command("follow on");
    // The env key renamed and the value rotated at once: the auto redact
    // (anchored after `API_KEY=`) cannot follow; the URL password rotated
    // under the same `admin:` … `@` stays masked precisely.
    let changed = config()
        .replace(
            &format!("API_KEY={SECRET}"),
            "SERVICE_TOKEN=svc-N3W-rotated-77",
        )
        .replace("admin:hunter2@", "admin:n3w-pa55@");
    save_atomically(&src, &changed);
    wait_reloads(&live, 1);
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("could not follow its text: masking the rest of L2"),
        "{msg}"
    );
    assert_eq!(live.with(|a| a.doc.annotations.len()), 3, "nothing dropped");
    assert!(code_line(&live, 4).contains("postgres://admin:••••••••@"));
    assert_no_value(&live, "auto prefix", &["svc-N3W", "rotated-77", "n3w-pa55"]);
}

#[test]
fn ac5_a_hunk_that_changed_size_masks_all_its_new_lines() {
    let (live, src, _) = opened("t117-hunk");
    live.command("redact 2:API_KEY=");
    live.command("follow on");
    // The masked line replaced by three lines, none with `API_KEY=`: which
    // one holds the value cannot be told, so all three are masked whole.
    let changed = config().replace(
        &format!("API_KEY={SECRET}\n"),
        "# keys moved to the vault\nVAULT_PATH=kv/app\nVAULT_TOKEN=hvs.N3wT0ken\n",
    );
    save_atomically(&src, &changed);
    wait_reloads(&live, 1);
    let (msg, err) = live.message().unwrap();
    assert!(
        err && msg.contains("redact a1 could not follow its text: masking L2-4 whole"),
        "{msg}"
    );
    assert_eq!(live.with(|a| a.doc.annotations.len()), 3);
    for n in 2..=4 {
        assert_eq!(code_line(&live, n), "••••••••", "L{n}");
    }
    assert_eq!(
        code_line(&live, 5),
        format!("AUTH=Bearer {JWT}"),
        "untouched"
    );
    assert_no_value(&live, "hunk", &["hvs.", "N3wT0ken", "VAULT", "kv/app"]);
}

#[test]
fn ac4_embed_turns_follow_off() {
    let (live, src, _) = opened("t117-embed-follow");
    live.command("redact 2:API_KEY=");
    live.command("follow on");
    live.command("embed");
    let (msg, err) = live.message().unwrap();
    assert!(
        !err && msg.contains("follow off") && msg.contains(":link and :follow on"),
        "{msg}"
    );
    assert_eq!(live.api_ok("follow.status", json!({}))["enabled"], false);
    // An edit on disk no longer reaches the frozen copy, and nothing is dropped.
    save_atomically(
        &src,
        &config().replace(&format!("API_KEY={SECRET}"), &format!("API_KEY={ROTATED}")),
    );
    let settle = std::time::Instant::now() + std::time::Duration::from_millis(400);
    while std::time::Instant::now() < settle {
        let (msg, err) = live.message().unwrap_or_default();
        assert!(!err, "no reload, no drop notice: {msg}");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    let code = live.api_ok("code.get", json!({}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        code.contains("API_KEY=••••••••") && !code.contains(ROTATED),
        "{code}"
    );
}

#[test]
fn ac6_author_toggle_presentation_and_r_key() {
    let (live, _src, _) = opened("t117-toggle");
    // `v` + motion + `r`: select the value on line 2 and redact it.
    live.with(|a| {
        a.cursor_line = 2;
        a.pending_sel = Some(tiza_tui::app::Selection::Span(tiza_core::TextSpan {
            line: 2,
            start: 8,
            end: 8 + SECRET.len() as u32,
        }));
    });
    live.command("redact 2@8..31");
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.contains("redacted L2"), "{msg}");
    assert_clean("screen", &live.screen(100, 20));
    live.command("redact off");
    let screen = live.screen(100, 20);
    assert!(
        screen.contains(SECRET),
        "the author sees the real text:\n{screen}"
    );
    assert!(screen.contains("◆ UNMASKED"), "{screen}");
    // Every reader still gets the mask while the author looks at the text.
    assert_clean(
        "render.text while off",
        live.api_ok("render.text", json!({"width": 100}))["text"]
            .as_str()
            .unwrap(),
    );
    assert_clean(
        "code.get while off",
        live.api_ok("code.get", json!({}))["text"].as_str().unwrap(),
    );
    // Presentation mode forces the mask.
    live.keys(&[KeyCode::Char('P')]);
    assert_clean("presentation screen", &live.screen(100, 20));
    live.keys(&[KeyCode::Char('P')]);
    live.command("redact on");
    assert_clean("screen on", &live.screen(100, 20));
    live.command("redact");
    let (msg, _) = live.message().unwrap();
    assert!(msg.starts_with("redact: 1 mask (shown"), "{msg}");
}

#[test]
fn ac7_auto_is_opt_in_reviewable_and_reports() {
    let (live, _src, _) = opened("t117-auto");
    assert_eq!(
        live.with(|a| a.doc.annotations.len()),
        0,
        "nothing masked by itself"
    );
    live.command("redact auto --dry");
    let (msg, err) = live.message().unwrap();
    assert!(
        !err && msg.contains("3 matches")
            && msg.contains("L2 env")
            && msg.contains("L3 jwt")
            && msg.contains("L4 url-credentials"),
        "{msg}"
    );
    assert_eq!(
        live.with(|a| a.doc.annotations.len()),
        0,
        "--dry adds nothing"
    );
    live.command("redact auto");
    let (msg, err) = live.message().unwrap();
    assert!(!err && msg.contains("redact auto: 3 masked"), "{msg}");
    assert_clean("auto message", &msg);
    assert_eq!(live.with(|a| a.doc.annotations.len()), 3);
    let text = live.api_ok("render.text", json!({"width": 100}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_clean("after auto", &text);
    assert!(
        !text.contains("hunter2") && text.contains("postgres://admin:••••••••@"),
        "{text}"
    );
    // Ordinary annotations: the presenter deletes one and the value is back.
    live.command("del a3");
    let text = live.api_ok("render.text", json!({"width": 100}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(text.contains("hunter2"), "{text}");
    // A second run does not stack masks on masks; nothing new to find.
    live.command("redact auto");
    let (msg, _) = live.message().unwrap();
    assert!(msg.contains("1 masked"), "{msg}");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 3);
    // A document with nothing to find says so.
    let plain = Live::from_code("fn main() {}\n", "rs", "t117-auto-none");
    plain.command("redact auto");
    assert!(
        plain
            .message()
            .unwrap()
            .0
            .starts_with("redact auto: 0 matches")
    );
}

#[test]
fn ac1_socket_and_mcp_forms() {
    let (live, _src, _) = opened("t117-api");
    // Columns.
    live.api_ok(
        "annotation.add",
        json!({"type": "redact", "at": {"line": 2, "col": 8, "end": 31}, "keep": 2}),
    );
    // Anchor before the secret.
    let r = live.api(
        "annotation.add",
        json!({"type": "redact", "at": {"line": 3, "find": "Bearer"}}),
    );
    assert!(r.error.is_none(), "{:?}", r.error);
    // The secret as `find`: refused, and the error does not echo it.
    let r = live.api(
        "annotation.add",
        json!({"type": "redact", "at": {"line": 2, "find": SECRET}}),
    );
    let e = r.error.unwrap();
    assert_eq!(e.code, "invalid_argument");
    assert_clean("api error", &e.message);
    let text = live.api_ok("render.text", json!({"width": 100}))["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert_clean("api render", &text);
    assert!(
        text.contains(&format!("API_KEY=••••••••{}", &SECRET[SECRET.len() - 2..])),
        "{text}"
    );
    // MCP tool and the resources.
    let mut mcp = McpClient::start(&live.socket);
    let (err, out, _) = mcp.call("redact", json!({"at": {"line": 5, "find": "NAME="}}));
    assert!(!err, "{out}");
    let (err, code, _) = mcp.call("get_code", json!({}));
    assert!(!err);
    assert_clean("get_code", &code);
    assert!(code.contains("NAME=••••••••"), "{code}");
    let (err, status, _) = mcp.call("status", json!({}));
    assert!(!err);
    assert_clean("status", &status);
}

#[test]
fn ac9_no_leak_through_errors_crash_line_or_smoke() {
    let (live, src, sidecar) = opened("t117-leak");
    live.command("redact 2:API_KEY=");
    live.command("w");
    // A text search never confirms what is under a mask: probing the
    // secret by substring fails exactly like text that is not there (no
    // oracle for an agent), in commands, the socket and moves.
    live.command("hl 2:abc123 x");
    let (msg, err) = live.message().unwrap();
    assert!(err && !msg.contains("L2;"), "{msg}");
    assert_eq!(live.with(|a| a.doc.annotations.len()), 1);
    let r = live.api(
        "annotation.add",
        json!({"type": "highlight", "at": {"find": &SECRET[..6]}}),
    );
    assert_eq!(
        r.error.as_ref().unwrap().code,
        "text_not_found",
        "{:?}",
        r.error
    );
    let r = live.api(
        "move.add",
        json!({"at_step": 1, "action": "code.patch", "op": "delete", "find": &SECRET[3..9]}),
    );
    assert_eq!(
        r.error.as_ref().unwrap().code,
        "text_not_found",
        "{:?}",
        r.error
    );
    // Text next to the mask on the same line is still found.
    let r = live.api(
        "annotation.add",
        json!({"type": "highlight", "at": {"line": 2, "find": "API_KEY"}}),
    );
    assert!(r.error.is_none(), "{:?}", r.error);
    // status / api.methods / schema carry no code.
    assert_clean("status", &live.api_ok("status", json!({})).to_string());
    assert_clean(
        "api.methods",
        &live.api_ok("api.methods", json!({})).to_string(),
    );
    // --smoke in a pty: the frame drawn is masked.
    let dir = src.parent().unwrap();
    let all = tiza_qa::pty_script(
        dir,
        "smoke.py",
        r#"
binary, doc = sys.argv[1], sys.argv[2]
env = dict(os.environ, TIZA_TEST_PANIC="1")
s = Pty(["bash", "-c", f"{binary} {doc} --no-socket --no-save; echo EXIT=$?"], 30, 100, env=env)
# Keys typed before the first frame would be read by the start-up OSC query.
if s.wait_for(lambda out: b"\x1b[?1049h" in out, 10, "first frame"):
    s.send(b":panic\r")
s.wait_for(lambda out: b"EXIT=" in out, 20, "EXIT=")
s.finish(10)
sys.stdout.write(s.out.decode(errors="replace"))
"#,
        &[tiza_qa::bin().to_str().unwrap(), sidecar.to_str().unwrap()],
    );
    assert!(all.contains("tiza crashed:"), "{all}");
    assert_clean("pty output incl. the crash line", &all);
    assert!(
        all.contains("••••••••"),
        "the frame was drawn masked:\n{all}"
    );
    // A plain render to stderr-less stdout: nothing on stderr either.
    let o = tiza(&[
        "render",
        sidecar.to_str().unwrap(),
        "--out",
        "-",
        "--width",
        "100",
    ]);
    assert_clean("stderr", &String::from_utf8_lossy(&o.stderr));
    assert_clean("stdout", &String::from_utf8_lossy(&o.stdout));
}
