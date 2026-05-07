use anyhow::{Context, Result, bail};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use which::which;

use super::PackageManager;
use crate::config::Dependency;
use crate::output;

#[derive(Default)]
pub struct Homebrew;

/// Parses `brew services info --json` output and returns whether the service is running.
/// The JSON is an array; the first element has a `"running"` boolean field.
fn parse_brew_service_info_json(stdout: &[u8]) -> Result<bool> {
    let json: serde_json::Value =
        serde_json::from_slice(stdout).context("Failed to parse `brew services info` JSON")?;
    let arr = json
        .as_array()
        .context("`brew services info` output was not a JSON array")?;
    if arr.is_empty() {
        anyhow::bail!(
            "`brew services info` returned an empty array — service may not be managed by brew"
        );
    }
    Ok(arr[0]["running"].as_bool().unwrap_or(false))
}

/// Parses `brew list --versions` output and extracts the version (second whitespace token).
fn parse_brew_version(line: &str) -> Option<String> {
    line.split_whitespace().nth(1).map(String::from)
}

impl Homebrew {
    fn brew_bin(&self) -> String {
        if let Ok(prefix) = std::env::var("HOMEBREW_PREFIX")
            && !prefix.is_empty()
        {
            return format!("{prefix}/bin/brew");
        }
        let default = if cfg!(target_arch = "aarch64") {
            "/opt/homebrew/bin/brew"
        } else {
            "/usr/local/bin/brew"
        };
        default.to_string()
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

    fn fetch_config_dir(&self, service: &str) -> Option<PathBuf> {
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

impl PackageManager for Homebrew {
    fn name(&self) -> &str {
        "brew"
    }

    fn is_available(&self) -> bool {
        which("brew").is_ok()
    }

    /// Installs Homebrew by fetching and executing the official install script.
    /// This is the only supported installation method. The script is fetched over
    /// HTTPS from raw.githubusercontent.com and executed via bash — no hash
    /// verification is performed. Users in high-security environments should
    /// install Homebrew manually before running `devy up`.
    /// Set `DEVY_NO_BOOTSTRAP=1` to bail instead of running the installer.
    fn bootstrap(&self) -> Result<()> {
        if std::env::var_os("DEVY_NO_BOOTSTRAP").is_some() {
            bail!(
                "Homebrew is not installed and DEVY_NO_BOOTSTRAP is set.\n\
                 Install Homebrew manually: https://brew.sh"
            );
        }
        output::step("Bootstrapping Homebrew (fetching install script from GitHub via bash)");
        output::warn("No hash verification is performed. See https://brew.sh for manual install.");
        let status = Command::new("sh")
            .arg("-c")
            .arg(concat!(
                "curl --connect-timeout 30 --max-time 300 -fsSL ",
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
        let output = self.run(&["services", "info", "--json", name])?;
        if !output.status.success() {
            return Ok(false);
        }
        parse_brew_service_info_json(&output.stdout)
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

    fn service_config_dir(&self, service: &str) -> Option<PathBuf> {
        self.fetch_config_dir(service)
    }

    fn validate_config(&self, dep: &Dependency) -> Result<()> {
        if let Some(ref tap) = dep.tap {
            validate_tap(tap).with_context(|| format!("{}: invalid tap", dep.name))?;
        }
        Ok(())
    }
}

/// Validates that a tap string has the form `org/repo` with no path components,
/// URL schemes, or shell-special characters. Prevents arbitrary GitHub repos from
/// being added via a malicious devy.yml tap field.
pub(crate) fn validate_tap(tap: &str) -> Result<()> {
    let parts: Vec<&str> = tap.split('/').collect();
    if parts.len() != 2 {
        bail!(
            "Invalid tap '{}': must be 'org/repo' (exactly one '/')",
            tap
        );
    }
    for part in &parts {
        if part.is_empty() {
            bail!("Invalid tap '{}': org and repo must not be empty", tap);
        }
        if !part
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.')
        {
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
        let b = Homebrew::default().brew_bin();
        assert!(!b.is_empty(), "brew_bin must not be empty");
        assert!(
            b.contains("brew"),
            "Expected path to contain 'brew', got: {b}"
        );
    }

    #[test]
    fn brew_bin_uses_homebrew_prefix_env_var() {
        let _guard = crate::test_support::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("HOMEBREW_PREFIX").ok();
        // SAFETY: serialised by ENV_LOCK; HOMEBREW_PREFIX is only read by brew_bin().
        unsafe { std::env::set_var("HOMEBREW_PREFIX", "/custom/homebrew") };
        let result = Homebrew::default().brew_bin();
        unsafe {
            match prev {
                Some(v) => std::env::set_var("HOMEBREW_PREFIX", v),
                None => std::env::remove_var("HOMEBREW_PREFIX"),
            }
        }
        assert_eq!(result, "/custom/homebrew/bin/brew");
    }

    #[test]
    fn brew_bin_falls_back_to_hardcoded_when_prefix_absent() {
        let _guard = crate::test_support::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("HOMEBREW_PREFIX").ok();
        // SAFETY: serialised by ENV_LOCK; HOMEBREW_PREFIX is only read by brew_bin().
        unsafe { std::env::remove_var("HOMEBREW_PREFIX") };
        let result = Homebrew::default().brew_bin();
        unsafe {
            match prev {
                Some(v) => std::env::set_var("HOMEBREW_PREFIX", v),
                None => std::env::remove_var("HOMEBREW_PREFIX"),
            }
        }
        assert!(result.contains("homebrew") || result.contains("local"));
        assert!(result.ends_with("/bin/brew"));
    }

    // ── name ──────────────────────────────────────────────────────────────────

    #[test]
    fn brew_name_is_brew() {
        assert_eq!(Homebrew::default().name(), "brew");
    }

    // ── parse_brew_service_info_json ─────────────────────────────────────────

    #[test]
    fn parse_brew_service_info_json_returns_true_when_running() {
        let json = br#"[{"name":"mysql","running":true}]"#;
        assert!(parse_brew_service_info_json(json).unwrap());
    }

    #[test]
    fn parse_brew_service_info_json_returns_false_when_stopped() {
        let json = br#"[{"name":"mysql","running":false}]"#;
        assert!(!parse_brew_service_info_json(json).unwrap());
    }

    #[test]
    fn parse_brew_service_info_json_returns_false_on_malformed_json() {
        let bad = b"not json";
        assert!(parse_brew_service_info_json(bad).is_err());
    }

    #[test]
    fn parse_brew_service_info_json_returns_false_when_running_absent() {
        // "running" key missing — default to false (service status unknown = not running)
        let json = br#"[{"name":"mysql","status":"stopped"}]"#;
        assert!(!parse_brew_service_info_json(json).unwrap());
    }

    #[test]
    fn parse_brew_service_info_json_returns_err_on_empty_array() {
        // Empty array means brew does not manage the service — must error, not silently return false.
        let json = b"[]";
        assert!(
            parse_brew_service_info_json(json).is_err(),
            "empty array must return Err, not silently return false"
        );
    }

    #[test]
    fn parse_brew_service_info_json_returns_err_on_non_array() {
        let json = br#"{"name":"mysql","running":true}"#;
        assert!(
            parse_brew_service_info_json(json).is_err(),
            "non-array JSON must return Err"
        );
    }

    // ── bootstrap ────────────────────────────────────────────────────────────

    #[test]
    fn bootstrap_returns_err_when_devy_no_bootstrap_set() {
        let _guard = crate::test_support::ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // SAFETY: serialised by ENV_LOCK; DEVY_NO_BOOTSTRAP is only read by bootstrap().
        unsafe { std::env::set_var("DEVY_NO_BOOTSTRAP", "1") };
        let result = Homebrew::default().bootstrap();
        unsafe { std::env::remove_var("DEVY_NO_BOOTSTRAP") };
        assert!(
            result.is_err(),
            "bootstrap must bail when DEVY_NO_BOOTSTRAP is set"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("DEVY_NO_BOOTSTRAP"),
            "error must mention DEVY_NO_BOOTSTRAP, got: {msg}"
        );
    }

    // ── parse_brew_version ────────────────────────────────────────────────────

    #[test]
    fn parse_brew_version_extracts_second_token() {
        assert_eq!(parse_brew_version("mysql 8.0.36"), Some("8.0.36".into()));
        assert_eq!(
            parse_brew_version("node@20 20.11.0"),
            Some("20.11.0".into())
        );
    }

    #[test]
    fn parse_brew_version_returns_none_for_single_token() {
        assert!(parse_brew_version("mysql").is_none());
        assert!(parse_brew_version("").is_none());
    }
}
