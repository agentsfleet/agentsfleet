//! Report precedence by input name without logging configuration values.

use std::collections::BTreeMap;

use af_core::env::EnvSource;

use super::{ConfigOverrides, settings::ConfigToml};

#[derive(Debug)]
pub(super) struct Origins(Names);

type Names = Vec<(&'static str, &'static str)>;

impl Origins {
    pub(super) fn collect(
        environment: &(impl EnvSource + ?Sized),
        values: &BTreeMap<&'static str, String>,
        settings: &ConfigToml,
        overrides: ConfigOverrides,
    ) -> Self {
        let mut names = Vec::new();
        for key in values.keys() {
            if environment.get(key).is_some() {
                let source = if *key == "PORT" && overrides.port.is_some() {
                    "command_line"
                } else {
                    "configuration_file"
                };
                names.push((*key, source));
            }
        }
        pool(
            &mut names,
            environment,
            settings.database.api,
            [
                "DATABASE_POOL_SIZE_API",
                "DATABASE_MIN_POOL_SIZE_API",
                "DATABASE_ACQUIRE_TIMEOUT_MS_API",
            ],
        );
        pool(
            &mut names,
            environment,
            settings.database.migrator,
            [
                "DATABASE_POOL_SIZE_MIGRATOR",
                "DATABASE_MIN_POOL_SIZE_MIGRATOR",
                "DATABASE_ACQUIRE_TIMEOUT_MS_MIGRATOR",
            ],
        );
        if settings.telemetry.otlp.export_timeout_seconds.is_some()
            && environment.get("OTEL_EXPORTER_OTLP_TIMEOUT").is_some()
        {
            names.push(("OTEL_EXPORTER_OTLP_TIMEOUT", "configuration_file"));
        }
        names.sort_unstable();
        names.dedup();
        Self(names)
    }

    pub(super) fn announce(&self) {
        for &(knob, source) in &self.0 {
            tracing::warn!(
                event = "daemon_configuration_overrides_environment",
                knob,
                source
            );
        }
    }
}

fn pool(
    names: &mut Names,
    environment: &(impl EnvSource + ?Sized),
    settings: afd_db::config::PoolOverrides,
    scoped: [&'static str; 3],
) {
    let present = [
        settings.max_connections.is_some(),
        settings.min_connections.is_some(),
        settings.acquire_timeout_ms.is_some(),
    ];
    let bases = [
        "DATABASE_POOL_SIZE",
        "DATABASE_MIN_POOL_SIZE",
        "DATABASE_ACQUIRE_TIMEOUT_MS",
    ];
    for ((present, scoped), base) in present.into_iter().zip(scoped).zip(bases) {
        if present {
            for knob in [scoped, base] {
                if environment.get(knob).is_some() {
                    names.push((knob, "configuration_file"));
                }
            }
        }
    }
}
