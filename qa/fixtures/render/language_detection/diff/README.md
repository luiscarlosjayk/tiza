# A real diff on standard input

`git.diff` and `unified.diff` are the change from `old.rs` to `new.rs`, written by two tools:

- `git.diff`: `git diff --no-index old.rs new.rs > git.diff`. It starts with `diff --git`.
- `unified.diff`: `diff -u old.rs new.rs`, with the dates removed from the two header lines. It starts with `--- old.rs` and `+++ new.rs`.

`language_detection.rs` sends each one on standard input to `tiza render -`. Tiza finds the diff language from the header, so the colors are the same as those of the `.diff` file.
