//! Renewal: a held lease bought more time, tick by tick, until the daemon says
//! no.

use std::time::Duration;

use afd_core::error_code::RUN_BUDGET_EXCEEDED;
use afd_core::id::Uuid7;
use afd_core::timing::RENEWAL_TICK_MS;
use afd_wire::report::FailureClass;
use tokio::time::{Instant, MissedTickBehavior};

use crate::client::ControlPlane;
use crate::error::Error;

/// How often a held lease is renewed: inside the renewal window, so one missed
/// tick still lands before the lease lapses (`afd_core::timing`).
pub const RENEWAL_TICK: Duration = Duration::from_millis(RENEWAL_TICK_MS.unsigned_abs());
const EVENT_RENEWED: &str = "lease_renewed";
const EVENT_RETRY: &str = "renew_failed_retry";
const EVENT_TERMINATED: &str = "lease_terminated_by_renewal";

/// Renews one lease.
#[derive(Debug, Clone, Copy)]
pub struct Renewal<'a> {
    plane: &'a ControlPlane,
    lease_id: &'a Uuid7,
}

impl<'a> Renewal<'a> {
    /// Renews `lease_id` through `plane`.
    #[must_use]
    pub const fn new(plane: &'a ControlPlane, lease_id: &'a Uuid7) -> Self {
        Self { plane, lease_id }
    }

    /// Renews every [`RENEWAL_TICK`] until the daemon refuses, and returns why
    /// the lease ended.
    ///
    /// A transport failure or a 5xx keeps the lease: the daemon may be briefly
    /// unwell and the next tick may land. A 4xx ends it — revoked, lapsed, out
    /// of budget — and no tick would change that.
    pub async fn keep(self) -> FailureClass {
        let mut ticks = tokio::time::interval_at(Instant::now() + RENEWAL_TICK, RENEWAL_TICK);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let lease_id = self.lease_id.as_str();
        loop {
            ticks.tick().await;
            match self.plane.renew(self.lease_id).await {
                Ok(()) => {
                    let event = EVENT_RENEWED;
                    tracing::debug!(lease_id, event);
                }
                Err(failure) if failure.is_retryable() => {
                    let code = failure.code().as_str();
                    let event = EVENT_RETRY;
                    tracing::warn!(
                        error_code = code,
                        lease_id,
                        event,
                        "a renewal failed; the lease is kept"
                    );
                }
                Err(refusal) => {
                    let code = refusal.code().as_str();
                    let event = EVENT_TERMINATED;
                    tracing::warn!(
                        error_code = code,
                        lease_id,
                        event,
                        "the daemon ended the lease"
                    );
                    return class_of(&refusal);
                }
            }
        }
    }
}

/// The failure class a renewal refusal ends a run with.
fn class_of(refusal: &Error) -> FailureClass {
    if refusal.refusal_code() == Some(RUN_BUDGET_EXCEEDED) {
        FailureClass::BudgetBreach
    } else {
        FailureClass::RenewalTerminate
    }
}

#[cfg(test)]
#[path = "renew/tests.rs"]
mod tests;
