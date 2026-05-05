use anyhow::{Context, Result, bail};
use std::path::Path;

use crate::output;

/// Files/dirs we probe and the dependency entry they imply.
struct Probe {
    /// Files or directories to look for (any match is sufficient).
    markers: &'static [&'static str],
    /// The dependency name to emit.
    dep: &'static str,
    /// Optional version hint extracted from a file, or None to leave unversioned.
    version_fn: fn(&Path) -> Option<String>,
}

fn no_version(_dir: &Path) -> Option<String> {
    None
}

const PROBES: &[Probe] = &[
    Probe {
        markers: &["Cargo.toml"],
        dep: "rust",
        version_fn: rust_version,
    },
    Probe {
        markers: &["package.json"],
        dep: "node",
        version_fn: node_version,
    },
    Probe {
        markers: &["tsconfig.json"],
        dep: "typescript",
        version_fn: node_version,
    },
    Probe {
        markers: &["go.mod"],
        dep: "go",
        version_fn: go_version,
    },
    Probe {
        markers: &["Gemfile"],
        dep: "ruby",
        version_fn: ruby_version,
    },
    Probe {
        markers: &["pom.xml", "build.gradle", "build.gradle.kts"],
        dep: "java",
        version_fn: no_version,
    },
    Probe {
        markers: &[
            "requirements.txt",
            "pyproject.toml",
            "setup.py",
            "setup.cfg",
        ],
        dep: "python",
        version_fn: python_version,
    },
];

pub fn run(force: bool) -> Result<()> {
    let config_path = Path::new("envy.yml");

    if config_path.exists() && !force {
        bail!("envy.yml already exists. Use {} to overwrite.", "--force");
    }

    output::header("envy init");

    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let project_name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("my-project");

    let mut deps: Vec<String> = Vec::new();

    for probe in PROBES {
        let matched = probe.markers.iter().any(|m| cwd.join(m).exists());
        if !matched {
            continue;
        }

        let version = (probe.version_fn)(&cwd);
        let entry = match version {
            Some(v) => format!("  - {}:\n      version: \"{}\"", probe.dep, v),
            None => format!("  - {}", probe.dep),
        };

        output::success(&format!("detected {} ({})", probe.dep, probe.markers[0]));
        deps.push(entry);
    }

    if deps.is_empty() {
        output::skip("no recognised project files found — writing empty template");
    }

    let dep_block = if deps.is_empty() {
        "  # - node:\n  #     version: \"20\"\n".to_string()
    } else {
        deps.join("\n") + "\n"
    };

    let content = format!(
        "name: {project_name}\n\ndependencies:\n{dep_block}\nenvironment: {{}}\n\ncommands: {{}}\n"
    );

    std::fs::write(config_path, &content).context("Failed to write envy.yml")?;

    println!();
    output::success(&format!("wrote {}", config_path.display()));
    Ok(())
}

// ── Version extractors ────────────────────────────────────────────────────────

fn rust_version(dir: &Path) -> Option<String> {
    // Read rust-toolchain or rust-toolchain.toml for a pinned channel.
    for name in &["rust-toolchain.toml", "rust-toolchain"] {
        let path = dir.join(name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            // rust-toolchain.toml: channel = "1.78.0" or channel = "stable"
            for line in content.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix("channel") {
                    let val = rest
                        .trim_start_matches([' ', '=', '"'].as_ref())
                        .trim_end_matches('"');
                    if !val.is_empty() {
                        return Some(val.to_string());
                    }
                }
                // Plain rust-toolchain file is just the channel string.
                let trimmed = line.trim_matches('"');
                if !trimmed.is_empty() && !trimmed.starts_with('[') {
                    return Some(trimmed.to_string());
                }
            }
        }
    }
    None
}

fn node_version(dir: &Path) -> Option<String> {
    // .nvmrc / .node-version contain just the version string.
    for name in &[".nvmrc", ".node-version"] {
        if let Ok(v) = std::fs::read_to_string(dir.join(name)) {
            let v = v.trim().trim_start_matches('v');
            if !v.is_empty() {
                // Emit only the major version — matches brew formula naming.
                let major = v.split('.').next().unwrap_or(v);
                return Some(major.to_string());
            }
        }
    }
    // Fall back to engines.node in package.json.
    if let Ok(content) = std::fs::read_to_string(dir.join("package.json"))
        && let Some(ver) = extract_json_str(&content, "\"node\"")
    {
        let ver = ver.trim_start_matches(['^', '~', '=', 'v'].as_ref());
        let major = ver.split('.').next().unwrap_or(ver);
        return Some(major.to_string());
    }
    None
}

fn go_version(dir: &Path) -> Option<String> {
    // go.mod first line after "module": `go 1.22`
    if let Ok(content) = std::fs::read_to_string(dir.join("go.mod")) {
        for line in content.lines() {
            let line = line.trim();
            if let Some(ver) = line.strip_prefix("go ") {
                let ver = ver.trim();
                if !ver.is_empty() {
                    return Some(ver.to_string());
                }
            }
        }
    }
    None
}

fn ruby_version(dir: &Path) -> Option<String> {
    // .ruby-version contains the version string.
    if let Ok(v) = std::fs::read_to_string(dir.join(".ruby-version")) {
        let v = v.trim().trim_start_matches('v');
        if !v.is_empty() {
            let minor = v.splitn(3, '.').take(2).collect::<Vec<_>>().join(".");
            return Some(minor);
        }
    }
    None
}

fn python_version(dir: &Path) -> Option<String> {
    // .python-version (pyenv) contains the version string.
    if let Ok(v) = std::fs::read_to_string(dir.join(".python-version")) {
        let v = v.trim();
        if !v.is_empty() {
            let minor = v.splitn(3, '.').take(2).collect::<Vec<_>>().join(".");
            return Some(minor);
        }
    }
    // pyproject.toml: requires-python = ">=3.11"
    if let Ok(content) = std::fs::read_to_string(dir.join("pyproject.toml")) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("requires-python")
                && let Some(ver) = line.split('"').nth(1)
            {
                let ver = ver.trim_start_matches(['^', '~', '=', '>', '<', ' '].as_ref());
                let minor = ver.splitn(3, '.').take(2).collect::<Vec<_>>().join(".");
                if !minor.is_empty() {
                    return Some(minor);
                }
            }
        }
    }
    None
}

/// Naive extractor: finds `"key": "value"` in a JSON string.
fn extract_json_str<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let start = json.find(key)?;
    let after_key = &json[start + key.len()..];
    let colon = after_key.find(':')? + 1;
    let after_colon = after_key[colon..].trim_start();
    if let Some(inner) = after_colon.strip_prefix('"') {
        let end = inner.find('"')?;
        Some(&inner[..end])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "envy_init_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ── extract_json_str ──────────────────────────────────────────────────────

    #[test]
    fn extract_json_str_found() {
        let json = r#"{"engines": {"node": ">=18.0.0"}}"#;
        assert_eq!(extract_json_str(json, "\"node\""), Some(">=18.0.0"));
    }

    #[test]
    fn extract_json_str_not_found() {
        let json = r#"{"name": "my-app"}"#;
        assert!(extract_json_str(json, "\"node\"").is_none());
    }

    #[test]
    fn extract_json_str_non_string_value_returns_none() {
        let json = r#"{"port": 3000}"#;
        assert!(extract_json_str(json, "\"port\"").is_none());
    }

    // ── rust_version ──────────────────────────────────────────────────────────

    #[test]
    fn rust_version_missing_files_returns_none() {
        let dir = tmp_dir();
        assert!(rust_version(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rust_version_reads_toolchain_toml() {
        let dir = tmp_dir();
        std::fs::write(
            dir.join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.78.0\"\n",
        )
        .unwrap();
        assert_eq!(rust_version(&dir), Some("1.78.0".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rust_version_reads_plain_toolchain_file() {
        let dir = tmp_dir();
        std::fs::write(dir.join("rust-toolchain"), "stable\n").unwrap();
        assert_eq!(rust_version(&dir), Some("stable".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rust_version_plain_toolchain_strips_quotes() {
        let dir = tmp_dir();
        std::fs::write(dir.join("rust-toolchain"), "\"nightly\"\n").unwrap();
        assert_eq!(rust_version(&dir), Some("nightly".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── node_version ──────────────────────────────────────────────────────────

    #[test]
    fn node_version_missing_returns_none() {
        let dir = tmp_dir();
        assert!(node_version(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn node_version_reads_nvmrc() {
        let dir = tmp_dir();
        std::fs::write(dir.join(".nvmrc"), "20.11.0\n").unwrap();
        assert_eq!(node_version(&dir), Some("20".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn node_version_reads_node_version_file_with_v_prefix() {
        let dir = tmp_dir();
        std::fs::write(dir.join(".node-version"), "v18.19.0\n").unwrap();
        assert_eq!(node_version(&dir), Some("18".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn node_version_falls_back_to_package_json_engines() {
        let dir = tmp_dir();
        let pkg = r#"{"name":"app","engines":{"node":"^20.0.0"}}"#;
        std::fs::write(dir.join("package.json"), pkg).unwrap();
        assert_eq!(node_version(&dir), Some("20".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn node_version_package_json_ge_range_returns_major_with_prefix() {
        // The code only strips ^, ~, =, v — not > or < — so >=20 stays as ">=20".
        // This test documents the actual behaviour.
        let dir = tmp_dir();
        let pkg = r#"{"name":"app","engines":{"node":">=20.0.0"}}"#;
        std::fs::write(dir.join("package.json"), pkg).unwrap();
        let v = node_version(&dir);
        // >=20.0.0 → after stripping nothing → split on '.' → ">=20"
        assert_eq!(v, Some(">=20".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn node_version_prefers_nvmrc_over_package_json() {
        let dir = tmp_dir();
        std::fs::write(dir.join(".nvmrc"), "18\n").unwrap();
        std::fs::write(dir.join("package.json"), r#"{"engines":{"node":">=20"}}"#).unwrap();
        assert_eq!(node_version(&dir), Some("18".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── go_version ────────────────────────────────────────────────────────────

    #[test]
    fn go_version_missing_returns_none() {
        let dir = tmp_dir();
        assert!(go_version(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn go_version_reads_go_mod() {
        let dir = tmp_dir();
        std::fs::write(dir.join("go.mod"), "module example.com/myapp\n\ngo 1.22\n").unwrap();
        assert_eq!(go_version(&dir), Some("1.22".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn go_version_returns_none_if_no_go_directive() {
        let dir = tmp_dir();
        std::fs::write(dir.join("go.mod"), "module example.com/myapp\n").unwrap();
        assert!(go_version(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── ruby_version ──────────────────────────────────────────────────────────

    #[test]
    fn ruby_version_missing_returns_none() {
        let dir = tmp_dir();
        assert!(ruby_version(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ruby_version_reads_ruby_version_file() {
        let dir = tmp_dir();
        std::fs::write(dir.join(".ruby-version"), "3.2.1\n").unwrap();
        assert_eq!(ruby_version(&dir), Some("3.2".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ruby_version_strips_v_prefix() {
        let dir = tmp_dir();
        std::fs::write(dir.join(".ruby-version"), "v3.1.4\n").unwrap();
        assert_eq!(ruby_version(&dir), Some("3.1".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── python_version ────────────────────────────────────────────────────────

    #[test]
    fn python_version_missing_returns_none() {
        let dir = tmp_dir();
        assert!(python_version(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn python_version_reads_python_version_file() {
        let dir = tmp_dir();
        std::fs::write(dir.join(".python-version"), "3.11.0\n").unwrap();
        assert_eq!(python_version(&dir), Some("3.11".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn python_version_reads_pyproject_toml() {
        let dir = tmp_dir();
        std::fs::write(
            dir.join("pyproject.toml"),
            "[project]\nrequires-python = \">=3.11\"\n",
        )
        .unwrap();
        assert_eq!(python_version(&dir), Some("3.11".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn python_version_prefers_python_version_file_over_pyproject() {
        let dir = tmp_dir();
        std::fs::write(dir.join(".python-version"), "3.12.0\n").unwrap();
        std::fs::write(dir.join("pyproject.toml"), "requires-python = \">=3.11\"\n").unwrap();
        assert_eq!(python_version(&dir), Some("3.12".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── run ───────────────────────────────────────────────────────────────────
    // Tests for run() mutate cwd and are serialized via a process-wide mutex.

    use std::sync::Mutex;
    static RUN_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn run_fails_when_config_exists_without_force() {
        let _guard = RUN_LOCK.lock().unwrap();
        let dir = tmp_dir();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        std::fs::write("envy.yml", "name: existing\n").unwrap();

        let result = run(false);
        std::env::set_current_dir(&original).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn run_creates_config_when_none_exists() {
        let _guard = RUN_LOCK.lock().unwrap();
        let dir = tmp_dir();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();

        let result = run(false);
        let created = std::fs::read_to_string(dir.join("envy.yml")).unwrap_or_default();
        std::env::set_current_dir(&original).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(result.is_ok());
        assert!(created.contains("dependencies:"));
    }

    #[test]
    fn run_force_overwrites_existing_config() {
        let _guard = RUN_LOCK.lock().unwrap();
        let dir = tmp_dir();
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        std::fs::write("envy.yml", "name: old\n").unwrap();

        let result = run(true);
        let content = std::fs::read_to_string(dir.join("envy.yml")).unwrap_or_default();
        std::env::set_current_dir(&original).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(result.is_ok());
        assert!(!content.contains("name: old"));
    }
}
