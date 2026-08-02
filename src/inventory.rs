//! What is installed, and what each package actually puts on your PATH.
//!
//! Every collector is best-effort: a missing package manager is not an error,
//! it just contributes nothing. Reporting requires knowing the binaries a
//! package owns, because you type `rg`, never `ripgrep`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

#[derive(Debug, Clone)]
pub struct Binary {
    pub name: String,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub manager: Manager,
    pub binaries: Vec<Binary>,
}

impl Package {
    /// The name the package is known by locally. See [`short_name`].
    pub fn short_name(&self) -> &str {
        short_name(&self.name)
    }
}

/// Strip the namespace from a package name: `org/tap/tool` is installed as
/// `tool`, and the npm package `@scope/tool` provides `tool`.
///
/// One definition, because this is the name used to find a package's install
/// directory, to match it against usage, and to match it against the ignore
/// list — three places that must not be allowed to disagree.
pub fn short_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// An enum rather than a string so that shells can complete the values, and
/// so that adding a manager makes the compiler point at every place that has
/// to handle it — uninstall commands in particular.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Manager {
    Brew,
    Cargo,
    Uv,
    Pipx,
    Npm,
    Go,
}

impl Manager {
    pub const ALL: [Manager; 6] = [
        Manager::Brew,
        Manager::Cargo,
        Manager::Uv,
        Manager::Pipx,
        Manager::Npm,
        Manager::Go,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Manager::Brew => "brew",
            Manager::Cargo => "cargo",
            Manager::Uv => "uv",
            Manager::Pipx => "pipx",
            Manager::Npm => "npm",
            Manager::Go => "go",
        }
    }
}

pub fn collect(managers: &[Manager]) -> Vec<Package> {
    let mut packages = Vec::new();
    for manager in managers {
        packages.extend(match manager {
            Manager::Brew => brew(),
            Manager::Cargo => cargo(),
            Manager::Uv => uv(),
            Manager::Pipx => pipx(),
            Manager::Npm => npm(),
            Manager::Go => go(),
        });
    }
    packages
}

/// Homebrew, restricted to `leaves`: packages you asked for, rather than the
/// dependencies they dragged in. Suggesting removal of a dependency would be
/// both useless and dangerous.
fn brew() -> Vec<Package> {
    let Some(prefix) = run("brew", &["--prefix"]) else {
        return Vec::new();
    };
    let prefix = PathBuf::from(prefix.trim());
    let Some(leaves) = run("brew", &["leaves"]) else {
        return Vec::new();
    };
    parse_brew(&leaves, &prefix)
}

fn parse_brew(leaves: &str, prefix: &Path) -> Vec<Package> {
    leaves
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| {
            // Tapped formulae are named `org/tap/tool`, installed as `tool`.
            let short = short_name(name);
            let binaries = bins_in(&prefix.join("opt").join(short).join("bin"))
                .unwrap_or_else(|| vec![resolve(short)]);
            Package {
                name: name.to_string(),
                manager: Manager::Brew,
                binaries,
            }
        })
        .collect()
}

/// `cargo install --list` prints `name v1.2.3:` then its binaries, indented.
fn cargo() -> Vec<Package> {
    let Some(out) = run("cargo", &["install", "--list"]) else {
        return Vec::new();
    };
    parse_cargo(&out, cargo_bin_dir().as_deref())
}

fn parse_cargo(out: &str, bin_dir: Option<&Path>) -> Vec<Package> {
    let mut packages: Vec<Package> = Vec::new();

    for line in out.lines() {
        let indented = line.starts_with(' ') || line.starts_with('\t');
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if indented {
            if let Some(pkg) = packages.last_mut() {
                pkg.binaries.push(named(trimmed, bin_dir));
            }
        } else if let Some(name) = trimmed.strip_suffix(':') {
            let name = name.split_whitespace().next().unwrap_or(name);
            packages.push(Package {
                name: name.to_string(),
                manager: Manager::Cargo,
                binaries: Vec::new(),
            });
        }
    }
    packages
}

/// `uv tool list` prints `name v1.2.3` then `- binary` lines.
fn uv() -> Vec<Package> {
    let Some(out) = run("uv", &["tool", "list"]) else {
        return Vec::new();
    };
    parse_uv(&out)
}

fn parse_uv(out: &str) -> Vec<Package> {
    let mut packages: Vec<Package> = Vec::new();

    for line in out.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match trimmed.strip_prefix("- ") {
            Some(bin) => {
                if let Some(pkg) = packages.last_mut() {
                    pkg.binaries.push(resolve(bin.trim()));
                }
            }
            None => {
                let name = trimmed.split_whitespace().next().unwrap_or(trimmed);
                packages.push(Package {
                    name: name.to_string(),
                    manager: Manager::Uv,
                    binaries: Vec::new(),
                });
            }
        }
    }
    packages
}

/// pipx exposes the exact list of apps per venv, which is the most reliable
/// package-to-binary mapping of any manager here.
fn pipx() -> Vec<Package> {
    let Some(out) = run("pipx", &["list", "--json"]) else {
        return Vec::new();
    };
    parse_pipx(&out)
}

fn parse_pipx(out: &str) -> Vec<Package> {
    let Ok(json) = serde_json::from_str::<Value>(out) else {
        return Vec::new();
    };
    let Some(venvs) = json.get("venvs").and_then(Value::as_object) else {
        return Vec::new();
    };

    venvs
        .iter()
        .map(|(name, venv)| {
            let apps = venv
                .pointer("/metadata/main_package/apps")
                .and_then(Value::as_array)
                .map(|apps| {
                    apps.iter()
                        .filter_map(Value::as_str)
                        .map(resolve)
                        .collect::<Vec<_>>()
                })
                .filter(|apps| !apps.is_empty())
                .unwrap_or_else(|| vec![resolve(name)]);
            Package {
                name: name.clone(),
                manager: Manager::Pipx,
                binaries: apps,
            }
        })
        .collect()
}

/// npm does not report the binaries a global package installs, so the package
/// name (minus its scope) is the best guess available.
fn npm() -> Vec<Package> {
    let Some(out) = run("npm", &["ls", "-g", "--depth=0", "--json"]) else {
        return Vec::new();
    };
    parse_npm(&out)
}

fn parse_npm(out: &str) -> Vec<Package> {
    let Ok(json) = serde_json::from_str::<Value>(out) else {
        return Vec::new();
    };
    let Some(deps) = json.get("dependencies").and_then(Value::as_object) else {
        return Vec::new();
    };

    deps.keys()
        .filter(|name| name.as_str() != "npm")
        .map(|name| {
            let short = short_name(name);
            Package {
                name: name.clone(),
                manager: Manager::Npm,
                binaries: vec![resolve(short)],
            }
        })
        .collect()
}

/// `go install` leaves binaries in GOPATH/bin with no manifest, so each
/// binary is treated as its own package.
fn go() -> Vec<Package> {
    let Some(gopath) = run("go", &["env", "GOPATH"]) else {
        return Vec::new();
    };
    let bin_dir = PathBuf::from(gopath.trim()).join("bin");
    let Some(binaries) = bins_in(&bin_dir) else {
        return Vec::new();
    };

    binaries
        .into_iter()
        .map(|bin| Package {
            name: bin.name.clone(),
            manager: Manager::Go,
            binaries: vec![bin],
        })
        .collect()
}

fn cargo_bin_dir() -> Option<PathBuf> {
    let dir = match std::env::var_os("CARGO_HOME") {
        Some(home) => PathBuf::from(home).join("bin"),
        None => crate::signals::home_dir()?.join(".cargo/bin"),
    };
    dir.is_dir().then_some(dir)
}

/// Executables in a directory, or `None` when the directory does not exist.
fn bins_in(dir: &Path) -> Option<Vec<Binary>> {
    let entries = std::fs::read_dir(dir).ok()?;
    // BTreeMap keeps output stable across runs, which matters for diffing
    // reports over time.
    let bins: BTreeMap<String, PathBuf> = entries
        .flatten()
        .filter(|e| !e.path().is_dir())
        .filter_map(|e| Some((e.file_name().into_string().ok()?, e.path())))
        .collect();

    (!bins.is_empty()).then(|| {
        bins.into_iter()
            .map(|(name, path)| Binary {
                name,
                path: Some(path),
            })
            .collect()
    })
}

fn named(name: &str, dir: Option<&Path>) -> Binary {
    match dir.map(|d| d.join(name)).filter(|p| p.exists()) {
        Some(path) => Binary {
            name: name.to_string(),
            path: Some(path),
        },
        None => resolve(name),
    }
}

/// Locate a binary on PATH so its access time can be read later.
fn resolve(name: &str) -> Binary {
    let path = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.exists())
    });
    Binary {
        name: name.to_string(),
        path,
    }
}

/// Run a command, returning its stdout only when it actually succeeded.
fn run(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bin_names(pkg: &Package) -> Vec<&str> {
        pkg.binaries.iter().map(|b| b.name.as_str()).collect()
    }

    #[test]
    fn parses_cargo_install_list() {
        let out = "cargo-watch v8.5.3:\n    cargo-watch\nripgrep v14.1.0:\n    rg\n";
        let pkgs = parse_cargo(out, None);

        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[0].name, "cargo-watch");
        // The reason this mapping exists at all: the package is `ripgrep`,
        // but what you type — and what the history records — is `rg`.
        assert_eq!(pkgs[1].name, "ripgrep");
        assert_eq!(bin_names(&pkgs[1]), ["rg"]);
    }

    #[test]
    fn cargo_accepts_tab_indentation_and_several_binaries() {
        let pkgs = parse_cargo("coreutils v0.1.0:\n\tls\n\tcat\n", None);
        assert_eq!(bin_names(&pkgs[0]), ["ls", "cat"]);
    }

    #[test]
    fn parses_uv_tool_list() {
        let out = "ruff v0.5.0\n- ruff\nhttpie v3.2.0\n- http\n- https\n";
        let pkgs = parse_uv(out);

        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs[1].name, "httpie");
        assert_eq!(bin_names(&pkgs[1]), ["http", "https"]);
    }

    #[test]
    fn parses_pipx_apps() {
        let out = r#"{"venvs":{"ansible-core":{"metadata":{"main_package":
                     {"apps":["ansible","ansible-playbook"]}}}}}"#;
        let pkgs = parse_pipx(out);

        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0].name, "ansible-core");
        assert_eq!(bin_names(&pkgs[0]), ["ansible", "ansible-playbook"]);
    }

    #[test]
    fn pipx_falls_back_to_the_package_name_when_apps_are_missing() {
        let pkgs = parse_pipx(r#"{"venvs":{"sometool":{"metadata":{}}}}"#);
        assert_eq!(bin_names(&pkgs[0]), ["sometool"]);
    }

    #[test]
    fn parses_npm_globals_and_drops_npm_itself() {
        let out = r#"{"dependencies":{"npm":{},"typescript":{},"@scope/some-tool":{}}}"#;
        let pkgs = parse_npm(out);
        let names: Vec<&str> = pkgs.iter().map(|p| p.name.as_str()).collect();

        // npm installs itself globally; proposing its removal would be absurd.
        assert!(!names.contains(&"npm"));
        assert_eq!(names.len(), 2);

        // Scoped packages are *guessed* to provide the unscoped name. This is
        // the weakest mapping in the tool, and `clean` warns about it.
        let scoped = pkgs.iter().find(|p| p.name == "@scope/some-tool").unwrap();
        assert_eq!(bin_names(scoped), ["some-tool"]);
    }

    #[test]
    fn parses_brew_leaves_including_tapped_formulae() {
        let pkgs = parse_brew("git\n\nhashicorp/tap/vault\n", Path::new("/nonexistent"));

        assert_eq!(pkgs.len(), 2, "blank lines are not packages");
        assert_eq!(pkgs[1].name, "hashicorp/tap/vault");
        assert_eq!(pkgs[1].short_name(), "vault");
    }

    /// A package manager changing its output format should make this tool
    /// report nothing, never crash and never invent packages.
    #[test]
    fn unexpected_output_degrades_quietly() {
        assert!(parse_pipx("not json at all").is_empty());
        assert!(parse_npm("{}").is_empty());
        assert!(parse_npm("<html>error</html>").is_empty());
        assert!(parse_cargo("", None).is_empty());
        assert!(parse_uv("").is_empty());
        assert!(parse_brew("   \n\n", Path::new("/nonexistent")).is_empty());
    }

    /// Binaries listed before any package header have nothing to attach to.
    #[test]
    fn orphan_binary_lines_are_ignored() {
        assert!(parse_cargo("    rg\n", None).is_empty());
        assert!(parse_uv("- ruff\n").is_empty());
    }

    #[test]
    fn short_name_strips_namespaces() {
        // A tapped Homebrew formula, installed under its bare name.
        assert_eq!(short_name("hashicorp/tap/vault"), "vault");
        // A scoped npm package.
        assert_eq!(short_name("@acme/legacy-codegen"), "legacy-codegen");
        // Anything else is already short.
        assert_eq!(short_name("ripgrep"), "ripgrep");
    }

    /// Same reasoning as the verdict labels: two spellings of one name, kept
    /// honest by a test rather than by discipline.
    #[test]
    fn manager_names_match_the_values_the_cli_accepts() {
        use clap::ValueEnum;
        for manager in Manager::ALL {
            let cli_value = manager.to_possible_value().expect("manager is selectable");
            assert_eq!(manager.as_str(), cli_value.get_name());
        }
    }

    #[test]
    fn all_lists_every_manager() {
        use clap::ValueEnum;
        assert_eq!(Manager::ALL.len(), Manager::value_variants().len());
    }
}
