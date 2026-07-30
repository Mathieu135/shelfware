## What this changes

<!-- And why. If it fixes an issue, link it. -->

## Effect on verdicts

<!--
Does this change how any package gets classified? If so, say which direction.

Keeping a package someone did not need is cheap. Telling someone to delete a
tool they use every day is not — it destroys trust in the whole report. A
change that trades a false negative for a false positive needs a good reason.

Write "none" if this is a refactor, docs, or tooling.
-->

## Checks

- [ ] `cargo fmt`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] Non-trivial logic has a test
- [ ] Docs updated if behaviour changed (`README.md`, `docs/`, `--help`)

## Tested on

<!-- OS, and whether your filesystem is mounted noatime. -->
