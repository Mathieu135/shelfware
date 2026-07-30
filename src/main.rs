//! shelfware — find the tools you installed and stopped using.

mod classify;
mod inventory;
mod report;
mod signals;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use clap::Parser;
use serde::Deserialize;

use classify::{Signals, Thresholds, Verdict};
use inventory::Package;

/// A package plus the evidence we gathered about it.
pub struct Assessment {
    pub package: Package,
    pub signals: Signals,
    pub verdict: Verdict,
}

#[derive(Parser)]
#[command(
    name = "shelfware",
    version,
    about = "Find the tools you installed and stopped using.",
    long_about = "Cross-references installed packages against how you actually use \
                  your machine: shell history, binary access times, and shell \
                  startup files. Reports what earns its place and what does not."
)]
struct Cli {
    /// Package managers to inspect
    #[arg(long, value_delimiter = ',')]
    managers: Option<Vec<String>>,

    /// Uses at or above this count mean a package is plainly alive
    #[arg(long)]
    active_hits: Option<u32>,

    /// How recently a binary must have run to still count as in use
    #[arg(long)]
    stale_days: Option<u64>,

    /// Extra history sources (an Atuin .db, or a plain history file)
    #[arg(long, value_name = "PATH")]
    history: Vec<PathBuf>,

    /// Packages to leave out of the report entirely
    #[arg(long, value_delimiter = ',')]
    ignore: Vec<String>,

    /// Show every package, not just the actionable ones
    #[arg(long)]
    all: bool,

    /// Show only one verdict (active, indirect, rare, integration, unused)
    #[arg(long, value_name = "VERDICT")]
    only: Option<String>,

    /// Emit JSON instead of a human report
    #[arg(long)]
    json: bool,
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

    if let Some(only) = &cli.only {
        let wanted = only.to_lowercase();
        assessments.retain(|a| a.verdict.label() == wanted);
    }

    // Best verdict first between sections, busiest package first inside one.
    assessments.sort_by(|a, b| {
        a.verdict
            .cmp(&b.verdict)
            .then(b.signals.interactive_hits.cmp(&a.signals.interactive_hits))
            .then(a.package.name.cmp(&b.package.name))
    });

    if cli.json {
        println!("{}", report::json(&assessments, now));
    } else {
        report::text(&assessments, now, cli.all);
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
