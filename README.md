# shelfware

Find the tools you installed and stopped using.

`shelfware` cross-references what is installed on your machine against how you
actually use it, then tells you what earns its disk space and what does not.

```
shelfware — 65 packages inspected

  active         13   Typed regularly at your prompt
  indirect       42   Executed recently, but not by you typing it
  rare            3   Typed occasionally, and not executed lately
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

Every one of those is a false positive that erodes trust in the report. So
`shelfware` reads four independent signals, each blind in a different way:

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

```sh
cargo install --git https://github.com/<you>/shelfware
```

Requires Rust 1.88 or newer. SQLite is compiled in, so Atuin support needs no
system library.

## Usage

```sh
shelfware                          # the report
shelfware --all                    # include everything, not just what is actionable
shelfware --only unused            # one category
shelfware --json | jq              # for scripts
shelfware --managers brew,cargo    # narrow the inventory
shelfware --stale-days 90          # "recent" means the last quarter
shelfware --active-hits 5          # lower the bar for "clearly alive"
```

`-h` prints a one-screen summary; `--help` prints the full manual, including
what each verdict means, what each signal can and cannot see, and worked
examples.

## Removing things

```sh
shelfware --uninstall-script > cleanup.sh
```

This writes a shell script that uninstalls the packages in question, keeping
the evidence for each one as a comment above its command. It is printed, never
executed: redirect it, delete the lines you disagree with, then run it
yourself.

That separation is deliberate. Some verdicts rest on guessed binary names, and
every signal here has a documented blind spot — an audit tool has no business
uninstalling anything on that basis. Defaults to the `unused` set; pair it with
`--only` for another category.

## Shell completion

```sh
shelfware --completions zsh  > ~/.config/zsh/completions/_shelfware
shelfware --completions bash > ~/.local/share/bash-completion/completions/shelfware
shelfware --completions fish > ~/.config/fish/completions/shelfware.fish
```

`elvish` and `powershell` are supported too. For zsh, make sure the directory
is on your `fpath` before `compinit` runs.

Supported inventories: Homebrew (`brew leaves` only, so dependencies are never
suggested for removal), cargo, uv, pipx, npm globals, and `go install`
binaries. A package manager you do not have contributes nothing and is not an
error.

## Configuration

Optional, at `~/.config/shelfware/config.toml`:

```toml
active_hits = 20
stale_days  = 30
managers    = ["brew", "cargo"]
ignore      = ["pam-reattach", "switchaudio-osx"]
```

`ignore` exists because some answers are permanent: a tool you knowingly keep
for once-a-year use should stop being proposed every month.

## Limitations

Worth knowing before you uninstall anything:

- **Access times can be disabled.** On Linux, filesystems mounted `noatime`
  give no signal at all. macOS/APFS updates them, as do most default Linux
  mounts, but check yours before trusting the `indirect` verdict.
- **Symlinks lie.** A package manager's `bin/` entry keeps its own access time
  frozen. `shelfware` resolves symlinks before asking; anything that copies
  rather than links may still read as stale.
- **Other machines do not count.** Atuin syncs history across hosts, but access
  times are local. A tool you only use on a server will look idle here.
- **Containers are invisible.** Anything you run inside Docker leaves no trace
  on the host.
- **npm binary names are guessed** from the package name, since npm does not
  report what a global package installs.
- **Casks and GUI apps are out of scope.** They have no command-line surface to
  measure.

The report is evidence for a decision, not the decision.

## License

MIT
