mod brew;

pub use brew::Homebrew;

use anyhow::Result;

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
    return Ok(Box::new(Homebrew::new()));

    #[cfg(not(target_os = "macos"))]
    anyhow::bail!("No supported package manager for this operating system")
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
}
