use anyhow::{Context, Result};
use colored::Colorize;
use std::collections::HashMap;
use std::path::Path;

use crate::commands::exec::run_hook;
use crate::config::{Dependency, EnvyConfig, RawCommand};
use crate::env_manager::{EnvManager, Shadowenv};
use crate::lock::{LockFile, LockedDep};
use crate::modules;
use crate::output;
use crate::package_manager;
use crate::secrets;

#[mutants::skip] // thick I/O wrapper — requires a real envy.yml, PM, and shadowenv
pub fn run(update: bool, profile: &str) -> Result<()> {
    let config = EnvyConfig::load_default()?;

    let project_name = config.name.as_deref().unwrap_or("project");
    output::header(&format!("envy up · {} [{}]", project_name, profile));

    if let Some(ref hook) = config.hooks.before_up {
        output::header("Hooks");
        run_hook("before_up", hook)?;
    }

    let pm = package_manager::detect()?;

    output::step(&format!("Checking for {}", pm.name()));
    pm.ensure_available()
        .with_context(|| format!("Failed to bootstrap {}", pm.name()))?;
    output::success(&format!("{} available", pm.name()));

    let lock_path = Path::new(crate::lock::PATH);

    // Load the lock file unless --update was passed.
    let lock = if update {
        if lock_path.exists() {
            output::step("Ignoring envy.lock (--update)");
        }
        None
    } else {
        LockFile::load(lock_path).context("Failed to read envy.lock")?
    };

    let deps = config.normalized_dependencies(profile);

    // Collect module-suggested env vars after each dep installs successfully.
    // This ensures failed installs don't pollute the environment config.
    let mut module_env: HashMap<String, String> = HashMap::new();

    if !deps.is_empty() {
        output::header("Dependencies");
        for dep in &deps {
            let effective = apply_lock(dep, lock.as_ref());
            install_dep(pm.as_ref(), &effective)?;
            module_env.extend(modules::get(&dep.name).env_vars(&effective));
        }
    }

    // Build the merged environment: module defaults < plain vars < secrets.
    module_env.extend(config.environment.clone());
    let mut merged_env = module_env;
    let mut secret_count = 0usize;

    if let Some(ref secrets_file) = config.secrets {
        output::header("Secrets");
        let path = Path::new(secrets_file);

        secrets::ensure_available(pm.as_ref()).context("Failed to ensure ejson is installed")?;

        output::step(&format!("Decrypting {}", secrets_file));
        let decrypted = secrets::decrypt(path)
            .with_context(|| format!("Failed to decrypt {}", secrets_file))?;

        secret_count = decrypted.len();
        // Merge without printing values.
        merged_env.extend(decrypted);
        output::success(&format!(
            "{} secret{} loaded",
            secret_count,
            if secret_count == 1 { "" } else { "s" }
        ));
    }

    if !merged_env.is_empty() {
        output::header("Environment");
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        let env_mgr = Shadowenv::new();

        if !env_mgr.is_available() {
            output::step("Installing shadowenv");
            pm.install_package(&Dependency::simple("shadowenv"))
                .context("Failed to install shadowenv")?;
            output::success("Installed shadowenv");
        }

        output::step(&format!("Writing {} config", env_mgr.name()));
        env_mgr
            .setup(&cwd, &merged_env)
            .context("Failed to configure environment variables")?;

        let plain_count = config.environment.len();
        let summary = match (plain_count, secret_count) {
            (p, 0) => format!("{p} variable{}", if p == 1 { "" } else { "s" }),
            (0, s) => format!("{s} secret{}", if s == 1 { "" } else { "s" }),
            (p, s) => format!(
                "{p} variable{} + {s} secret{}",
                if p == 1 { "" } else { "s" },
                if s == 1 { "" } else { "s" }
            ),
        };
        output::success(&format!("Environment configured ({summary})"));

        output::info(&format!(
            "Activate with: {}",
            "eval \"$(shadowenv hook zsh)\"".bold()
        ));
    }

    // Write the lock file with resolved versions for every dependency.
    write_lock(&deps, pm.as_ref(), lock_path)?;

    if let Some(ref hook) = config.hooks.after_up {
        output::header("Hooks");
        run_hook("after_up", hook)?;
    }

    println!();
    output::success(&format!("{} is ready", project_name));

    Ok(())
}

/// If the dep has no pinned version and the lock file has a resolved version
/// for it, return a clone pinned to that version; otherwise return as-is.
pub(crate) fn apply_lock(dep: &Dependency, lock: Option<&LockFile>) -> Dependency {
    if dep.version.is_some() {
        return dep.clone();
    }
    if let Some(locked) = lock.and_then(|l| l.get(&dep.name))
        && locked.resolved_version.is_some()
    {
        return Dependency {
            version: locked.resolved_version.clone(),
            ..dep.clone()
        };
    }
    dep.clone()
}

pub(crate) fn install_dep(pm: &dyn package_manager::PackageManager, dep: &Dependency) -> Result<()> {
    let module = modules::get(&dep.name);
    let display = dep.versioned_name();

    if module.is_installed(pm, dep)? {
        output::skip(&format!("{} already installed", display));
    } else {
        output::step(&format!("Installing {}", display));
        module
            .install(pm, dep)
            .with_context(|| format!("Failed to install {}", display))?;
        output::success(&format!("Installed {}", display));

        if let Some(cmd) = &dep.after_install {
            run_hook("after_install", &RawCommand::Simple(cmd.clone()))?;
        }
    }

    if module.is_service() {
        if module.is_running(pm, dep)? {
            output::skip(&format!("{} service already running", dep.name));
        } else {
            output::step(&format!("Starting {} service", dep.name));
            module
                .start(pm, dep)
                .with_context(|| format!("Failed to start {} service", dep.name))?;
            output::success(&format!("{} service started", dep.name));
        }
        output::step(&format!("Waiting for {} to be ready", dep.name));
        module.wait_for_ready(dep)?;
        output::success(&format!("{} is ready", dep.name));
    }

    Ok(())
}

pub(crate) fn write_lock(
    deps: &[Dependency],
    pm: &dyn package_manager::PackageManager,
    path: &Path,
) -> Result<()> {
    let mut locked = HashMap::new();
    for dep in deps {
        let module = modules::get(&dep.name);
        let resolved = module.resolved_version(pm, dep)?;
        locked.insert(
            dep.name.clone(),
            LockedDep {
                resolved_version: resolved,
                source: module.source().to_string(),
            },
        );
    }
    let lock = LockFile { dependencies: locked };
    lock.write(path).context("Failed to write envy.lock")?;
    output::success(&format!("Lock file written to {}", crate::lock::PATH));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Dependency;
    use crate::lock::{LockFile, LockedDep};
    use crate::package_manager::MockPackageManager;
    use std::collections::HashMap;

    fn tmp_path() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "envy_up_{}_{}.lock",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ))
    }

    // ── apply_lock ────────────────────────────────────────────────────────────

    #[test]
    fn apply_lock_pins_version_from_lock_file() {
        // Kills `delete field version from struct Dependency expression in apply_lock`.
        let dep = Dependency::simple("node");
        let mut deps = HashMap::new();
        deps.insert("node".into(), LockedDep {
            resolved_version: Some("20.11.0".into()),
            source: "homebrew".into(),
        });
        let lock = LockFile { dependencies: deps };
        let effective = apply_lock(&dep, Some(&lock));
        assert_eq!(
            effective.version.as_deref(),
            Some("20.11.0"),
            "apply_lock must pin version from lock file"
        );
    }

    #[test]
    fn apply_lock_does_not_override_existing_version() {
        let mut dep = Dependency::simple("node");
        dep.version = Some("18.0.0".into());
        let mut deps = HashMap::new();
        deps.insert("node".into(), LockedDep {
            resolved_version: Some("20.11.0".into()),
            source: "homebrew".into(),
        });
        let lock = LockFile { dependencies: deps };
        let effective = apply_lock(&dep, Some(&lock));
        assert_eq!(effective.version.as_deref(), Some("18.0.0"));
    }

    #[test]
    fn apply_lock_returns_dep_unchanged_when_no_lock() {
        let dep = Dependency::simple("node");
        let effective = apply_lock(&dep, None);
        assert!(effective.version.is_none());
    }

    #[test]
    fn apply_lock_returns_dep_unchanged_when_not_in_lock() {
        let lock = LockFile::default();
        let dep = Dependency::simple("node");
        let effective = apply_lock(&dep, Some(&lock));
        assert!(effective.version.is_none());
    }

    // ── install_dep ───────────────────────────────────────────────────────────

    #[test]
    fn install_dep_propagates_install_error() {
        // Kills `replace install_dep -> Ok(())` — mutation always returns Ok.
        // Use node (non-service) to avoid wait_for_ready TCP check.
        let pm = MockPackageManager { install_fails: true, ..Default::default() };
        let dep = Dependency::simple("node");
        assert!(
            install_dep(&pm, &dep).is_err(),
            "install failure must propagate as Err"
        );
    }

    #[test]
    fn install_dep_returns_ok_when_already_installed() {
        let pm = MockPackageManager { installed: true, ..Default::default() };
        let dep = Dependency::simple("node");
        assert!(install_dep(&pm, &dep).is_ok());
    }

    // ── write_lock ────────────────────────────────────────────────────────────

    #[test]
    fn write_lock_creates_file() {
        // Kills `replace write_lock -> Ok(())` — mutation returns Ok without writing.
        let path = tmp_path();
        let pm = MockPackageManager::default();
        let deps = vec![Dependency::simple("node")];
        write_lock(&deps, &pm, &path).unwrap();
        assert!(path.exists(), "write_lock must create the lock file");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn write_lock_includes_dep_name_in_file() {
        let path = tmp_path();
        let pm = MockPackageManager::default();
        let deps = vec![Dependency::simple("redis")];
        write_lock(&deps, &pm, &path).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("redis"), "lock file must contain dep name");
        let _ = std::fs::remove_file(&path);
    }
}
