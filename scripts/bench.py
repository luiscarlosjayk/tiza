#!/usr/bin/env python3
"""Speed baseline of the release binary.

Times fixed scenarios with `target/release/tiza` — one warm-up run, then the
median of N runs — writes `qa/bench/latest.json` and compares it with this
target's entry in `qa/bench/baseline.json`, printing a WARN line for every
scenario more than 20 % slower. It never fails on speed: machines differ and a
laptop is noisy, so the numbers are a signal for a human, not a gate.

Scenarios: `--version`; `render` of step 7 of the ownership demo as it was
when the baseline was taken (qa/fixtures/ownership/ownership.rs.tiza) to
.txt and .png; a generated 2,000-line Rust file and a generated large
TypeScript file to .txt; the native GIF of the example; the TUI's first frame
on a pseudo-terminal (qa/src/pty.py: non-blocking reads, a deadline, the
child killed at the end); the qa crate's frame median (2,000 lines + 200
annotations, laid out and drawn into 120x40, from `cargo test --release`);
and the same in presentation mode with 10 moves over 20 steps
(`present_moves_frame`, from the qa crate's presentation frame test).

`--install` also times a cold `cargo install --path crates/tiza-cli --root
<tmp>` (fresh target dir, so every crate compiles) and records it as
information. `--update-baseline --note "why"` stores the run as this target's
baseline (do it on purpose, with the note, in the commit that explains it).

Usage: scripts/bench.sh [--runs N] [--no-build] [--skip-frame] [--install]
                        [--update-baseline --note TEXT]
"""
import argparse, datetime, json, os, platform, re, shutil, statistics, subprocess, sys, tempfile, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BENCH = os.path.join(ROOT, "qa", "bench")
EXAMPLE = os.path.join(ROOT, "qa", "fixtures", "ownership", "ownership.rs.tiza")
WARN_PCT = 20


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
env = dict(os.environ, TIZA_CONFIG="/nonexistent/config.toml")
t0 = time.monotonic()
s = Pty([binary, doc, "--no-socket", "--no-save"], 40, 120, env=env, ctty=True, raw=True)
ok = s.wait_for(lambda out: b"\x1b[?1049h" in out and "takes(s);" in plain(out), 10, "first frame")
t1 = time.monotonic()
s.finish(0.0)
print("FIRST_FRAME", (t1 - t0) if ok else -1, flush=True)
"""


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
    env = dict(os.environ, TIZA_PTY_DEADLINE="20")
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


def frame_median(test="t-054", name="ac1_a_frame_lays_out_once_and_does_not_highlight", label="median frame"):
    """A qa test's own measurement: median of 21 frames, release build of the
    qa crate (the authoring and the presentation frame tests)."""
    r = subprocess.run(
        ["cargo", "test", "--release", "--locked", "-p", "tiza-qa", "--test", test, "--",
         name, "--nocapture"],
        cwd=ROOT, env=cargo_env(), capture_output=True, text=True, timeout=3600)
    m = re.search(label + r": ([\d.]+)(ns|µs|us|ms|s)\b", r.stderr + r.stdout)
    if r.returncode != 0 or not m:
        raise RuntimeError(f"qa/tests/{test}.rs failed or printed no median:\n{(r.stderr + r.stdout)[-1500:]}")
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
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--runs", type=int, default=7, help="timed runs per scenario after one warm-up (default 7)")
    ap.add_argument("--no-build", action="store_true", help="use target/release/tiza as it is")
    ap.add_argument("--skip-frame", action="store_true", help="skip the frame medians (they need a release build of the qa crate)")
    ap.add_argument("--install", action="store_true", help="also time a cold `cargo install --path crates/tiza-cli`")
    ap.add_argument("--update-baseline", action="store_true", help="store this run as the target's baseline")
    ap.add_argument("--note", default="", help="one line: why the baseline moves (required with --update-baseline)")
    ap.add_argument("--bin", help="time this binary instead of target/release/tiza (implies --no-build)")
    ap.add_argument("--only", help="comma-separated scenario names (default: all)")
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
    if not a.skip_frame and wants("t054_frame_median"):
        ms = frame_median()
        results["t054_frame_median"] = {"median_ms": round(ms, 2), "runs": 21}
        print(f"{'t054_frame_median':<34} {ms:>9.2f} ms", flush=True)
    # Presentation mode on a 2,000-line document with 10 moves over 20 steps:
    # the take's geometry is computed once, not once per frame.
    if not a.skip_frame and wants("present_moves_frame"):
        ms = frame_median("t-191", "ac6_presentation_with_moves_reuses_the_geometry_every_frame",
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

    if a.update_baseline:
        entry = dict(latest, note=a.note.strip())
        if not a.install and base and base.get("cargo_install_cold_s"):
            entry["cargo_install_cold_s"] = base["cargo_install_cold_s"]
        baseline["targets"][target] = entry
        with open(path, "w") as f:
            json.dump(baseline, f, indent=2, ensure_ascii=False)
            f.write("\n")
        print(f"baseline of {target} updated in qa/bench/baseline.json — commit it with the note")

    print(f"{warned} scenario(s) over +{warn_pct} %" if warned else "no scenario over the warning threshold")


if __name__ == "__main__":
    main()
