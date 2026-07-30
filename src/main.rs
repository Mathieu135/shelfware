//! shelfware — find the tools you installed and stopped using.

mod classify;
mod inventory;
mod report;
mod signals;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::Shell;
use serde::Deserialize;

use classify::{Signals, Thresholds, Verdict};
use inventory::Package;

/// A package plus the evidence we gathered about it.
pub struct Assessment {
    pub package: Package,
    pub signals: Signals,
    pub verdict: Verdict,
}

const LONG_ABOUT: &str = "\
Cross-references what is installed on your machine against how you actually
use it, then reports what earns its disk space and what does not.

Counting your shell history is not enough on its own. Tools invoked by CI
jobs, git hooks, Makefiles and coding agents never reach your history, and a
tool hidden behind an alias is spelled differently than it is installed. Left
uncorrected, all of those look abandoned. shelfware reads four independent
signals so that a package is only ever proposed for removal when every one of
them stays silent.";

const AFTER_LONG_HELP: &str = "\
VERDICTS
  active        Typed often enough to be beyond question.
  indirect      Executed recently, but not by you typing it. Scripts, CI,
                git hooks and coding agents land here.
  rare          Typed occasionally, and nothing has run it lately.
  integration   Never executed, but referenced by your shell config. Plugin
                managers and shell hooks land here.
  unused        No signal from any source. The only removal candidates.

SIGNALS
  shell history   Atuin if present, otherwise zsh/bash/fish history files.
                  Knows what you ran and how often; blind to everything a
                  script or agent runs on your behalf.
  access times    Whether a binary ran at all, whoever launched it. Symlinks
                  are resolved first, since a package manager's bin/ entry
                  keeps its own access time frozen.
  shell config    Startup files, for integrations that are loaded but never
                  typed.
  aliases         So that `alias cat=bat` credits your `cat` usage to bat.

EXAMPLES
  shelfware
      The report. Only categories you can act on are detailed.

  shelfware --all
      Detail every category, including what you clearly use.

  shelfware --only rare --stale-days 90
      Tools typed a few times that nothing has run this quarter.

  shelfware --uninstall-script > cleanup.sh
      A removal script to review and run yourself. Never executed for you.

  shelfware --json | jq '.packages[] | select(.verdict==\"unused\") | .name'
      For scripting.

  shelfware --completions zsh > ~/.config/zsh/completions/_shelfware
      Shell completion. Also supports bash, fish, elvish and powershell.

CONFIGURATION
  ~/.config/shelfware/config.toml, all keys optional:

      active_hits = 20
      stale_days  = 30
      managers    = [\"brew\", \"cargo\"]
      ignore      = [\"pam-reattach\"]

  `ignore` is for permanent decisions: a tool you knowingly keep for
  once-a-year use should stop being proposed at every audit.

CAVEATS
  Access times are unavailable on filesystems mounted noatime, invisible for
  anything run inside a container, and local-only even when your history
  syncs across machines. npm binary names are guessed, since npm does not
  report what a global package installs. The report is evidence for a
  decision, not the decision.";

#[derive(Parser)]
#[command(
    name = "shelfware",
    version,
    about = "Find the tools you installed and stopped using.",
    long_about = LONG_ABOUT,
    after_long_help = AFTER_LONG_HELP,
)]
struct Cli {
    /// Package managers to inspect
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "LIST",
        long_help = "Comma-separated package managers to inspect, in place of all of them.\n\
                     Supported: brew, cargo, uv, pipx, npm, go.\n\
                     Homebrew is limited to `brew leaves`, so dependencies pulled in by\n\
                     something else are never proposed for removal."
    )]
    managers: Option<Vec<String>>,

    /// Uses at or above this count mean a package is plainly alive [default: 20]
    #[arg(
        long,
        value_name = "N",
        long_help = "How many times a package must appear in your shell history to be\n\
                     called `active` outright. Lower it if your history is short or\n\
                     recently reset; raise it to be stricter about what counts as busy."
    )]
    active_hits: Option<u32>,

    /// How recently a binary must have run to still count as in use [default: 30]
    #[arg(
        long,
        value_name = "DAYS",
        long_help = "Age limit, in days, for the access-time signal. A binary executed\n\
                     within this window is `indirect` rather than idle. Raise it for\n\
                     tools you only touch once a quarter, such as release tooling."
    )]
    stale_days: Option<u64>,

    /// Extra history sources (an Atuin .db, or a plain history file)
    #[arg(
        long,
        value_name = "PATH",
        long_help = "Read these history sources instead of auto-detecting. A path ending\n\
                     in .db is opened as an Atuin database, anything else is parsed as a\n\
                     plain history file. Repeatable, and useful for auditing history\n\
                     copied over from another machine."
    )]
    history: Vec<PathBuf>,

    /// Packages to leave out of the report entirely
    #[arg(
        long,
        value_delimiter = ',',
        value_name = "LIST",
        long_help = "Comma-separated packages to omit. Matches a package name or any\n\
                     binary it provides. For decisions you want to keep, prefer the\n\
                     `ignore` key in the config file over retyping this every time."
    )]
    ignore: Vec<String>,

    /// Show every package, not just the actionable ones
    #[arg(
        long,
        long_help = "Detail every category. By default `active` and `indirect` are only\n\
                     counted in the summary, since listing the tools you obviously use\n\
                     buries the handful worth deciding on."
    )]
    all: bool,

    /// Show only one verdict
    #[arg(
        long,
        value_name = "VERDICT",
        long_help = "Restrict the detailed listing to one verdict: active, indirect,\n\
                     rare, integration or unused. The summary counts still describe the\n\
                     whole inventory."
    )]
    only: Option<String>,

    /// Print a removal script to review, instead of a report
    #[arg(
        long,
        long_help = "Emit a shell script that uninstalls the selected packages, with the\n\
                     evidence for each one kept as a comment. It is printed, never run:\n\
                     redirect it to a file, delete the lines you disagree with, then run\n\
                     it yourself. Defaults to `unused`; combine with --only for another\n\
                     category."
    )]
    uninstall_script: bool,

    /// Emit JSON instead of a human report
    #[arg(long, long_help = "Emit JSON on stdout. Honours --only.")]
    json: bool,

    /// Generate a shell completion script
    #[arg(
        long,
        value_name = "SHELL",
        long_help = "Write a completion script for the given shell to stdout, then exit.\n\
                     Supported: bash, zsh, fish, elvish, powershell."
    )]
    completions: Option<Shell>,
}

/// Optional `~/.config/shelfware/config.toml`. Exists so the ignore list —
/// which is personal and grows over time — does not live on an ever-longer
/// command line.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Config {
    active_hits: Option<u32>,
    stale_days: Option<u64>,
    managers: Option<Vec<String>>,
    ignore: Vec<String>,
}

impl Config {
    fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match toml::from_str(&text) {
            Ok(config) => config,
            Err(err) => {
                // A broken config should not block an audit.
                eprintln!("shelfware: ignoring {}: {err}", path.display());
                Self::default()
            }
        }
    }

    fn path() -> Option<PathBuf> {
        let base = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) => PathBuf::from(dir),
            None => signals::home_dir()?.join(".config"),
        };
        Some(base.join("shelfware/config.toml"))
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Completions describe the CLI itself, so nothing needs inspecting.
    if let Some(shell) = cli.completions {
        let mut cmd = Cli::command();
        let name = cmd.get_name().to_string();
        clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
        return Ok(());
    }

    let config = Config::load();
    let now = SystemTime::now();
    let defaults = Thresholds::default();

    let thresholds = Thresholds {
        active_hits: cli
            .active_hits
            .or(config.active_hits)
            .unwrap_or(defaults.active_hits),
        stale_days: cli
            .stale_days
            .or(config.stale_days)
            .unwrap_or(defaults.stale_days),
    };

    let managers = cli
        .managers
        .or(config.managers)
        .unwrap_or_else(|| inventory::MANAGERS.iter().map(|m| m.to_string()).collect());

    let mut ignore = config.ignore;
    ignore.extend(cli.ignore);

    // Kept as a display filter rather than applied to the data: the summary
    // counts stay honest about the whole inventory even when one verdict is
    // singled out.
    let only = match &cli.only {
        Some(label) => match Verdict::from_label(label) {
            Some(verdict) => Some(verdict),
            None => anyhow::bail!(
                "unknown verdict `{label}`; expected one of: active, indirect, rare, integration, unused"
            ),
        },
        None => None,
    };

    let counts = usage_counts(&cli.history);
    let mentions = signals::shell_config_mentions();

    let mut assessments: Vec<Assessment> = inventory::collect(&managers)
        .into_iter()
        .filter(|pkg| !is_ignored(pkg, &ignore))
        .map(|package| {
            let signals = gather(&package, &counts, &mentions);
            let verdict = classify::classify(&signals, thresholds, now);
            Assessment {
                package,
                signals,
                verdict,
            }
        })
        .collect();

    // Best verdict first between sections, busiest package first inside one.
    assessments.sort_by(|a, b| {
        a.verdict
            .cmp(&b.verdict)
            .then(b.signals.interactive_hits.cmp(&a.signals.interactive_hits))
            .then(a.package.name.cmp(&b.package.name))
    });

    if cli.uninstall_script {
        report::uninstall_script(&assessments, now, only);
    } else if cli.json {
        println!("{}", report::json(&assessments, now, only));
    } else {
        report::text(&assessments, now, cli.all, only);
    }
    Ok(())
}

fn is_ignored(pkg: &Package, ignore: &[String]) -> bool {
    let short = pkg.name.rsplit('/').next().unwrap_or(&pkg.name);
    ignore
        .iter()
        .any(|i| i == &pkg.name || i == short || pkg.binaries.iter().any(|b| &b.name == i))
}

/// Fold every signal source into one verdict-ready struct.
fn gather(pkg: &Package, counts: &HashMap<String, u32>, mentions: &HashSet<String>) -> Signals {
    let interactive_hits = pkg
        .binaries
        .iter()
        .map(|b| counts.get(&b.name).copied().unwrap_or(0))
        .sum();

    // Newest access time across all of a package's binaries: using one tool
    // from a suite keeps the whole package alive.
    let last_exec = pkg
        .binaries
        .iter()
        .filter_map(|b| b.path.as_deref())
        .filter_map(signals::last_exec)
        .max();

    let short = pkg.name.rsplit('/').next().unwrap_or(&pkg.name);
    let configured = mentions.contains(&pkg.name)
        || mentions.contains(short)
        || pkg.binaries.iter().any(|b| mentions.contains(&b.name));

    Signals {
        interactive_hits,
        last_exec,
        configured,
    }
}

/// Command frequencies, with alias usage credited to the tool behind it.
fn usage_counts(explicit: &[PathBuf]) -> HashMap<String, u32> {
    let mut counts = HashMap::new();

    if !explicit.is_empty() {
        for path in explicit {
            merge(&mut counts, read_history(path));
        }
    } else if let Some(db) = signals::default_atuin_db() {
        // Atuin supersedes the plain history file it replaced; reading both
        // would double-count everything they have in common.
        merge(&mut counts, signals::atuin_counts(&db));
    } else {
        for path in signals::default_history_files() {
            merge(&mut counts, signals::history_file_counts(&path));
        }
    }

    // `alias cat=bat` means every `cat` you type is really a `bat` run.
    // Without this, some of the most-used tools on a machine look untouched.
    for (alias, target) in signals::alias_map() {
        if let Some(hits) = counts.get(&alias).copied() {
            *counts.entry(target).or_insert(0) += hits;
        }
    }

    counts
}

fn read_history(path: &Path) -> Result<HashMap<String, u32>> {
    if path.extension().is_some_and(|e| e == "db") {
        signals::atuin_counts(path)
    } else {
        signals::history_file_counts(path)
    }
}

fn merge(into: &mut HashMap<String, u32>, from: Result<HashMap<String, u32>>) {
    match from {
        Ok(counts) => {
            for (cmd, n) in counts {
                *into.entry(cmd).or_insert(0) += n;
            }
        }
        Err(err) => eprintln!("shelfware: {err:#}"),
    }
}
