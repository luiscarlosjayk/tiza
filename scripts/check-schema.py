#!/usr/bin/env python3
"""Validates every .tiza under examples/ and qa/fixtures/ against schema/tiza.schema.json (CI).

A fixture named qa/fixtures/compat/v1-newer-*.tiza is a file of a newer Tiza:
it does not follow the schema of this version on purpose, so it must fail
the validation. If one of them validates, the check fails too.
"""
import fnmatch, glob, json, sys
from jsonschema import Draft202012Validator

NEWER = "qa/fixtures/compat/v1-newer-*.tiza"

schema = json.load(open("schema/tiza.schema.json"))
Draft202012Validator.check_schema(schema)
v = Draft202012Validator(schema)
bad = 0
for f in sorted(glob.glob("examples/*.tiza") + glob.glob("qa/fixtures/**/*.tiza", recursive=True)):
    errs = list(v.iter_errors(json.load(open(f))))
    if fnmatch.fnmatch(f, NEWER):
        if errs:
            print(f, "expected FAIL (a file of a newer Tiza)")
        else:
            print(f, "FAIL: it validates, but a file of a newer Tiza must not")
            bad += 1
        continue
    print(f, "OK" if not errs else "FAIL")
    for e in errs[:5]:
        print("   ", list(e.path), e.message[:120]); bad += 1
sys.exit(1 if bad else 0)
