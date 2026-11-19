//! A retry can consume only the time remaining on the original live slot.

use super::meter::Observed;
use crate::{Result, client::ControlPlane, error};
use af_core::{clock::Clock, id::Uuid7};
use af_wire::authoring::{Renew, Run};
use jiff::Timestamp;
use serde_json::Value;
use std::time::Duration;
use tokio::time::Instant;

pub(super) async fn keep(
    plane: &ControlPlane,
    id: &Uuid7,
    run: &Run<Value>,
    clock: &dyn Clock,
    observed: &Observed,
) -> Result<()> {
    let maximum = deadline(run.deadline, clock)
        .min(Instant::now() + Duration::from_secs(u64::from(af_wire::authoring::WALL_SECONDS_MAX)));
    let lifetime = plane
        .stream
        .renewing
        .authoring(id, deadline(run.expires_at, clock).min(maximum))?;
    let settings = plane.runtime.settings();
    let interval = Duration::from_millis(settings.authoring_renewal_interval_ms);
    let retry = Duration::from_millis(settings.authoring_renewal_retry_ms);
    let mut pause = interval;
    loop {
        let expiry = lifetime.deadline();
        let renew_at = (Instant::now() + pause).min(expiry).min(maximum);
        tokio::time::sleep_until(renew_at).await;
        if Instant::now() >= expiry || Instant::now() >= maximum {
            return Err(error::config("the private authoring run expired"));
        }
        let request = Renew {
            fence: run.fence.clone(),
            usage: observed.snapshot()?,
        };
        let response =
            tokio::time::timeout_at(expiry.min(maximum), plane.authoring_renew(id, &request))
                .await
                .map_err(|source| {
                    error::boundary("authoring renewal exceeded the live deadline", source)
                })?;
        match response {
            Ok(reply) if !reply.stop_requested => {
                lifetime.extend(deadline(reply.expires_at, clock).min(maximum));
                pause = interval;
            }
            Ok(_) => return Ok(()),
            Err(failure) if failure.is_retryable() => pause = retry,
            Err(failure) => return Err(failure),
        }
    }
}

pub(super) fn deadline(at: Timestamp, clock: &dyn Clock) -> Instant {
    Instant::now() + af_core::clock::elapsed(clock.now(), at)
}
