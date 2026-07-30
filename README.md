# shelfware

Find the tools you installed and stopped using.

`shelfware` cross-references what is installed on your machine against how you
actually use it, then tells you what earns its disk space and what does not.

```
shelfware — 66 packages inspected

  active         11   Typed regularly at your prompt
  indirect       44   Executed recently, but not by you typing it
  rare            4   Typed occasionally, and not executed lately
  integration     1   Never executed, but wired into your shell config
  unused          6   No signal from any source — removal candidates

UNUSED — No signal from any source — removal candidates
  git-filter-repo     brew    never typed · last run 34d ago
  pam-reattach        brew    never typed · last run 4mo ago
  tauri-cli           cargo   never typed · last run 4mo ago
```

## Why not just count your history

Because counting is the easy half, and it is wrong on its own.

Grep your shell history for tools you never type and you will be told to
uninstall `ripgrep`, `fd` and `shellcheck` — three things you may use hundreds
of times a day. They just never reach your history, because a Makefile, a CI
job, a git hook or a coding agent runs them on your behalf. Meanwhile `bat`
looks dead because you spelled it `cat` ever since you aliased it, and `direnv`
looks dead because nobody types the name of a shell plugin.

On the machine this was written for, a naive count produced 13 removal
candidates. Six were real.

So `shelfware` reads four independent signals, each blind in a different way:

| Signal | Sees | Misses |
| --- | --- | --- |
| Shell history (Atuin, zsh, bash, fish) | what *you* ran, and how often | anything run by a script, hook, CI job or agent |
| Binary access times | that a binary ran at all, whoever launched it | how often, and by whom |
| Shell startup files | plugins and integrations loaded at login | anything configured elsewhere |
| Alias definitions | that `cat` is really `bat` | aliases defined outside your dotfiles |

A package is only reported as removable when *all four* stay silent.

## Verdicts

| Verdict | Meaning |
| --- | --- |
| `active` | Typed often enough to be beyond question |
| `indirect` | Executed recently, but not by you typing it — scripts, CI, agents |
| `rare` | Typed occasionally, and nothing has run it lately |
| `integration` | Never executed, but referenced by your shell config |
| `unused` | No signal from any source |

## Install

Requires Rust 1.88+ and a C compiler (SQLite is built from source, so Atuin
support needs no system library).

```sh
# Linux — install a compiler first if you have none
sudo apt install build-essential      # or: dnf install gcc / pacman -S base-devel

cargo install --git https://github.com/Mathieu135/shelfware
```

```sh
# macOS
xcode-select --install
cargo install --git https://github.com/Mathieu135/shelfware
```

## Use

```sh
shelfware                     # the report
shelfware clean               # a removal script to review
shelfware completions zsh     # shell completion
```

That is the whole interface. The one setting worth knowing is `--since`, which
decides how far back an execution still counts as "in use":

```sh
shelfware --since 6mo
```

`shelfware clean` prints a script rather than removing anything — you review
it, delete what you disagree with, and run it yourself. Some verdicts rest on
guessed binary names, so the final call stays yours.

Supported inventories: Homebrew (`brew leaves` only, so dependencies are never
suggested for removal), cargo, uv, pipx, npm globals, and `go install`
binaries. A package manager you do not have contributes nothing.

## Documentation

- [Usage](docs/usage.md) — installation, every option, configuration, JSON, cron
- [How it works](docs/how-it-works.md) — the signals, the verdict rules, the blind spots

`--help` carries the same material in the terminal; `-h` stays a single screen.

## Limitations

- **Access times can be disabled.** On Linux, `noatime` mounts give no signal
  at all. Check yours before trusting the `indirect` verdict.
- **Package upgrades touch files**, which can make a dead package look
  recently used. Conservative in the right direction.
- **Containers are invisible**, and access times are local even when your
  history syncs across machines.
- **npm binary names are guessed**, since npm does not report what a global
  package installs.
- **Casks and GUI apps are out of scope** — no command-line surface to measure.

Every row carries the evidence behind its verdict, so a wrong call is visible
rather than buried. The report is evidence for a decision, not the decision.

## License

MIT
