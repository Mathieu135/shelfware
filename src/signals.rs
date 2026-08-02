//! Where usage evidence comes from.
//!
//! Four independent sources, deliberately kept separate because each one is
//! blind in a different way:
//!
//! - shell history knows what *you* ran, and nothing about scripts or agents;
//! - access times know that a binary ran, but not who ran it or how often;
//! - shell config knows what gets loaded at startup, which is never typed;
//! - aliases explain why a heavily used tool looks untouched.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags};

/// Words that stand in front of the command we actually care about.
/// Without this, half the history collapses into `sudo` and `xargs`.
const WRAPPERS: &[&str] = &[
    "sudo", "doas", "command", "builtin", "exec", "env", "time", "nohup", "nice", "xargs", "watch",
    "then", "else", "elif", "do", "done", "fi", "if", "while", "until", "for", "in", "function",
];

/// Pull every command invocation out of a single history line.
///
/// A line is rarely one command: `git status | rg foo && make build` is three.
/// Missing the ones after a pipe is how a tool ends up reported as unused.
pub fn extract_invocations(line: &str) -> Vec<String> {
    line.split(['|', ';', '&', '(', ')', '\n', '`', '{', '}'])
        .filter_map(first_command)
        .collect()
}

fn first_command(segment: &str) -> Option<String> {
    for word in segment.split_whitespace() {
        if is_env_assignment(word) {
            continue;
        }
        let word = word.trim_matches(|c| matches!(c, '"' | '\'' | '$' | '!'));
        if word.is_empty()
            || word.starts_with('-')
            || word.starts_with('<')
            || word.starts_with('>')
            || word.starts_with('#')
        {
            continue;
        }
        // Count `/opt/homebrew/bin/rg` as `rg`.
        let base = word.rsplit('/').next().unwrap_or(word);
        if base.is_empty() || WRAPPERS.contains(&base) {
            continue;
        }
        return Some(base.to_string());
    }
    None
}

/// `FOO=bar cmd` — the assignment is not the command.
fn is_env_assignment(word: &str) -> bool {
    match word.split_once('=') {
        Some((key, _)) => {
            !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        None => false,
    }
}

/// Command frequencies from an Atuin database.
///
/// Read-only on purpose: this tool must never be the reason a shell history
/// gets corrupted.
pub fn atuin_counts(db: &Path) -> Result<HashMap<String, u32>> {
    let conn = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening Atuin database at {}", db.display()))?;

    // `deleted_at` arrived in later Atuin schemas; fall back for older ones.
    const LIVE_ONLY: &str = "SELECT command FROM history WHERE deleted_at IS NULL";
    const EVERYTHING: &str = "SELECT command FROM history";
    let sql = if conn.prepare(LIVE_ONLY).is_ok() {
        LIVE_ONLY
    } else {
        EVERYTHING
    };

    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

    let mut counts = HashMap::new();
    for row in rows {
        for cmd in extract_invocations(&row?) {
            *counts.entry(cmd).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

/// Command frequencies from a plain history file (zsh, bash, fish).
pub fn history_file_counts(path: &Path) -> Result<HashMap<String, u32>> {
    let raw = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    // History files routinely contain invalid UTF-8 from mistyped input.
    let text = String::from_utf8_lossy(&raw);

    let mut counts = HashMap::new();
    for line in text.lines() {
        for cmd in extract_invocations(strip_history_metadata(line)) {
            *counts.entry(cmd).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

/// zsh extended history is `: <epoch>:<elapsed>;<command>`; fish is `- cmd: …`.
fn strip_history_metadata(line: &str) -> &str {
    if let Some(rest) = line.strip_prefix(": ")
        && let Some((meta, cmd)) = rest.split_once(';')
        && meta.chars().all(|c| c.is_ascii_digit() || c == ':')
    {
        return cmd;
    }
    line.strip_prefix("- cmd: ").unwrap_or(line)
}

/// When a binary was last executed, according to the filesystem.
///
/// This is the only signal that sees commands run by scripts, CI and coding
/// agents. Package managers hand out symlinks whose own access time never
/// moves, so the link has to be resolved before asking.
pub fn last_exec(path: &Path) -> Option<SystemTime> {
    let target = fs::canonicalize(path).ok()?;
    fs::metadata(target).ok()?.accessed().ok()
}

/// Every bare word appearing in a shell startup file.
///
/// Coarse on purpose: a plugin manager mentioned anywhere in your config is
/// doing something, and that is enough to spare it from the removal list.
pub fn shell_config_mentions() -> HashSet<String> {
    let mut words = HashSet::new();
    for file in shell_config_files() {
        let Ok(raw) = fs::read(&file) else { continue };
        for line in String::from_utf8_lossy(&raw).lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            for word in line
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'))
            {
                if word.len() > 1 {
                    words.insert(word.to_string());
                }
            }
        }
    }
    words
}

fn shell_config_files() -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };

    let mut files: Vec<PathBuf> = [
        ".zshrc",
        ".zshenv",
        ".zprofile",
        ".zlogin",
        ".bashrc",
        ".bash_profile",
        ".profile",
        ".config/fish/config.fish",
    ]
    .iter()
    .map(|name| home.join(name))
    .filter(|p| p.is_file())
    .collect();

    // XDG layouts keep the real config in a directory of sourced fragments.
    let mut dirs = vec![home.join(".config/zsh"), home.join(".config/fish/conf.d")];
    if let Ok(zdotdir) = std::env::var("ZDOTDIR") {
        dirs.push(PathBuf::from(zdotdir));
    }
    for dir in dirs {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        files.extend(entries.flatten().map(|e| e.path()).filter(|p| p.is_file()));
    }

    files
}

/// Aliases currently defined in the running shell, as `alias -> target`.
///
/// This is what rescues a tool like `bat` from looking dead: it is used all
/// day long, but always spelled `cat`.
pub fn alias_map() -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    for file in shell_config_files() {
        let Ok(raw) = fs::read(&file) else { continue };
        for line in String::from_utf8_lossy(&raw).lines() {
            if let Some((name, target)) = parse_alias(line) {
                aliases.insert(name, target);
            }
        }
    }
    aliases
}

/// `alias cat='bat --paging=never'` becomes `("cat", "bat")`.
fn parse_alias(line: &str) -> Option<(String, String)> {
    let rest = line.trim().strip_prefix("alias ")?;
    let (name, value) = rest.split_once('=')?;
    // `alias -g` and friends: the flag is not an alias name.
    let name = name.trim();
    if name.is_empty() || name.starts_with('-') {
        return None;
    }
    let value = value.trim_matches(|c| matches!(c, '"' | '\'' | ' '));
    let target = first_command(value)?;
    // A self-referencing alias teaches us nothing.
    (target != name).then(|| (name.to_string(), target))
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Default Atuin database location, honouring XDG overrides.
pub fn default_atuin_db() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) => PathBuf::from(dir),
        None => home_dir()?.join(".local/share"),
    };
    let db = base.join("atuin/history.db");
    db.is_file().then_some(db)
}

/// Plain history files worth reading when Atuin is not in use.
pub fn default_history_files() -> Vec<PathBuf> {
    let Some(home) = home_dir() else {
        return Vec::new();
    };
    let mut candidates = Vec::new();
    if let Some(histfile) = std::env::var_os("HISTFILE") {
        candidates.push(PathBuf::from(histfile));
    }
    candidates.extend(
        [
            ".zsh_history",
            ".bash_history",
            ".local/share/fish/fish_history",
        ]
        .iter()
        .map(|name| home.join(name)),
    );
    candidates.retain(|p| p.is_file());
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_command_not_its_flags() {
        assert_eq!(extract_invocations("git status -sb"), vec!["git"]);
    }

    #[test]
    fn sees_past_pipes_and_operators() {
        // Only counting the first word would report rg and make as unused.
        assert_eq!(
            extract_invocations("git log | rg fix && make build"),
            vec!["git", "rg", "make"]
        );
    }

    #[test]
    fn skips_wrappers_that_hide_the_real_command() {
        assert_eq!(
            extract_invocations("sudo systemctl restart"),
            vec!["systemctl"]
        );
        assert_eq!(
            extract_invocations("env FOO=1 kubectl get pods"),
            vec!["kubectl"]
        );
    }

    #[test]
    fn strips_leading_environment_assignments() {
        assert_eq!(
            extract_invocations("RUST_LOG=debug cargo test"),
            vec!["cargo"]
        );
    }

    #[test]
    fn reduces_absolute_paths_to_the_binary_name() {
        assert_eq!(extract_invocations("/opt/homebrew/bin/rg foo"), vec!["rg"]);
    }

    #[test]
    fn reads_command_substitutions() {
        assert_eq!(
            extract_invocations("echo $(brew --prefix)"),
            vec!["echo", "brew"]
        );
    }

    #[test]
    fn handles_zsh_extended_history_format() {
        assert_eq!(
            strip_history_metadata(": 1748000000:0;helm upgrade"),
            "helm upgrade"
        );
    }

    #[test]
    fn leaves_ordinary_lines_alone() {
        assert_eq!(strip_history_metadata("helm upgrade"), "helm upgrade");
    }

    #[test]
    fn does_not_mistake_a_colon_command_for_history_metadata() {
        assert_eq!(strip_history_metadata(": not;metadata"), ": not;metadata");
    }

    #[test]
    fn ignores_empty_and_flag_only_segments() {
        assert!(extract_invocations("  ").is_empty());
        assert!(extract_invocations("--help").is_empty());
    }
}
