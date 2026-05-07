#[cfg(any(test, target_os = "macos"))]
mod brew;
#[cfg(target_os = "macos")]
pub use brew::Homebrew;

#[cfg(any(test, target_os = "linux"))]
mod apt;
#[cfg(target_os = "linux")]
pub use apt::Apt;

#[cfg(any(test, target_os = "windows"))]
mod winget;
#[cfg(target_os = "windows")]
pub use winget::WinGet;

use anyhow::Result;
use std::path::PathBuf;

use crate::config::Dependency;

pub trait PackageManager {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
    fn bootstrap(&self) -> Result<()>;
    fn is_package_installed(&self, dep: &Dependency) -> Result<bool>;
    fn install_package(&self, dep: &Dependency) -> Result<()>;
    fn is_service_running(&self, name: &str) -> Result<bool>;
    fn start_service(&self, name: &str) -> Result<()>;
    fn stop_service(&self, name: &str) -> Result<()>;
    /// Returns the exact version string currently installed, e.g. "20.11.0" or "7.2.3".
    fn resolved_version(&self, dep: &Dependency) -> Result<Option<String>>;

    /// Returns the directory where service config files should be written, if supported.
    /// Returns `None` if the platform does not support writing config for the given service.
    fn service_config_dir(&self, _service: &str) -> Option<PathBuf> {
        None
    }

    /// Validates dependency configuration before install (e.g. tap allowlist).
    /// Called by `devy check` so issues surface without triggering any installs.
    fn validate_config(&self, _dep: &Dependency) -> Result<()> {
        Ok(())
    }

    fn ensure_available(&self) -> Result<()> {
        if !self.is_available() {
            self.bootstrap()
        } else {
            Ok(())
        }
    }
}

pub fn detect() -> Result<Box<dyn PackageManager>> {
    #[cfg(target_os = "macos")]
    return Ok(Box::new(Homebrew));

    #[cfg(target_os = "linux")]
    return Ok(Box::new(Apt::new()));

    #[cfg(target_os = "windows")]
    return Ok(Box::new(WinGet::new()));

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    anyhow::bail!("No supported package manager for this operating system")
}

/// A configurable PackageManager implementation for use in unit tests.
/// Available in all `#[cfg(test)]` contexts via `crate::package_manager::MockPackageManager`.
#[cfg(test)]
pub struct MockPackageManager {
    pub name: &'static str,
    pub installed: bool,
    pub service_running: bool,
    pub install_fails: bool,
    pub start_service_fails: bool,
    pub stop_service_fails: bool,
    pub is_running_fails: bool,
    pub config_dir: Option<std::path::PathBuf>,
    /// When set, `is_package_installed` returns true only when `dep.name == installed_pkg`.
    pub installed_pkg: Option<&'static str>,
    /// Tracks which service names were passed to `start_service`.
    pub started_services: std::cell::RefCell<Vec<String>>,
    /// Tracks which service names were passed to `stop_service`.
    pub stopped_services: std::cell::RefCell<Vec<String>>,
    /// Tracks every package name passed to `install_package` (in dep.name form).
    pub installed_packages: std::cell::RefCell<Vec<String>>,
    /// When set, `resolved_version` returns this value instead of Ok(None).
    pub version: Option<String>,
    /// When true, `validate_config` returns an error.
    pub validate_config_fails: bool,
}

#[cfg(test)]
impl Default for MockPackageManager {
    fn default() -> Self {
        Self {
            name: "mock",
            installed: false,
            service_running: false,
            install_fails: false,
            start_service_fails: false,
            stop_service_fails: false,
            is_running_fails: false,
            config_dir: None,
            installed_pkg: None,
            started_services: std::cell::RefCell::new(Vec::new()),
            stopped_services: std::cell::RefCell::new(Vec::new()),
            installed_packages: std::cell::RefCell::new(Vec::new()),
            version: None,
            validate_config_fails: false,
        }
    }
}

#[cfg(test)]
impl PackageManager for MockPackageManager {
    fn name(&self) -> &str {
        self.name
    }
    fn is_available(&self) -> bool {
        true
    }
    fn bootstrap(&self) -> Result<()> {
        Ok(())
    }
    fn is_package_installed(&self, dep: &Dependency) -> Result<bool> {
        if let Some(pkg) = self.installed_pkg {
            Ok(dep.name == pkg)
        } else {
            Ok(self.installed)
        }
    }
    fn install_package(&self, dep: &Dependency) -> Result<()> {
        self.installed_packages.borrow_mut().push(dep.name.clone());
        if self.install_fails {
            anyhow::bail!("mock install failure")
        } else {
            Ok(())
        }
    }
    fn is_service_running(&self, name: &str) -> Result<bool> {
        if self.is_running_fails {
            anyhow::bail!("mock is_service_running failure")
        }
        // Return false if this service was already stopped (simulates real stop behaviour).
        if self.stopped_services.borrow().contains(&name.to_string()) {
            return Ok(false);
        }
        Ok(self.service_running)
    }
    fn start_service(&self, name: &str) -> Result<()> {
        self.started_services.borrow_mut().push(name.to_string());
        if self.start_service_fails {
            anyhow::bail!("mock start_service failure")
        } else {
            Ok(())
        }
    }
    fn stop_service(&self, name: &str) -> Result<()> {
        self.stopped_services.borrow_mut().push(name.to_string());
        if self.stop_service_fails {
            anyhow::bail!("mock stop_service failure")
        } else {
            Ok(())
        }
    }
    fn resolved_version(&self, _: &Dependency) -> Result<Option<String>> {
        Ok(self.version.clone())
    }
    fn service_config_dir(&self, _: &str) -> Option<std::path::PathBuf> {
        self.config_dir.clone()
    }
    fn validate_config(&self, _dep: &Dependency) -> Result<()> {
        if self.validate_config_fails {
            anyhow::bail!("mock validate_config failure")
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Dependency;

    struct AvailablePm;
    impl PackageManager for AvailablePm {
        fn name(&self) -> &str {
            "available"
        }
        fn is_available(&self) -> bool {
            true
        }
        fn bootstrap(&self) -> Result<()> {
            panic!("bootstrap should not be called")
        }
        fn is_package_installed(&self, _: &Dependency) -> Result<bool> {
            Ok(false)
        }
        fn install_package(&self, _: &Dependency) -> Result<()> {
            Ok(())
        }
        fn is_service_running(&self, _: &str) -> Result<bool> {
            Ok(false)
        }
        fn start_service(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn stop_service(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn resolved_version(&self, _: &Dependency) -> Result<Option<String>> {
            Ok(None)
        }
    }

    struct UnavailablePm {
        bootstrap_called: std::cell::Cell<bool>,
    }
    impl UnavailablePm {
        fn new() -> Self {
            Self {
                bootstrap_called: std::cell::Cell::new(false),
            }
        }
    }
    impl PackageManager for UnavailablePm {
        fn name(&self) -> &str {
            "unavailable"
        }
        fn is_available(&self) -> bool {
            false
        }
        fn bootstrap(&self) -> Result<()> {
            self.bootstrap_called.set(true);
            Ok(())
        }
        fn is_package_installed(&self, _: &Dependency) -> Result<bool> {
            Ok(false)
        }
        fn install_package(&self, _: &Dependency) -> Result<()> {
            Ok(())
        }
        fn is_service_running(&self, _: &str) -> Result<bool> {
            Ok(false)
        }
        fn start_service(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn stop_service(&self, _: &str) -> Result<()> {
            Ok(())
        }
        fn resolved_version(&self, _: &Dependency) -> Result<Option<String>> {
            Ok(None)
        }
    }

    #[test]
    fn detect_returns_ok_on_current_platform() {
        assert!(
            detect().is_ok(),
            "detect() must return Ok on the current platform"
        );
    }

    #[test]
    fn ensure_available_skips_bootstrap_when_already_available() {
        let pm = AvailablePm;
        assert!(pm.ensure_available().is_ok());
    }

    #[test]
    fn ensure_available_calls_bootstrap_when_not_available() {
        let pm = UnavailablePm::new();
        pm.ensure_available().unwrap();
        assert!(pm.bootstrap_called.get());
    }

    #[test]
    fn default_service_config_dir_returns_none() {
        assert!(AvailablePm.service_config_dir("mysql").is_none());
        assert!(AvailablePm.service_config_dir("redis").is_none());
    }
}
