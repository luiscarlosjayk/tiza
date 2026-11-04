#!/usr/bin/env python3
"""Speed baseline of the release binary.

Times fixed scenarios with `target/release/tiza` — one warm-up run, then the
median of N runs — writes `qa/bench/latest.json` and compares it with this
target's entry in `qa/bench/baseline.json`, printing a WARN line for every
scenario more than 20 % slower. It never fails on speed: machines differ and a
laptop is noisy, so the numbers are a signal for a human, not a gate.

Scenarios: `--version`; `render` of step 7 of the ownership demo as it was
when the baseline was taken (qa/fixtures/shared/ownership/ownership.rs.tiza) to
.txt and .png; a generated 2,000-line Rust file and a generated large
TypeScript file to .txt; the native GIF of the example; the TUI's first frame
on a pseudo-terminal (qa/src/pty.py: non-blocking reads, a deadline, the
child killed at the end); the qa crate's frame median (2,000 lines + 200
annotations, laid out and drawn into 120x40, from `cargo test --release`);
and the same in presentation mode with 10 moves over 20 steps
(`present_moves_frame`, from the qa crate's presentation frame test).

`idle_cpu` is the CPU that the TUI uses at rest, on a pseudo-terminal of
120x40: after the first frame (and a key that plays an enter effect), 3 s for
the effect to end, then 10 s with no input. Three cases: presentation mode
after an effect, normal mode after an effect, and follow mode. A loop that
does not sleep shows here. Its limit is absolute, `idle_cpu_max_pct` in
qa/bench/baseline.json (1 %): the CPU time of a process does not depend on the
load of the machine. Over the limit, bench prints FAIL and exits 1. Run
`scripts/bench.sh --only idle_cpu` before you push a change to a timed loop of
the TUI.

`--install` also times a cold `cargo install --path crates/tiza-cli --root
<tmp>` (fresh target dir, so every crate compiles) and records it as
information. `--update-baseline --note "why"` stores the run as this target's
baseline (do it on purpose, with the note, in the commit that explains it).

Usage: scripts/bench.sh [--runs N] [--no-build] [--skip-frame] [--skip-idle] [--install]
                        [--update-baseline --note TEXT]
"""
import argparse, atexit, datetime, json, os, platform, re, shutil, statistics, subprocess, sys, tempfile, time


def no_config():
    """Every tiza that this script starts reads an empty config file, and
    an empty config folder: the config of whoever runs it never changes a
    result."""
    folder = tempfile.mkdtemp(prefix="tiza-no-config-")
    open(os.path.join(folder, "config.toml"), "w").close()
    os.environ["TIZA_CONFIG"] = os.path.join(folder, "config.toml")
    os.environ["XDG_CONFIG_HOME"] = folder
    atexit.register(shutil.rmtree, folder, True)

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BENCH = os.path.join(ROOT, "qa", "bench")
EXAMPLE = os.path.join(ROOT, "qa", "fixtures", "shared", "ownership", "ownership.rs.tiza")
EFFECTS = os.path.join(ROOT, "qa", "fixtures", "render", "enter_effects", "labels.tiza")
WARN_PCT = 20
IDLE_MAX_PCT = 1.0


def host_target():
    out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    return next(line.split(": ", 1)[1] for line in out.splitlines() if line.startswith("host: "))


def commit():
    r = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, capture_output=True, text=True)
    return r.stdout.strip() or "unknown"


def cargo_env():
    return dict(os.environ, CARGO_INCREMENTAL=os.environ.get("CARGO_INCREMENTAL", "0"))


def rust_2000():
    """2,000 lines of ordinary Rust: functions, structs, matches, comments."""
    out = []
    i = 0
    while len(out) < 2000:
        out += [
            f"/// Item {i}: owns a buffer and hands it over.",
            f"#[derive(Debug, Clone, PartialEq)]",
            f"pub struct Item{i} {{ name: String, items: Vec<u32>, ratio: f64 }}",
            f"impl Item{i} {{",
            f"    pub fn new(name: &str) -> Self {{",
            f"        Self {{ name: name.to_string(), items: vec![{i}, {i + 1}, {i + 2}], ratio: {i}.5 }}",
            f"    }}",
            f"    pub fn take(self) -> Result<usize, String> {{",
            f"        match self.items.len() {{",
            f"            0 => Err(format!(\"{{}} is empty\", self.name)),",
            f"            n if n > {i % 7 + 1} => Ok(n * 2), // a \"string\" in a comment",
            f"            n => Ok(n),",
            f"        }}",
            f"    }}",
            f"}}",
            f"",
        ]
        i += 1
    return "\n".join(out[:2000]) + "\n"


def typescript_large():
    """~3,000 lines of TypeScript: interfaces, generics, classes, async, template literals."""
    out = ["import { readFile } from \"node:fs/promises\";", ""]
    i = 0
    while len(out) < 3000:
        out += [
            f"export interface Order{i}<T extends {{ id: string }}> {{",
            f"  readonly id: `order-${{number}}`;",
            f"  items: ReadonlyArray<T>;",
            f"  total?: number | null;",
            f"}}",
            f"",
            f"export class Service{i} implements Disposable {{",
            f"  #cache = new Map<string, Order{i}<{{ id: string }}>>();",
            f"  constructor(private readonly base: URL, private retries = {i % 5}) {{}}",
            f"  async load(path: string): Promise<Order{i}<{{ id: string }}>[]> {{",
            f"    const raw = await readFile(new URL(path, this.base), \"utf8\");",
            f"    const parsed: unknown = JSON.parse(raw);",
            f"    if (!Array.isArray(parsed)) throw new TypeError(`bad ${{path}} (#{i})`);",
            f"    return parsed.filter((o): o is Order{i}<{{ id: string }}> => typeof o?.id === \"string\")",
            f"      .map((o) => ({{ ...o, total: o.items.reduce((n, _) => n + 1, 0) * {i} }}));",
            f"  }}",
            f"  [Symbol.dispose](): void {{ this.#cache.clear(); }}",
            f"}}",
            f"",
        ]
        i += 1
    return "\n".join(out[:3000]) + "\n"


TUI = r"""
binary, doc = sys.argv[1], sys.argv[2]
env = dict(os.environ)
t0 = time.monotonic()
s = Pty([binary, doc, "--no-socket", "--no-save"], 40, 120, env=env, ctty=True, raw=True)
ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and "takes(s);" in plain(out), 10, "first frame")
t1 = time.monotonic()
s.finish(0.0)
print("FIRST_FRAME", (t1 - t0) if ok else -1, flush=True)
"""


IDLE = r"""
binary, doc, key, shown, ready, settle, still = sys.argv[1:8]
mode = sys.argv[8:]


def cpu_seconds(pid):
    # Linux: utime + stime of /proc (ps rounds to whole seconds there).
    stat = f"/proc/{pid}/stat"
    if os.path.exists(stat):
        fields = open(stat).read().rsplit(")", 1)[1].split()
        return (int(fields[11]) + int(fields[12])) / os.sysconf("SC_CLK_TCK")
    # macOS: [[hh:]mm:]ss.cc
    t = subprocess.run(["ps", "-o", "time=", "-p", str(pid)], capture_output=True,
                       text=True).stdout.strip()
    secs = 0.0
    for part in t.split(":"):
        secs = secs * 60 + float(part)
    return secs


s = Pty([binary, doc, "--no-socket", "--no-save", *mode], 40, 120, env=dict(os.environ),
        ctty=True, raw=True)
if not s.wait_for(lambda out: b"\x1b[?1049h" in out and ready in plain(out), 15, "first frame"):
    s.finish(2.0)
    print("IDLE -1 -1", flush=True)
    sys.exit(0)
if key:
    s.send(key.encode())
    # The step comes in: the label of its box shows after its stroke.
    if not s.wait_for(lambda out: shown in plain(out), 15, "the step after the key"):
        s.finish(2.0)
        print("IDLE -2 -2", flush=True)
        sys.exit(0)
s.settle(float(settle))
t0, c0 = time.monotonic(), cpu_seconds(s.p.pid)
s.settle(float(still))
t1, c1 = time.monotonic(), cpu_seconds(s.p.pid)
# A TUI that exits in the measure uses no CPU: that is not a pass.
gone = s.p.poll() is not None or t1 - t0 < 0.9 * float(still)
s.kill()
s.finish(2.0)
if gone:
    print("IDLE -3 -3", flush=True)
    sys.exit(0)
print("IDLE", 100 * (c1 - c0) / (t1 - t0), c1 - c0, flush=True)
"""

def idle_cpu(binary, tmp, settle_s=3.0, still_s=10.0):
    """The CPU of the TUI at rest, in % of one core, for each case."""
    script = os.path.join(tmp, "idle_cpu.py")
    with open(os.path.join(ROOT, "qa", "src", "pty.py")) as f:
        prelude = f.read()
    with open(script, "w") as f:
        f.write(prelude + "\nimport subprocess\n" + IDLE)
    follow = os.path.join(tmp, "follow")
    os.makedirs(follow, exist_ok=True)
    for name in ("ownership.rs", "ownership.rs.tiza"):
        shutil.copy(os.path.join(os.path.dirname(EXAMPLE), name), follow)
    cases = [
        # name, document, key, text that the key shows, text of the start, mode
        ("presentation_after_effect", EFFECTS, "n", "the queue", "", ["--present"]),
        ("normal_after_effect", EFFECTS, "n", "the queue", "", []),
        ("follow", os.path.join(follow, "ownership.rs.tiza"), "", "", "following", ["--follow"]),
    ]
    env = dict(os.environ, TIZA_PTY_DEADLINE="60", TIZA_NO_OSC="1")
    out = {}
    errors = {"-1": "no first frame", "-2": "the key did not show its step",
              "-3": "the TUI exited during the measure"}
    for name, doc, key, shown, ready, mode in cases:
        try:
            r = subprocess.run([sys.executable, "-I", script, binary, doc, key, shown, ready,
                                str(settle_s), str(still_s), *mode],
                               cwd=tmp, env=env, capture_output=True, text=True, timeout=90)
        except subprocess.TimeoutExpired:
            raise RuntimeError(f"idle_cpu {name}: no result in 90 s") from None
        m = re.search(r"IDLE (\S+) (\S+)", r.stdout)
        if not m or float(m.group(1)) < 0:
            why = errors.get(m.group(1) if m else "-1", "no result")
            raise RuntimeError(f"idle_cpu {name}: {why}:\n{r.stdout[-600:]}\n{r.stderr[-600:]}")
        out[name] = {"cpu_pct": round(float(m.group(1)), 2), "cpu_s": round(float(m.group(2)), 3),
                     "still_s": still_s}
    return out


def time_cmd(argv, runs):
    """One warm-up, then `runs` timed runs; each must exit 0."""
    samples = []
    for k in range(runs + 1):
        t = time.monotonic()
        r = subprocess.run(argv, cwd=ROOT, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=120)
        dt = time.monotonic() - t
        if r.returncode != 0:
            raise RuntimeError(f"{' '.join(argv)} exited {r.returncode}: {r.stderr.decode(errors='replace')[:400]}")
        if k:
            samples.append(dt * 1000)
    return samples


def time_tui(binary, runs, tmp):
    script = os.path.join(tmp, "tui_first_frame.py")
    with open(os.path.join(ROOT, "qa", "src", "pty.py")) as f:
        prelude = f.read()
    with open(script, "w") as f:
        f.write(prelude + "\n" + TUI)
    # This pty never answers the width probe, so Tiza would wait its 100 ms
    # for an answer at each start (docs/reference/terminals.md). The scenario
    # times the start of Tiza, not that wait: TIZA_NO_OSC stops the probe.
    # The example has a theme, so no color query waits either.
    env = dict(os.environ, TIZA_PTY_DEADLINE="20", TIZA_NO_OSC="1")
    samples = []
    for k in range(runs + 1):
        r = subprocess.run([sys.executable, "-I", script, binary, EXAMPLE], cwd=tmp, env=env,
                           capture_output=True, text=True, timeout=40)
        m = re.search(r"FIRST_FRAME (\S+)", r.stdout)
        if not m or float(m.group(1)) < 0:
            raise RuntimeError(f"TUI first frame not seen:\n{r.stdout[-600:]}\n{r.stderr[-600:]}")
        if k:
            samples.append(float(m.group(1)) * 1000)
    return samples


def frame_median(test="cache", name="one_layout_per_frame::a_frame_lays_out_once_and_does_not_highlight", label="median frame"):
    """A qa test's own measurement: median of 21 frames, release build of the
    qa crate (the authoring and the presentation frame tests)."""
    r = subprocess.run(
        ["cargo", "test", "--release", "--locked", "-p", "tiza-qa", "--test", test, "--",
         name, "--nocapture"],
        cwd=ROOT, env=cargo_env(), capture_output=True, text=True, timeout=3600)
    m = re.search(label + r": ([\d.]+)(ns|µs|us|ms|s)\b", r.stderr + r.stdout)
    if r.returncode != 0 or not m:
        raise RuntimeError(f"qa/tests/{test} ({name}) failed or printed no median:\n{(r.stderr + r.stdout)[-1500:]}")
    scale = {"ns": 1e-6, "µs": 1e-3, "us": 1e-3, "ms": 1.0, "s": 1000.0}[m.group(2)]
    return float(m.group(1)) * scale


def cold_install():
    with tempfile.TemporaryDirectory(prefix="tiza-install-") as tmp:
        env = dict(cargo_env(), CARGO_TARGET_DIR=os.path.join(tmp, "target"))
        t = time.monotonic()
        subprocess.run(["cargo", "install", "--locked", "--path", "crates/tiza-cli", "--root",
                        os.path.join(tmp, "root")], cwd=ROOT, env=env, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        dt = time.monotonic() - t
        size = os.path.getsize(os.path.join(tmp, "root", "bin", "tiza"))
        return dt, size


def summary(samples):
    return {
        "median_ms": round(statistics.median(samples), 2),
        "min_ms": round(min(samples), 2),
        "max_ms": round(max(samples), 2),
        "runs": len(samples),
    }


def main():
    no_config()
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--runs", type=int, default=7, help="timed runs per scenario after one warm-up (default 7)")
    ap.add_argument("--no-build", action="store_true", help="use target/release/tiza as it is")
    ap.add_argument("--skip-frame", action="store_true", help="skip the frame medians (they need a release build of the qa crate)")
    ap.add_argument("--skip-idle", action="store_true", help="skip idle_cpu (the TUI at rest, about 45 s)")
    ap.add_argument("--install", action="store_true", help="also time a cold `cargo install --path crates/tiza-cli`")
    ap.add_argument("--update-baseline", action="store_true", help="store this run as the target's baseline")
    ap.add_argument("--note", default="", help="one line: why the baseline moves (required with --update-baseline)")
    ap.add_argument("--bin", help="time this binary instead of target/release/tiza (implies --no-build)")
    ap.add_argument("--only", help="comma-separated scenario names (default: all)")
    # Shorter waits for the test of the limit (qa/tests/release): not a measure.
    ap.add_argument("--idle-settle", type=float, default=3.0, help=argparse.SUPPRESS)
    ap.add_argument("--idle-still", type=float, default=10.0, help=argparse.SUPPRESS)
    ap.add_argument("--baseline", default=os.path.join(BENCH, "baseline.json"), help=argparse.SUPPRESS)
    ap.add_argument("--latest", default=os.path.join(BENCH, "latest.json"), help=argparse.SUPPRESS)
    a = ap.parse_args()
    if a.update_baseline and (not a.note.strip() or "\n" in a.note):
        sys.exit("bench: --update-baseline needs --note with one line saying why")

    binary = a.bin or os.path.join(ROOT, "target", "release", "tiza")
    only = set(a.only.split(",")) if a.only else None
    wants = lambda name: only is None or name in only
    if not a.bin and not a.no_build:
        subprocess.run(["cargo", "build", "--release", "--locked", "-p", "tiza"], cwd=ROOT, env=cargo_env(), check=True)
    if not os.path.isfile(binary):
        sys.exit(f"bench: no binary at {binary}")

    target = host_target()
    results = {}
    with tempfile.TemporaryDirectory(prefix="tiza-bench-") as tmp:
        big_rs = os.path.join(tmp, "big.rs")
        big_ts = os.path.join(tmp, "big.ts")
        with open(big_rs, "w") as f:
            f.write(rust_2000())
        with open(big_ts, "w") as f:
            f.write(typescript_large())
        out = lambda ext: os.path.join(tmp, "out." + ext)
        step7 = ["render", EXAMPLE, "--step", "7", "--width", "100", "--out"]
        scenarios = [
            ("version", [binary, "--version"]),
            ("render_txt_step7", [binary, *step7, out("txt")]),
            ("render_png_step7", [binary, *step7, out("png")]),
            ("render_txt_rust_2000_lines", [binary, "render", big_rs, "--width", "100", "--out", out("txt")]),
            ("render_txt_typescript_3000_lines", [binary, "render", big_ts, "--width", "100", "--out", out("txt")]),
            # `--scale 1`: the baselines were measured before the GIF default
            # became 2; scale 2 rasterises 4× the pixels.
            ("render_gif_example", [binary, "render", EXAMPLE, "--width", "100", "--scale", "1", "--out", out("gif")]),
        ]
        for name, argv in scenarios:
            if not wants(name):
                continue
            results[name] = summary(time_cmd(argv, a.runs))
            print(f"{name:<34} {results[name]['median_ms']:>9.1f} ms", flush=True)
        if wants("tui_first_frame"):
            results["tui_first_frame"] = summary(time_tui(binary, a.runs, tmp))
            print(f"{'tui_first_frame':<34} {results['tui_first_frame']['median_ms']:>9.1f} ms", flush=True)
        if not a.skip_idle and wants("idle_cpu"):
            results["idle_cpu"] = idle_cpu(binary, tmp, a.idle_settle, a.idle_still)
            for name, r in results["idle_cpu"].items():
                print(f"{'idle_cpu ' + name:<34} {r['cpu_pct']:>9.2f} % CPU "
                      f"({r['cpu_s']:.3f} s in {r['still_s']:.0f} s)", flush=True)
    if not a.skip_frame and wants("frame_median"):
        ms = frame_median()
        results["frame_median"] = {"median_ms": round(ms, 2), "runs": 21}
        print(f"{'frame_median':<34} {ms:>9.2f} ms", flush=True)
    # Presentation mode on a 2,000-line document with 10 moves over 20 steps:
    # the take's geometry is computed once, not once per frame.
    if not a.skip_frame and wants("present_moves_frame"):
        ms = frame_median("render", "presentation_geometry::presentation_with_moves_reuses_the_geometry_every_frame",
                          "median presentation frame")
        results["present_moves_frame"] = {"median_ms": round(ms, 2), "runs": 21}
        print(f"{'present_moves_frame':<34} {ms:>9.2f} ms", flush=True)

    latest = {
        "target": target,
        "commit": commit(),
        "date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d %H:%M UTC"),
        "machine": f"{platform.system()} {platform.machine()}, {os.cpu_count()} CPUs",
        "binary_bytes": os.path.getsize(binary),
        "scenarios": results,
    }
    if a.install:
        dt, size = cold_install()
        latest["cargo_install_cold_s"] = round(dt, 1)
        print(f"{'cargo install (cold)':<34} {dt:>9.1f} s  (binary {size:,} bytes)", flush=True)
    else:
        print("cargo install (cold)               skipped (--install to time it; takes minutes)")

    os.makedirs(os.path.dirname(os.path.abspath(a.latest)), exist_ok=True)
    with open(a.latest, "w") as f:
        json.dump(latest, f, indent=2, ensure_ascii=False)
        f.write("\n")

    path = a.baseline
    baseline = json.load(open(path)) if os.path.exists(path) else {"warn_pct": WARN_PCT, "targets": {}}
    warn_pct = baseline.get("warn_pct", WARN_PCT)
    base = baseline["targets"].get(target)
    warned = 0
    if base is None:
        print(f"note: qa/bench/baseline.json has no entry for {target}; "
              f"store one with --update-baseline --note \"first baseline on …\"")
    else:
        print(f"vs baseline {base.get('commit', '?')} ({base.get('date', '?')}, {base.get('machine', '?')}):")
        for name, r in results.items():
            if "median_ms" not in r:
                continue  # idle_cpu: an absolute limit, below
            was = base.get("scenarios", {}).get(name, {}).get("median_ms")
            if not was:
                print(f"  {name:<32} new scenario")
                continue
            pct = (r["median_ms"] - was) / was * 100
            flag = ""
            if pct > warn_pct:
                flag = f"  WARN: more than {warn_pct} % slower"
                warned += 1
            print(f"  {name:<32} {was:>9.1f} → {r['median_ms']:>9.1f} ms  {pct:+6.1f} %{flag}")
        if a.install and base.get("cargo_install_cold_s"):
            was = base["cargo_install_cold_s"]
            print(f"  {'cargo install (cold)':<32} {was:>9.1f} → {latest['cargo_install_cold_s']:>9.1f} s  "
                  f"{(latest['cargo_install_cold_s'] - was) / was * 100:+6.1f} % (information)")

    # The CPU at rest has an absolute limit, and it fails: a loop that does
    # not sleep is a bug, and the CPU time of a process does not depend on
    # the load of the machine.
    limit = baseline.get("idle_cpu_max_pct", IDLE_MAX_PCT)
    over = [(n, r["cpu_pct"]) for n, r in results.get("idle_cpu", {}).items() if r["cpu_pct"] > limit]

    if a.update_baseline and over:
        print("baseline not updated: idle_cpu is over its limit")
    elif a.update_baseline:
        entry = dict(latest, note=a.note.strip())
        if not a.install and base and base.get("cargo_install_cold_s"):
            entry["cargo_install_cold_s"] = base["cargo_install_cold_s"]
        baseline["targets"][target] = entry
        with open(path, "w") as f:
            json.dump(baseline, f, indent=2, ensure_ascii=False)
            f.write("\n")
        print(f"baseline of {target} updated in qa/bench/baseline.json — commit it with the note")

    print(f"{warned} scenario(s) over +{warn_pct} %" if warned else "no scenario over the warning threshold")

    for name, pct in over:
        print(f"FAIL: idle_cpu {name}: {pct:.2f} % CPU at rest (limit {limit} %)")
    if over:
        sys.exit(1)
    if "idle_cpu" in results:
        print(f"idle_cpu: every case at or under {limit} % CPU at rest")


if __name__ == "__main__":
    main()
