//! What several modules of this area share: the real TUI on a pty with a
//! config file of its own.

use std::path::Path;

use serde_json::Value;
use tiza_qa::bin;

/// The real TUI on a pty in `dir`, `cols` columns wide, with
/// `TIZA_CONFIG=config`. It waits for the first frame (and, when `bar` is
/// not empty, until the screen shows `bar`), then types each `(command,
/// text that the screen shows after it)`. It prints `STARTED`, `PRE` (what
/// came before the first frame: stderr), `BAR0` (the text of the first
/// frames), `STATUS0` and `STATUS1` (`status` before and after the
/// commands), `TEXT0` (`scene text --width 60`), `MOUSE_ON0`, and for each
/// command `CMDi` and `MOUSE_OFFi`.
pub fn tui(
    dir: &Path,
    config: &Path,
    cols: u16,
    args: &[&str],
    bar: &str,
    commands: &[(&str, &str)],
) -> String {
    let body = r##"
import json
binary, cwd, args, steps, bar, cfg, cols = sys.argv[1:8]
os.chdir(cwd)
args, steps = json.loads(args), json.loads(steps)
env = dict(os.environ, TERM="xterm-256color", TIZA_CONFIG=cfg)
s = Pty([binary, "--theme", "dark", "--socket", "s.sock", *args], 30, int(cols), env=env, ctty=True, raw=True)
def api(*a):
    r = subprocess.run([binary, "--socket", "s.sock", *a], capture_output=True, text=True, timeout=10)
    return r.stdout
ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and os.path.exists("s.sock"), 10, "first frame")
print("STARTED", ok, flush=True)
first = s.out.index(b"\x1b[?1049h") if ok else 0
if bar:
    s.wait_for(lambda out: bar in plain(out[first:]), 10, "the bar shows " + bar)
print("PRE", json.dumps(plain(s.out[:first])), flush=True)
print("BAR0", json.dumps(plain(s.out[first:])), flush=True)
print("STATUS0", json.dumps(json.loads(api("api", "status") or "{}")), flush=True)
print("TEXT0", json.dumps(api("scene", "text", "--width", "60")), flush=True)
print("MOUSE_ON0", b"\x1b[?1000h" in s.out, flush=True)
for i, (cmd, want) in enumerate(steps):
    mark = len(s.out)
    s.send(b":" + cmd.encode() + b"\r")
    got = s.wait_for(lambda out: want in plain(out[mark:]), 10, ":" + cmd)
    print(f"CMD{i}", got, flush=True)
    # The bar is drawn first, and the capture changes on the next turn of
    # the loop: wait for the sequence itself.
    off = lambda out: b"\x1b[?1000l" in out[mark:]
    seen = s.wait_for(off, 10, "mouse capture off") if "mouse off" in cmd else off(s.out)
    print(f"MOUSE_OFF{i}", seen, flush=True)
print("STATUS1", json.dumps(json.loads(api("api", "status") or "{}")), flush=True)
s.send(b"\x1b:q!\r")
print("EXIT", s.finish(10), flush=True)
"##;
    let steps: Vec<[&str; 2]> = commands.iter().map(|(c, w)| [*c, *w]).collect();
    tiza_qa::pty_script(
        dir,
        "tui.py",
        body,
        &[
            bin().to_str().unwrap(),
            dir.to_str().unwrap(),
            &serde_json::to_string(args).unwrap(),
            &serde_json::to_string(&steps).unwrap(),
            bar,
            config.to_str().unwrap(),
            &cols.to_string(),
        ],
    )
}

/// The value printed after `KEY ` on its own line.
pub fn field<'a>(out: &'a str, key: &str) -> &'a str {
    out.lines()
        .find_map(|l| l.strip_prefix(&format!("{key} ")))
        .unwrap_or_else(|| panic!("no {key} in:\n{out}"))
}

pub fn json_field(out: &str, key: &str) -> Value {
    serde_json::from_str(field(out, key)).unwrap()
}

pub fn text_field(out: &str, key: &str) -> String {
    json_field(out, key).as_str().unwrap().to_string()
}

/// A scratch folder with `file.rs` and the config fixture `config` (a
/// path under `qa/fixtures/`) as `cfg.toml`.
pub fn setup(name: &str, config: &str) -> std::path::PathBuf {
    let dir = tiza_qa::scratch(name);
    std::fs::copy(
        tiza_qa::fixture("shared/personal_config/file.rs"),
        dir.join("file.rs"),
    )
    .unwrap();
    std::fs::copy(tiza_qa::fixture(config), dir.join("cfg.toml")).unwrap();
    dir
}
