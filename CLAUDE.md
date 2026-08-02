# Working on shelfware

## Answering

Be brief. State what changed and what it means, not the reasoning that got
there. No walkthrough of the alternatives considered, no explanation of why an
approach was picked, no recap of steps already visible in the diff. If the
reasoning matters, one sentence.

Detail is on request only.

## Never publish a real inventory

The report lists what is installed on a machine, which describes its owner more
than it describes the tool. Screenshots, docs examples, test fixtures and issue
templates all use the example machine in `docs/media/*.ansi` — `hey`, `cloc`,
`starship`, `zoxide`, `cargo-watch`, `imagemagick`, `@acme/legacy-codegen` and
friends. Never paste the output of a real run into a tracked file.

## Conventions

- Conventional Commits, no `Co-Authored-By`.
- Every change goes through a branch and a PR, using the repository template.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and `cargo test`
  before pushing. CI runs Linux, macOS and an MSRV check on 1.88.
- A release is a `v*` tag. The workflow builds the binaries and attaches them;
  the Homebrew formula in `Mathieu135/homebrew-tap` needs its URLs and
  checksums bumped afterwards.
