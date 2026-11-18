//! Deployment files feed the same validators as environment-based startup.

mod settings;
mod values;

#[cfg(test)]
mod tests;

use std::{collections::BTreeMap, path::Path};

use af_core::env::{EnvSource, optional};

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
    pub fn build(self) -> Result<Source<'a, E>, Refusal> {
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
        Ok(Source {
            environment: self.environment,
            values: values::collect(&settings),
        })
    }
}

/// A boot-scoped file overlay over the caller's injectable environment.
pub struct Source<'a, E: ?Sized> {
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
        // Downstream preflight may receive this resolved source and must not reopen the file.
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
