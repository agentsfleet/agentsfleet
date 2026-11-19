//! Runtime policy is resolved once before opening any service.

use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use af_core::{
    env::EnvSource,
    timing::{LeaseClock, LeaseSettings},
};
use garde::Validate;

use super::{Fault, Refusal};

/// Deployment budgets passed to the stores and scheduler.
#[derive(Debug, Clone)]
pub struct RuntimeSettings {
    pub(crate) operations: Arc<OperationalSettings>,
    pub(crate) clock: LeaseClock,
    pub(crate) resume_from_checkpoint: bool,
    pub(crate) fallback: Duration,
    pub(crate) claims: NonZeroUsize,
    pub(crate) requests: NonZeroUsize,
}

#[derive(Debug)]
pub(crate) struct OperationalSettings {
    pub(crate) outbound_retries: afd_outbound::retry::RetryPolicy,
    pub(crate) inference: af_inference::Timeouts,
    pub(crate) telemetry: garde::Valid<af_otlp::PipelineSettings>,
    pub(crate) maintenance: crate::config::maintenance::MaintenancePolicy,
    pub(crate) tasks: afd_fleet::task::policy::TaskPolicy,
    pub(crate) authoring: afd_authoring::turn_policy::TurnPolicy,
}

impl RuntimeSettings {
    pub(crate) fn announce(&self) {
        let (lease_ttl_seconds, resume_from_checkpoint, offer_fallback_seconds) = (
            self.clock.ttl().as_secs(),
            self.resume_from_checkpoint,
            self.fallback.as_secs(),
        );
        let (max_concurrent_claims, max_in_flight_requests) =
            (self.claims.get(), self.requests.get());
        tracing::info!(
            event = "daemon_configuration_loaded",
            lease_ttl_seconds,
            resume_from_checkpoint,
            offer_fallback_seconds,
            max_concurrent_claims,
            max_in_flight_requests
        );
    }
}

#[derive(Validate)]
struct Readings {
    #[garde(skip)]
    clock: LeaseClock,
    #[garde(custom(|value: &NonZeroUsize, &()| maximum::<64>(value.get())))]
    claims: NonZeroUsize,
    #[garde(custom(|value: &NonZeroUsize, &()| maximum::<65_536>(value.get())))]
    requests: NonZeroUsize,
    #[garde(range(min = 1, max = self.clock.ttl().as_secs().saturating_sub(1)))]
    fallback_seconds: u64,
}

#[cfg(test)]
pub(crate) fn read(env: &(impl EnvSource + ?Sized)) -> Result<RuntimeSettings, Refusal> {
    let operations = OperationalSettings {
        tasks: afd_fleet::task::policy::TaskPolicy::default(),
        authoring: afd_authoring::turn_policy::TurnPolicy::default(),
        inference: af_inference::Timeouts::DEFAULT,
        outbound_retries: afd_outbound::retry::RetryPolicy::DEFAULT,
        maintenance: crate::config::maintenance::MaintenancePolicy::resolve(
            crate::config::maintenance::Maintenance::default(),
        )
        .map_err(|report| refused("maintenance", report.to_string()))?,
        telemetry: garde::Unvalidated::new(af_otlp::PipelineSettings::default())
            .validate()
            .map_err(|report| refused("telemetry.otlp", report.to_string()))?,
    };
    configured(env, operations, LeaseSettings::default())
}

pub(crate) fn configured(
    env: &(impl EnvSource + ?Sized),
    operations: OperationalSettings,
    mut lease: LeaseSettings,
) -> Result<RuntimeSettings, Refusal> {
    lease.ttl_seconds = value(env, "AGENTSFLEET_LEASE_TTL_SECONDS", lease.ttl_seconds)?;
    let lease = garde::Unvalidated::new(lease)
        .validate()
        .map_err(|report| refused("runner.lease", report.to_string()))?;
    let defaults = afd_gateway::RecoverySettings::default();
    let readings = garde::Unvalidated::new(Readings {
        clock: LeaseClock::new(&lease),
        claims: value(
            env,
            "AGENTSFLEET_RECOVERY_CONCURRENT_CLAIMS",
            defaults.max_concurrent_claims,
        )?,
        requests: value(
            env,
            "AGENTSFLEET_MAX_IN_FLIGHT_REQUESTS",
            afd_api::DEFAULT_MAX_IN_FLIGHT,
        )?,
        fallback_seconds: value(
            env,
            "AGENTSFLEET_OFFER_FALLBACK_SECONDS",
            defaults.offer_fallback.as_secs(),
        )?,
    })
    .validate()
    .map_err(|report| refused("runtime", report.to_string()))?;
    Ok(RuntimeSettings {
        operations: Arc::new(operations),
        clock: readings.clock,
        resume_from_checkpoint: value(env, "AGENTSFLEET_RESUME_FROM_CHECKPOINT", false)?,
        fallback: Duration::from_secs(readings.fallback_seconds),
        claims: readings.claims,
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
