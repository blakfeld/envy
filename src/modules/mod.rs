pub(crate) mod helpers;

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
mod java;
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
mod python;
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

use crate::config::Dependency;
use crate::output;
use crate::package_manager::PackageManager;
use helpers::{
    PackageModule, extra_port, extra_strs, node_pkg, pm_dep, run_cmd, tcp_ping, write_mysql_config,
};

pub struct ServiceConfig {
    pub health_check_max_attempts: u32,
    pub health_check_sleep_ms: u64,
    pub shutdown_max_attempts: u32,
    pub shutdown_sleep_ms: u64,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            health_check_max_attempts: 10,
            health_check_sleep_ms: 500,
            shutdown_max_attempts: 10,
            shutdown_sleep_ms: 500,
        }
    }
}

pub trait Module: Sync {
    /// Whether this module manages a background service.
    fn is_service(&self) -> bool {
        false
    }

    /// The default TCP port this service listens on, if any.
    /// Used by `check_port_conflicts` to detect conflicts even when `port` is not
    /// explicitly set in devy.yml.
    fn default_port(&self) -> Option<u16> {
        None
    }

    /// The install source recorded in devy.lock (e.g. "homebrew", "rustup").
    /// Return `None` to derive the source from the active package manager name.
    fn source(&self) -> Option<&'static str> {
        None
    }

    fn is_installed(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<bool>;
    fn install(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()>;

    /// The name used when managing this service via the package manager (start/stop/status).
    /// Override when the PM service name differs from the dependency name in devy.yml.
    fn service_name<'a>(&self, dep: &'a Dependency) -> Cow<'a, str> {
        Cow::Borrowed(&dep.name)
    }

    fn is_running(&self, _pm: &dyn PackageManager, _dep: &Dependency) -> Result<bool> {
        Ok(false)
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

    /// Timing and attempt configuration for health-check and shutdown polling.
    /// Override `service_config()` for slow-starting or slow-stopping services.
    fn service_config(&self) -> ServiceConfig {
        ServiceConfig::default()
    }

    /// Polls `health_check` until the service is ready or attempts are exhausted.
    fn wait_for_ready(&self, dep: &Dependency) -> Result<()> {
        let cfg = self.service_config();
        let max = cfg.health_check_max_attempts;
        if max == 0 {
            anyhow::bail!(
                "health_check_max_attempts returned 0 for '{}'; must be > 0",
                dep.name
            );
        }
        let sleep_ms = cfg.health_check_sleep_ms;
        let mut last_err = anyhow::anyhow!("{} health check produced no error", dep.name);
        for attempt in 1..=max {
            match self.health_check(dep) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_err = e;
                    if attempt < max {
                        if attempt % 10 == 0 {
                            output::step(&format!(
                                "Still waiting for {} ({}/{})",
                                dep.name, attempt, max
                            ));
                        }
                        std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
                    }
                }
            }
        }
        Err(last_err)
            .with_context(|| format!("{} did not become healthy after {max} attempts", dep.name))
    }

    /// Polls `is_running` until the service has stopped or attempts are exhausted.
    fn wait_for_stopped(&self, pm: &dyn PackageManager, dep: &Dependency) -> Result<()> {
        let cfg = self.service_config();
        let max = cfg.shutdown_max_attempts;
        if max == 0 {
            anyhow::bail!(
                "shutdown_max_attempts returned 0 for '{}'; must be > 0",
                dep.name
            );
        }
        let sleep_ms = cfg.shutdown_sleep_ms;
        for attempt in 1..=max {
            if !self.is_running(pm, dep)? {
                return Ok(());
            }
            if attempt < max {
                std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
            }
        }
        anyhow::bail!("{} did not stop after {} attempts", dep.name, max)
    }

    /// Environment variables this module injects when active (e.g. SMTP_HOST, VAULT_ADDR).
    /// User-configured vars in devy.yml always take precedence over these defaults.
    fn env_vars(
        &self,
        _dep: &Dependency,
        _project_root: &std::path::Path,
    ) -> HashMap<String, String> {
        HashMap::new()
    }

    /// PATH entries to prepend when this module is active.
    /// Emitted as shadowenv `env/prepend-to-pathlist` directives so they compose
    /// correctly with the user's existing PATH.
    fn path_prepends(&self, _dep: &Dependency, _project_root: &std::path::Path) -> Vec<String> {
        vec![]
    }

    /// The set of keys this module reads from `dep.extra`.
    ///
    /// `None` skips key checking (e.g. GenericModule accepts any key).
    /// `Some(&[])` warns on any extra key (the default — correct for modules with no config).
    /// `Some(&["port", ...])` declares a known-key allowlist.
    fn known_extra_keys(&self) -> Option<&'static [&'static str]> {
        Some(&[])
    }

    /// Called unconditionally after a dependency is installed or confirmed installed.
    /// Implementations should be idempotent.
    fn post_setup(
        &self,
        _dep: &Dependency,
        _pm: &dyn PackageManager,
        _project_root: &std::path::Path,
    ) -> Result<()> {
        Ok(())
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

    /// Returns informational warnings about the dependency's configuration.
    /// Called by `devy check` before any installation to surface issues early.
    /// Warnings are printed but do not count as blocking issues.
    fn config_warnings(&self, _dep: &Dependency) -> Vec<String> {
        vec![]
    }
}

// ── Module statics ─────────────────────────────────────────────────────────────
// Service modules
static MYSQL: mysql::MysqlModule = mysql::MysqlModule;
static REDIS: redis::RedisModule = redis::RedisModule;
static POSTGRES: postgres::PostgresModule = postgres::PostgresModule;
static MONGODB: mongodb::MongodbModule = mongodb::MongodbModule;
static NGINX: nginx::NginxModule = nginx::NginxModule;
static KAFKA: kafka::KafkaModule = kafka::KafkaModule;
static RABBITMQ: rabbitmq::RabbitmqModule = rabbitmq::RabbitmqModule;
static MEMCACHED: memcached::MemcachedModule = memcached::MemcachedModule;
static ELASTICSEARCH: elasticsearch::ElasticsearchModule = elasticsearch::ElasticsearchModule;
static OPENSEARCH: opensearch::OpenSearchModule = opensearch::OpenSearchModule;
static MEILISEARCH: meilisearch::MeilisearchModule = meilisearch::MeilisearchModule;
static MINIO: minio::MinioModule = minio::MinioModule;
static MAILHOG: mailhog::MailhogModule = mailhog::MailhogModule;
static MARIADB: mariadb::MariadbModule = mariadb::MariadbModule;
static VAULT: vault::VaultModule = vault::VaultModule;
// Language / runtime modules
static RUST: rust::RustModule = rust::RustModule;
static NODE: node::NodeModule = node::NodeModule;
static TYPESCRIPT: typescript::TypeScriptModule = typescript::TypeScriptModule;
static RUBY: ruby::RubyModule = ruby::RubyModule;
static PYTHON: python::PythonModule = python::PythonModule;
static JAVA: java::JavaModule = java::JavaModule;
static KOTLIN: kotlin::KotlinModule = kotlin::KotlinModule;
static ELIXIR: elixir::ElixirModule = elixir::ElixirModule;
static ERLANG: erlang::ErlangModule = erlang::ErlangModule;
static DENO: deno::DenoModule = deno::DenoModule;
static BUN: bun::BunModule = bun::BunModule;
static DOTNET: dotnet::DotnetModule = dotnet::DotnetModule;
static DART: dart::DartModule = dart::DartModule;
static ZIG: zig::ZigModule = zig::ZigModule;
static CRYSTAL: crystal::CrystalModule = crystal::CrystalModule;
static GCLOUD: gcloud::GcloudModule = gcloud::GcloudModule;
static GENERIC: generic::GenericModule = generic::GenericModule;
// Package modules (per-PM name tables)
static GO: PackageModule = PackageModule {
    default: "go",
    apt: "golang-go",
    winget: "GoLang.Go",
};
static SCALA: PackageModule = PackageModule {
    default: "scala",
    apt: "scala",
    winget: "EPFL.Scala",
};
static PHP: PackageModule = PackageModule {
    default: "php",
    apt: "php",
    winget: "PHP.PHP",
};
static AWSCLI: PackageModule = PackageModule {
    default: "awscli",
    apt: "awscli",
    winget: "Amazon.AWSCLI",
};
static GH: PackageModule = PackageModule {
    default: "gh",
    apt: "gh",
    winget: "GitHub.cli",
};
static KUBECTL: PackageModule = PackageModule {
    default: "kubectl",
    apt: "kubectl",
    winget: "Kubernetes.kubectl",
};
static HELM: PackageModule = PackageModule {
    default: "helm",
    apt: "helm",
    winget: "Helm.Helm",
};
static TERRAFORM: PackageModule = PackageModule {
    default: "terraform",
    apt: "terraform",
    winget: "Hashicorp.Terraform",
};
static AZURE_CLI: PackageModule = PackageModule {
    default: "azure-cli",
    apt: "azure-cli",
    winget: "Microsoft.AzureCLI",
};
static SWIFT: PackageModule = PackageModule {
    default: "swift",
    apt: "swift",
    winget: "Swift.Toolchain",
};

/// Canonical-name → module registry. One entry per canonical name.
/// Aliases (e.g. "postgres" → "postgresql") live in ALIASES below.
/// Add a module here; adding it anywhere else is not required.
pub(crate) static REGISTRY: &[(&str, &dyn Module)] = &[
    // Services
    ("mysql", &MYSQL),
    ("redis", &REDIS),
    ("postgresql", &POSTGRES),
    ("mongodb", &MONGODB),
    ("nginx", &NGINX),
    ("kafka", &KAFKA),
    ("rabbitmq", &RABBITMQ),
    ("memcached", &MEMCACHED),
    ("elasticsearch", &ELASTICSEARCH),
    ("opensearch", &OPENSEARCH),
    ("meilisearch", &MEILISEARCH),
    ("minio", &MINIO),
    ("mailhog", &MAILHOG),
    ("mariadb", &MARIADB),
    ("vault", &VAULT),
    // Languages / runtimes
    ("rust", &RUST),
    ("node", &NODE),
    ("typescript", &TYPESCRIPT),
    ("ruby", &RUBY),
    ("python", &PYTHON),
    ("go", &GO),
    ("java", &JAVA),
    ("kotlin", &KOTLIN),
    ("scala", &SCALA),
    ("php", &PHP),
    ("elixir", &ELIXIR),
    ("erlang", &ERLANG),
    ("deno", &DENO),
    ("bun", &BUN),
    ("dotnet", &DOTNET),
    ("dart", &DART),
    ("zig", &ZIG),
    ("crystal", &CRYSTAL),
    // CLI / infrastructure tools
    ("awscli", &AWSCLI),
    ("gh", &GH),
    ("gcloud", &GCLOUD),
    ("kubectl", &KUBECTL),
    ("helm", &HELM),
    ("terraform", &TERRAFORM),
    ("azure-cli", &AZURE_CLI),
    ("swift", &SWIFT),
];

/// Alias → canonical name. The canonical name must exist in REGISTRY.
static ALIASES: &[(&str, &str)] = &[
    // Service aliases
    ("postgres", "postgresql"),
    ("mongo", "mongodb"),
    ("elastic", "elasticsearch"),
    ("meili", "meilisearch"),
    ("hashicorp-vault", "vault"),
    // Language / runtime aliases
    ("rustup", "rust"),
    ("nodejs", "node"),
    ("javascript", "node"),
    ("js", "node"),
    ("ts", "typescript"),
    ("python3", "python"),
    ("golang", "go"),
    ("openjdk", "java"),
    ("dotnet-sdk", "dotnet"),
    ("csharp", "dotnet"),
    // CLI aliases
    ("aws", "awscli"),
    ("aws-cli", "awscli"),
    ("github-cli", "gh"),
    ("google-cloud-sdk", "gcloud"),
    ("kubernetes-cli", "kubectl"),
    ("az", "azure-cli"),
];

/// Resolves a dependency name to its module, falling back to a generic install.
static REGISTRY_MAP: std::sync::LazyLock<HashMap<&'static str, &'static dyn Module>> =
    std::sync::LazyLock::new(|| REGISTRY.iter().copied().collect());

static ALIASES_MAP: std::sync::LazyLock<HashMap<&'static str, &'static str>> =
    std::sync::LazyLock::new(|| ALIASES.iter().map(|&(a, t)| (a, t)).collect());

pub fn get(name: &str) -> &'static dyn Module {
    let canonical = ALIASES_MAP.get(name).copied().unwrap_or(name);
    REGISTRY_MAP.get(canonical).copied().unwrap_or(&GENERIC)
}

/// Returns the canonical registry name for `name`, resolving aliases.
/// `"postgres"` → `"postgresql"`, `"js"` → `"node"`, unknown → unchanged.
pub fn canonical_name(name: &str) -> &str {
    ALIASES_MAP.get(name).copied().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ── registry integrity ────────────────────────────────────────────────────

    #[test]
    fn registry_names_are_unique() {
        let mut names: Vec<&str> = REGISTRY.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        for w in names.windows(2) {
            assert_ne!(w[0], w[1], "REGISTRY contains duplicate name '{}'", w[0]);
        }
    }

    #[test]
    fn alias_targets_exist_in_registry() {
        let canonical_names: std::collections::HashSet<&str> =
            REGISTRY.iter().map(|(n, _)| *n).collect();
        for (alias, canon) in ALIASES {
            assert!(
                canonical_names.contains(canon),
                "ALIASES: '{}' → '{}' but '{}' is not in REGISTRY",
                alias,
                canon,
                canon
            );
        }
    }

    #[test]
    fn alias_names_do_not_shadow_registry_names() {
        let canonical_names: std::collections::HashSet<&str> =
            REGISTRY.iter().map(|(n, _)| *n).collect();
        for (alias, _) in ALIASES {
            assert!(
                !canonical_names.contains(alias),
                "ALIASES entry '{}' shadows a REGISTRY canonical name",
                alias
            );
        }
    }

    // ── extra_port ────────────────────────────────────────────────────────────

    #[test]
    fn extra_port_returns_default_when_key_absent() {
        let dep = Dependency::simple("redis");
        assert_eq!(extra_port(&dep, "port", 6379).unwrap(), 6379);
    }

    #[test]
    fn extra_port_returns_custom_value() {
        let mut extra = HashMap::new();
        extra.insert(
            "port".into(),
            crate::config::ExtraValue::Number(6380u64.into()),
        );
        let dep = Dependency::with_extra("redis", extra);
        assert_eq!(extra_port(&dep, "port", 6379).unwrap(), 6380);
    }

    #[test]
    fn extra_port_bails_on_overflow() {
        let mut extra = HashMap::new();
        extra.insert(
            "port".into(),
            crate::config::ExtraValue::Number(99999u64.into()),
        );
        let dep = Dependency::with_extra("redis", extra);
        assert!(extra_port(&dep, "port", 6379).is_err());
    }

    #[test]
    fn extra_port_bails_on_zero() {
        let mut extra = HashMap::new();
        extra.insert(
            "port".into(),
            crate::config::ExtraValue::Number(0u64.into()),
        );
        let dep = Dependency::with_extra("redis", extra);
        let err = extra_port(&dep, "port", 6379).unwrap_err();
        assert!(
            err.to_string().contains("out of range"),
            "error must say 'out of range' for port 0"
        );
    }

    #[test]
    fn extra_port_uses_provided_key_name() {
        let mut extra = HashMap::new();
        extra.insert(
            "smtp_port".into(),
            crate::config::ExtraValue::Number(1025u64.into()),
        );
        let dep = Dependency::with_extra("mailhog", extra);
        assert_eq!(extra_port(&dep, "smtp_port", 1025).unwrap(), 1025);
        assert_eq!(extra_port(&dep, "port", 80).unwrap(), 80); // different key → default
    }

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
        assert_eq!(get("deno").source(), Some("deno-installer"));
    }

    #[test]
    fn get_bun_source_is_bun_installer() {
        assert_eq!(get("bun").source(), Some("bun-installer"));
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
        assert_eq!(get("gcloud").source(), Some("gcloud-installer"));
        assert_eq!(get("google-cloud-sdk").source(), Some("gcloud-installer"));
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
        assert_eq!(get("rust").source(), Some("rustup"));
        assert_eq!(get("rustup").source(), Some("rustup"));
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
        assert_eq!(m.source(), None);
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
        // RubyModule.source() returns Some("rbenv"), which GenericModule does not.
        assert_eq!(get("ruby").source(), Some("rbenv"));
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
            crate::config::ExtraValue::Sequence(vec![
                crate::config::ExtraValue::String("typescript".into()),
                crate::config::ExtraValue::String("eslint".into()),
            ]),
        );
        let dep = Dependency {
            name: "node".into(),
            version: None,
            tap: None,
            after_install: None,
            shell: None,
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
            crate::config::ExtraValue::String("ts".into()),
        );
        let dep = Dependency {
            name: "node".into(),
            version: None,
            tap: None,
            after_install: None,
            shell: None,
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
            after_install: None,
            shell: None,
            extra: HashMap::new(),
        };
        let remapped = pm_dep(&dep, "ruby@3.2");
        assert_eq!(remapped.name, "ruby@3.2");
        assert_eq!(remapped.version, Some("3.2".into()));
        assert_eq!(remapped.tap, Some("homebrew/core".into()));
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
    fn default_is_running_returns_false() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("test");
        assert!(!DefaultModule.is_running(&pm, &dep).unwrap());
    }

    #[test]
    fn default_env_vars_returns_empty_map() {
        let dep = Dependency::simple("test");
        assert!(
            DefaultModule
                .env_vars(&dep, std::path::Path::new("/tmp"))
                .is_empty()
        );
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
        fn service_config(&self) -> ServiceConfig {
            ServiceConfig {
                health_check_sleep_ms: 0,
                ..Default::default()
            }
        }
    }

    #[test]
    fn wait_for_ready_succeeds_immediately_when_healthy() {
        let dep = Dependency::simple("testservice");
        HealthyModule.wait_for_ready(&dep).unwrap();
    }

    struct ZeroAttemptsModule;
    impl Module for ZeroAttemptsModule {
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
        fn service_config(&self) -> ServiceConfig {
            ServiceConfig {
                health_check_max_attempts: 0,
                ..Default::default()
            }
        }
    }

    #[test]
    fn wait_for_ready_returns_err_when_max_attempts_is_zero() {
        let dep = Dependency::simple("zeroservice");
        let result = ZeroAttemptsModule.wait_for_ready(&dep);
        assert!(
            result.is_err(),
            "must return Err (not panic) when max_attempts is 0"
        );
        assert!(result.unwrap_err().to_string().contains("zeroservice"));
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
        fn service_config(&self) -> ServiceConfig {
            ServiceConfig {
                health_check_sleep_ms: 0,
                ..Default::default()
            }
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
        calls: std::sync::atomic::AtomicU32,
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
            let n = self
                .calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < self.fail_for {
                anyhow::bail!("not ready yet")
            } else {
                Ok(())
            }
        }
        fn service_config(&self) -> ServiceConfig {
            ServiceConfig {
                health_check_sleep_ms: 0,
                ..Default::default()
            }
        }
    }

    #[test]
    fn wait_for_ready_retries_until_healthy() {
        let m = EventuallyHealthyModule {
            calls: std::sync::atomic::AtomicU32::new(0),
            fail_for: 2,
        };
        let dep = Dependency::simple("eventually");
        m.wait_for_ready(&dep).unwrap();
        assert!(
            m.calls.load(std::sync::atomic::Ordering::Relaxed) >= 3,
            "Expected at least 3 health_check calls"
        );
    }

    // ── wait_for_stopped ─────────────────────────────────────────────────────

    struct AlreadyStoppedModule;
    impl Module for AlreadyStoppedModule {
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
        fn is_running(
            &self,
            _pm: &dyn crate::package_manager::PackageManager,
            _dep: &Dependency,
        ) -> Result<bool> {
            Ok(false)
        }
        fn service_config(&self) -> ServiceConfig {
            ServiceConfig {
                shutdown_sleep_ms: 0,
                ..Default::default()
            }
        }
    }

    #[test]
    fn wait_for_stopped_returns_ok_when_already_stopped() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("stoppedservice");
        AlreadyStoppedModule.wait_for_stopped(&pm, &dep).unwrap();
    }

    struct NeverStopsModule;
    impl Module for NeverStopsModule {
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
        fn is_running(
            &self,
            _pm: &dyn crate::package_manager::PackageManager,
            _dep: &Dependency,
        ) -> Result<bool> {
            Ok(true)
        }
        fn service_config(&self) -> ServiceConfig {
            ServiceConfig {
                shutdown_sleep_ms: 0,
                ..Default::default()
            }
        }
    }

    #[test]
    fn wait_for_stopped_fails_after_max_attempts() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("stubborn");
        let err = NeverStopsModule.wait_for_stopped(&pm, &dep).unwrap_err();
        assert!(
            err.to_string().contains("stubborn"),
            "error must name the service"
        );
        assert!(
            err.to_string().contains("10 attempts"),
            "error must mention attempt count"
        );
    }

    struct EventuallyStopsModule {
        calls: std::sync::atomic::AtomicU32,
        stop_after: u32,
    }
    impl Module for EventuallyStopsModule {
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
        fn is_running(
            &self,
            _pm: &dyn crate::package_manager::PackageManager,
            _dep: &Dependency,
        ) -> Result<bool> {
            let n = self
                .calls
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(n < self.stop_after)
        }
        fn service_config(&self) -> ServiceConfig {
            ServiceConfig {
                shutdown_sleep_ms: 0,
                ..Default::default()
            }
        }
    }

    #[test]
    fn wait_for_stopped_retries_until_stopped() {
        let pm = crate::package_manager::MockPackageManager::default();
        let m = EventuallyStopsModule {
            calls: std::sync::atomic::AtomicU32::new(0),
            stop_after: 2,
        };
        let dep = Dependency::simple("slowstopper");
        m.wait_for_stopped(&pm, &dep).unwrap();
        assert!(m.calls.load(std::sync::atomic::Ordering::Relaxed) >= 3);
    }

    struct ZeroShutdownModule;
    impl Module for ZeroShutdownModule {
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
        fn service_config(&self) -> ServiceConfig {
            ServiceConfig {
                shutdown_max_attempts: 0,
                ..Default::default()
            }
        }
    }

    #[test]
    fn wait_for_stopped_returns_err_when_max_attempts_is_zero() {
        let pm = crate::package_manager::MockPackageManager::default();
        let dep = Dependency::simple("zeroservice");
        let result = ZeroShutdownModule.wait_for_stopped(&pm, &dep);
        assert!(
            result.is_err(),
            "must return Err (not silent bail) when shutdown_max_attempts is 0"
        );
        assert!(result.unwrap_err().to_string().contains("zeroservice"));
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
        let dir = crate::test_support::tmp_dir();
        write_mysql_config(&dir, port, cli_args).unwrap();
        std::fs::read_to_string(dir.join("my.cnf")).unwrap()
        // dir is dropped here, cleaning up automatically
    }

    /// Like `write_and_read` but also returns the number of warnings emitted.
    fn write_and_read_with_warnings(port: u16, cli_args: Option<&str>) -> (String, usize) {
        let mut content = String::new();
        let warn_count = crate::output::with_warn_capture(|| {
            content = write_and_read(port, cli_args);
        });
        (content, warn_count)
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
        let (content, warns) = write_and_read_with_warnings(3306, Some("skip_grant_tables=1"));
        assert!(!content.contains("skip_grant_tables"));
        assert!(warns > 0, "must warn when -- prefix is missing");
    }

    #[test]
    fn write_mysql_config_rejects_key_with_special_chars() {
        let (content, warns) = write_and_read_with_warnings(3306, Some("--bad;key=val"));
        assert!(!content.contains("bad;key"));
        assert!(warns > 0, "must warn when key contains unsafe characters");
    }

    #[test]
    fn write_mysql_config_skips_args_without_equals() {
        let (content, warns) = write_and_read_with_warnings(3306, Some("--no-value-here"));
        assert!(!content.contains("no-value-here"));
        assert!(warns > 0, "must warn when no = is present in arg");
    }

    #[test]
    fn write_mysql_config_treats_newline_in_args_as_separator() {
        // \n is whitespace — split_whitespace splits the arg list on it.
        // "--key=value" is valid and written; "line2" has no -- prefix and warns.
        let (content, warns) = write_and_read_with_warnings(3306, Some("--key=value\nline2"));
        assert!(
            content.contains("key = value"),
            "Expected valid arg to be written"
        );
        assert!(
            !content.contains("line2"),
            "Bare token after newline must be skipped"
        );
        assert!(warns > 0, "must warn for the bare 'line2' token");
    }

    #[test]
    fn write_mysql_config_rejects_value_with_null_byte() {
        let (content, warns) = write_and_read_with_warnings(3306, Some("--key=val\x00ue"));
        assert!(!content.contains("val"));
        assert!(warns > 0, "must warn when value contains null byte");
    }

    #[test]
    fn write_mysql_config_carriage_return_splits_token() {
        // \r is ASCII whitespace; split_whitespace splits "--key=val\rue" into
        // "--key=val" (accepted) and "ue" (no -- prefix, dropped with a warning).
        let (content, warns) = write_and_read_with_warnings(3306, Some("--key=val\rue"));
        assert!(
            content.contains("key = val"),
            "portion before \\r is accepted"
        );
        assert!(!content.contains("ue"), "portion after \\r is dropped");
        assert!(warns > 0, "must warn for the bare 'ue' token");
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
