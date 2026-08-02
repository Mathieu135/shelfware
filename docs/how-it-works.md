# How it works

The counting is the easy half. This document is about the other half: not
concluding that a package is dead because you never type its name.

## The problem

Grep your shell history for tools you never type, and it will tell you to
uninstall `ripgrep`, `fd` and `shellcheck` — three things you may use hundreds
of times a day.

They never reach your history because something else runs them: a Makefile, a
git hook, a CI job, a coding agent. Meanwhile `bat` looks dead because you
spelled it `cat` ever since you aliased it, and `direnv` looks dead because
nobody types the name of a shell plugin.

On the machine this tool was written for, a naive count produced 13 removal
candidates. Six were real. The other seven were tools in daily use.

Each false positive costs more than it looks: a report you have to
second-guess is a report you stop reading.

## Four signals

Rather than one better heuristic, `shelfware` reads four sources that are
blind in different ways.

| Signal | Sees | Cannot see |
| --- | --- | --- |
| Shell history | What you ran, and how often | Anything a script, hook, CI job or agent runs |
| Access times | That a binary ran at all, whoever launched it | How often, and by whom |
| Shell config | Plugins and integrations loaded at login | Anything configured elsewhere |
| Aliases | That `cat` is really `bat` | Aliases defined outside your dotfiles |

### Shell history

Atuin's SQLite database if it exists, otherwise `.zsh_history`,
`.bash_history` or fish's history file. Atuin takes precedence rather than
adding to the plain file it replaced, since reading both would double-count
everything they share.

A history line is rarely one command. `git log | rg fix && make build` is
three, and only counting the first word is how `rg` and `make` end up looking
unused. Lines are split on pipes and operators, wrappers like `sudo`, `env`
and `xargs` are skipped to reach the real command, leading `FOO=bar`
assignments are ignored, and paths are reduced to their basename so
`/opt/homebrew/bin/rg` counts as `rg`.

### Access times

The only signal that sees commands run by scripts, CI and coding agents.

Package managers hand out symlinks whose own access time never moves, so the
link is resolved before asking the filesystem — this detail is the difference
between the signal working and returning nothing at all.

Access times are a yes/no, not a frequency. That is why they promote a package
to `indirect` rather than `active`: we know something ran it, not how much it
matters.

### Shell config

Startup files — `.zshrc`, `.zshenv`, fish config, and XDG fragment
directories — are scanned for any mention of a package or its binaries.

Deliberately coarse. A plugin manager referenced anywhere in your config is
doing something, and that is enough to keep it off the removal list. A false
positive here can only move a package from `unused` to `integration`, which
errs toward keeping things.

The cost of that choice is that `integration` is the least trustworthy of the
five verdicts. The match is on a bare word, so a package whose name is also an
ordinary word — `time`, `parallel`, `watch` — lands there as soon as that word
appears anywhere in a startup file, for any reason. Read `integration` as "this
name turns up in your shell config", not as "this is wired into your shell".
The other four verdicts rest on something that was actually observed running.

### Aliases

`alias cat='bat --paging=never'` means every `cat` you type is really a `bat`
run, so the alias's usage is credited to the tool behind it. Without this,
some of the most-used tools on a machine look untouched.

## Reaching a verdict

The rules are checked in order, and every one of them exists to rescue a
package from a wrong `unused`:

1. **`active`** — enough history entries to be beyond question.
2. **`indirect`** — executed within `--since`. Checked before the history
   count, because "something ran this yesterday" outranks "you rarely type
   it".
3. **`rare`** — appears in history, but nothing has run it lately.
4. **`integration`** — never executed, but referenced by your shell config.
5. **`unused`** — nothing anywhere.

A package reaches `unused` only when all four signals stay silent. Reordering
these rules is how the tool would start producing the false positives it
exists to avoid.

### The activity threshold

`active` needs a number, and the right number depends on how much history
there is: a fresh machine with 500 recorded commands cannot clear the same bar
as one with 20 000.

It is derived as one per thousand recorded command invocations, clamped to
3..50. Asking users to work that out themselves is why it stopped being a
visible flag. `--active-hits` still overrides it.

## Packages to binaries

You type `rg`, never `ripgrep`, so every collector has to map a package to the
binaries it provides. Reliability varies:

| Manager | Mapping | Confidence |
| --- | --- | --- |
| brew | Contents of `$(brew --prefix)/opt/<pkg>/bin` | Exact |
| pipx | The `apps` list in `pipx list --json` | Exact |
| cargo | Binaries listed by `cargo install --list` | Exact |
| uv | The `- binary` lines of `uv tool list` | Exact |
| go | Each binary in `GOPATH/bin` is its own package | Exact, but no package identity |
| npm | Guessed from the package name, minus its scope | **Weak** |

npm does not report what a global package installs, so a scoped package whose
binary has a different name will look untouched. That is why `clean` prints an
explicit warning next to every npm entry.

Homebrew is limited to `brew leaves` — packages you asked for, not the
dependencies they pulled in. Suggesting the removal of a dependency would be
both useless and dangerous.

A package manager you do not have contributes nothing and is not an error.

## Blind spots

Worth knowing before uninstalling anything:

- **`noatime` mounts.** Common on Linux, and the access-time signal returns
  nothing at all there. macOS/APFS and most default Linux mounts do update
  them. Check yours before trusting `indirect`.
- **Package upgrades touch files.** A `brew upgrade` can refresh access times,
  making a dead package look recently used. Conservative in the right
  direction — it keeps something you might not need, rather than removing
  something you do.
- **Containers are invisible.** Anything run inside Docker leaves no trace on
  the host.
- **Other machines don't count.** Atuin syncs history across hosts; access
  times are local. A tool you only use on a server looks idle here.
- **Casks and GUI apps are out of scope.** No command-line surface to measure.

None of these produce a silent wrong answer: each row carries the evidence
behind its verdict, so a wrong call is visible rather than buried.

The report is evidence for a decision, not the decision.
