#!/usr/bin/env python3
"""Checks the docs that a release reads, before the tag:

    scripts/release-check.py v0.1.3     # before the tag; release.yml runs it too
    scripts/release-check.py            # the version of Cargo.toml, no tag

- The status line of README.md, `> Status: **vX.Y.Z**`, gives the version
  of the workspace. A tag gives that version too: vX.Y.Z, or vX.Y.Z-rc.N
  for a prerelease of it.
- CONTRIBUTING.md has the section `## Releases (maintainers)`, and
  .github/workflows/release.yml links it. Each link `CONTRIBUTING.md#ANCHOR`
  of release.yml goes to a heading of CONTRIBUTING.md.
- The commands in the code blocks of that section come in their order.
  The last one, `cargo publish --workspace --locked`, publishes the crates:
  cargo puts them in dependency order. A line with `--dry-run` is not a
  step without `--dry-run`: a dry run does not publish.

It writes one line for each problem and exits 1. It needs python3 (3.9 or
later) and cargo, for `cargo metadata`. .github/workflows/release.yml and
scripts/release-local.sh run it.
"""
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.realpath(__file__)))
TAG = re.compile(r"v([0-9]+\.[0-9]+\.[0-9]+)(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?")
SECTION = "Releases (maintainers)"
# The commands of the release section, in the order that they must come.
STEPS = [
    "git status --short",
    "scripts/bump-version.sh X.Y.Z",
    "$EDITOR docs/release-notes/vX.Y.Z.md",
    "git add docs/release-notes/vX.Y.Z.md",
    'git commit -am "vX.Y.Z"',
    "cargo publish --workspace --dry-run --locked",
    "scripts/release-check.py vX.Y.Z",
    "git push origin main",
    "git tag vX.Y.Z && git push origin vX.Y.Z",
    "cargo login",
    "cargo publish --workspace --locked",
]
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})")

problems = []


def problem(text):
    problems.append(text)


def read(rel):
    """The lines of a file of the repository, without a byte order mark
    and with any line end. None, with a problem, when it cannot be read."""
    try:
        with open(os.path.join(ROOT, rel), "rb") as f:
            data = f.read()
    except OSError as e:
        problem(f"{rel}: {e.strerror}")
        return None
    try:
        text = data.decode("utf-8-sig")
    except UnicodeDecodeError as e:
        problem(f"{rel}: not UTF-8 (byte {e.start})")
        return None
    if not text.strip():
        problem(f"{rel} is empty")
        return None
    return text.splitlines()


def runs(line, step):
    """Whether the command of `line` (before a `#` comment) runs `step`.
    A dry run is not the step that it tries."""
    command = line.split("#", 1)[0]
    return step in command and ("--dry-run" in step or "--dry-run" not in command)


def blocks(lines):
    """The ATX headings outside a fenced code block, as (line number, level,
    text), and the line numbers inside a fenced code block."""
    heads, code, fence = [], set(), None
    for n, line in enumerate(lines, 1):
        if fence:
            t = line.strip()
            if t and set(t) == {fence[0]} and len(t) >= len(fence):
                fence = None
            else:
                code.add(n)
            continue
        m = FENCE.match(line)
        if m:
            fence = m.group(1)
            continue
        h = re.match(r"^ {0,3}(#{1,6})[ \t]+(.*?)(?:[ \t]+#+)?[ \t]*$", line)
        if h:
            heads.append((n, len(h.group(1)), h.group(2)))
    return heads, code


def anchor(heading):
    """The id that GitHub gives a heading."""
    return "".join(
        "-" if c == " " else c
        for c in heading.lower()
        if c == " " or c.isalnum() or c in "-_"
    )


def workspace():
    """The version of the tiza package."""
    try:
        out = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline"],
            cwd=ROOT, capture_output=True, text=True, check=False,
        )
    except OSError as e:
        sys.exit(f"release-check: cannot run cargo: {e.strerror}")
    if out.returncode != 0:
        sys.exit(f"release-check: cargo metadata failed:\n{out.stderr.strip()}")
    packages = json.loads(out.stdout)["packages"]
    version = next((p["version"] for p in packages if p["name"] == "tiza"), None)
    if version is None:
        sys.exit("release-check: the workspace has no package named tiza")
    return version


def check_status(version, tag):
    readme = read("README.md")
    if readme is None:
        return
    status = [(n, l) for n, l in enumerate(readme, 1) if l.startswith("> Status: **v")]
    if len(status) != 1:
        problem(f"README.md has {len(status)} status lines (`> Status: **vX.Y.Z**`), not one")
    for n, line in status:
        if not line.startswith(f"> Status: **v{version}**"):
            problem(f"README.md:{n}: the status line does not give v{version}, "
                    f"the version of Cargo.toml: {line}")
    if tag is not None and tag.group(1) != version:
        problem(f"the tag {tag.group(0)} is not version {version} of Cargo.toml: "
                "run scripts/bump-version.sh first")


def check_contributing():
    page = read("CONTRIBUTING.md")
    if page is None:
        return
    heads, code = blocks(page)
    ids = {anchor(text) for _, _, text in heads}
    workflow = read(".github/workflows/release.yml")
    links = []
    for n, line in enumerate(workflow or [], 1):
        for a in re.findall(r"CONTRIBUTING\.md#([A-Za-z0-9_-]+)", line):
            links.append(a)
            if a not in ids:
                problem(f".github/workflows/release.yml:{n}: CONTRIBUTING.md has no "
                        f"heading with the id #{a}")
    if workflow is not None and anchor(SECTION) not in links:
        problem(f".github/workflows/release.yml does not link "
                f"CONTRIBUTING.md#{anchor(SECTION)}")
    start = next((n for n, level, text in heads if level == 2 and text == SECTION), None)
    if start is None:
        problem(f"CONTRIBUTING.md has no section `## {SECTION}`")
        return
    end = next((n for n, level, _ in heads if level <= 2 and n > start), len(page) + 1)
    # The commands are the lines of the code blocks of the section.
    section = [(n, page[n - 1].strip()) for n in range(start + 1, end) if n in code]
    # The commands, in order: the first line that has each one.
    at = []
    for step in STEPS:
        n = next((n for n, l in section if runs(l, step)), None)
        if n is None:
            problem(f"CONTRIBUTING.md: the section `## {SECTION}` has no `{step}`")
        else:
            at.append((n, step))
    for (n1, s1), (n2, s2) in zip(at, at[1:]):
        if n2 < n1:
            problem(f"CONTRIBUTING.md:{n2}: `{s2}` comes before `{s1}` (line {n1})")


def main(args):
    if args in (["-h"], ["--help"]):
        print(__doc__.strip())
        return 0
    if len(args) > 1:
        print("usage: scripts/release-check.py [vX.Y.Z]", file=sys.stderr)
        return 2
    tag = None
    if args:
        tag = TAG.fullmatch(args[0])
        if tag is None:
            print(f"the tag is vX.Y.Z or vX.Y.Z-suffix (for example v0.1.0 or "
                  f"v0.1.0-rc.1), got '{args[0]}'", file=sys.stderr)
            return 2
    version = workspace()
    check_status(version, tag)
    check_contributing()
    for p in problems:
        print(f"release-check: {p}", file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
