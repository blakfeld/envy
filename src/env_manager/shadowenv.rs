use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use which::which;

pub const ENV_FILE: &str = ".shadowenv.d/500_envy.lisp";

use super::EnvManager;

pub struct Shadowenv;

impl Shadowenv {
    pub fn new() -> Self {
        Self
    }

    pub fn is_available(&self) -> bool {
        which("shadowenv").is_ok()
    }

    fn write_env_file(&self, dir: &Path, vars: &HashMap<String, String>) -> Result<()> {
        let shadowenv_dir = dir.join(".shadowenv.d");
        fs::create_dir_all(&shadowenv_dir).context("Failed to create .shadowenv.d")?;

        let mut content = String::from("(provide \"envy\" \"1.0.0\")\n\n");
        for (key, value) in vars {
            let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
            content.push_str(&format!("(env/set \"{}\" \"{}\")\n", key, escaped));
        }

        fs::write(shadowenv_dir.join("500_envy.lisp"), content)
            .context("Failed to write shadowenv environment file")?;

        Ok(())
    }

    fn trust(&self, dir: &Path) -> Result<()> {
        let status = Command::new("shadowenv")
            .args(["trust", "."])
            .current_dir(dir)
            .status()
            .context("Failed to run shadowenv trust")?;
        if !status.success() {
            bail!("shadowenv trust failed");
        }
        Ok(())
    }
}

/// Parses `(env/set "KEY" "VALUE")` lines from the shadowenv lisp file.
/// Returns `None` if the file does not exist yet.
pub fn read_vars(path: &Path) -> Option<HashMap<String, String>> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut vars = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("(env/set \"")
            && let Some((key, rest)) = rest.split_once("\" \"")
            && let Some(value) = rest.strip_suffix("\")")
        {
            vars.insert(
                key.to_string(),
                value.replace("\\\"", "\"").replace("\\\\", "\\"),
            );
        }
    }
    Some(vars)
}

impl EnvManager for Shadowenv {
    fn name(&self) -> &str {
        "shadowenv"
    }

    fn setup(&self, dir: &Path, vars: &HashMap<String, String>) -> Result<()> {
        self.write_env_file(dir, vars)?;
        self.trust(dir)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "envy_shadow_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    // ── read_vars ─────────────────────────────────────────────────────────────

    #[test]
    fn read_vars_missing_file_returns_none() {
        let path = std::path::Path::new("/nonexistent/500_envy_test.lisp");
        assert!(read_vars(path).is_none());
    }

    #[test]
    fn read_vars_parses_env_set_lines() {
        let dir = tmp_dir();
        let file = dir.join("500_envy.lisp");
        std::fs::write(
            &file,
            "(provide \"envy\" \"1.0.0\")\n\n(env/set \"FOO\" \"bar\")\n(env/set \"BAZ\" \"qux\")\n",
        ).unwrap();

        let vars = read_vars(&file).unwrap();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars["FOO"], "bar");
        assert_eq!(vars["BAZ"], "qux");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_vars_ignores_non_env_set_lines() {
        let dir = tmp_dir();
        let file = dir.join("500_envy.lisp");
        std::fs::write(
            &file,
            "(provide \"envy\" \"1.0.0\")\n; a comment\n(env/set \"KEY\" \"value\")\n",
        )
        .unwrap();

        let vars = read_vars(&file).unwrap();
        assert_eq!(vars.len(), 1);
        assert_eq!(vars["KEY"], "value");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_vars_unescapes_backslash_and_quote() {
        let dir = tmp_dir();
        let file = dir.join("500_envy.lisp");
        // Stored as: (env/set "KEY" "a\\b\"c")
        std::fs::write(&file, "(env/set \"KEY\" \"a\\\\b\\\"c\")\n").unwrap();

        let vars = read_vars(&file).unwrap();
        assert_eq!(vars["KEY"], "a\\b\"c");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_vars_empty_file_returns_empty_map() {
        let dir = tmp_dir();
        let file = dir.join("500_envy.lisp");
        std::fs::write(&file, "").unwrap();

        let vars = read_vars(&file).unwrap();
        assert!(vars.is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── write_env_file ────────────────────────────────────────────────────────

    #[test]
    fn write_env_file_creates_directory_and_file() {
        let dir = tmp_dir();
        let shadowenv = Shadowenv::new();
        let mut vars = HashMap::new();
        vars.insert("MY_VAR".into(), "hello".into());

        shadowenv.write_env_file(&dir, &vars).unwrap();

        let file = dir.join(".shadowenv.d").join("500_envy.lisp");
        assert!(file.exists());
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("(provide \"envy\""));
        assert!(content.contains("MY_VAR"));
        assert!(content.contains("hello"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_env_file_escapes_special_chars() {
        let dir = tmp_dir();
        let shadowenv = Shadowenv::new();
        let mut vars = HashMap::new();
        vars.insert("K".into(), "back\\slash and \"quote\"".into());

        shadowenv.write_env_file(&dir, &vars).unwrap();

        let file = dir.join(".shadowenv.d").join("500_envy.lisp");
        let content = std::fs::read_to_string(&file).unwrap();
        // Verify the written content can be round-tripped by read_vars.
        let parsed = read_vars(&file).unwrap();
        assert_eq!(parsed["K"], "back\\slash and \"quote\"");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = content;
    }

    // ── Shadowenv::name ───────────────────────────────────────────────────────

    #[test]
    fn shadowenv_name_is_shadowenv() {
        assert_eq!(Shadowenv::new().name(), "shadowenv");
    }
}
