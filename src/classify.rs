//! Turning raw signals into a verdict.
//!
//! The hard part of this tool is not counting commands, it is *not* concluding
//! that a package is dead because you never type its name. Plenty of installed
//! software is used constantly without ever appearing at your prompt: linters
//! invoked by CI, tools called by coding agents, binaries hidden behind an
//! alias, shell plugins loaded at startup. Each of those has its own signal,
//! and the ordering below is what keeps them from being reported as garbage.

use std::time::{Duration, SystemTime};

/// What we think of a package, in decreasing order of confidence that it earns
/// its disk space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Verdict {
    Active,
    Indirect,
    Rare,
    Integration,
    Unused,
}

impl Verdict {
    pub const ALL: [Verdict; 5] = [
        Verdict::Active,
        Verdict::Indirect,
        Verdict::Rare,
        Verdict::Integration,
        Verdict::Unused,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Verdict::Active => "active",
            Verdict::Indirect => "indirect",
            Verdict::Rare => "rare",
            Verdict::Integration => "integration",
            Verdict::Unused => "unused",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|v| v.label() == label.to_lowercase())
    }

    pub fn headline(self) -> &'static str {
        match self {
            Verdict::Active => "Typed regularly at your prompt",
            Verdict::Indirect => "Executed recently, but not by you typing it",
            Verdict::Rare => "Typed occasionally, and not executed lately",
            Verdict::Integration => "Never executed, but wired into your shell config",
            Verdict::Unused => "No signal from any source — removal candidates",
        }
    }

    /// Whether removing this package is worth even considering.
    pub fn is_removal_candidate(self) -> bool {
        matches!(self, Verdict::Unused | Verdict::Rare)
    }
}

/// Everything we managed to observe about one package.
#[derive(Debug, Default, Clone)]
pub struct Signals {
    /// Times one of its binaries started a command in your shell history.
    pub interactive_hits: u32,
    /// Newest access time across its binaries, symlinks resolved.
    pub last_exec: Option<SystemTime>,
    /// Whether its name appears in a shell startup file.
    pub configured: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// Interactive hits at or above which a package is plainly alive.
    pub active_hits: u32,
    /// How recent an execution has to be to still count as "in use".
    pub stale_days: u64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            active_hits: 20,
            stale_days: 30,
        }
    }
}

/// Order matters: every rule below exists to rescue a package from a wrong
/// `Unused` verdict, so the cheapest-to-trust evidence is checked first.
pub fn classify(signals: &Signals, thresholds: Thresholds, now: SystemTime) -> Verdict {
    if signals.interactive_hits >= thresholds.active_hits {
        return Verdict::Active;
    }
    if executed_recently(signals.last_exec, thresholds.stale_days, now) {
        return Verdict::Indirect;
    }
    if signals.interactive_hits > 0 {
        return Verdict::Rare;
    }
    if signals.configured {
        return Verdict::Integration;
    }
    Verdict::Unused
}

fn executed_recently(last: Option<SystemTime>, stale_days: u64, now: SystemTime) -> bool {
    let Some(last) = last else { return false };
    match now.duration_since(last) {
        Ok(age) => age < Duration::from_secs(stale_days * 86_400),
        // A timestamp in the future means clock skew or a restored backup.
        // Either way the file was touched, so err on the side of keeping it.
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn days_ago(n: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(500 * 86_400 - n * 86_400)
    }

    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(500 * 86_400)
    }

    #[test]
    fn frequent_typing_is_active() {
        let s = Signals {
            interactive_hits: 3329,
            ..Default::default()
        };
        assert_eq!(classify(&s, Thresholds::default(), now()), Verdict::Active);
    }

    #[test]
    fn never_typed_but_recently_run_is_indirect() {
        // The ripgrep case: an agent or a Makefile runs it constantly, the
        // shell history has almost nothing to show for it.
        let s = Signals {
            interactive_hits: 0,
            last_exec: Some(days_ago(1)),
            configured: false,
        };
        assert_eq!(
            classify(&s, Thresholds::default(), now()),
            Verdict::Indirect
        );
    }

    #[test]
    fn recent_execution_outranks_a_thin_typing_record() {
        let s = Signals {
            interactive_hits: 5,
            last_exec: Some(days_ago(2)),
            configured: false,
        };
        assert_eq!(
            classify(&s, Thresholds::default(), now()),
            Verdict::Indirect
        );
    }

    #[test]
    fn typed_a_little_and_long_dormant_is_rare() {
        let s = Signals {
            interactive_hits: 3,
            last_exec: Some(days_ago(400)),
            configured: false,
        };
        assert_eq!(classify(&s, Thresholds::default(), now()), Verdict::Rare);
    }

    #[test]
    fn shell_plugins_are_integrations_not_garbage() {
        // The direnv/antidote case: loaded at every shell startup, yet nobody
        // has ever typed their name.
        let s = Signals {
            interactive_hits: 0,
            last_exec: None,
            configured: true,
        };
        assert_eq!(
            classify(&s, Thresholds::default(), now()),
            Verdict::Integration
        );
    }

    #[test]
    fn no_signal_at_all_is_unused() {
        assert_eq!(
            classify(&Signals::default(), Thresholds::default(), now()),
            Verdict::Unused
        );
    }

    #[test]
    fn future_timestamps_do_not_panic_and_count_as_recent() {
        let s = Signals {
            last_exec: Some(now() + Duration::from_secs(86_400)),
            ..Default::default()
        };
        assert_eq!(
            classify(&s, Thresholds::default(), now()),
            Verdict::Indirect
        );
    }

    #[test]
    fn thresholds_are_honoured() {
        let s = Signals {
            interactive_hits: 6,
            ..Default::default()
        };
        let strict = Thresholds {
            active_hits: 5,
            stale_days: 30,
        };
        assert_eq!(classify(&s, strict, now()), Verdict::Active);
    }
}
