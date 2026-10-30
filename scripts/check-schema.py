#!/usr/bin/env python3
"""Validates every .tiza under examples/ and qa/fixtures/ against schema/tiza.schema.json (CI).

Two kinds of fixture must fail the validation, and the check fails when one
of them validates:
- qa/fixtures/compat/v1-newer-*.tiza is a file of a newer Tiza, which does
  not follow the schema of this version on purpose;
- qa/fixtures/schema/invalid-*.tiza breaks a rule that the schema states,
  so that the schema keeps that rule.
"""
import fnmatch, glob, json, sys
from jsonschema import Draft202012Validator

MUST_FAIL = {
    "qa/fixtures/compat/v1-newer-*.tiza": "a file of a newer Tiza",
    "qa/fixtures/schema/invalid-*.tiza": "a file that breaks a rule of the schema",
}

schema = json.load(open("schema/tiza.schema.json"))
Draft202012Validator.check_schema(schema)
v = Draft202012Validator(schema)
bad = 0
for f in sorted(glob.glob("examples/*.tiza") + glob.glob("qa/fixtures/**/*.tiza", recursive=True)):
    errs = list(v.iter_errors(json.load(open(f))))
    why = next((w for p, w in MUST_FAIL.items() if fnmatch.fnmatch(f, p)), None)
    if why:
        if errs:
            print(f, f"expected FAIL ({why})")
        else:
            print(f, f"FAIL: it validates, but {why} must not")
            bad += 1
        continue
    print(f, "OK" if not errs else "FAIL")
    for e in errs[:5]:
        print("   ", list(e.path), e.message[:120]); bad += 1
sys.exit(1 if bad else 0)
