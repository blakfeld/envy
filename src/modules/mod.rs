mod bun;
mod crystal;
mod dart;
mod deno;
mod dotnet;
mod elasticsearch;
mod elixir;
mod erlang;
mod gcloud;
mod generic;
mod kafka;
mod kotlin;
mod mailhog;
mod mariadb;
mod meilisearch;
mod memcached;
mod minio;
mod mongodb;
mod mysql;
mod nginx;
mod node;
mod opensearch;
mod postgres;
mod rabbitmq;
mod redis;
mod ruby;
mod rust;
mod typescript;
mod vault;
mod zig;

use anyhow::{Context, Result};
use std::borrow::Cow;
use std::collections::HashMap;
use std::process::Command;

use crate::config::Dependency;
use crate::package_manager::PackageManager;

pub trait Module {
    /// Whether this module manages a background service.
    fn is_service(&self) -> bool {
        false
    }

    /// The install source recorded in envy.lock (e.g. "homebrew", "rustup").
    fn source(&self) -> &'static str {
        "homebrew"
    }

    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool>;
    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()>;

    /// The name used when managing this service via the package manager (start/stop/status).
    /// Override when the PM service name differs from the dependency name in envy.yml.
    fn service_name<'a>(&self, dep: &'a Dependency) -> Cow<'a, str> {
        Cow::Borrowed(&dep.name)
    }

    fn is_running(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<bool> {
        Ok(true)
    }

    fn start(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<()> {
        Ok(())
    }

    fn stop(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<()> {
        Ok(())
    }

    /// Probes the service directly to confirm it is accepting connections.
    /// Override in service modules; default always passes.
    fn health_check(&self, _dep: &Dependency) -> Result<()> {
        Ok(())
    }

    /// Polls `health_check` until the service is ready or attempts are exhausted.
    fn wait_for_ready(&self, dep: &Dependency) -> Result<()> {
        const MAX: u32 = 10;
        const SLEEP_MS: u64 = 500;
        for attempt in 1..=MAX {
            match self.health_check(dep) {
                Ok(()) => return Ok(()),
                Err(_) if attempt < MAX => {
                    std::thread::sleep(std::time::Duration::from_millis(SLEEP_MS));
                }
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!("{} did not become healthy after {MAX} attempts", dep.name)
                    });
                }
            }
        }
        Ok(())
    }

    /// Environment variables this module injects when active (e.g. SMTP_HOST, VAULT_ADDR).
    /// User-configured vars in envy.yml always take precedence over these defaults.
    fn env_vars(&self, _dep: &Dependency) -> HashMap<String, String> {
        HashMap::new()
    }

    /// Returns the exact version string currently installed.
    /// Default delegates to the package manager; override for non-brew sources.
    fn resolved_version(
        &self,
        pm: &dyn PackageManager,
        dep: &Dependency,
    ) -> Result<Option<String>> {
        pm.resolved_version(dep)
    }
}

/// Resolves a dependency name to its module, falling back to a generic install.
pub fn get(name: &str) -> Box<dyn Module> {
    match name {
        // ── Services ──────────────────────────────────────────────────────────
        "mysql" => Box::new(mysql::MysqlModule),
        "redis" => Box::new(redis::RedisModule),
        "postgresql" | "postgres" => Box::new(postgres::PostgresModule),
        "mongodb" | "mongo" => Box::new(mongodb::MongodbModule),
        "nginx" => Box::new(nginx::NginxModule),
        "kafka" => Box::new(kafka::KafkaModule),
        "rabbitmq" => Box::new(rabbitmq::RabbitmqModule),
        "memcached" => Box::new(memcached::MemcachedModule),
        "elasticsearch" | "elastic" => Box::new(elasticsearch::ElasticsearchModule),
        "opensearch" => Box::new(opensearch::OpenSearchModule),
        "meilisearch" | "meili" => Box::new(meilisearch::MeilisearchModule),
        "minio" => Box::new(minio::MinioModule),
        "mailhog" => Box::new(mailhog::MailhogModule),
        "mariadb" => Box::new(mariadb::MariadbModule),
        "vault" | "hashicorp-vault" => Box::new(vault::VaultModule),
        // ── Languages / runtimes ──────────────────────────────────────────────
        "rust" | "rustup" => Box::new(rust::RustModule),
        "node" | "nodejs" | "javascript" | "js" => Box::new(node::NodeModule),
        "typescript" | "ts" => Box::new(typescript::TypeScriptModule),
        "ruby" => Box::new(ruby::RubyModule),
        "python" | "python3" => Box::new(PackageModule {
            default: "python",
            apt: "python3",
            winget: "Python.Python.3",
        }),
        "go" | "golang" => Box::new(PackageModule {
            default: "go",
            apt: "golang-go",
            winget: "GoLang.Go",
        }),
        "java" | "openjdk" => Box::new(PackageModule {
            default: "openjdk",
            apt: "default-jdk",
            winget: "Microsoft.OpenJDK.21",
        }),
        "kotlin" => Box::new(kotlin::KotlinModule),
        "scala" => Box::new(PackageModule {
            default: "scala",
            apt: "scala",
            winget: "EPFL.Scala",
        }),
        "php" => Box::new(PackageModule {
            default: "php",
            apt: "php",
            winget: "PHP.PHP",
        }),
        "elixir" => Box::new(elixir::ElixirModule),
        "erlang" => Box::new(erlang::ErlangModule),
        "deno" => Box::new(deno::DenoModule),
        "bun" => Box::new(bun::BunModule),
        "dotnet" | "dotnet-sdk" | "csharp" => Box::new(dotnet::DotnetModule),
        "dart" => Box::new(dart::DartModule),
        "zig" => Box::new(zig::ZigModule),
        "crystal" => Box::new(crystal::CrystalModule),
        // ── CLI / infrastructure tools ────────────────────────────────────────
        "awscli" | "aws" | "aws-cli" => Box::new(PackageModule {
            default: "awscli",
            apt: "awscli",
            winget: "Amazon.AWSCLI",
        }),
        "gh" | "github-cli" => Box::new(PackageModule {
            default: "gh",
            apt: "gh",
            winget: "GitHub.cli",
        }),
        "gcloud" | "google-cloud-sdk" => Box::new(gcloud::GcloudModule),
        "kubectl" | "kubernetes-cli" => Box::new(PackageModule {
            default: "kubectl",
            apt: "kubectl",
            winget: "Kubernetes.kubectl",
        }),
        "helm" => Box::new(PackageModule {
            default: "helm",
            apt: "helm",
            winget: "Helm.Helm",
        }),
        "terraform" => Box::new(PackageModule {
            default: "terraform",
            apt: "terraform",
            winget: "Hashicorp.Terraform",
        }),
        "azure-cli" | "az" => Box::new(PackageModule {
            default: "azure-cli",
            apt: "azure-cli",
            winget: "Microsoft.AzureCLI",
        }),
        "swift" => Box::new(PackageModule {
            default: "swift",
            apt: "swift",
            winget: "Swift.Toolchain",
        }),
        _ => Box::new(generic::GenericModule),
    }
}

/// A simple package install module with per-PM package names.
/// Use for languages and tools that have no special install logic beyond `install_package`.
struct PackageModule {
    default: &'static str,
    apt: &'static str,
    winget: &'static str,
}

impl PackageModule {
    fn name_for(&self, pm: &dyn PackageManager) -> &'static str {
        match pm.name() {
            "apt" => self.apt,
            "winget" => self.winget,
            _ => self.default,
        }
    }
}

impl Module for PackageModule {
    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool> {
        pm.is_package_installed(&pm_dep(dep, self.name_for(pm)))
    }

    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        pm.install_package(&pm_dep(dep, self.name_for(pm)))
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Writes a `my.cnf` snippet for MySQL-compatible services (MySQL and MariaDB share the same
/// config format). Creates the directory if absent.
pub(super) fn write_mysql_config(
    config_dir: &std::path::Path,
    port: u16,
    cli_args: Option<&str>,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(config_dir)
        .with_context(|| format!("Failed to create config dir {}", config_dir.display()))?;

    let mut ini = format!("[mysqld]\nport = {}\n", port);
    if let Some(args) = cli_args {
        for arg in args.split_whitespace() {
            // Require the -- prefix so bare key=value tokens can't inject directives.
            let Some(rest) = arg.strip_prefix("--") else {
                continue;
            };
            let Some((key, val)) = rest.split_once('=') else {
                continue;
            };
            // Keys must be safe ini identifiers: alphanumeric, hyphens, underscores.
            if !key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                continue;
            }
            // Values must not contain newlines or null bytes (would break ini structure).
            if val.contains('\n') || val.contains('\0') {
                continue;
            }
            ini.push_str(&format!("{} = {}\n", key, val));
        }
    }

    std::fs::write(config_dir.join("my.cnf"), ini).context("Failed to write my.cnf")?;
    Ok(())
}

/// Runs a command, inheriting stdio, and bails on non-zero exit.
pub(super) fn run_cmd(prog: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(prog)
        .args(args)
        .status()
        .with_context(|| format!("failed to start `{prog}`"))?;
    if !status.success() {
        anyhow::bail!("`{prog} {}` failed", args.join(" "));
    }
    Ok(())
}

/// Reads a YAML sequence of strings from dep.extra.
pub(super) fn extra_strs(dep: &Dependency, key: &str) -> Vec<String> {
    dep.extra
        .get(key)
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Returns a copy of `dep` with the name replaced by the platform-appropriate package name.
pub(super) fn pm_dep(dep: &Dependency, name: &str) -> Dependency {
    Dependency {
        name: name.to_string(),
        version: dep.version.clone(),
        tap: dep.tap.clone(),
        profiles: dep.profiles.clone(),
        after_install: dep.after_install.clone(),
        extra: dep.extra.clone(),
    }
}

/// Returns the platform-appropriate package name for Node.js.
/// Shared between NodeModule and TypeScriptModule.
pub(super) fn node_pkg(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "apt" => "nodejs",
        "winget" => "OpenJS.NodeJS",
        _ => "node",
    }
}

/// Returns the platform-appropriate package name for Ruby.
pub(super) fn ruby_pkg(pm: &dyn PackageManager) -> &'static str {
    match pm.name() {
        "winget" => "RubyInstallerTeam.Ruby.3",
        _ => "ruby",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── get ───────────────────────────────────────────────────────────────────

    #[test]
    fn get_mysql_is_service() {
        assert!(get("mysql").is_service());
    }

    #[test]
    fn get_redis_is_service() {
        assert!(get("redis").is_service());
    }

    #[test]
    fn get_postgres_is_service() {
        assert!(get("postgresql").is_service());
        assert!(get("postgres").is_service());
    }

    #[test]
    fn get_mongodb_is_service() {
        assert!(get("mongodb").is_service());
        assert!(get("mongo").is_service());
    }

    #[test]
    fn get_nginx_is_service() {
        assert!(get("nginx").is_service());
    }

    #[test]
    fn get_rabbitmq_is_service() {
        assert!(get("rabbitmq").is_service());
    }

    #[test]
    fn get_kafka_is_service() {
        assert!(get("kafka").is_service());
    }

    #[test]
    fn get_memcached_is_service() {
        assert!(get("memcached").is_service());
    }

    #[test]
    fn get_elasticsearch_is_service() {
        assert!(get("elasticsearch").is_service());
        assert!(get("elastic").is_service());
    }

    #[test]
    fn get_opensearch_is_service() {
        assert!(get("opensearch").is_service());
    }

    #[test]
    fn get_meilisearch_is_service() {
        assert!(get("meilisearch").is_service());
        assert!(get("meili").is_service());
    }

    #[test]
    fn get_minio_is_service() {
        assert!(get("minio").is_service());
    }

    #[test]
    fn get_mailhog_is_service() {
        assert!(get("mailhog").is_service());
    }

    #[test]
    fn get_mariadb_is_service() {
        assert!(get("mariadb").is_service());
    }

    #[test]
    fn get_vault_is_service() {
        assert!(get("vault").is_service());
        assert!(get("hashicorp-vault").is_service());
    }

    #[test]
    fn get_erlang_is_not_a_service() {
        assert!(!get("erlang").is_service());
    }

    #[test]
    fn get_elixir_is_not_a_service() {
        assert!(!get("elixir").is_service());
    }

    #[test]
    fn get_deno_source_is_deno_installer() {
        assert_eq!(get("deno").source(), "deno-installer");
    }

    #[test]
    fn get_bun_source_is_bun_installer() {
        assert_eq!(get("bun").source(), "bun-installer");
    }

    #[test]
    fn get_dotnet_aliases_resolve() {
        for name in &["dotnet", "dotnet-sdk", "csharp"] {
            assert!(!get(name).is_service(), "{} should not be a service", name);
        }
    }

    #[test]
    fn get_dart_is_not_a_service() {
        assert!(!get("dart").is_service());
    }

    #[test]
    fn get_zig_is_not_a_service() {
        assert!(!get("zig").is_service());
    }

    #[test]
    fn get_crystal_is_not_a_service() {
        assert!(!get("crystal").is_service());
    }

    #[test]
    fn get_awscli_aliases_resolve() {
        for name in &["awscli", "aws", "aws-cli"] {
            assert!(!get(name).is_service(), "{} should not be a service", name);
        }
    }

    #[test]
    fn get_gh_aliases_resolve() {
        for name in &["gh", "github-cli"] {
            assert!(!get(name).is_service(), "{} should not be a service", name);
        }
    }

    #[test]
    fn get_gcloud_source_is_gcloud_installer() {
        assert_eq!(get("gcloud").source(), "gcloud-installer");
        assert_eq!(get("google-cloud-sdk").source(), "gcloud-installer");
    }

    #[test]
    fn get_kubectl_aliases_resolve() {
        for name in &["kubectl", "kubernetes-cli"] {
            assert!(!get(name).is_service(), "{} should not be a service", name);
        }
    }

    #[test]
    fn get_helm_is_not_a_service() {
        assert!(!get("helm").is_service());
    }

    #[test]
    fn get_terraform_is_not_a_service() {
        assert!(!get("terraform").is_service());
    }

    #[test]
    fn get_azure_cli_aliases_resolve() {
        for name in &["azure-cli", "az"] {
            assert!(!get(name).is_service(), "{} should not be a service", name);
        }
    }

    #[test]
    fn get_rust_source_is_rustup() {
        assert_eq!(get("rust").source(), "rustup");
        assert_eq!(get("rustup").source(), "rustup");
    }

    #[test]
    fn get_node_aliases_resolve() {
        for name in &["node", "nodejs", "javascript", "js"] {
            let m = get(name);
            assert!(!m.is_service(), "{} should not be a service", name);
        }
    }

    #[test]
    fn get_language_aliases_are_not_services() {
        for name in &[
            "python",
            "python3",
            "java",
            "openjdk",
            "go",
            "golang",
            "ruby",
            "typescript",
            "ts",
            "kotlin",
            "scala",
            "php",
            "elixir",
        ] {
            assert!(!get(name).is_service(), "{} should not be a service", name);
        }
    }

    #[test]
    fn get_unknown_falls_back_to_generic() {
        let m = get("somerandompkg");
        assert!(!m.is_service());
        assert_eq!(m.source(), "homebrew");
    }

    // ── get() match arm identity tests ───────────────────────────────────────
    // These use an apt PM that reports "nodejs" as installed but not "node",
    // distinguishing NodeModule (which maps to "nodejs" on apt) from GenericModule
    // (which passes through the raw dep name "node").

    #[test]
    fn get_node_routes_to_node_module_on_apt() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            installed_pkg: Some("nodejs"),
            ..Default::default()
        };
        let dep = Dependency::simple("node");
        assert!(get("node").is_installed(&pm, &dep).unwrap());
        assert!(get("nodejs").is_installed(&pm, &dep).unwrap());
        assert!(get("javascript").is_installed(&pm, &dep).unwrap());
        assert!(get("js").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_typescript_routes_to_typescript_module_on_apt() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            installed_pkg: Some("nodejs"),
            ..Default::default()
        };
        let dep = Dependency::simple("typescript");
        assert!(get("typescript").is_installed(&pm, &dep).unwrap());
        assert!(get("ts").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_ruby_routes_to_ruby_module() {
        // ruby_pkg on brew returns "ruby"; GenericModule also passes "ruby" through.
        // Use installed_pkg to distinguish: ruby_pkg on apt also returns "ruby".
        // Best way: verify source() is "homebrew" (which GenericModule also returns…)
        // Instead verify via winget: ruby_pkg("winget") = "RubyInstallerTeam.Ruby.3".
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("RubyInstallerTeam.Ruby.3"),
            ..Default::default()
        };
        let dep = Dependency::simple("ruby");
        assert!(get("ruby").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_python_routes_to_package_module_on_apt() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            installed_pkg: Some("python3"),
            ..Default::default()
        };
        let dep = Dependency::simple("python");
        assert!(get("python").is_installed(&pm, &dep).unwrap());
        assert!(get("python3").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_go_routes_to_package_module_on_apt() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            installed_pkg: Some("golang-go"),
            ..Default::default()
        };
        let dep = Dependency::simple("go");
        assert!(get("go").is_installed(&pm, &dep).unwrap());
        assert!(get("golang").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_java_routes_to_package_module_on_apt() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            installed_pkg: Some("default-jdk"),
            ..Default::default()
        };
        let dep = Dependency::simple("java");
        assert!(get("java").is_installed(&pm, &dep).unwrap());
        assert!(get("openjdk").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_kotlin_routes_to_kotlin_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("JetBrains.Kotlin"),
            ..Default::default()
        };
        let dep = Dependency::simple("kotlin");
        assert!(get("kotlin").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_scala_routes_to_package_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("EPFL.Scala"),
            ..Default::default()
        };
        let dep = Dependency::simple("scala");
        assert!(get("scala").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_php_routes_to_package_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("PHP.PHP"),
            ..Default::default()
        };
        let dep = Dependency::simple("php");
        assert!(get("php").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_elixir_routes_to_elixir_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("Erlang-Solutions.Elixir"),
            ..Default::default()
        };
        let dep = Dependency::simple("elixir");
        assert!(get("elixir").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_erlang_routes_to_erlang_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("Erlang-Solutions.Erlang"),
            ..Default::default()
        };
        let dep = Dependency::simple("erlang");
        assert!(get("erlang").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_dotnet_routes_to_dotnet_module_on_apt() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            installed_pkg: Some("dotnet-sdk-8.0"),
            ..Default::default()
        };
        let dep = Dependency::simple("dotnet");
        assert!(get("dotnet").is_installed(&pm, &dep).unwrap());
        assert!(get("dotnet-sdk").is_installed(&pm, &dep).unwrap());
        assert!(get("csharp").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_dart_routes_to_dart_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("Dart.Dart"),
            ..Default::default()
        };
        let dep = Dependency::simple("dart");
        assert!(get("dart").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_zig_routes_to_zig_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("zig-lang.zig"),
            ..Default::default()
        };
        let dep = Dependency::simple("zig");
        assert!(get("zig").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_crystal_routes_to_crystal_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("Manas.Crystal"),
            ..Default::default()
        };
        let dep = Dependency::simple("crystal");
        assert!(get("crystal").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_awscli_routes_to_package_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("Amazon.AWSCLI"),
            ..Default::default()
        };
        let dep = Dependency::simple("awscli");
        assert!(get("awscli").is_installed(&pm, &dep).unwrap());
        assert!(get("aws").is_installed(&pm, &dep).unwrap());
        assert!(get("aws-cli").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_gh_routes_to_package_module_on_apt() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            installed_pkg: Some("gh"),
            ..Default::default()
        };
        let dep = Dependency::simple("gh");
        assert!(get("gh").is_installed(&pm, &dep).unwrap());
        assert!(get("github-cli").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_kubectl_routes_to_package_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("Kubernetes.kubectl"),
            ..Default::default()
        };
        let dep = Dependency::simple("kubectl");
        assert!(get("kubectl").is_installed(&pm, &dep).unwrap());
        assert!(get("kubernetes-cli").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_helm_routes_to_package_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("Helm.Helm"),
            ..Default::default()
        };
        let dep = Dependency::simple("helm");
        assert!(get("helm").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_terraform_routes_to_package_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("Hashicorp.Terraform"),
            ..Default::default()
        };
        let dep = Dependency::simple("terraform");
        assert!(get("terraform").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_azure_cli_routes_to_package_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("Microsoft.AzureCLI"),
            ..Default::default()
        };
        let dep = Dependency::simple("azure-cli");
        assert!(get("azure-cli").is_installed(&pm, &dep).unwrap());
        assert!(get("az").is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn get_swift_routes_to_package_module_on_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("Swift.Toolchain"),
            ..Default::default()
        };
        let dep = Dependency::simple("swift");
        assert!(get("swift").is_installed(&pm, &dep).unwrap());
    }

    // ── extra_strs ────────────────────────────────────────────────────────────

    #[test]
    fn extra_strs_missing_key_returns_empty() {
        let dep = Dependency::simple("node");
        assert!(extra_strs(&dep, "global_packages").is_empty());
    }

    #[test]
    fn extra_strs_sequence_returns_strings() {
        let mut extra = HashMap::new();
        extra.insert(
            "global_packages".to_string(),
            serde_yaml::Value::Sequence(vec![
                serde_yaml::Value::String("typescript".into()),
                serde_yaml::Value::String("eslint".into()),
            ]),
        );
        let dep = Dependency {
            name: "node".into(),
            version: None,
            tap: None,
            profiles: None,
            after_install: None,
            extra,
        };
        let pkgs = extra_strs(&dep, "global_packages");
        assert_eq!(pkgs, vec!["typescript", "eslint"]);
    }

    #[test]
    fn extra_strs_non_sequence_value_returns_empty() {
        let mut extra = HashMap::new();
        extra.insert(
            "global_packages".to_string(),
            serde_yaml::Value::String("ts".into()),
        );
        let dep = Dependency {
            name: "node".into(),
            version: None,
            tap: None,
            profiles: None,
            after_install: None,
            extra,
        };
        assert!(extra_strs(&dep, "global_packages").is_empty());
    }

    // ── pm_dep ────────────────────────────────────────────────────────────────

    #[test]
    fn pm_dep_replaces_name_preserves_other_fields() {
        let dep = Dependency {
            name: "ruby".into(),
            version: Some("3.2".into()),
            tap: Some("homebrew/core".into()),
            profiles: Some(vec!["dev".into()]),
            after_install: None,
            extra: HashMap::new(),
        };
        let remapped = pm_dep(&dep, "ruby@3.2");
        assert_eq!(remapped.name, "ruby@3.2");
        assert_eq!(remapped.version, Some("3.2".into()));
        assert_eq!(remapped.tap, Some("homebrew/core".into()));
        assert_eq!(remapped.profiles, Some(vec!["dev".into()]));
    }

    // ── Module trait defaults ─────────────────────────────────────────────────

    struct DefaultModule;
    impl Module for DefaultModule {
        fn is_installed(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<bool> {
            Ok(true)
        }
        fn install(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn default_service_name_returns_dep_name() {
        let dep = Dependency::simple("myservice");
        assert_eq!(DefaultModule.service_name(&dep).as_ref(), "myservice");
    }

    #[test]
    fn default_service_name_different_names() {
        assert_eq!(
            DefaultModule
                .service_name(&Dependency::simple("redis"))
                .as_ref(),
            "redis"
        );
        assert_eq!(
            DefaultModule
                .service_name(&Dependency::simple("mysql"))
                .as_ref(),
            "mysql"
        );
    }

    #[test]
    fn default_is_running_returns_true() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("test");
        assert!(DefaultModule.is_running(&pm, &dep).unwrap());
    }

    #[test]
    fn default_env_vars_returns_empty_map() {
        let dep = Dependency::simple("test");
        assert!(DefaultModule.env_vars(&dep).is_empty());
    }

    #[test]
    fn default_resolved_version_delegates_to_pm() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("test");
        assert!(DefaultModule.resolved_version(&pm, &dep).unwrap().is_none());
    }

    #[test]
    fn default_resolved_version_delegates_to_pm_returns_some_when_pm_has_version() {
        let pm = crate::package_manager::MockPackageManager {
            version: Some("1.2.3".into()),
            ..Default::default()
        };
        let dep = Dependency::simple("test");
        assert_eq!(
            DefaultModule.resolved_version(&pm, &dep).unwrap(),
            Some("1.2.3".into())
        );
    }

    // ── wait_for_ready ────────────────────────────────────────────────────────

    struct HealthyModule;
    impl Module for HealthyModule {
        fn is_installed(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<bool> {
            Ok(true)
        }
        fn install(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<()> {
            Ok(())
        }
        fn health_check(&self, _: &Dependency) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn wait_for_ready_succeeds_immediately_when_healthy() {
        let dep = Dependency::simple("testservice");
        HealthyModule.wait_for_ready(&dep).unwrap();
    }

    struct SickModule;
    impl Module for SickModule {
        fn is_installed(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<bool> {
            Ok(true)
        }
        fn install(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<()> {
            Ok(())
        }
        fn health_check(&self, _: &Dependency) -> Result<()> {
            anyhow::bail!("not healthy")
        }
    }

    #[test]
    fn wait_for_ready_fails_with_context_after_max_attempts() {
        let dep = Dependency::simple("testservice");
        let err = SickModule.wait_for_ready(&dep).unwrap_err();
        assert!(err.to_string().contains("testservice"));
        assert!(err.to_string().contains("10 attempts"));
    }

    struct EventuallyHealthyModule {
        calls: std::cell::Cell<u32>,
        fail_for: u32,
    }
    impl Module for EventuallyHealthyModule {
        fn is_installed(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<bool> {
            Ok(true)
        }
        fn install(
            &self,
            _: &dyn crate::package_manager::PackageManager,
            _: &Dependency,
        ) -> Result<()> {
            Ok(())
        }
        fn health_check(&self, _: &Dependency) -> Result<()> {
            let n = self.calls.get();
            self.calls.set(n + 1);
            if n < self.fail_for {
                anyhow::bail!("not ready yet")
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn wait_for_ready_retries_until_healthy() {
        let m = EventuallyHealthyModule {
            calls: std::cell::Cell::new(0),
            fail_for: 2,
        };
        let dep = Dependency::simple("eventually");
        m.wait_for_ready(&dep).unwrap();
        assert!(m.calls.get() >= 3, "Expected at least 3 health_check calls");
    }

    // ── node_pkg ──────────────────────────────────────────────────────────────

    #[test]
    fn node_pkg_apt() {
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            ..Default::default()
        };
        assert_eq!(node_pkg(&pm), "nodejs");
    }

    #[test]
    fn node_pkg_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(node_pkg(&pm), "OpenJS.NodeJS");
    }

    #[test]
    fn node_pkg_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(node_pkg(&pm), "node");
    }

    // ── ruby_pkg ──────────────────────────────────────────────────────────────

    #[test]
    fn ruby_pkg_winget() {
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(ruby_pkg(&pm), "RubyInstallerTeam.Ruby.3");
    }

    #[test]
    fn ruby_pkg_brew_default() {
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(ruby_pkg(&pm), "ruby");
    }

    // ── PackageModule ─────────────────────────────────────────────────────────

    #[test]
    fn package_module_name_for_apt() {
        let m = PackageModule {
            default: "go",
            apt: "golang-go",
            winget: "GoLang.Go",
        };
        let pm = crate::package_manager::MockPackageManager {
            name: "apt",
            ..Default::default()
        };
        assert_eq!(m.name_for(&pm), "golang-go");
    }

    #[test]
    fn package_module_name_for_winget() {
        let m = PackageModule {
            default: "go",
            apt: "golang-go",
            winget: "GoLang.Go",
        };
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            ..Default::default()
        };
        assert_eq!(m.name_for(&pm), "GoLang.Go");
    }

    #[test]
    fn package_module_name_for_default() {
        let m = PackageModule {
            default: "go",
            apt: "golang-go",
            winget: "GoLang.Go",
        };
        let pm = crate::package_manager::MockPackageManager {
            name: "brew",
            ..Default::default()
        };
        assert_eq!(m.name_for(&pm), "go");
    }

    #[test]
    fn package_module_is_installed_true() {
        let m = PackageModule {
            default: "go",
            apt: "golang-go",
            winget: "GoLang.Go",
        };
        let pm = crate::package_manager::MockPackageManager {
            installed: true,
            ..Default::default()
        };
        let dep = Dependency::simple("go");
        assert!(m.is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn package_module_is_installed_false() {
        let m = PackageModule {
            default: "go",
            apt: "golang-go",
            winget: "GoLang.Go",
        };
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("go");
        assert!(!m.is_installed(&pm, &dep).unwrap());
    }

    #[test]
    fn package_module_install_propagates_pm_error() {
        let m = PackageModule {
            default: "go",
            apt: "golang-go",
            winget: "GoLang.Go",
        };
        let pm = crate::package_manager::MockPackageManager {
            install_fails: true,
            ..Default::default()
        };
        let dep = Dependency::simple("go");
        assert!(m.install(&pm, &dep).is_err());
    }

    // ── write_mysql_config ────────────────────────────────────────────────────

    fn write_and_read(port: u16, cli_args: Option<&str>) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "envy_mysql_sec_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        write_mysql_config(&dir, port, cli_args).unwrap();
        let content = std::fs::read_to_string(dir.join("my.cnf")).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        content
    }

    #[test]
    fn write_mysql_config_includes_port() {
        let content = write_and_read(3307, None);
        assert!(content.contains("port = 3307"));
    }

    #[test]
    fn write_mysql_config_accepts_valid_double_dash_arg() {
        let content = write_and_read(3306, Some("--innodb-buffer-pool-size=256M"));
        assert!(content.contains("innodb-buffer-pool-size = 256M"));
    }

    #[test]
    fn write_mysql_config_rejects_bare_key_value_without_double_dash() {
        // Without --, the token should be silently skipped.
        let content = write_and_read(3306, Some("skip_grant_tables=1"));
        assert!(!content.contains("skip_grant_tables"));
    }

    #[test]
    fn write_mysql_config_rejects_key_with_special_chars() {
        // A key with characters outside [a-zA-Z0-9_-] must be dropped.
        let content = write_and_read(3306, Some("--bad;key=val"));
        assert!(!content.contains("bad;key"));
    }

    #[test]
    fn write_mysql_config_skips_args_without_equals() {
        let content = write_and_read(3306, Some("--no-value-here"));
        assert!(!content.contains("no-value-here"));
    }

    #[test]
    fn write_mysql_config_treats_newline_in_args_as_separator() {
        // \n is whitespace — split_whitespace splits the arg list on it.
        // The token "line2" has no "--" prefix and is skipped; "--key=value" is written normally.
        let content = write_and_read(3306, Some("--key=value\nline2"));
        assert!(
            content.contains("key = value"),
            "Expected valid arg to be written"
        );
        assert!(
            !content.contains("line2"),
            "Bare token after newline must be skipped"
        );
    }

    #[test]
    fn write_mysql_config_rejects_value_with_null_byte() {
        let content = write_and_read(3306, Some("--key=val\x00ue"));
        assert!(!content.contains("val"));
    }

    #[test]
    fn write_mysql_config_accepts_value_without_newline_or_null() {
        let content = write_and_read(3306, Some("--max-connections=200"));
        assert!(content.contains("max-connections = 200"));
    }

    // ── run_cmd ───────────────────────────────────────────────────────────────

    #[test]
    fn run_cmd_succeeds_on_true() {
        assert!(run_cmd("true", &[]).is_ok());
    }

    #[test]
    fn run_cmd_fails_on_false() {
        assert!(run_cmd("false", &[]).is_err());
    }

    #[test]
    fn run_cmd_error_message_contains_program_name() {
        let err = run_cmd("false", &["--arg"]).unwrap_err();
        assert!(err.to_string().contains("false"));
    }

    // ── get: match arm identity for "gh"/"github-cli" ─────────────────────────

    #[test]
    fn get_gh_routes_to_package_module_not_generic_on_winget() {
        // PackageModule maps "gh"/"github-cli" to "GitHub.cli" on winget.
        // GenericModule would pass the raw dep name "gh" through — would not match "GitHub.cli".
        let pm = crate::package_manager::MockPackageManager {
            name: "winget",
            installed_pkg: Some("GitHub.cli"),
            ..Default::default()
        };
        let dep = Dependency::simple("gh");
        assert!(get("gh").is_installed(&pm, &dep).unwrap());
        assert!(get("github-cli").is_installed(&pm, &dep).unwrap());
    }
}
