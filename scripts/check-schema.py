#!/usr/bin/env python3
"""Validates every .codedraw under examples/ and qa/fixtures/ against schema/codedraw.schema.json (CI)."""
import glob, json, sys
from jsonschema import Draft202012Validator
schema = json.load(open("schema/codedraw.schema.json"))
Draft202012Validator.check_schema(schema)
v = Draft202012Validator(schema)
bad = 0
for f in sorted(glob.glob("examples/*.codedraw") + glob.glob("qa/fixtures/*/*.codedraw")):
    errs = list(v.iter_errors(json.load(open(f))))
    print(f, "OK" if not errs else "FAIL")
    for e in errs[:5]:
        print("   ", list(e.path), e.message[:120]); bad += 1
sys.exit(1 if bad else 0)
