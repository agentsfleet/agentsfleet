//! Runtime policy is resolved once before opening any service.

use std::{num::NonZeroUsize, sync::Arc};

use af_core::{
    env::EnvSource,
    timing::{RunClock, RunSettings},
};
use garde::Validate;

use super::{Fault, Refusal};

/// Deployment budgets passed to the stores and scheduler.
#[derive(Debug, Clone)]
pub struct RuntimeSettings {
    pub(crate) operations: Arc<OperationalSettings>,
    pub(crate) clock: RunClock,
    pub(crate) resume_from_checkpoint: bool,
    pub(crate) assignments: NonZeroUsize,
    pub(crate) requests: NonZeroUsize,
}

#[derive(Debug)]
pub(crate) struct OperationalSettings {
    pub(crate) outbound_retries: afd_outbound::retry::RetryPolicy,
    pub(crate) telemetry: garde::Valid<af_otlp::PipelineSettings>,
    pub(crate) tasks: afd_fleet::task::policy::TaskPolicy,
    /// Every checked numeric section of the deployment file.
    pub(crate) tuning: crate::config::tuning::Tuning,
    /// `[connectors.qstash]`, proved to end inside its call budget.
    pub(crate) scheduler_retries: afd_cron::qstash::RetryPolicy,
}

impl RuntimeSettings {
    pub(crate) fn announce(&self) {
        let (run_ttl_seconds, resume_from_checkpoint) =
            (self.clock.ttl().as_secs(), self.resume_from_checkpoint);
        let (max_concurrent_assignments, max_in_flight_requests) =
            (self.assignments.get(), self.requests.get());
        tracing::info!(
            event = "daemon_configuration_loaded",
            run_ttl_seconds,
            resume_from_checkpoint,
            max_concurrent_assignments,
            max_in_flight_requests
        );
    }
}

#[derive(Validate)]
struct Readings {
    #[garde(skip)]
    clock: RunClock,
    #[garde(custom(|value: &NonZeroUsize, &()| maximum::<64>(value.get())))]
    assignments: NonZeroUsize,
    #[garde(custom(|value: &NonZeroUsize, &()| maximum::<65_536>(value.get())))]
    requests: NonZeroUsize,
}

#[cfg(test)]
pub(crate) fn read(env: &(impl EnvSource + ?Sized)) -> Result<RuntimeSettings, Refusal> {
    let operations = operations()?;
    configured(env, operations, RunSettings::default())
}

/// The default operational policy.
#[cfg(test)]
pub(crate) fn operations() -> Result<OperationalSettings, Refusal> {
    Ok(OperationalSettings {
        tasks: afd_fleet::task::policy::TaskPolicy::default(),
        outbound_retries: afd_outbound::retry::RetryPolicy::DEFAULT,
        telemetry: garde::Unvalidated::new(af_otlp::PipelineSettings::default())
            .validate()
            .map_err(|report| refused("telemetry.otlp", report.to_string()))?,
        tuning: crate::config::tuning::Tuning::default(),
        scheduler_retries: afd_cron::qstash::RetryPolicy::default(),
    })
}

pub(crate) fn configured(
    env: &(impl EnvSource + ?Sized),
    operations: OperationalSettings,
    mut run: RunSettings,
) -> Result<RuntimeSettings, Refusal> {
    run.ttl_seconds = value(env, "AGENTSFLEET_RUN_TTL_SECONDS", run.ttl_seconds)?;
    let run = garde::Unvalidated::new(run)
        .validate()
        .map_err(|report| refused("runner.run", report.to_string()))?;
    let defaults = afd_gateway::RecoverySettings::default();
    let readings = garde::Unvalidated::new(Readings {
        clock: RunClock::new(&run),
        assignments: value(
            env,
            "AGENTSFLEET_RECOVERY_CONCURRENT_ASSIGNMENTS",
            defaults.max_concurrent_assignments,
        )?,
        requests: value(
            env,
            "AGENTSFLEET_MAX_IN_FLIGHT_REQUESTS",
            afd_http::in_flight::DEFAULT_MAX_IN_FLIGHT,
        )?,
    })
    .validate()
    .map_err(|report| refused("runtime", report.to_string()))?;
    Ok(RuntimeSettings {
        operations: Arc::new(operations),
        clock: readings.clock,
        resume_from_checkpoint: value(env, "AGENTSFLEET_RESUME_FROM_CHECKPOINT", false)?,
        assignments: readings.assignments,
        requests: readings.requests,
    })
}

fn maximum<const CAP: usize>(value: usize) -> garde::Result {
    garde::rules::range::apply(&value, (None, Some(CAP)))
}

fn value<T: std::str::FromStr>(
    env: &(impl EnvSource + ?Sized),
    knob: &'static str,
    fallback: T,
) -> Result<T, Refusal> {
    env.get(knob).map_or(Ok(fallback), |raw| {
        raw.parse()
            .map_err(|_invalid| refused(knob, "requires a valid configured value".to_owned()))
    })
}

fn refused(knob: &'static str, why: String) -> Refusal {
    Refusal::new(vec![Fault::Invalid { knob, why }])
}
