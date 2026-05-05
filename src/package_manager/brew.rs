use anyhow::{Context, Result, bail};
use std::process::{Command, Stdio};
use which::which;

use super::PackageManager;
use crate::config::Dependency;

pub struct Homebrew;

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
        "Homebrew"
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
            self.run_interactive(&["tap", tap])?;
        }
        self.run_interactive(&["install", &dep.versioned_name()])
    }

    fn is_service_running(&self, name: &str) -> Result<bool> {
        let output = self.run(&["services", "list"])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(stdout
            .lines()
            .any(|line| line.starts_with(name) && line.contains("started")))
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
        Ok(line.split_whitespace().nth(1).map(String::from))
    }
}
