#!/usr/bin/env python3
"""Prepare the Zola content of the docs site from README.md and docs/.

The pages in docs/ stay plain GitHub Markdown: no front matter, no shortcodes.
This script copies the pages that nav.json lists into content/, and:

- takes the title from the H1 of the page and removes that H1,
- adds the front matter (title, path, weight, source),
- writes the _index.md of each section,
- gives every heading the same id as GitHub ({#slug}), so #anchor links work,
- rewrites relative links: a page on the site becomes an @/ internal link,
  an image becomes a site path, anything else goes to the file on GitHub;
  a link to a file that does not exist, or to a path outside the repo, is an
  error,
- serves a raw.githubusercontent.com file of this repo from the site, so
  the site makes no request to another host. A file at a release tag
  vX.Y.Z comes from git at that tag, so the notes of each release show
  the image of that release; the tag must be in the clone. A file at
  another ref, or at a tag that does not exist yet, is an asset of the
  tree,
- turns a code block with the info string "text prompt" into a prompt
  block (<pre class="prompt">); site.js gives it the title "Ask your agent"
  and a "Copy prompt" button. On GitHub it stays a plain text block,
- puts each table in a <div class="table-wrap">, so a wide table scrolls in
  its own box and the page does not scroll sideways,
- copies the images and the favicon into static/,
- lists the release notes by itself: an entry "docs/release-notes/" in
  nav.json becomes the notes of every release vX.Y.Z, the newest first,
- writes release.toml: the version of the workspace Cargo.toml and the git
  ref that the site describes, for the header and the source links.

The site describes one ref: the release tag that CI deploys, or main in a
build that CI only checks. Links to other files of the repo and "View the
source" go to that ref on GitHub.

Python 3.9+ standard library only. Usage:
    python3 build.py [--repo REPO_ROOT] [--ref REF]
REPO_ROOT defaults to the parent folder of this script (site/ in the repo).
REF defaults to main.
"""

import argparse
import html
import json
import os
import posixpath
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))

FENCE_RE = re.compile(r"^(\s*)(`{3,}|~{3,})")
HEADING_RE = re.compile(r"^(#{1,6})\s+(.*?)\s*#*\s*$")
# The delimiter row under the header row of a table: | --- | :--: |
TABLE_DELIM_RE = re.compile(r"^\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$")
TABLE_OPEN = '<div class="table-wrap">'
# The info string of a prompt block: the text that a reader gives an agent.
PROMPT_INFO = "text prompt"
# A URL in a prompt: the site shows it in the code font.
PROMPT_URL_RE = re.compile(r"https?://[^\s<]+")
# A release tag: a raw file at such a tag comes from git.
TAG_RE = re.compile(r"^v\d+\.\d+\.\d+$")
# [text](target "title") ; the target has no spaces and no parentheses.
MD_LINK_RE = re.compile(r"(\]\()([^)\s]+)((?:\s+\"[^\"]*\")?\))")
HTML_ATTR_RE = re.compile(r"""(\s(?:src|href)=")([^"]+)(")""")


class BuildError(Exception):
    pass


def workspace_version(repo):
    """The version in [workspace.package] of Cargo.toml, as bump-version.sh writes it."""
    section = None
    with open(os.path.join(repo, "Cargo.toml"), encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if line.startswith("["):
                section = line
                continue
            m = re.match(r'^version = "([^"]+)"$', line)
            if m and section == "[workspace.package]":
                return m.group(1)
    raise BuildError("no version in [workspace.package] of Cargo.toml")


def gh_slug(text):
    """The heading id that GitHub makes for this heading text."""
    t = re.sub(r"!?\[([^\]]*)\]\([^)]*\)", r"\1", text)  # links and images: keep the text
    t = t.replace("`", "")
    t = re.sub(r"(\*\*|__)(.+?)\1", r"\2", t)
    t = re.sub(r"(?<!\w)[*_](.+?)[*_](?!\w)", r"\1", t)
    t = re.sub(r"<[^>]+>", "", t)  # inline HTML tags
    t = t.strip().lower()
    t = re.sub(r"[^\w\- ]", "", t)
    return t.replace(" ", "-")


def split_code_spans(line):
    """Split a line into (is_code, text) parts, so links inside `code` stay as they are."""
    parts = []
    i = 0
    while i < len(line):
        if line[i] == "`":
            n = len(line[i:]) - len(line[i:].lstrip("`"))
            ticks = line[i:i + n]
            end = line.find(ticks, i + n)
            if end != -1:
                parts.append((True, line[i:end + n]))
                i = end + n
                continue
            parts.append((False, ticks))
            i += n
            continue
        j = line.find("`", i)
        j = len(line) if j == -1 else j
        parts.append((False, line[i:j]))
        i = j
    return parts


class Site:
    def __init__(self, repo, site, nav, version, ref):
        self.repo = repo
        self.version = version
        self.ref = ref
        self.site = site
        self.nav = nav
        self.repo_url = nav["repo_url"].rstrip("/")
        self.raw_prefix = self.repo_url.replace(
            "https://github.com/", "https://raw.githubusercontent.com/", 1) + "/"
        # The branch that the absolute GitHub links of the README name.
        self.branch = nav["branch"]
        # repo path of a page -> content path (for @/ links)
        self.pages = {nav["home"]: "_index.md"}
        for sec in nav["sections"]:
            for p in sec["pages"]:
                self.pages[p] = "%s/%s" % (sec["id"], posixpath.basename(p))
        self.assets = dict(nav.get("assets", {}))
        # The frames of the reference pages: every file in docs/images/ is an asset.
        for root, _, files in os.walk(os.path.join(repo, "docs", "images")):
            for name in files:
                rel = os.path.relpath(os.path.join(root, name), repo).replace(os.sep, "/")
                self.assets.setdefault(rel, rel[len("docs/"):])
        # (tag, path) -> site path of a raw file at a release tag, from git.
        self.tag_assets = {}
        self.warnings = []
        self.where = ""  # file:line of the line that convert() reads

    # -- links ---------------------------------------------------------
    def github_url(self, repo_path, anchor):
        if repo_path == ".." or repo_path.startswith(("../", "/")):
            raise BuildError("%s: the link goes outside the repo: %s" % (self.where, repo_path))
        full = os.path.join(self.repo, repo_path)
        kind = "tree" if os.path.isdir(full) else "blob"
        if not os.path.exists(full):
            raise BuildError("%s: the link goes to a file that does not exist: %s"
                             % (self.where, repo_path))
        return "%s/%s/%s/%s%s" % (self.repo_url, kind, self.ref, repo_path, anchor)

    def rewrite_target(self, target, src):
        if target.startswith(("mailto:", "#")):
            return target
        prefix = "%s/blob/%s/" % (self.repo_url, self.branch)
        tree_prefix = "%s/tree/%s/" % (self.repo_url, self.branch)
        if target.startswith(prefix) or target.startswith(tree_prefix):
            rest = target.split("/%s/" % self.branch, 1)[1]
            path, _, frag = rest.partition("#")
            if path in self.pages:
                return "@/%s%s" % (self.pages[path], "#" + frag if frag else "")
            return target
        if target.startswith(self.raw_prefix):
            # raw.githubusercontent.com/OWNER/REPO/REF/PATH: serve the file from the site.
            ref, _, rest = target[len(self.raw_prefix):].partition("/")
            path = rest.partition("#")[0]
            if TAG_RE.match(ref) and ref != self.ref:
                if self.has_tag(ref):
                    dst = "releases/%s/%s" % (ref, path)
                    self.tag_assets[(ref, path)] = dst
                    return "/" + dst
                if path not in self.assets:
                    raise BuildError("%s: %s is at the tag %s, and this clone does not have "
                                     "the tag. Fetch the tags: git fetch --tags"
                                     % (self.where, path, ref))
                self.warnings.append("%s: the tag %s is not in this clone: the site serves "
                                     "%s of the tree" % (self.where, ref, path))
            if path in self.assets:
                return "/" + self.assets[path]
            return target
        if re.match(r"^[a-z][a-z0-9+.-]*:", target):
            return target
        path, _, frag = target.partition("#")
        anchor = "#" + frag if frag else ""
        repo_path = posixpath.normpath(posixpath.join(posixpath.dirname(src), path)).rstrip("/")
        if repo_path in self.pages:
            return "@/%s%s" % (self.pages[repo_path], anchor)
        if repo_path in self.assets:
            return "/" + self.assets[repo_path]
        return self.github_url(repo_path, anchor)

    def has_tag(self, tag):
        r = subprocess.run(["git", "-C", self.repo, "rev-parse", "--verify", "--quiet",
                            "refs/tags/%s^{commit}" % tag], capture_output=True)
        return r.returncode == 0

    def read_at_tag(self, tag, path):
        r = subprocess.run(["git", "-C", self.repo, "show", "%s:%s" % (tag, path)],
                           capture_output=True)
        if r.returncode != 0:
            raise BuildError("the tag %s has no file %s: %s"
                             % (tag, path, r.stderr.decode(errors="replace").strip()))
        return r.stdout

    def rewrite_links(self, text, src):
        out = []
        for is_code, part in split_code_spans(text):
            if not is_code:
                part = MD_LINK_RE.sub(
                    lambda m: m.group(1) + self.rewrite_target(m.group(2), src) + m.group(3), part)
                part = HTML_ATTR_RE.sub(
                    lambda m: m.group(1) + self.html_target(m.group(2), src) + m.group(3), part)
            out.append(part)
        return "".join(out)

    def html_target(self, target, src):
        t = self.rewrite_target(target, src)
        # Raw HTML is not parsed by Zola for @/ links: give it the final URL.
        if t.startswith("@/"):
            path, _, frag = t[2:].partition("#")
            t = "/" + self.url_of(path) + ("#" + frag if frag else "")
        return t

    def url_of(self, content_path):
        for repo_path, cp in self.pages.items():
            if cp == content_path:
                return page_path(repo_path)
        return ""

    # -- pages ---------------------------------------------------------
    def convert(self, src):
        with open(os.path.join(self.repo, src), encoding="utf-8") as f:
            lines = f.read().split("\n")
        title = None
        seen = {}
        out = []
        fence = None
        prompt = None  # the lines of a prompt block, while inside one
        table = False  # inside a table that has its wrapper open
        for n, line in enumerate(lines, 1):
            self.where = "%s:%d" % (src, n)
            if table and not line.strip():
                out.extend(["", "</div>"])
                table = False
            m = FENCE_RE.match(line)
            if fence:
                closes = m and m.group(2)[0] == fence[0] and len(m.group(2)) >= len(fence) \
                    and line.strip() == m.group(2)
                if prompt is not None:
                    if not closes:
                        prompt.append(line)
                        continue
                    # One HTML block: <pre> ends only at </pre>, so blank
                    # lines in the prompt do not end it.
                    # One block per line: a long URL that wraps keeps its
                    # lines close, and the lines of the prompt keep space
                    # between them. site.js copies the text with innerText,
                    # which gives one line for each block.
                    text = "".join(
                        '<span class="prompt-line">%s</span>' % PROMPT_URL_RE.sub(
                            # <wbr> lets a long URL wrap after a "/".
                            lambda u: '<span class="prompt-url">%s</span>'
                            % u.group(0).replace("/", "/<wbr>"),
                            html.escape(l, quote=False))
                        for l in prompt)
                    out.append('<pre class="prompt"><code>%s</code></pre>' % text)
                    prompt = None
                else:
                    out.append(line)
                if closes:
                    fence = None
                continue
            if m:
                fence = m.group(2)
                if line[m.end():].strip() == PROMPT_INFO:
                    if m.group(1):
                        raise BuildError("%s: a prompt block must not be indented" % self.where)
                    prompt = []
                    continue
                out.append(line)
                continue
            # A table starts at a header row with "|", followed by a delimiter row.
            # The HTML lines need blank lines around them, so that the table
            # between them is still Markdown.
            if not table and "|" in line and not line[:1].isspace() and n < len(lines) \
                    and "|" in lines[n] and TABLE_DELIM_RE.match(lines[n]):
                out.extend([TABLE_OPEN, ""])
                table = True
            h = HEADING_RE.match(line)
            if h:
                level, text = len(h.group(1)), h.group(2)
                if level == 1 and title is None:
                    title = text
                    continue
                slug = gh_slug(text)
                n = seen.get(slug, 0)
                seen[slug] = n + 1
                if n:
                    slug = "%s-%d" % (slug, n)
                out.append("%s %s {#%s}" % (h.group(1), self.rewrite_links(text, src), slug))
                continue
            out.append(self.rewrite_links(line, src))
        if table:
            out.extend(["", "</div>"])
        body = "\n".join(out).lstrip("\n")
        return title, body


def page_path(repo_path):
    """The URL path of a page: its path under docs/, without .md."""
    p = repo_path[len("docs/"):] if repo_path.startswith("docs/") else repo_path
    return p[:-3] if p.endswith(".md") else p


def toml_str(s):
    return json.dumps(s, ensure_ascii=False)


def plain_title(md):
    return re.sub(r"`", "", md)


RELEASE_NOTE_RE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)\.md$")


def expand_release_notes(repo, nav):
    """Replace a "docs/release-notes/" entry of nav.json with the notes of
    each release vX.Y.Z, the newest first. The notes of a prerelease
    (vX.Y.Z-rc.N) are not on the site: the site shows releases only."""
    folder = "docs/release-notes/"
    for sec in nav["sections"]:
        if folder not in sec["pages"]:
            continue
        names = os.listdir(os.path.join(repo, folder))
        found = [(tuple(int(n) for n in m.groups()), name)
                 for name in names for m in [RELEASE_NOTE_RE.match(name)] if m]
        notes = [folder + name for _, name in sorted(found, reverse=True)]
        i = sec["pages"].index(folder)
        sec["pages"][i:i + 1] = notes
        nav.setdefault("skip", []).append(folder)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=os.path.dirname(HERE))
    ap.add_argument("--ref", default="main")
    args = ap.parse_args()
    repo = os.path.abspath(args.repo)
    with open(os.path.join(HERE, "nav.json"), encoding="utf-8") as f:
        nav = json.load(f)
    try:
        expand_release_notes(repo, nav)
        version = workspace_version(repo)
        build(repo, nav, version, args.ref)
    except BuildError as e:
        print("build.py: error: %s" % e, file=sys.stderr)
        sys.exit(1)


def build(repo, nav, version, ref):
    site = Site(repo, HERE, nav, version, ref)

    content = os.path.join(HERE, "content")
    shutil.rmtree(content, ignore_errors=True)
    os.makedirs(content)

    # Home: the README.
    _, body = site.convert(nav["home"])
    with open(os.path.join(content, "_index.md"), "w", encoding="utf-8") as f:
        f.write("+++\ntitle = %s\ntemplate = \"index.html\"\n\n[extra]\nsource = %s\n+++\n\n%s"
                % (toml_str(nav["home_title"]), toml_str(nav["home"]), body))

    for si, sec in enumerate(nav["sections"]):
        d = os.path.join(content, sec["id"])
        os.makedirs(d)
        with open(os.path.join(d, "_index.md"), "w", encoding="utf-8") as f:
            f.write("+++\ntitle = %s\nweight = %d\nsort_by = \"weight\"\ntemplate = \"section.html\"\n+++\n"
                    % (toml_str(sec["title"]), si + 1))
        for pi, src in enumerate(sec["pages"]):
            title, body = site.convert(src)
            if not title:
                site.warnings.append("no H1 in %s" % src)
                title = posixpath.basename(src)[:-3]
            fm = [
                "+++",
                "title = %s" % toml_str(plain_title(title)),
                "path = %s" % toml_str(page_path(src)),
                "weight = %d" % (pi + 1),
                "",
                "[extra]",
                "title_md = %s" % toml_str(title),
                "source = %s" % toml_str(src),
                "+++",
                "",
            ]
            with open(os.path.join(d, posixpath.basename(src)), "w", encoding="utf-8") as f:
                f.write("\n".join(fm) + body)

    # The version and the ref, for the header and the source links.
    with open(os.path.join(HERE, "release.toml"), "w", encoding="utf-8") as f:
        f.write("# Written by build.py. Do not edit.\nversion = %s\nref = %s\n"
                % (toml_str(version), toml_str(ref)))

    # Assets.
    static = os.path.join(HERE, "static")
    for src, dst in site.assets.items():
        out = os.path.join(static, dst)
        os.makedirs(os.path.dirname(out), exist_ok=True)
        shutil.copyfile(os.path.join(repo, src), out)
    for (tag, path), dst in site.tag_assets.items():
        out = os.path.join(static, dst)
        os.makedirs(os.path.dirname(out), exist_ok=True)
        with open(out, "wb") as f:
            f.write(site.read_at_tag(tag, path))

    # Pages in docs/ that are neither on the site nor skipped.
    listed = set(site.pages)
    skip = tuple(nav.get("skip", []))
    for root, _, files in os.walk(os.path.join(repo, "docs")):
        for name in files:
            rel = os.path.relpath(os.path.join(root, name), repo).replace(os.sep, "/")
            if rel.endswith(".md") and rel not in listed and not rel.startswith(skip):
                site.warnings.append("page not on the site: %s (add it to nav.json or to skip)" % rel)

    for w in site.warnings:
        print("build.py: warning: " + w, file=sys.stderr)
    print("build.py: %d pages in %d sections" % (len(site.pages), len(nav["sections"])))


if __name__ == "__main__":
    main()
