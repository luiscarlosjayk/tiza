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
- checks a link to a file that is not on the site, relative or a GitHub URL
  at the branch: the file must exist and, for a .md file, the #anchor must
  be one of its heading ids, as GitHub gives them. The site has no page for
  that file, so the link check of the site cannot see it,
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
- gives the navigation a short title and a group of pages under a page,
  when nav.json has them (see section_pages),
- lists the release notes by itself: an entry "docs/release-notes/" in
  nav.json becomes the page "Release notes", with the notes of the newest
  release vX.Y.Z and links to the earlier ones. The notes of each release
  also have their own page, /release-notes/vX.Y.Z/, which the navigation
  does not show,
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
# An id that raw HTML gives an element: a link can go to it too.
ID_ATTR_RE = re.compile(r'\sid="([^"]+)"')


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


def closes_fence(fence, m, line):
    """True when line (with its FENCE_RE match m) ends the fenced block
    that the fence opened."""
    return bool(m) and m.group(2)[0] == fence[0] and len(m.group(2)) >= len(fence) \
        and line.strip() == m.group(2)


def section_pages(sec):
    """The pages of a section of nav.json, in order, as (repo path, short
    title, parent). An entry is a path, or an object:
    {"page": PATH, "nav_title": TITLE, "pages": [ENTRY, ...]}. The navigation
    shows TITLE for the page instead of its title, and the pages of "pages"
    in a group under the page. A page in a group has no group of its own."""
    out = []
    for e in sec["pages"]:
        e = {"page": e} if isinstance(e, str) else e
        out.append((e["page"], e.get("nav_title"), None))
        for c in e.get("pages", []):
            c = {"page": c} if isinstance(c, str) else c
            if "pages" in c:
                raise BuildError("nav.json: %s: a group in a group" % c["page"])
            out.append((c["page"], c.get("nav_title"), e["page"]))
    return out


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
        self.notes = []
        for sec in nav["sections"]:
            for p, _, _ in sec["entries"]:
                if p == RELEASE_NOTES:
                    self.notes = release_notes(repo)
                    self.pages[p.rstrip("/")] = "%s/release-notes.md" % sec["id"]
                    for n in self.notes:
                        self.pages[n] = "%s/%s" % (NOTES_SECTION, posixpath.basename(n))
                else:
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
        self.ids = {}  # repo path of a .md file -> its heading ids

    # -- links ---------------------------------------------------------
    def github_url(self, repo_path, anchor):
        if repo_path == ".." or repo_path.startswith(("../", "/")):
            raise BuildError("%s: the link goes outside the repo: %s" % (self.where, repo_path))
        full = os.path.join(self.repo, repo_path)
        kind = "tree" if os.path.isdir(full) else "blob"
        if not os.path.exists(full):
            raise BuildError("%s: the link goes to a file that does not exist: %s"
                             % (self.where, repo_path))
        if anchor and kind == "blob" and repo_path.endswith(".md") \
                and anchor[1:] not in self.heading_ids(repo_path):
            raise BuildError("%s: the link goes to a heading that does not exist: %s%s"
                             % (self.where, repo_path, anchor))
        return "%s/%s/%s/%s%s" % (self.repo_url, kind, self.ref, repo_path, anchor)

    def heading_ids(self, repo_path):
        """The ids that GitHub gives in a .md file: the slug of each heading
        outside a fenced block, the H1 too, with -1, -2 for a slug that comes
        again; and each id="..." of its HTML."""
        if repo_path not in self.ids:
            with open(os.path.join(self.repo, repo_path), encoding="utf-8") as f:
                lines = f.read().split("\n")
            ids = set()
            seen = {}
            fence = None
            for line in lines:
                m = FENCE_RE.match(line)
                if fence:
                    if closes_fence(fence, m, line):
                        fence = None
                    continue
                if m:
                    fence = m.group(2)
                    continue
                ids.update(ID_ATTR_RE.findall(line))
                h = HEADING_RE.match(line)
                if h:
                    slug = gh_slug(h.group(2))
                    n = seen.get(slug, 0)
                    seen[slug] = n + 1
                    ids.add("%s-%d" % (slug, n) if n else slug)
            self.ids[repo_path] = ids
        return self.ids[repo_path]

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
            # Not on the site: the file and its heading must exist.
            self.github_url(path.rstrip("/"), "#" + frag if frag else "")
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
    def convert(self, src, shift=0):
        """The title (the first H1) and the Zola Markdown of a page. shift
        moves each other heading down that many levels, to put the page
        under a heading of another page."""
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
                closes = closes_fence(fence, m, line)
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
                out.append("%s %s {#%s}" % ("#" * min(level + shift, 6),
                                            self.rewrite_links(text, src), slug))
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
# The entry of nav.json for the page "Release notes".
RELEASE_NOTES = "docs/release-notes/"
# The section of the pages of each release: it is not in the navigation
# (config.toml, nav_sections) and has no page of its own.
NOTES_SECTION = "notes"


def release_notes(repo):
    """The notes of each release vX.Y.Z, the newest first. The notes of a
    prerelease (vX.Y.Z-rc.N) are not on the site: the site shows releases only."""
    names = os.listdir(os.path.join(repo, RELEASE_NOTES))
    found = [(tuple(int(n) for n in m.groups()), name)
             for name in names for m in [RELEASE_NOTE_RE.match(name)] if m]
    if not found:
        raise BuildError("%s has no notes vX.Y.Z.md" % RELEASE_NOTES)
    return [RELEASE_NOTES + name for _, name in sorted(found, reverse=True)]


def write_page(path, src, title, url_path, weight, body, search=True, **extra):
    """Write a page of the site with its front matter. extra: more keys of
    [extra]; a key with the value None is left out."""
    fm = [
        "+++",
        "title = %s" % toml_str(plain_title(title)),
        "path = %s" % toml_str(url_path),
        "weight = %d" % weight,
    ]
    if not search:
        fm.append("in_search_index = false")
    fm += [
        "",
        "[extra]",
        "title_md = %s" % toml_str(title),
        "source = %s" % toml_str(src),
    ]
    fm += ["%s = %s" % (k, toml_str(v)) for k, v in extra.items() if v is not None]
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(fm + ["+++", ""]) + body)


def write_release_notes(site, content, path, weight):
    """Write the page "Release notes" to path: the notes of the newest
    release, then links to the notes of the earlier ones. Write the notes of
    each release to its own page too, in NOTES_SECTION."""
    d = os.path.join(content, NOTES_SECTION)
    os.makedirs(d)
    with open(os.path.join(d, "_index.md"), "w", encoding="utf-8") as f:
        f.write("+++\ntitle = \"Release notes\"\nrender = false\nsort_by = \"weight\"\n+++\n")
    titles = []
    for i, src in enumerate(site.notes):
        title, body = site.convert(src)
        titles.append(title)
        # The notes of the newest release are in the search once: on "Release notes".
        write_page(os.path.join(d, posixpath.basename(src)), src, title,
                   page_path(src), i + 1, body, search=i > 0)
    newest, earlier = site.notes[0], site.notes[1:]
    _, body = site.convert(newest, shift=1)
    lines = []
    if earlier:
        lines += ["For the notes of an earlier release, see [Earlier releases](#earlier-releases).", ""]
    lines += ["## %s {#%s}" % (titles[0], gh_slug(titles[0])), "", body.rstrip("\n")]
    if earlier:
        lines += ["", "## Earlier releases {#earlier-releases}", ""]
        lines += ["- [%s](@/%s)" % (t, site.pages[src]) for src, t in zip(earlier, titles[1:])]
    url_path = page_path(RELEASE_NOTES.rstrip("/"))
    # nav_prefix: the navigation marks "Release notes" on the page of each release too.
    write_page(path, newest, "Release notes", url_path, weight, "\n".join(lines) + "\n",
               nav_prefix="/%s/" % url_path)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=os.path.dirname(HERE))
    ap.add_argument("--ref", default="main")
    args = ap.parse_args()
    repo = os.path.abspath(args.repo)
    with open(os.path.join(HERE, "nav.json"), encoding="utf-8") as f:
        nav = json.load(f)
    try:
        for sec in nav["sections"]:
            sec["entries"] = section_pages(sec)
            if any(p == RELEASE_NOTES for p, _, _ in sec["entries"]):
                # The notes of a prerelease are not on the site.
                nav.setdefault("skip", []).append(RELEASE_NOTES)
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
        for pi, (src, nav_title, parent) in enumerate(sec["entries"]):
            if src == RELEASE_NOTES:
                write_release_notes(site, content, os.path.join(d, "release-notes.md"), pi + 1)
                continue
            title, body = site.convert(src)
            if not title:
                site.warnings.append("no H1 in %s" % src)
                title = posixpath.basename(src)[:-3]
            # The navigation shows the pages of a group under this page.
            group = any(up == src for _, _, up in sec["entries"])
            write_page(os.path.join(d, posixpath.basename(src)), src, title, page_path(src),
                       pi + 1, body, nav_title=nav_title, parent=parent, group=group or None)

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
