use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use which::which;

use super::PackageManager;
use crate::config::Dependency;

pub struct Homebrew;

/// Returns true when the services output contains a line for `name` that shows "started".
fn parse_brew_service_running(stdout: &str, name: &str) -> bool {
    stdout
        .lines()
        .any(|line| line.starts_with(name) && line.contains("started"))
}

/// Parses `brew list --versions` output and extracts the version (second whitespace token).
fn parse_brew_version(line: &str) -> Option<String> {
    line.split_whitespace().nth(1).map(String::from)
}

impl Homebrew {
    pub fn new() -> Self {
        Self
    }

    fn brew_bin(&self) -> &'static str {
        if cfg!(target_arch = "aarch64") {
            "/opt/homebrew/bin/brew"
        } else {
            "/usr/local/bin/brew"
        }
    }

    fn run(&self, args: &[&str]) -> Result<std::process::Output> {
        Command::new(self.brew_bin())
            .args(args)
            .output()
            .with_context(|| format!("Failed to run: brew {}", args.join(" ")))
    }

    fn run_interactive(&self, args: &[&str]) -> Result<()> {
        let status = Command::new(self.brew_bin())
            .args(args)
            .status()
            .with_context(|| format!("Failed to run: brew {}", args.join(" ")))?;
        if !status.success() {
            bail!("brew {} exited with non-zero status", args.join(" "));
        }
        Ok(())
    }
}

impl PackageManager for Homebrew {
    fn name(&self) -> &str {
        "brew"
    }

    fn is_available(&self) -> bool {
        which("brew").is_ok()
    }

    fn bootstrap(&self) -> Result<()> {
        let status = Command::new("sh")
            .arg("-c")
            .arg(concat!(
                "curl -fsSL ",
                "https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh",
                " | bash"
            ))
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run Homebrew install script")?;

        if !status.success() {
            bail!("Homebrew installation failed");
        }
        Ok(())
    }

    fn is_package_installed(&self, dep: &Dependency) -> Result<bool> {
        let pkg = dep.versioned_name();
        let output = self.run(&["list", "--versions", &pkg])?;
        Ok(output.status.success() && !String::from_utf8_lossy(&output.stdout).trim().is_empty())
    }

    fn install_package(&self, dep: &Dependency) -> Result<()> {
        if let Some(tap) = &dep.tap {
            validate_tap(tap)?;
            self.run_interactive(&["tap", tap])?;
        }
        self.run_interactive(&["install", &dep.versioned_name()])
    }

    fn is_service_running(&self, name: &str) -> Result<bool> {
        let output = self.run(&["services", "list"])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_brew_service_running(&stdout, name))
    }

    fn start_service(&self, name: &str) -> Result<()> {
        self.run_interactive(&["services", "start", name])
    }

    fn stop_service(&self, name: &str) -> Result<()> {
        self.run_interactive(&["services", "stop", name])
    }

    fn resolved_version(&self, dep: &Dependency) -> Result<Option<String>> {
        let output = self.run(&["list", "--versions", &dep.versioned_name()])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.trim();
        // Output: "formula 1.2.3" or "formula@major 1.2.3_4" — take second token.
        Ok(parse_brew_version(line))
    }

    /// Runs `brew --prefix <service>` as a subprocess — not cheap to call in a loop.
    fn service_config_dir(&self, service: &str) -> Option<PathBuf> {
        let output = Command::new(self.brew_bin())
            .args(["--prefix", service])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Some(std::path::Path::new(&prefix).join("etc"))
    }
}

/// Validates that a tap string has the form `org/repo` with no path components,
/// URL schemes, or shell-special characters. Prevents arbitrary GitHub repos from
/// being added via a malicious envy.yml tap field.
fn validate_tap(tap: &str) -> Result<()> {
    let parts: Vec<&str> = tap.split('/').collect();
    if parts.len() != 2 {
        bail!("Invalid tap '{}': must be 'org/repo' (exactly one '/')", tap);
    }
    for part in &parts {
        if part.is_empty() {
            bail!("Invalid tap '{}': org and repo must not be empty", tap);
        }
        if !part.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.') {
            bail!(
                "Invalid tap '{}': only alphanumeric characters, hyphens, underscores, and dots are allowed",
                tap
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_tap_accepts_valid_org_repo() {
        assert!(validate_tap("hashicorp/tap").is_ok());
        assert!(validate_tap("my-org/my-repo").is_ok());
        assert!(validate_tap("org.name/repo_name").is_ok());
    }

    #[test]
    fn validate_tap_rejects_path_components() {
        assert!(validate_tap("org/repo/extra").is_err());
        assert!(validate_tap("org").is_err());
    }

    #[test]
    fn validate_tap_rejects_empty_parts() {
        assert!(validate_tap("/repo").is_err());
        assert!(validate_tap("org/").is_err());
    }

    #[test]
    fn validate_tap_rejects_shell_special_chars() {
        assert!(validate_tap("org/repo;rm -rf /").is_err());
        assert!(validate_tap("org/repo$(evil)").is_err());
        assert!(validate_tap("https://github.com/org/repo").is_err());
    }

    // ── brew_bin ──────────────────────────────────────────────────────────────

    #[test]
    fn brew_bin_returns_non_empty_path() {
        let b = Homebrew::new().brew_bin();
        assert!(!b.is_empty(), "brew_bin must not be empty");
        assert!(b.contains("brew"), "Expected path to contain 'brew', got: {b}");
    }

    #[test]
    fn brew_bin_is_not_xyzzy() {
        assert_ne!(Homebrew::new().brew_bin(), "xyzzy");
    }

    // ── name ──────────────────────────────────────────────────────────────────

    #[test]
    fn brew_name_is_brew() {
        assert_eq!(Homebrew::new().name(), "brew");
    }

    // ── parse_brew_service_running ────────────────────────────────────────────

    #[test]
    fn parse_brew_service_running_started() {
        let stdout = "mysql started /some/path\nnginx stopped\n";
        assert!(parse_brew_service_running(stdout, "mysql"));
        assert!(!parse_brew_service_running(stdout, "nginx"));
    }

    #[test]
    fn parse_brew_service_running_requires_both_conditions() {
        let stdout = "nginx stopped\n";
        assert!(!parse_brew_service_running(stdout, "nginx"),
            "stopped service must not report as running");
        let stdout2 = "other started\nnginx none\n";
        assert!(!parse_brew_service_running(stdout2, "nginx"),
            "nginx must not match 'other started' line");
    }

    #[test]
    fn parse_brew_service_running_empty_returns_false() {
        assert!(!parse_brew_service_running("", "mysql"));
    }

    // ── parse_brew_version ────────────────────────────────────────────────────

    #[test]
    fn parse_brew_version_extracts_second_token() {
        assert_eq!(parse_brew_version("mysql 8.0.36"), Some("8.0.36".into()));
        assert_eq!(parse_brew_version("node@20 20.11.0"), Some("20.11.0".into()));
    }

    #[test]
    fn parse_brew_version_returns_none_for_single_token() {
        assert!(parse_brew_version("mysql").is_none());
        assert!(parse_brew_version("").is_none());
    }
}
