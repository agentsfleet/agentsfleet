//! Runtime policy is resolved once before opening any service.

use std::{num::NonZeroUsize, time::Duration};

use af_core::{
    env::EnvSource,
    timing::{LeaseClock, LeaseSettings},
};
use garde::Validate;

use super::{Fault, Refusal};

/// Deployment budgets passed to the stores and scheduler.
#[derive(Debug, Clone, Copy)]
pub struct RuntimeSettings {
    pub(crate) clock: LeaseClock,
    pub(crate) resume_from_checkpoint: bool,
    pub(crate) fallback: Duration,
    pub(crate) claims: NonZeroUsize,
    pub(crate) requests: NonZeroUsize,
}

impl RuntimeSettings {
    pub(crate) fn announce(self) {
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
    #[garde(dive)]
    lease: LeaseSettings,
    #[garde(range(min = 1, max = 64))]
    claims: usize,
    #[garde(range(min = 1, max = 65_536))]
    requests: usize,
    #[garde(range(min = 1, max = self.lease.ttl_seconds.saturating_sub(1)))]
    fallback_seconds: u64,
}

pub(crate) fn read(env: &(impl EnvSource + ?Sized)) -> Result<RuntimeSettings, Refusal> {
    let lease = LeaseSettings {
        ttl_seconds: value(
            env,
            "AGENTSFLEET_LEASE_TTL_SECONDS",
            LeaseSettings::default().ttl_seconds,
        )?,
    };
    let readings = garde::Unvalidated::new(Readings {
        lease,
        claims: value(env, "AGENTSFLEET_RECOVERY_CONCURRENT_CLAIMS", 8)?,
        requests: value(env, "AGENTSFLEET_MAX_IN_FLIGHT_REQUESTS", 256)?,
        fallback_seconds: value(env, "AGENTSFLEET_OFFER_FALLBACK_SECONDS", 5)?,
    })
    .validate()
    .map_err(|report| refused("runtime", report.to_string()))?;
    let lease = garde::Unvalidated::new(readings.lease)
        .validate()
        .map_err(|report| refused("AGENTSFLEET_LEASE_TTL_SECONDS", report.to_string()))?;
    Ok(RuntimeSettings {
        clock: LeaseClock::new(&lease),
        resume_from_checkpoint: value(env, "AGENTSFLEET_RESUME_FROM_CHECKPOINT", false)?,
        fallback: Duration::from_secs(readings.fallback_seconds),
        claims: NonZeroUsize::new(readings.claims).ok_or_else(|| {
            refused(
                "AGENTSFLEET_RECOVERY_CONCURRENT_CLAIMS",
                "must be positive".to_owned(),
            )
        })?,
        requests: NonZeroUsize::new(readings.requests).ok_or_else(|| {
            refused(
                "AGENTSFLEET_MAX_IN_FLIGHT_REQUESTS",
                "must be positive".to_owned(),
            )
        })?,
    })
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
