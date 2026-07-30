# Security Policy

## Supported versions

This project is pre-1.0. Only the latest commit on `main` receives fixes.

## Reporting a vulnerability

Please report privately through GitHub:
[open a security advisory](https://github.com/Mathieu135/shelfware/security/advisories/new).

Do not open a public issue for a vulnerability.

You can expect an initial response within a week. This is a spare-time
project, so please be patient beyond that.

## Threat model

Worth knowing what this tool does, since it reads more than it writes.

**What it reads**

- Your shell history database or history files
- Shell startup files (`.zshrc`, `.bashrc`, and equivalents)
- Access times of installed binaries
- The output of package managers you have installed

**What it writes**

Nothing. It prints to stdout. `shelfware clean` emits a shell script but never
executes it, and the tool never removes a package itself.

Shell history is opened **read-only**, so an audit cannot corrupt it.

**What it executes**

It runs package managers to inventory them: `brew`, `cargo`, `uv`, `pipx`,
`npm` and `go`, with fixed read-only arguments. No value from your history or
config is ever passed to a shell.

**Worth flagging**

- Shell history frequently contains secrets that were typed on a command line.
  This tool only counts command names and never prints history content — but
  a bug here would be sensitive, so reports in this area are especially
  welcome.
- The generated `clean` script contains uninstall commands. Read it before
  running it, as its own header says.
