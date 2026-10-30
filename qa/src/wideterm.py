"""A small terminal emulator for the pty tests: it draws a chosen set of
characters two cells wide (as some terminals draw U+26A0 and the like),
with autowrap (pending wrap at the last column, scroll at the last row) or
without it, and it answers a cursor position request (`ESC [ 6 n`) and the
background query (OSC 11) as such a terminal would. `feed` gives the
answers back, so that the test decides when they reach the program. Grown
from `terminal_rows` of `pty.py`, which measures with Unicode and has no
wrap and no scroll.
"""
import re
import unicodedata

SEQ = re.compile(
    r"\x1b\[([0-9;?<>=]*)([ -/]*)([@-~])"      # CSI
    r"|\x1b\]([^\x07\x1b]*)(?:\x07|\x1b\\)"     # OSC
    r"|\x1b[P^_][^\x1b]*\x1b\\"                 # DCS, PM, APC
    r"|\x1b(.)"                                 # ESC x
    r"|([^\x1b])",
    re.S,
)
# What may be the start of a sequence that is not complete yet.
PARTIAL = re.compile(r"\x1b(\[[0-9;?<>=]*[ -/]*|\][^\x07\x1b]*(\x1b)?|[P^_][^\x1b]*(\x1b)?)?$", re.S)

VS16 = "\ufe0f"


class Term:
    def __init__(self, rows, cols, wide=(), autowrap=True, cpr=True, vs16_wide=True):
        self.rows, self.cols = rows, cols
        self.wide = set(wide)
        self.autowrap = autowrap
        self.cpr = cpr
        self.vs16_wide = vs16_wide
        self.grid = [[" "] * cols for _ in range(rows)]
        self.r = self.c = 0
        self.pending = False
        self.scrolls = 0
        self.cpr_asked = 0
        self.rest = ""
        self.last = None  # (row, col) of the last character drawn
        self.saved = None  # the cursor of `ESC 7`

    # ---- cells ----
    def width(self, ch):
        if ch in self.wide:
            return 2
        if ch == VS16 or unicodedata.combining(ch) or ch in "\u200b\u200c\u200d\u2060":
            return 0
        return 2 if unicodedata.east_asian_width(ch) in "WF" else 1

    def scroll(self):
        self.grid.pop(0)
        self.grid.append([" "] * self.cols)
        self.scrolls += 1

    def newline(self):
        if self.r == self.rows - 1:
            self.scroll()
        else:
            self.r += 1

    def put(self, ch):
        w = self.width(ch)
        if w == 0:
            # VS16 after a narrow base: emoji presentation, two cells in
            # such a terminal (the base grows by one).
            if ch == VS16 and self.vs16_wide and self.last is not None:
                r, c = self.last
                if self.grid[r][c] and self.grid[r][c] not in self.wide and c + 1 < self.cols and self.c == c + 1:
                    self.grid[r][c + 1] = ""
                    self.advance(1)
            return
        if self.pending and self.autowrap:
            self.c = 0
            self.newline()
            self.pending = False
        if w == 2 and self.c == self.cols - 1:
            if self.autowrap:
                self.c = 0
                self.newline()
            else:
                return
        self.grid[self.r][self.c] = ch
        if w == 2:
            self.grid[self.r][self.c + 1] = ""
        self.last = (self.r, self.c)
        self.advance(w)

    def advance(self, w):
        self.c += w
        if self.c >= self.cols:
            self.c = self.cols - 1
            self.pending = self.autowrap

    # ---- the stream ----
    def feed(self, data):
        """Feeds output of the program; returns the bytes the terminal
        answers (CPR, OSC 11)."""
        text = self.rest + data.decode("utf-8", errors="replace")
        m = PARTIAL.search(text)
        if m and m.group(0):
            self.rest, text = text[m.start():], text[: m.start()]
        else:
            self.rest = ""
        out = b""
        for m in SEQ.finditer(text):
            params, inter, final, osc, esc, ch = m.groups()
            if ch is not None:
                if ch == "\r":
                    self.c, self.pending = 0, False
                elif ch == "\n":
                    self.newline()
                    self.pending = False
                elif ch == "\b":
                    self.c, self.pending = max(self.c - 1, 0), False
                elif ch >= " ":
                    self.put(ch)
            elif esc is not None:
                # DECSC and DECRC: save and restore the cursor.
                if esc == "7":
                    self.saved = (self.r, self.c)
                elif esc == "8" and self.saved is not None:
                    (self.r, self.c), self.pending = self.saved, False
            elif osc is not None:
                if osc.startswith("11;?"):
                    out += b"\x1b]11;rgb:0000/0000/0000\x1b\\"
            elif final is not None:
                out += self.csi(params or "", final)
        return out

    def csi(self, params, final):
        if params.startswith("?"):
            modes = params[1:].split(";")
            if final in "hl" and "7" in modes:
                self.autowrap = final == "h"
            if final in "hl" and "1049" in modes:
                self.grid = [[" "] * self.cols for _ in range(self.rows)]
                self.r = self.c = 0
            return b""
        n = [int(x) if x else 0 for x in params.split(";")] if params else []
        first = n[0] if n else 0
        if final in "Hf":
            self.r = min(max((n[0] if n else 1) - 1, 0), self.rows - 1)
            self.c = min(max((n[1] if len(n) > 1 else 1) - 1, 0), self.cols - 1)
            self.pending = False
        elif final == "G":
            self.c = min(max(first - 1, 0), self.cols - 1)
            self.pending = False
        elif final == "C":
            self.c = min(self.c + max(first, 1), self.cols - 1)
            self.pending = False
        elif final == "J":
            if first == 2 or first == 3:
                self.grid = [[" "] * self.cols for _ in range(self.rows)]
            elif first == 0:
                for x in range(self.c, self.cols):
                    self.grid[self.r][x] = " "
                for y in range(self.r + 1, self.rows):
                    self.grid[y] = [" "] * self.cols
        elif final == "K":
            lo, hi = {1: (0, self.c + 1), 2: (0, self.cols)}.get(first, (self.c, self.cols))
            for x in range(lo, min(hi, self.cols)):
                self.grid[self.r][x] = " "
        elif final == "n" and first == 6:
            self.cpr_asked += 1
            if self.cpr:
                return f"\x1b[{self.r + 1};{self.c + 1}R".encode()
        return b""

    def lines(self):
        return ["".join(row).rstrip() for row in self.grid]
