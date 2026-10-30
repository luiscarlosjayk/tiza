# Files that the schema must refuse

Each `invalid-*.tiza` file breaks one rule that `schema/tiza.schema.json` states. `scripts/check-schema.py` expects each one to fail the validation, and fails when one validates. Thus the schema cannot lose the rule.

- `invalid-move-at-step-0.tiza` — a move with `at_step` 0. Step 0 is the code with no change, so a move starts at step 1. Tiza also refuses to load this file.
