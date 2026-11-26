//! Runner-owned profile types over the application-independent TOML loader.

use std::{path::PathBuf, process::ExitCode};

use af_core::env::EnvSource;
use garde::Validate;
use serde::Deserialize;

#[cfg(feature = "firecracker")]
mod firecracker;
mod tuning;

use self::tuning::{
    DaemonTuning, EgressTuning, ExecutorTuning, ProviderTuning, SandboxTuning, TelemetryTuning,
    ToolsTuning, carries_a_read,
};

/// Optional numeric deployment profile; tokens and bootstrap addresses stay in the environment.
pub(super) const ENV_CONFIG_FILE: &str = "AGENTSFLEET_RUNNER_CONFIG_FILE";

#[derive(Default, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
struct Profile {
    #[garde(dive)]
    inference: af_providers::settings::TimeoutSettings,
    #[garde(dive)]
    provider_retries: af_providers::settings::RetrySettings,
    #[garde(dive)]
    supervisor: afr_supervisor::config::RuntimeSettings,
    #[garde(dive)]
    sandbox: SandboxTuning,
    #[garde(dive)]
    tools: ToolsTuning,
    #[garde(dive, custom(carries_a_read(self.tools.max_file_read_mib)))]
    executor: ExecutorTuning,
    #[garde(dive)]
    provider: ProviderTuning,
    #[garde(dive)]
    egress: EgressTuning,
    #[garde(dive)]
    daemon: DaemonTuning,
    #[garde(dive)]
    telemetry: TelemetryTuning,
    #[cfg(feature = "firecracker")]
    #[garde(dive)]
    firecracker: firecracker::FirecrackerSection,
}

pub(super) struct Settings {
    pub(super) inference: af_providers::settings::Timeouts,
    pub(super) provider_retries: af_providers::settings::RetryPolicy,
    pub(super) supervisor: afr_supervisor::config::RuntimePolicy,
    /// The supervisor's daemon link, decision calls and sandbox limits.
    pub(super) tuning: afr_supervisor::config::Tuning,
    /// How the engine starts and paces every sandbox.
    pub(super) engine: afr_engine_bubblewrap::EngineTuning,
    /// What our Firecracker engine boots every microVM with.
    #[cfg(feature = "firecracker")]
    pub(super) firecracker: afr_engine_firecracker::FirecrackerTuning,
    /// The microVMs our Firecracker engine boots ahead of their runs.
    #[cfg(feature = "firecracker")]
    pub(super) warm_pool: usize,
    /// What the agent loop holds every run to.
    pub(super) run_limits: afr_agent::RunLimits,
    /// The bounds every egress request is held to.
    pub(super) egress: af_egress::Limits,
    /// Spans one run may export.
    pub(super) max_run_spans: u32,
}

pub(super) fn read(env: &impl EnvSource) -> Result<Settings, ExitCode> {
    let profile = load(env).map_err(|error| super::refused(&error))?;
    let inference = af_providers::settings::Timeouts::new(&checked(profile.inference)?);
    let sizes = tuning::sizes(&profile.tools, profile.executor);
    let engine = profile.sandbox.engine(sizes);
    Ok(Settings {
        inference: profile.provider.counting(inference),
        provider_retries: af_providers::settings::RetryPolicy::new(&checked(
            profile.provider_retries,
        )?),
        supervisor: afr_supervisor::config::RuntimePolicy::new(&checked(profile.supervisor)?),
        tuning: tuning::supervisor(&profile.sandbox, &profile.provider, &profile.daemon),
        engine,
        #[cfg(feature = "firecracker")]
        firecracker: profile.firecracker.tuning(engine, profile.sandbox.limits()),
        #[cfg(feature = "firecracker")]
        warm_pool: profile.firecracker.warm_pool(),
        run_limits: profile.tools.run_limits(sizes),
        egress: profile.egress.limits(),
        max_run_spans: profile.telemetry.max_spans_per_run,
    })
}

fn load(env: &impl EnvSource) -> af_config::Result<garde::Valid<Profile>> {
    // Blank means unset, as for the daemon's AGENTSFLEET_CONFIG_FILE.
    let file = af_core::env::optional(env, ENV_CONFIG_FILE).map(|path| PathBuf::from(&*path));
    af_config::ConfigBuilder::default()
        .file(file.as_deref())
        .build::<Profile>()
}

fn checked<T: Validate<Context = ()>>(raw: T) -> Result<garde::Valid<T>, ExitCode> {
    garde::Unvalidated::new(raw)
        .validate()
        .map_err(|report| super::refused(&report))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tuning_tests;
