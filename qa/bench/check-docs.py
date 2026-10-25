#!/usr/bin/env python3
"""The documents `tiza check` is timed on (the clean width tests), written into DIR.

    python3 qa/bench/check-docs.py DIR

Everything is generated, nothing is stored: the code is the bench's
TypeScript (`scripts/bench.py`, `typescript_large`: 3,000 lines), and each
step adds four annotations near one of its classes (a note, a highlight with
a label, an arrow across three lines with a label and an `auto` callout).

- `bare.tiza`: the 3,000 lines, no annotations.
- `big-tutorial.tiza`: the 3,000 lines, 16 annotations in 4 steps.
- `medium.tiza`: the first 300 lines, 12 annotations in 3 steps.
- `stress.tiza`: the 3,000 lines, 32 annotations in 8 steps.

Then, for instance, with the binary of each commit:

    tiza check DIR/big-tutorial.tiza              # at 100 columns
    taskset -c 0 tiza check DIR/big-tutorial.tiza # on one core
"""
import importlib.util
import json
import os
import sys

sys.dont_write_bytecode = True
ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def typescript_large():
    spec = importlib.util.spec_from_file_location("bench", os.path.join(ROOT, "scripts", "bench.py"))
    bench = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(bench)
    return bench.typescript_large()


def doc(code_file, steps, stride):
    """`steps` steps of four annotations, step s on the class that starts at
    line 3 + 19·stride·(s-1) (the TypeScript repeats every 19 lines)."""
    anns = []
    for s in range(1, steps + 1):
        l = 3 + 19 * stride * (s - 1)
        anns += [
            {"id": f"n{s}", "step": s, "type": "note", "line": l + 9,
             "text": "loads the orders from a file next to the base URL"},
            {"id": f"h{s}", "step": s, "type": "highlight",
             "span": {"line": l + 10, "start": 4, "end": 20}, "label": "read the raw text"},
            {"id": f"a{s}", "step": s, "type": "arrow", "from": {"line": l + 11, "col": 10},
             "to": {"line": l + 13, "col": 6}, "label": "parsed feeds the filter"},
            {"id": f"c{s}", "step": s, "type": "callout", "from_line": l + 14, "to_line": l + 15,
             "text": "map adds the total; the reduce counts the items, then scales by the index"},
        ]
    return {"schema": 1, "title": f"check bench: {code_file}, {steps} steps",
            "source": {"language": "ts", "first_line": 1}, "code": {"file": code_file},
            "annotations": anns}


def main():
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    ts = typescript_large()
    with open(os.path.join(out, "big.ts"), "w") as f:
        f.write(ts)
    with open(os.path.join(out, "medium.ts"), "w") as f:
        f.write("\n".join(ts.split("\n")[:300]) + "\n")
    docs = {
        "bare.tiza": dict(doc("big.ts", 0, 0), title="check bench: big.ts, no annotations"),
        "big-tutorial.tiza": doc("big.ts", 4, 38),
        "medium.tiza": doc("medium.ts", 3, 5),
        "stress.tiza": doc("big.ts", 8, 19),
    }
    for name, d in docs.items():
        with open(os.path.join(out, name), "w") as f:
            json.dump(d, f, indent=2)
            f.write("\n")
    print("\n".join(os.path.join(out, n) for n in docs))


if __name__ == "__main__":
    main()
