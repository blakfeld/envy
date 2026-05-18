use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::PackageManager;
use crate::config::Dependency;
use crate::modules;
use crate::output;

pub struct NixPackageManager {
    /// Project-local Nix profile: `<project_root>/.devy/nix-profile`.
    /// All installs target this profile so packages are scoped to the project.
    profile_path: PathBuf,
    /// Cached result of probing whether the installed Nix supports flakes-era
    /// `nix profile` commands. Computed once on first use; safe to cache because
    /// style detection only runs after `ensure_available` completes.
    style: std::sync::OnceLock<NixStyle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NixStyle {
    /// Modern Nix with flakes: `nix profile install --profile <path> nixpkgs#attr`
    Profile,
    /// Legacy Nix: `nix-env --profile <path> -iA nixpkgs.attr`
    Env,
}

/// Searches PATH then well-known Nix installation directories for a binary.
/// Returns `None` only if the binary cannot be found anywhere.
fn find_nix_binary(name: &str) -> Option<PathBuf> {
    if let Ok(p) = which::which(name) {
        return Some(p);
    }
    // Standard paths for Nix installations that may not be on PATH in non-login shells:
    //   - Determinate Installer (macOS & Linux): /nix/var/nix/profiles/default/bin
    //   - Single-user official installer: ~/.nix-profile/bin
    //   - NixOS system profile: /run/current-system/sw/bin
    let home_nix = std::env::var("HOME")
        .map(|h| format!("{h}/.nix-profile/bin/{name}"))
        .unwrap_or_default();
    let candidates = [
        format!("/nix/var/nix/profiles/default/bin/{name}"),
        home_nix,
        format!("/run/current-system/sw/bin/{name}"),
    ];
    candidates
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .find(|p| p.exists())
}

/// Resolves a Nix binary by name, falling back to the bare name if not found.
fn resolve_nix_binary(name: &str) -> PathBuf {
    find_nix_binary(name).unwrap_or_else(|| PathBuf::from(name))
}

impl NixPackageManager {
    /// Construct with an explicit project root so the profile path is always
    /// relative to the project, not the shell's current working directory.
    pub fn for_project(project_root: &Path) -> Self {
        let profile_path = project_root.join(".devy").join("nix-profile");
        Self {
            profile_path,
            style: std::sync::OnceLock::new(),
        }
    }

    fn profile_bin(&self) -> PathBuf {
        self.profile_path.join("bin")
    }

    /// Resolves the `nix` binary on every call so post-bootstrap installs are
    /// picked up without requiring the caller to reconstruct the PM struct.
    fn nix_bin(&self) -> PathBuf {
        resolve_nix_binary("nix")
    }

    fn nix_env_bin(&self) -> PathBuf {
        resolve_nix_binary("nix-env")
    }

    fn effective_style(&self) -> NixStyle {
        *self.style.get_or_init(|| detect_style(&self.nix_bin()))
    }
}

/// Probe whether the installed Nix supports `nix profile` (flakes-era CLI).
fn detect_style(nix_bin: &Path) -> NixStyle {
    let ok = Command::new(nix_bin)
        .args(["profile", "list"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        NixStyle::Profile
    } else {
        output::info("nix profile list unavailable — using legacy nix-env install style");
        NixStyle::Env
    }
}

// ── Profile (flakes) helpers ──────────────────────────────────────────────────

fn profile_list_json(nix_bin: &Path, profile_path: &Path) -> Result<serde_json::Value> {
    let out = Command::new(nix_bin)
        .args(["profile", "list", "--json", "--profile"])
        .arg(profile_path)
        .output()
        .context("Failed to run `nix profile list --json`")?;
    serde_json::from_slice(&out.stdout).context("Failed to parse `nix profile list --json` output")
}

fn profile_find_pkg(json: &serde_json::Value, pname: &str) -> Option<String> {
    // Old format (Nix < 2.18): a JSON array of entries each with a "pname" field.
    if let Some(arr) = json.as_array() {
        return arr.iter().find_map(|entry| {
            if entry.get("pname")?.as_str()? == pname {
                Some(
                    entry
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                )
            } else {
                None
            }
        });
    }
    // New format (Nix ≥ 2.18): {"version":2,"elements":{"name":{...}}}
    // Element values may have "pname" directly, or the attr name is the last component
    // of "attrPath" (e.g. "legacyPackages.x86_64-linux.redis" → "redis").
    let elements = json.get("elements")?.as_object()?;
    elements.values().find_map(|entry| {
        let matched = entry
            .get("pname")
            .and_then(|p| p.as_str())
            .map(|ep| ep == pname)
            .unwrap_or_else(|| {
                entry
                    .get("attrPath")
                    .and_then(|a| a.as_str())
                    .and_then(|a| a.rsplit('.').next())
                    .map(|tail| tail == pname)
                    .unwrap_or(false)
            });
        if matched {
            Some(
                entry
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
            )
        } else {
            None
        }
    })
}

// ── nix-env (legacy) helpers ──────────────────────────────────────────────────

fn env_list_json(nix_env_bin: &Path, profile_path: &Path) -> Result<serde_json::Value> {
    let out = Command::new(nix_env_bin)
        .args(["--profile"])
        .arg(profile_path)
        .args(["-q", "--json"])
        .output()
        .context("Failed to run `nix-env -q --json`")?;
    serde_json::from_slice(&out.stdout).context("Failed to parse `nix-env -q --json` output")
}

fn env_find_pkg(json: &serde_json::Value, pname: &str) -> Option<String> {
    json.as_object()?.values().find_map(|entry| {
        let ep = entry.get("pname")?.as_str()?;
        if ep == pname {
            entry.get("version")?.as_str().map(String::from)
        } else {
            None
        }
    })
}

#[cfg(target_os = "macos")]
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ── Service management — macOS (launchd) ──────────────────────────────────────

#[cfg(target_os = "macos")]
fn launchagent_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    Ok(PathBuf::from(home).join("Library").join("LaunchAgents"))
}

#[cfg(target_os = "macos")]
fn launchagent_path(name: &str) -> Result<PathBuf> {
    Ok(launchagent_dir()?.join(format!("sh.devy.{name}.plist")))
}

#[cfg(target_os = "macos")]
fn write_launchagent(name: &str, exec: &str, profile_bin: &Path) -> Result<()> {
    let dir = launchagent_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;

    let bin = profile_bin.join(exec);
    let label = format!("sh.devy.{name}");
    let log = std::env::temp_dir().join(format!("devy-{name}.log"));
    let label_e = xml_escape(&label);
    let bin_e = xml_escape(&bin.to_string_lossy());
    let log_e = xml_escape(&log.to_string_lossy());
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label_e}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bin_e}</string>
    </array>
    <key>KeepAlive</key>
    <false/>
    <key>RunAtLoad</key>
    <false/>
    <key>StandardOutPath</key>
    <string>{log_e}</string>
    <key>StandardErrorPath</key>
    <string>{log_e}</string>
</dict>
</plist>
"#,
    );

    let path = launchagent_path(name)?;
    std::fs::write(&path, plist).with_context(|| format!("Failed to write {}", path.display()))
}

#[cfg(target_os = "macos")]
fn launchctl(args: &[&str]) -> Result<()> {
    let status = Command::new("launchctl")
        .args(args)
        .status()
        .context("Failed to run launchctl")?;
    if !status.success() {
        bail!("launchctl {} failed", args.join(" "));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn is_running_macos(name: &str) -> Result<bool> {
    let label = format!("sh.devy.{name}");
    let out = Command::new("launchctl")
        .args(["list", &label])
        .output()
        .context("Failed to run launchctl list")?;
    if !out.status.success() {
        return Ok(false);
    }
    // Output contains "PID" = <num>; key is absent when service is stopped.
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout.contains("\"PID\"") || stdout.contains("PID ="))
}

#[cfg(target_os = "macos")]
fn start_service_macos(name: &str, profile_bin: &Path) -> Result<()> {
    let exec = modules::get(name).service_exec_name().ok_or_else(|| {
        anyhow::anyhow!(
            "Service management for '{}' is not yet supported with the nix backend on macOS. \
             Start it manually using the binary in .devy/nix-profile/bin/.",
            name
        )
    })?;
    write_launchagent(name, exec, profile_bin)?;
    let plist = launchagent_path(name)?;
    let plist_str = plist
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("non-UTF-8 plist path"))?;
    launchctl(&["load", plist_str])?;
    launchctl(&["start", &format!("sh.devy.{name}")])
}

#[cfg(target_os = "macos")]
fn stop_service_macos(name: &str) -> Result<()> {
    let label = format!("sh.devy.{name}");
    let _ = launchctl(&["stop", &label]); // best-effort; may not be running
    let plist = launchagent_path(name)?;
    if plist.exists() {
        let plist_str = plist
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non-UTF-8 plist path"))?;
        launchctl(&["unload", plist_str])?;
    }
    Ok(())
}

// ── Service management — Linux (systemd user units) ───────────────────────────

#[cfg(target_os = "linux")]
fn systemd_user_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("$HOME not set")?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("systemd")
        .join("user"))
}

#[cfg(target_os = "linux")]
fn systemd_unit_path(name: &str) -> Result<PathBuf> {
    Ok(systemd_user_dir()?.join(format!("devy-{name}.service")))
}

#[cfg(target_os = "linux")]
fn write_systemd_unit(name: &str, exec: &str, profile_bin: &Path) -> Result<()> {
    let dir = systemd_user_dir()?;
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;

    let bin = profile_bin.join(exec);
    let unit = format!(
        "[Unit]\nDescription=devy managed {name} service\n\n\
         [Service]\nExecStart={bin}\nRestart=on-failure\n\n\
         [Install]\nWantedBy=default.target\n",
        name = name,
        bin = bin.display(),
    );

    let path = systemd_unit_path(name)?;
    std::fs::write(&path, unit).with_context(|| format!("Failed to write {}", path.display()))
}

#[cfg(target_os = "linux")]
fn systemctl_user(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .context("Failed to run systemctl")?;
    if !status.success() {
        bail!("systemctl --user {} failed", args.join(" "));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn is_running_linux(name: &str) -> Result<bool> {
    let unit = format!("devy-{name}");
    let status = Command::new("systemctl")
        .args(["--user", "is-active", "--quiet", &unit])
        .status()
        .context("Failed to run systemctl is-active")?;
    Ok(status.success())
}

#[cfg(target_os = "linux")]
fn start_service_linux(name: &str, profile_bin: &Path) -> Result<()> {
    let exec = modules::get(name).service_exec_name().ok_or_else(|| {
        anyhow::anyhow!(
            "Service management for '{}' is not yet supported with the nix backend on Linux. \
             Start it manually using the binary in .devy/nix-profile/bin/.",
            name
        )
    })?;
    write_systemd_unit(name, exec, profile_bin)?;
    systemctl_user(&["daemon-reload"])?;
    systemctl_user(&["start", &format!("devy-{name}")])
}

#[cfg(target_os = "linux")]
fn stop_service_linux(name: &str) -> Result<()> {
    let unit = format!("devy-{name}");
    systemctl_user(&["stop", &unit])
}

// ── PackageManager impl ───────────────────────────────────────────────────────

impl NixPackageManager {
    fn query_version(&self, dep: &Dependency) -> Result<Option<String>> {
        // Profile symlink doesn't exist yet → nothing installed in it. Guard here
        // to prevent Nix from falling back to the user's global profile, which would
        // cause packages installed globally (but not in this project) to appear installed.
        if !self.profile_path.exists() {
            return Ok(None);
        }
        match self.effective_style() {
            NixStyle::Profile => {
                let json = profile_list_json(&self.nix_bin(), &self.profile_path)?;
                Ok(profile_find_pkg(&json, &dep.name))
            }
            NixStyle::Env => {
                let json = env_list_json(&self.nix_env_bin(), &self.profile_path)?;
                Ok(env_find_pkg(&json, &dep.name))
            }
        }
    }
}

impl PackageManager for NixPackageManager {
    fn name(&self) -> &str {
        "nix"
    }

    fn install_url(&self) -> &str {
        "https://nixos.org/download/"
    }

    fn is_available(&self) -> bool {
        self.nix_bin().exists()
    }

    fn bootstrap(&self) -> Result<()> {
        output::step(
            "Installing Nix via the Determinate Installer \
             (https://install.determinate.systems). \
             Transport is secured with TLS; install Nix manually: https://nixos.org/download/",
        );
        let status = Command::new("sh")
            .arg("-c")
            .arg(concat!(
                "curl --proto '=https' --tlsv1.2 --connect-timeout 30 --max-time 300 -sSf -L ",
                "https://install.determinate.systems/nix",
                " | sh -s -- install --no-confirm"
            ))
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("Failed to run Nix install script")?;

        if !status.success() {
            bail!("Nix installation failed");
        }
        Ok(())
    }

    fn is_package_installed(&self, dep: &Dependency) -> Result<bool> {
        self.query_version(dep).map(|v| v.is_some())
    }

    fn install_package(&self, dep: &Dependency) -> Result<()> {
        // nix requires the profile's parent directory to exist before it can
        // create the profile symlink (.devy/nix-profile → nix store path).
        if let Some(parent) = self.profile_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        match self.effective_style() {
            NixStyle::Profile => {
                let attr = format!("nixpkgs#{}", dep.name);
                output::step(&format!("nix profile install {attr}"));
                let status = Command::new(self.nix_bin())
                    .args(["profile", "install", "--profile"])
                    .arg(&self.profile_path)
                    .arg(&attr)
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()
                    .with_context(|| format!("Failed to run: nix profile install {attr}"))?;
                if !status.success() {
                    bail!("`nix profile install {attr}` failed — check output above");
                }
                Ok(())
            }
            NixStyle::Env => {
                let attr = format!("nixpkgs.{}", dep.name);
                output::step(&format!("nix-env -iA {attr}"));
                let status = Command::new(self.nix_env_bin())
                    .args(["--profile"])
                    .arg(&self.profile_path)
                    .args(["-iA", &attr])
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()
                    .with_context(|| format!("Failed to run: nix-env -iA {attr}"))?;
                if !status.success() {
                    bail!("`nix-env -iA {attr}` failed — check output above");
                }
                Ok(())
            }
        }
    }

    fn is_service_running(&self, name: &str) -> Result<bool> {
        #[cfg(target_os = "macos")]
        return is_running_macos(name);

        #[cfg(target_os = "linux")]
        return is_running_linux(name);

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        bail!("Service management is not supported on this platform with the nix backend");
    }

    fn start_service(&self, name: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        return start_service_macos(name, &self.profile_bin());

        #[cfg(target_os = "linux")]
        return start_service_linux(name, &self.profile_bin());

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        bail!("Service management is not supported on this platform with the nix backend");
    }

    fn stop_service(&self, name: &str) -> Result<()> {
        #[cfg(target_os = "macos")]
        return stop_service_macos(name);

        #[cfg(target_os = "linux")]
        return stop_service_linux(name);

        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        bail!("Service management is not supported on this platform with the nix backend");
    }

    fn resolved_version(&self, dep: &Dependency) -> Result<Option<String>> {
        self.query_version(dep)
    }

    /// Advertises the project-local Nix profile bin dir so shadowenv adds it to PATH.
    /// This ensures `devy up` activates the project's Nix packages without touching
    /// the user's global profile or requiring a manual PATH change.
    fn path_prepends(&self, _project_root: &Path) -> Vec<String> {
        vec![self.profile_bin().to_string_lossy().into_owned()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nix_name_is_nix() {
        let pm = NixPackageManager::for_project(&crate::test_support::tmp_dir());
        assert_eq!(pm.name(), "nix");
    }

    #[test]
    fn nix_is_available_when_nix_binary_found() {
        // is_available() returns true iff the resolved nix binary exists.
        // find_nix_binary checks PATH then standard locations, so this holds
        // even when Nix is installed but not on PATH.
        let pm = NixPackageManager::for_project(&crate::test_support::tmp_dir());
        let expected = find_nix_binary("nix").is_some();
        assert_eq!(pm.is_available(), expected);
    }

    #[test]
    fn profile_path_is_project_local() {
        let dir = crate::test_support::tmp_dir();
        let pm = NixPackageManager::for_project(&dir);
        assert!(
            pm.profile_path.ends_with(".devy/nix-profile"),
            "profile_path must be project-local, got: {}",
            pm.profile_path.display()
        );
        assert!(
            !pm.profile_path.to_string_lossy().contains(".nix-profile/"),
            "profile_path must not reference the global ~/.nix-profile"
        );
    }

    #[test]
    fn path_prepends_includes_profile_bin() {
        let pm = NixPackageManager::for_project(&crate::test_support::tmp_dir());
        let prepends = pm.path_prepends(std::path::Path::new("/irrelevant"));
        assert_eq!(prepends.len(), 1);
        // Use Path::ends_with (component-aware) rather than str::ends_with so the
        // check works on Windows where to_string_lossy() produces backslashes.
        assert!(std::path::Path::new(&prepends[0]).ends_with(".devy/nix-profile/bin"));
    }

    #[test]
    fn service_exec_name_known_services() {
        use crate::modules;
        assert_eq!(
            modules::get("redis").service_exec_name(),
            Some("redis-server")
        );
        assert_eq!(modules::get("mysql").service_exec_name(), Some("mysqld"));
        assert_eq!(
            modules::get("postgresql").service_exec_name(),
            Some("postgres")
        );
        assert_eq!(modules::get("nginx").service_exec_name(), Some("nginx"));
        assert_eq!(
            modules::get("rabbitmq").service_exec_name(),
            Some("rabbitmq-server")
        );
        assert_eq!(
            modules::get("memcached").service_exec_name(),
            Some("memcached")
        );
        assert_eq!(modules::get("minio").service_exec_name(), Some("minio"));
        assert_eq!(modules::get("vault").service_exec_name(), Some("vault"));
    }

    #[test]
    fn service_exec_name_returns_none_for_unsupported() {
        use crate::modules;
        assert!(modules::get("unknownservice").service_exec_name().is_none());
        assert!(modules::get("kafka").service_exec_name().is_none());
    }

    #[test]
    fn profile_find_pkg_old_array_format() {
        let json = serde_json::json!([
            {"pname": "git", "version": "2.44.0"},
            {"pname": "redis", "version": "7.2.4"},
        ]);
        assert_eq!(profile_find_pkg(&json, "redis"), Some("7.2.4".into()));
        assert_eq!(profile_find_pkg(&json, "curl"), None);
    }

    #[test]
    fn profile_find_pkg_new_nix_218_format_with_pname() {
        // Nix ≥ 2.18 returns {"version":2,"elements":{"name":{...}}}
        let json = serde_json::json!({
            "version": 2,
            "elements": {
                "redis-7.2.4": {
                    "attrPath": "legacyPackages.x86_64-linux.redis",
                    "pname": "redis",
                    "version": "7.2.4",
                    "active": true
                }
            }
        });
        assert_eq!(profile_find_pkg(&json, "redis"), Some("7.2.4".into()));
        assert_eq!(profile_find_pkg(&json, "curl"), None);
    }

    #[test]
    fn profile_find_pkg_new_nix_218_format_attrpath_fallback() {
        // Some Nix 2.18+ elements omit pname; fall back to the last component of attrPath.
        let json = serde_json::json!({
            "version": 2,
            "elements": {
                "redis": {
                    "attrPath": "legacyPackages.x86_64-linux.redis",
                    "active": true
                }
            }
        });
        assert_eq!(profile_find_pkg(&json, "redis"), Some("unknown".into()));
        assert_eq!(profile_find_pkg(&json, "curl"), None);
    }

    fn pm_with_missing_profile() -> NixPackageManager {
        NixPackageManager {
            profile_path: PathBuf::from("/tmp/devy_test_nonexistent_profile_xyzzy"),
            style: std::sync::OnceLock::new(),
        }
    }

    #[test]
    fn is_package_installed_returns_false_when_profile_missing() {
        // If the profile symlink doesn't exist, Nix commands might fall back to the
        // user's global profile. We must short-circuit and return false here.
        let dep = crate::config::Dependency::simple("redis");
        assert!(
            !pm_with_missing_profile()
                .is_package_installed(&dep)
                .unwrap(),
            "must return false when the project profile does not exist"
        );
    }

    #[test]
    fn resolved_version_returns_none_when_profile_missing() {
        let dep = crate::config::Dependency::simple("redis");
        assert_eq!(
            pm_with_missing_profile().resolved_version(&dep).unwrap(),
            None
        );
    }

    #[test]
    fn env_find_pkg_matches_pname() {
        let json = serde_json::json!({
            "redis-7.2.4": {"pname": "redis", "version": "7.2.4"},
            "git-2.44.0": {"pname": "git", "version": "2.44.0"},
        });
        assert_eq!(env_find_pkg(&json, "redis"), Some("7.2.4".into()));
        assert_eq!(env_find_pkg(&json, "curl"), None);
    }
}
