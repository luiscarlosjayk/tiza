# Files that the schema must refuse

Each `invalid-*.tiza` file breaks one rule that `schema/tiza.schema.json` states. `scripts/check-schema.py` expects each one to fail the validation, and fails when one validates. Thus the schema cannot lose the rule.

- `invalid-move-at-step-0.tiza` — a move with `at_step` 0. Step 0 is the code with no change, so a move starts at step 1. Tiza also refuses to load this file.
- `invalid-talk-with-code.tiza` — a master with `code` and `talk`. A file is a document or a talk, not both. Tiza also refuses to open this file.
- `invalid-talk-unknown-key.tiza` — a key in `talk` other than `title`, `scenes` and `at`. Tiza also refuses to open this file.
- `invalid-talk-no-scene.tiza` — a talk with an empty list of scenes. Tiza also refuses to open this file.
