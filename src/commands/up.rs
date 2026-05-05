use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;

use crate::commands::exec::run_hook;
use crate::config::{Dependency, EnvyConfig};
use crate::env_manager::{EnvManager, Shadowenv};
use crate::lock::{LockFile, LockedDep};
use crate::modules;
use crate::output;
use crate::package_manager;
use crate::secrets;

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

    // Load the lock file unless --update was passed.
    let lock = if update {
        if Path::new(crate::lock::PATH).exists() {
            output::step("Ignoring envy.lock (--update)");
        }
        None
    } else {
        LockFile::load().context("Failed to read envy.lock")?
    };

    let deps = config.normalized_dependencies(profile);
    if !deps.is_empty() {
        output::header("Dependencies");
        for dep in &deps {
            let effective = apply_lock(dep, lock.as_ref());
            install_dep(pm.as_ref(), &effective)?;
        }
    }

    // Build the merged environment: plain vars first, then secrets (secrets win on conflict).
    let mut merged_env = config.environment.clone();
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
    write_lock(&deps, pm.as_ref())?;

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
fn apply_lock(dep: &Dependency, lock: Option<&LockFile>) -> Dependency {
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

fn install_dep(pm: &dyn package_manager::PackageManager, dep: &Dependency) -> Result<()> {
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

fn write_lock(deps: &[Dependency], pm: &dyn package_manager::PackageManager) -> Result<()> {
    let mut locked = Vec::new();
    for dep in deps {
        let module = modules::get(&dep.name);
        let resolved = module.resolved_version(pm, dep)?;
        locked.push(LockedDep {
            name: dep.name.clone(),
            resolved_version: resolved,
            source: module.source().to_string(),
        });
    }
    let lock = LockFile {
        dependencies: locked,
    };
    lock.write().context("Failed to write envy.lock")?;
    output::success(&format!("Lock file written to {}", crate::lock::PATH));
    Ok(())
}
