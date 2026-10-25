#!/usr/bin/env python3
"""Check that every link inside the built site goes to a page and an id that exist.

Usage: python3 check_links.py PUBLIC_DIR
Links to other hosts are not checked. A file that the browser loads (an
image, a script, a style sheet) must come from the site itself: the site
makes no request to another host. Exit status 1 on a broken link or on a
request to another host.
"""

import os
import sys
from html.parser import HTMLParser
from urllib.parse import urlsplit, unquote

# The tags whose src (or, for <link>, href) the browser loads with the page.
LOADS = ("img", "script", "link", "iframe", "source", "video", "audio", "embed", "track")
# <link rel=...> values that are not loaded.
NOT_LOADED = ("canonical", "alternate", "next", "prev")


class Collect(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids = set()
        self.links = []
        self.loads = []

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if "id" in a:
            self.ids.add(a["id"])
        for key in ("href", "src"):
            if a.get(key) and not (tag == "link" and a.get("rel") == "canonical"):
                self.links.append(a[key])
        if tag in LOADS:
            key = "href" if tag == "link" else "src"
            if a.get(key) and not (tag == "link" and a.get("rel") in NOT_LOADED):
                self.loads.append((tag, key, a[key]))


def main():
    public = os.path.abspath(sys.argv[1])
    pages = {}
    for root, _, files in os.walk(public):
        for name in files:
            if name.endswith(".html"):
                p = os.path.join(root, name)
                c = Collect()
                with open(p, encoding="utf-8") as f:
                    c.feed(f.read())
                pages[p] = c
    hosts = set()
    broken = []
    for p, c in pages.items():
        for tag, key, url in c.loads:
            host = urlsplit(url).netloc
            if host and not (host.startswith("127.0.0.1") or host == "tiza.luisk.dev"):
                broken.append("%s: <%s %s=%s> (a request to another host)"
                              % (os.path.relpath(p, public), tag, key, url))
        for link in c.links:
            u = urlsplit(link)
            if u.scheme in ("mailto", "data"):
                continue
            if u.netloc and u.netloc not in hosts:
                # Only the host of the site itself: base_url or 127.0.0.1.
                if not (u.netloc.startswith("127.0.0.1") or u.netloc == "tiza.luisk.dev"):
                    continue
            path = unquote(u.path)
            if not path:
                target = p
            else:
                if path.startswith("/"):
                    target = os.path.join(public, path.lstrip("/"))
                else:
                    target = os.path.join(os.path.dirname(p), path)
                if path.endswith("/") or os.path.isdir(target):
                    target = os.path.join(target, "index.html")
            if not os.path.exists(target):
                broken.append("%s: %s (no file)" % (os.path.relpath(p, public), link))
                continue
            if u.fragment and target in pages and unquote(u.fragment) not in pages[target].ids:
                broken.append("%s: %s (no id)" % (os.path.relpath(p, public), link))
    for b in sorted(set(broken)):
        print("check_links.py: broken: " + b, file=sys.stderr)
    print("check_links.py: %d pages, %d broken links" % (len(pages), len(set(broken))))
    sys.exit(1 if broken else 0)


if __name__ == "__main__":
    main()
