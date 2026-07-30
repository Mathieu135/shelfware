# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Cross-references installed packages against four independent usage signals:
  shell history (Atuin, zsh, bash, fish), binary access times, shell startup
  files, and alias definitions.
- Inventories from Homebrew (`brew leaves`), cargo, uv, pipx, npm globals and
  `go install`.
- Five verdicts — `active`, `indirect`, `rare`, `integration`, `unused` — with
  the supporting evidence printed on every row.
- `shelfware clean`, which prints a reviewable removal script instead of
  uninstalling anything. npm entries carry an explicit warning, since their
  binary names are guessed.
- `shelfware completions <shell>` for bash, zsh, fish, elvish and powershell.
- `--since`, accepting `30d`, `6w`, `6mo`, `1y` or a bare number of days.
- Optional `~/.config/shelfware/config.toml` for lasting preferences,
  particularly `ignore`.
- `--json` output for scripting.
- Full documentation in `docs/`, mirrored in `--help`.

- Shell completion proposes argument *values*, not just flag names: verdicts
  for `--only`, package managers for `--managers`, file paths for `--history`.
- Project documentation for contributors: `CONTRIBUTING.md`, `SECURITY.md`
  with a stated threat model, a code of conduct, and issue templates.

### Notes

The activity threshold calibrates itself to the size of your history — one per
thousand recorded command invocations, clamped to 3..50 — rather than asking
you to pick a number. `--active-hits` still overrides it.
