# Releases (maintainers)

Do these steps to publish a release.

A release is a version commit on `main`, a `vX.Y.Z` tag on it and then crates.io, by hand. In a clean, up-to-date checkout of `main`:

```sh
git switch main && git pull && git status --short   # must print nothing
gitleaks git --gitleaks-ignore-path qa/.gitleaksignore   # no secret in the history (the fake ones in tests and demos are listed there)
scripts/bump-version.sh X.Y.Z                       # Cargo.toml, Cargo.lock, CHANGELOG.md, the README status
$EDITOR docs/release-notes/vX.Y.Z.md                # the release's description: short, English, no card ids
                                                    # (keep `cargo install tiza` only if X.Y.Z reaches crates.io the same day)
git add docs/release-notes/vX.Y.Z.md                # new: `commit -a` would leave it out, and the tag would have none
git diff                                            # check the five versions and the CHANGELOG heading
git commit -am "vX.Y.Z"
cargo publish --workspace --dry-run --locked        # before the tag: pushing the tag starts the release
git push origin main
git tag vX.Y.Z && git push origin vX.Y.Z            # release.yml: the three archives and the GitHub Release
cargo login                                         # once per machine, with a crates.io API token
cargo publish --locked -p tiza-core
cargo publish --locked -p tiza-export
cargo publish --locked -p tiza-tui
cargo publish --locked -p tiza-mcp
cargo publish --locked -p tiza
```

`scripts/bump-version.sh` sets the workspace version and the `version` of the four internal crates in `[workspace.dependencies]` of `Cargo.toml` (a 0.x requirement is exact, so all five move together), runs `cargo update --workspace` for `Cargo.lock`, turns the `## [Unreleased]` entries of `CHANGELOG.md` into `## [X.Y.Z] - date`, under a new empty `## [Unreleased]`, and moves the version of the README's status line; it changes nothing else. The dry run packages and builds the five crates against each other and uploads nothing; it only means something for a version that is not on crates.io yet (a dry run of one crate alone, or of a version already published, builds against the crates already on crates.io). If it fails, fix it, commit, and run it again: nothing is tagged or pushed yet.

The crates go to crates.io once the GitHub Release is out, one at a time, in dependency order. Each `cargo publish` returns only once the crate is in the crates.io index (`Waiting for … to be available at registry crates-io`), so the next one finds it. If one stops waiting with `timed out waiting for … to be available`, check that the version is in the index before running the next — `curl -s https://index.crates.io/ti/za/tiza-core | grep '"vers":"X.Y.Z"'` (same path for every `tiza*` crate) — and go on when it prints a line. A crate that is already published fails with `already exists on crates.io index`: skip it and go on with the next. A tag with a suffix (`vX.Y.Z-rc.N`) is a prerelease on GitHub and never goes to crates.io.

When Actions is not available, `scripts/release-local.sh vX.Y.Z` builds the same archives on a clean checkout of the tag: on macOS it builds arm64 and Intel (cross-compiled), on a Linux x86_64 machine or container the Linux x86_64 one, and it leaves in `dist/vX.Y.Z/` one tar.gz with its `.sha256` per target — the binary, README, licences and examples, as `release.yml` packs them — and `notes.md`, the release notes of `docs/release-notes/` (`scripts/release-notes.sh`, as `release.yml` writes them: a `vX.Y.Z` tag without its notes, or with notes not committed, stops before building; a `-rc.N` tag takes its own notes, `vX.Y.Z-rc.N.md`, or two lines pointing at the CHANGELOG, never the notes of `X.Y.Z`, whose archive names are not its own; the Linux archive is built with glibc 2.35 at most, in an `ubuntu:22.04` container), to attach to a release created by hand.
