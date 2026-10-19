//! A retry can consume only the time remaining on the original live slot.

use super::meter::Observed;
use crate::{Result, client::ControlPlane, error};
use afd_core::{clock::Clock, id::Uuid7};
use afd_wire::authoring::{Lease, Renew};
use jiff::Timestamp;
use serde_json::Value;
use std::time::Duration;
use tokio::time::Instant;

const INTERVAL: Duration = Duration::from_secs(10);
const RETRY: Duration = Duration::from_millis(250);

pub(super) async fn keep(
    plane: &ControlPlane,
    id: &Uuid7,
    lease: &Lease<Value>,
    clock: &dyn Clock,
    observed: &Observed,
) -> Result<()> {
    let mut expiry = deadline(lease.expires_at, clock);
    let maximum = deadline(lease.deadline, clock).min(Instant::now() + Duration::from_secs(300));
    let mut pause = INTERVAL;
    loop {
        let renew_at = (Instant::now() + pause).min(expiry).min(maximum);
        tokio::time::sleep_until(renew_at).await;
        if Instant::now() >= expiry || Instant::now() >= maximum {
            return Err(error::config("the private authoring lease expired"));
        }
        let request = Renew {
            fence: lease.fence.clone(),
            usage: observed.snapshot()?,
        };
        let response =
            tokio::time::timeout_at(expiry.min(maximum), plane.authoring_renew(id, &request))
                .await
                .map_err(|_elapsed| {
                    error::config("authoring renewal exceeded the live deadline")
                })?;
        match response {
            Ok(reply) if !reply.stop_requested => {
                expiry = deadline(reply.expires_at, clock).min(maximum);
                pause = INTERVAL;
            }
            Ok(_) => return Ok(()),
            Err(failure) if failure.is_retryable() => pause = RETRY,
            Err(failure) => return Err(failure),
        }
    }
}

pub(super) fn deadline(at: Timestamp, clock: &dyn Clock) -> Instant {
    Instant::now() + crate::deadline::left(clock.now(), at)
}
