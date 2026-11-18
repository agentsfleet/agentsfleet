//! Deployment files feed the same validators as environment-based startup.

pub(crate) mod maintenance;
mod origins;
mod settings;
mod timing;
mod urls;
mod values;

#[cfg(test)]
mod maintenance_tests;
#[cfg(test)]
mod origin_tests;
#[cfg(test)]
mod task_tests;
#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, path::Path};

use af_core::{
    env::{EnvSource, optional},
    error_code::Coded as _,
};

use crate::error::{Fault, Refusal};
use settings::ConfigToml;

/// Optional application settings delivered alongside the daemon image.
pub const FILE_KNOB: &str = "AGENTSFLEET_CONFIG_FILE";

/// Explicit invocation settings have the highest precedence.
#[derive(Debug, Default, Clone, Copy)]
pub struct ConfigOverrides {
    /// Explicit HTTP port, as parsed by the command-line parser.
    pub port: Option<u16>,
}

/// Owns file selection and invocation precedence before typed boot validation.
pub struct ConfigBuilder<'a, E: ?Sized> {
    environment: &'a E,
    file: Option<&'a Path>,
    overrides: ConfigOverrides,
}

impl<E: ?Sized> std::fmt::Debug for ConfigBuilder<'_, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigBuilder")
            .field("file", &self.file)
            .finish_non_exhaustive()
    }
}

impl<'a, E: EnvSource + ?Sized> ConfigBuilder<'a, E> {
    /// Receives environment values through the same seam used by startup tests.
    #[must_use]
    pub fn new(environment: &'a E) -> Self {
        Self {
            environment,
            file: None,
            overrides: ConfigOverrides::default(),
        }
    }

    /// Selects the invocation's application file before consulting the environment.
    #[must_use]
    pub fn file(mut self, file: Option<&'a Path>) -> Self {
        self.file = file;
        self
    }

    /// Applies explicit invocation values above the file's settings.
    #[must_use]
    pub fn overrides(mut self, overrides: ConfigOverrides) -> Self {
        self.overrides = overrides;
        self
    }

    /// Resolves a single file into inputs for the existing typed boot validators.
    ///
    /// # Errors
    /// Names an unreadable or invalid file without rendering its contents.
    pub fn build(self) -> Result<Config<'a, E>, Refusal> {
        let selected = optional(self.environment, FILE_KNOB);
        let file = self.file.or_else(|| selected.as_deref().map(Path::new));
        let mut loader = af_config::ConfigBuilder::default().file(file);
        if let Some(port) = self.overrides.port {
            let server = toml::Table::from_iter([(
                "port".to_owned(),
                toml::Value::Integer(i64::from(port)),
            )]);
            loader = loader.overlay(toml::Value::Table(toml::Table::from_iter([(
                "server".to_owned(),
                toml::Value::Table(server),
            )])));
        }
        let settings: garde::Valid<ConfigToml> = loader
            .build()
            .map_err(|failure| invalid(failure.to_string()))?;
        Config::resolve(self.environment, &settings, self.overrides)
    }
}

/// Public settings are resolved before commands open their runtime or sockets.
pub struct Config<'a, E: ?Sized> {
    origins: origins::Origins,
    source: Source<'a, E>,
    runtime: crate::preflight::RuntimeSettings,
    api_pool: garde::Valid<afd_db::config::PoolSettings>,
    migrator_pool: garde::Valid<afd_db::config::PoolSettings>,
    migration_retry: afd_db::migrate::RetryPolicy,
}

impl<E: ?Sized> std::fmt::Debug for Config<'_, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl<'a, E: EnvSource + ?Sized> Config<'a, E> {
    fn resolve(
        environment: &'a E,
        settings: &garde::Valid<ConfigToml>,
        overrides: ConfigOverrides,
    ) -> Result<Self, Refusal> {
        let source = Source {
            environment,
            values: values::collect(settings),
        };
        let api_pool = afd_db::config::PoolSettings::resolve(
            environment,
            afd_db::config::DbRole::Api,
            settings.database.api,
        )
        .map_err(|error| budget_invalid("database.api", &error))?;
        let migrator_pool = afd_db::config::PoolSettings::resolve(
            environment,
            afd_db::config::DbRole::Migrator,
            settings.database.migrator,
        )
        .map_err(|error| budget_invalid("database.migrator", &error))?;
        let task_settings = garde::Unvalidated::new(settings.fleet.tasks)
            .validate()
            .map_err(|report| invalid(format!("fleet.tasks: {report}")))?;
        let maintenance = maintenance::MaintenancePolicy::resolve(settings.maintenance)
            .map_err(|report| invalid(format!("maintenance: {report}")))?;
        let runtime = crate::preflight::runtime::configured(
            &source,
            afd_fleet::task::policy::TaskPolicy::new(&task_settings),
            maintenance,
            garde::Unvalidated::new(settings.telemetry.otlp)
                .validate()
                .map_err(|report| invalid(format!("telemetry.otlp: {report}")))?,
            timing::settings(&settings.runner),
        )?;
        Ok(Config {
            origins: origins::Origins::collect(environment, &source.values, settings, overrides),
            runtime,
            migration_retry: afd_db::migrate::RetryPolicy::new(
                settings.database.migration.lock_max_attempts,
                std::time::Duration::from_millis(
                    settings.database.migration.lock_retry_interval_ms,
                ),
            ),
            api_pool,
            migrator_pool,
            source,
        })
    }

    pub(crate) fn environment(&self) -> &impl EnvSource {
        &self.source
    }

    pub(crate) fn announce(&self) {
        self.origins.announce();
    }

    /// Resolves required serving secrets without rereading the selected file.
    ///
    /// # Errors
    /// Names each absent or unusable serving input.
    pub fn preflight(&self) -> Result<crate::preflight::BootConfig, Refusal> {
        crate::preflight::resolved(&self.source, &self.api_pool, self.runtime.clone())
    }

    pub(crate) async fn migrate(&self) -> Result<afd_db::Applied, crate::error::MigrateFailure>
    where
        E: Sync,
    {
        crate::migrate::configured(&self.source, &self.migrator_pool, self.migration_retry).await
    }
}

fn budget_invalid(knob: &'static str, error: &afd_db::Error) -> Refusal {
    Refusal::new(vec![Fault::Invalid {
        knob,
        why: error.logged().reason,
    }])
}

/// A boot-scoped file overlay over the caller's injectable environment.
struct Source<'a, E: ?Sized> {
    environment: &'a E,
    values: BTreeMap<&'static str, String>,
}

impl<E: ?Sized> std::fmt::Debug for Source<'_, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Source")
            .field("keys", &self.values.keys())
            .finish_non_exhaustive()
    }
}

impl<E: EnvSource + ?Sized> EnvSource for Source<'_, E> {
    fn get(&self, key: &str) -> Option<String> {
        // File selection belongs to the builder; service readers receive resolved values.
        if key == FILE_KNOB {
            return None;
        }
        self.values
            .get(key)
            .cloned()
            .or_else(|| self.environment.get(key))
    }
}

fn invalid(why: String) -> Refusal {
    Refusal::new(vec![Fault::Invalid {
        knob: FILE_KNOB,
        why,
    }])
}
