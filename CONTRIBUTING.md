# Contributing

Thanks for looking. This is a small, focused tool, and contributions are
welcome.

## Getting started

```sh
git clone https://github.com/Mathieu135/shelfware
cd shelfware
cargo test
cargo run -- --all
```

Requires Rust 1.88+ and a C compiler (SQLite is built from source).

## Before opening a pull request

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs exactly these on Linux and macOS, plus a build against the minimum
supported Rust version. All three must pass.

## What this project is trying to be

A tool that tells you what you installed and stopped using, and is **right**
about it. The hard part is not counting commands — it is not concluding that a
package is dead because you never type its name.

That shapes what belongs here:

**Welcome**

- New package managers. Look at `src/inventory.rs`: each collector is
  independent, returns an empty list when its manager is absent, and maps a
  package to the binaries it actually provides.
- New usage signals, especially ones that see what the existing four miss.
- Better package-to-binary mapping, particularly for npm, where names are
  currently guessed.
- Platform fixes. Access-time behaviour varies, and the author only runs
  macOS.

**A harder sell**

- Anything that uninstalls packages automatically. `shelfware clean` prints a
  script on purpose; several verdicts rest on imperfect evidence, and the
  final call belongs to a human.
- New flags. The CLI was deliberately reduced to three commands and one
  setting. A new option needs to earn its place against the config file.
- Heuristics that trade a false negative for a false positive. Keeping a
  package you did not need is cheap; telling someone to delete a tool they use
  daily destroys trust in the whole report.

## Adding a package manager

1. Write a collector in `src/inventory.rs` returning `Vec<Package>`. Return an
   empty vector when the manager is not installed — that is not an error.
2. Add a variant to `Manager`. The compiler will point you at everything that
   must handle it, including the uninstall command in `src/report.rs`.
3. Add its uninstall command.
4. Document the reliability of its package-to-binary mapping in
   `docs/how-it-works.md`. If the mapping is a guess, say so, and make
   `clean` warn about it the way npm does.

## Style

Match the surrounding code. Comments explain *why* a decision was made, not
what a line does — most of them exist because the obvious implementation would
have been wrong. If you remove a safeguard, the comment explaining it should
tell you what breaks.

Non-trivial logic gets a test. The existing ones double as documentation of
edge cases: read `src/classify.rs` and `src/signals.rs` for the tone.

## Never commit a real report

A report is a list of what someone has installed, which describes its owner
more than it describes the tool. Screenshots, documentation examples and test
fixtures all use the example machine in `docs/media/report.ansi` and
`docs/media/clean.ansi` — `hey`, `cloc`, `starship`, `zoxide`, `cargo-watch`,
`imagemagick`, `@acme/legacy-codegen`. Extend that machine rather than pasting
your own output into a tracked file.

## Reporting a wrong verdict

The most useful bug report for this project is "it said X was unused and it is
not". Include:

- the row as printed, with its evidence (`typed 5× · last run 4mo ago`)
- how you actually use that tool
- your OS, and whether your filesystem is mounted `noatime`

That last one matters more than it sounds: without access times, one of the
four signals is silent.

## Code of conduct

Participation is covered by the [Code of Conduct](CODE_OF_CONDUCT.md).
