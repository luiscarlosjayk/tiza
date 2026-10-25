#!/usr/bin/env python3
"""Size budget of the release binary and its download.

Builds `tiza` with `cargo build --release --locked -p tiza`, packs it exactly as
`.github/workflows/release.yml` does (binary + README + licenses + examples/ in
a folder, `tar -czf`), prints both sizes and fails when:

- the binary is over `limits.binary_bytes` (16 MB) or the archive over
  `limits.archive_bytes` (8 MB), or
- either grew more than `limits.growth_pct` (10 %) over the baseline of this
  target in `qa/budgets.json`.

Moving the baseline is a decision: `--update --reason "one line"` rewrites this
target's entry, and the check refuses a `qa/budgets.json` whose numbers differ
from the committed ones (`git show HEAD:qa/budgets.json`) while the reason is
still the committed one or is empty / longer than one line. Commit the file in
the same commit as the change that moved the size.

Usage: scripts/size-check.sh [--no-build] [--bin PATH] [--update --reason TEXT]
"""
import argparse, datetime, json, os, shutil, subprocess, sys, tarfile, tempfile, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BUDGETS = os.path.join(ROOT, "qa", "budgets.json")
MB = 1_000_000


def host_target():
    out = subprocess.run(["rustc", "-vV"], capture_output=True, text=True, check=True).stdout
    return next(line.split(": ", 1)[1] for line in out.splitlines() if line.startswith("host: "))


def commit():
    r = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, capture_output=True, text=True)
    return r.stdout.strip() or "unknown"


def build():
    env = dict(os.environ, CARGO_INCREMENTAL=os.environ.get("CARGO_INCREMENTAL", "0"))
    t = time.monotonic()
    subprocess.run(["cargo", "build", "--release", "--locked", "-p", "tiza"], cwd=ROOT, env=env, check=True)
    return time.monotonic() - t


def archive_size(binary, target):
    """The release workflow's package step, byte for byte the same layout."""
    with tempfile.TemporaryDirectory(prefix="tiza-size-") as tmp:
        name = f"tiza-vX.Y.Z-{target}"
        d = os.path.join(tmp, "dist", name)
        os.makedirs(d)
        shutil.copy2(binary, d)
        for f in ("README.md", "LICENSE-MIT", "LICENSE-APACHE",
                  "crates/tiza-export/assets/fonts/LICENSE-JetBrainsMono-OFL.txt",
                  "crates/tiza-export/assets/fonts/LICENSE-TizaSymbols-DejaVu.txt"):
            shutil.copy2(os.path.join(ROOT, f), d)
        shutil.copytree(os.path.join(ROOT, "examples"), os.path.join(d, "examples"))
        out = os.path.join(tmp, "dist", name + ".tar.gz")
        # Same tool and default level as `tar -C dist -czf`.
        subprocess.run(["tar", "-C", os.path.join(tmp, "dist"), "-czf", out, name], check=True)
        return os.path.getsize(out)


def committed_budgets():
    r = subprocess.run(["git", "show", "HEAD:qa/budgets.json"], cwd=ROOT, capture_output=True, text=True)
    return json.loads(r.stdout) if r.returncode == 0 else None


def fmt(n):
    return f"{n:,} bytes ({n / MB:.2f} MB)"


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--no-build", action="store_true", help="measure target/release/tiza as it is")
    ap.add_argument("--bin", help="measure this binary instead of target/release/tiza (implies --no-build)")
    ap.add_argument("--update", action="store_true", help="write the measured sizes as this target's baseline")
    ap.add_argument("--reason", default="", help="one line: why the baseline moves (required with --update)")
    ap.add_argument("--budgets", default=BUDGETS, help=argparse.SUPPRESS)  # tests: another budgets file
    ap.add_argument("--committed", help=argparse.SUPPRESS)  # tests: stands for `git show HEAD:qa/budgets.json`
    a = ap.parse_args()

    budgets = json.load(open(a.budgets))
    limits = budgets["limits"]
    target = host_target()
    binary = a.bin or os.path.join(ROOT, "target", "release", "tiza")
    build_s = None
    if not a.bin and not a.no_build:
        build_s = build()
    if not os.path.isfile(binary):
        sys.exit(f"size-check: no binary at {binary} (build it or drop --no-build)")
    size = os.path.getsize(binary)
    arch = archive_size(binary, target)

    print(f"target   {target} @ {commit()}")
    print(f"binary   {fmt(size)}")
    print(f"archive  {fmt(arch)}  (tar.gz as the release workflow packs it)")
    if build_s is not None:
        print(f"build    {build_s:.1f} s (cargo build --release, incremental cache as it is)")

    if a.update:
        reason = a.reason.strip()
        if not reason or "\n" in reason:
            sys.exit("size-check: --update needs --reason with one line saying why the size moved")
        entry = budgets["targets"].get(target, {})
        entry.update(
            binary_bytes=size,
            archive_bytes=arch,
            commit=commit(),
            measured=datetime.date.today().isoformat(),
            reason=reason,
        )
        budgets["targets"][target] = entry
        with open(a.budgets, "w") as f:
            json.dump(budgets, f, indent=2, ensure_ascii=False)
            f.write("\n")
        print(f"baseline of {target} updated in qa/budgets.json — commit it with the change")

    fails = []
    if size > limits["binary_bytes"]:
        fails.append(f"binary {size / MB:.2f} MB is over the {limits['binary_bytes'] / MB:.0f} MB budget")
    if arch > limits["archive_bytes"]:
        fails.append(f"archive {arch / MB:.2f} MB is over the {limits['archive_bytes'] / MB:.0f} MB budget")

    base = budgets["targets"].get(target)
    growth = limits["growth_pct"] / 100
    if base is None:
        print(f"note: qa/budgets.json has no baseline for {target}; "
              f"record one with --update --reason \"first baseline on {target}\"")
    else:
        for what, now, was in (("binary", size, base["binary_bytes"]), ("archive", arch, base["archive_bytes"])):
            pct = (now - was) / was * 100
            print(f"{what:<8} {pct:+.1f} % vs baseline {was:,} ({base.get('commit', '?')}, {base.get('measured', '?')})")
            if now > was * (1 + growth):
                fails.append(f"{what} grew {pct:.1f} % over the baseline in qa/budgets.json "
                             f"(limit {limits['growth_pct']} %): update it with --update --reason \"…\" "
                             f"in the same commit if the growth is intended")
        committed = json.load(open(a.committed)) if a.committed else committed_budgets()
        old = (committed or {}).get("targets", {}).get(target)
        moved = old is not None and (old.get("binary_bytes"), old.get("archive_bytes")) != (
            base["binary_bytes"], base["archive_bytes"])
        reason = str(base.get("reason", "")).strip()
        if moved and (not reason or "\n" in reason or reason == str(old.get("reason", "")).strip()):
            fails.append(f"the baseline of {target} in qa/budgets.json changed without a new one-line reason")

    if fails:
        for f in fails:
            print(f"FAIL: {f}")
        sys.exit(1)
    print(f"OK (budget {limits['binary_bytes'] / MB:.0f} MB / {limits['archive_bytes'] / MB:.0f} MB, "
          f"growth ≤ {limits['growth_pct']} % over the baseline)")


if __name__ == "__main__":
    main()
