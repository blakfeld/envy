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
            // Both key and value are escaped: unescaped quotes or backslashes would
            // corrupt the Lisp expression; an unescaped key could inject directives.
            let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
            content.push_str(&format!("(env/set \"{}\" \"{}\")\n", esc(key), esc(value)));
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
            let unescape = |s: &str| s.replace("\\\"", "\"").replace("\\\\", "\\");
            vars.insert(unescape(key), unescape(value));
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
    fn write_env_file_escapes_special_chars_in_value() {
        let dir = tmp_dir();
        let shadowenv = Shadowenv::new();
        let mut vars = HashMap::new();
        vars.insert("K".into(), "back\\slash and \"quote\"".into());

        shadowenv.write_env_file(&dir, &vars).unwrap();

        let file = dir.join(".shadowenv.d").join("500_envy.lisp");
        let parsed = read_vars(&file).unwrap();
        assert_eq!(parsed["K"], "back\\slash and \"quote\"");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_env_file_escapes_special_chars_in_key() {
        let dir = tmp_dir();
        let shadowenv = Shadowenv::new();
        let mut vars = HashMap::new();
        // A key with a quote would be a Lisp injection if not escaped.
        vars.insert("KEY_WITH_\"QUOTE\"".into(), "value".into());

        shadowenv.write_env_file(&dir, &vars).unwrap();

        let file = dir.join(".shadowenv.d").join("500_envy.lisp");
        let parsed = read_vars(&file).unwrap();
        // Should round-trip correctly rather than breaking the Lisp structure.
        assert_eq!(parsed["KEY_WITH_\"QUOTE\""], "value");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Shadowenv::name ───────────────────────────────────────────────────────

    #[test]
    fn shadowenv_name_is_shadowenv() {
        assert_eq!(Shadowenv::new().name(), "shadowenv");
    }

    // ── Shadowenv::is_available ───────────────────────────────────────────────

    #[test]
    fn shadowenv_is_available_consistent_with_which() {
        let expected = which("shadowenv").is_ok();
        assert_eq!(Shadowenv::new().is_available(), expected);
    }

    #[test]
    fn shadowenv_is_available_true_when_installed() {
        if which("shadowenv").is_err() { return; }
        assert!(Shadowenv::new().is_available(), "must be true when shadowenv is on PATH");
    }

    #[test]
    fn shadowenv_is_available_false_when_not_installed() {
        if which("shadowenv").is_ok() { return; }
        assert!(!Shadowenv::new().is_available(), "must be false when shadowenv is absent");
    }

    // ── Shadowenv::trust / setup ───────────────────────────────────────────────

    #[test]
    fn shadowenv_setup_writes_env_file() {
        let dir = tmp_dir();
        let shadowenv = Shadowenv::new();
        let mut vars = HashMap::new();
        vars.insert("SETUP_KEY".into(), "setup_val".into());
        shadowenv.write_env_file(&dir, &vars).unwrap();
        let file = dir.join(".shadowenv.d").join("500_envy.lisp");
        assert!(file.exists(), "setup must create the lisp file");
        let content = std::fs::read_to_string(&file).unwrap();
        assert!(content.contains("SETUP_KEY"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shadowenv_setup_fails_when_shadowenv_not_installed() {
        // When shadowenv binary is absent, trust() must fail, and setup() propagates that.
        // This kills `replace setup -> Ok(())` and `replace trust -> Ok(())`.
        if which("shadowenv").is_ok() { return; }
        let dir = tmp_dir();
        let shadowenv = Shadowenv::new();
        let mut vars = HashMap::new();
        vars.insert("KEY".into(), "val".into());
        let result = shadowenv.setup(&dir, &vars);
        assert!(result.is_err(), "setup must fail when shadowenv binary is absent");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shadowenv_setup_succeeds_when_shadowenv_installed() {
        // When shadowenv IS installed, setup() must succeed (trust runs without bail).
        // This kills `delete ! in trust` — mutation bails on success, making this Err.
        if which("shadowenv").is_err() { return; }
        let dir = tmp_dir();
        let shadowenv = Shadowenv::new();
        let mut vars = HashMap::new();
        vars.insert("KEY".into(), "val".into());
        let result = shadowenv.setup(&dir, &vars);
        assert!(result.is_ok(), "setup must succeed when shadowenv is installed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
