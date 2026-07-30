//! Rendering. Two audiences: a human skimming a terminal, and a script.

use std::io::IsTerminal;
use std::time::SystemTime;

use serde_json::{Value, json};

use crate::Assessment;
use crate::classify::Verdict;

struct Style {
    bold: &'static str,
    dim: &'static str,
    reset: &'static str,
}

impl Style {
    fn detect() -> Self {
        // NO_COLOR is a de facto standard; respecting it costs one line.
        let plain = !std::io::stdout().is_terminal() || std::env::var_os("NO_COLOR").is_some();
        if plain {
            Style {
                bold: "",
                dim: "",
                reset: "",
            }
        } else {
            Style {
                bold: "\x1b[1m",
                dim: "\x1b[2m",
                reset: "\x1b[0m",
            }
        }
    }
}

pub fn text(assessments: &[Assessment], now: SystemTime, detailed: bool) {
    let s = Style::detect();

    println!(
        "{}shelfware{} — {} packages inspected",
        s.bold,
        s.reset,
        assessments.len()
    );
    println!();

    for verdict in Verdict::ALL {
        let count = assessments.iter().filter(|a| a.verdict == verdict).count();
        println!(
            "  {:<12} {:>4}   {}{}{}",
            verdict.label(),
            count,
            s.dim,
            verdict.headline(),
            s.reset
        );
    }
    println!();

    // Scoped npm names like `@scope/tool` are long; size the column to the
    // data rather than letting every row wrap.
    let name_width = assessments
        .iter()
        .map(|a| a.package.name.len())
        .max()
        .unwrap_or(20)
        .clamp(20, 44);

    for verdict in Verdict::ALL {
        // Active and indirect packages are the ones you keep; listing all of
        // them buries the handful you can act on.
        if !detailed && !verdict.is_removal_candidate() && verdict != Verdict::Integration {
            continue;
        }
        let group: Vec<&Assessment> = assessments
            .iter()
            .filter(|a| a.verdict == verdict)
            .collect();
        if group.is_empty() {
            continue;
        }

        println!(
            "{}{}{} — {}",
            s.bold,
            verdict.label().to_uppercase(),
            s.reset,
            verdict.headline()
        );
        for a in group {
            println!(
                "  {:<width$} {:<7} {}{}{}",
                a.package.name,
                a.package.manager,
                s.dim,
                evidence(a, now),
                s.reset,
                width = name_width
            );
        }
        println!();
    }

    let candidates: Vec<&str> = assessments
        .iter()
        .filter(|a| a.verdict == Verdict::Unused)
        .map(|a| a.package.name.as_str())
        .collect();

    if candidates.is_empty() {
        println!("Nothing looks abandoned. Your shelf is clean.");
    } else {
        // Deliberately not "never ran": a package lands here when nothing has
        // used it *lately*, which is a weaker and more honest claim.
        println!(
            "{}Review these {} first{} — no recent sign of use:",
            s.bold,
            candidates.len(),
            s.reset
        );
        println!("  {}", candidates.join(" "));
        println!();
        println!(
            "{}Check before removing.{} A tool used only inside containers, on \
             another machine, or by a job that has not run this month leaves no \
             trace here.",
            s.dim, s.reset
        );
    }
}

/// One line of "why we said that", so a verdict can be argued with.
fn evidence(a: &Assessment, now: SystemTime) -> String {
    let typed = match a.signals.interactive_hits {
        0 => "never typed".to_string(),
        1 => "typed once".to_string(),
        n => format!("typed {n}×"),
    };
    let run = match a.signals.last_exec {
        Some(t) => format!("last run {}", ago(t, now)),
        None => "never run".to_string(),
    };
    let wired = if a.signals.configured {
        " · in shell config"
    } else {
        ""
    };
    format!("{typed} · {run}{wired}")
}

fn ago(then: SystemTime, now: SystemTime) -> String {
    let Ok(delta) = now.duration_since(then) else {
        return "just now".to_string();
    };
    let days = delta.as_secs() / 86_400;
    match days {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        2..=60 => format!("{days}d ago"),
        _ => format!("{}mo ago", days / 30),
    }
}

pub fn json(assessments: &[Assessment], now: SystemTime) -> Value {
    let packages: Vec<Value> = assessments
        .iter()
        .map(|a| {
            json!({
                "name": a.package.name,
                "manager": a.package.manager,
                "verdict": a.verdict.label(),
                "interactive_hits": a.signals.interactive_hits,
                "last_exec_days_ago": a.signals.last_exec.map(|t| {
                    now.duration_since(t).map(|d| d.as_secs() / 86_400).unwrap_or(0)
                }),
                "in_shell_config": a.signals.configured,
                "binaries": a.package.binaries.iter().map(|b| &b.name).collect::<Vec<_>>(),
            })
        })
        .collect();

    json!({ "packages": packages })
}
