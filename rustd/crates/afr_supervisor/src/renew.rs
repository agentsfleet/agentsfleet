//! Renewal: a held lease bought more time, tick by tick, until the daemon says
//! no or the time it granted runs out.

use std::time::Duration;

use afd_core::clock::Clock;
use afd_core::error_code::{self, RUN_BUDGET_EXCEEDED};
use afd_core::id::Uuid7;
use afd_core::timing::{MAX_RUNTIME_MS, RENEWAL_TICK_MS};
use afd_wire::report::{FailureClass, RenewRequest};
use afr_agent::Meter;
use tokio::time::{Instant, MissedTickBehavior};

use crate::client::ControlPlane;
use crate::error::Error;
use crate::report::narrow;

/// How often a held lease is renewed: inside the renewal window, so one missed
/// tick still lands before the lease lapses (`afd_core::timing`).
pub(crate) const RENEWAL_TICK: Duration = Duration::from_millis(RENEWAL_TICK_MS.unsigned_abs());
/// How long before the daemon's deadline the runner gives the lease up. The
/// daemon may hand an expired lease to another runner, so a run must end
/// before then, not at it; this is one slow call's worth of slack.
pub(crate) const EXPIRY_MARGIN: Duration = Duration::from_secs(2);
/// The longest a lease can be granted for, which bounds any deadline a reply
/// can set, however far ahead it claims.
const LONGEST_GRANT: Duration = Duration::from_millis(MAX_RUNTIME_MS.unsigned_abs());
const EVENT_RENEWED: &str = "lease_renewed";
const EVENT_RETRY: &str = "renew_failed_retry";
const EVENT_TERMINATED: &str = "lease_terminated_by_renewal";
const EVENT_EXPIRED: &str = "lease_timed_out";

/// Renews one lease.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Renewal<'a> {
    plane: &'a ControlPlane,
    lease_id: &'a Uuid7,
    expires_at: i64,
    clock: &'a dyn Clock,
    meter: &'a Meter,
}

impl<'a> Renewal<'a> {
    /// Renews `lease_id`, granted until `expires_at` (Unix milliseconds), with
    /// `clock` reading the wall time the daemon's deadlines are written in and
    /// `meter` the tokens each renewal reports.
    pub(crate) const fn new(
        plane: &'a ControlPlane,
        lease_id: &'a Uuid7,
        expires_at: i64,
        clock: &'a dyn Clock,
        meter: &'a Meter,
    ) -> Self {
        Self {
            plane,
            lease_id,
            expires_at,
            clock,
            meter,
        }
    }

    /// Renews every [`RENEWAL_TICK`] and returns why the lease ended.
    ///
    /// A transport failure or a 5xx keeps the lease: the daemon may be briefly
    /// unwell and the next tick may land. A 4xx ends it — revoked, lapsed, out
    /// of budget. And whatever the failures were, the lease ends once the last
    /// granted deadline, less [`EXPIRY_MARGIN`], has passed: past it the daemon
    /// may already have given the event to another runner.
    pub(crate) async fn keep(self) -> FailureClass {
        let mut deadline = self.deadline(self.expires_at);
        let mut ticks = tokio::time::interval_at(Instant::now() + RENEWAL_TICK, RENEWAL_TICK);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            let renewed = tokio::select! {
                biased;
                () = tokio::time::sleep_until(deadline) => return self.expired(),
                renewed = async {
                    ticks.tick().await;
                    self.plane.renew(self.lease_id, &self.spent()).await
                } => renewed,
            };
            match renewed {
                Ok(expires_at) => {
                    deadline = self.deadline(expires_at);
                    let lease_id = self.lease_id.as_str();
                    let event = EVENT_RENEWED;
                    tracing::debug!(lease_id, event);
                }
                Err(failure) if failure.is_retryable() => self.kept(&failure),
                Err(refusal) => return self.terminated(&refusal),
            }
        }
    }

    /// The run's tokens so far, as the renewal reports them: cumulative, so
    /// the daemon meters the difference since the last one.
    fn spent(&self) -> RenewRequest {
        let usage = self.meter.read();
        RenewRequest {
            input_tokens: narrow(usage.input),
            cached_input_tokens: narrow(usage.cached_input),
            output_tokens: narrow(usage.output),
        }
    }

    /// When to give the lease up, for a grant until `expires_at`.
    fn deadline(&self, expires_at: i64) -> Instant {
        let left = expires_at.saturating_sub(self.clock.now().as_millis());
        let left = Duration::from_millis(u64::try_from(left).unwrap_or(0)).min(LONGEST_GRANT);
        Instant::now() + left.saturating_sub(EXPIRY_MARGIN)
    }

    fn kept(&self, failure: &Error) {
        let code = failure.code().as_str();
        let lease_id = self.lease_id.as_str();
        let event = EVENT_RETRY;
        tracing::warn!(
            error_code = code,
            lease_id,
            event,
            "a renewal failed; the lease is kept"
        );
    }

    fn terminated(&self, refusal: &Error) -> FailureClass {
        let code = refusal.code().as_str();
        let lease_id = self.lease_id.as_str();
        let event = EVENT_TERMINATED;
        tracing::warn!(
            error_code = code,
            lease_id,
            event,
            "the daemon ended the lease"
        );
        if refusal.refusal_code() == Some(RUN_BUDGET_EXCEEDED) {
            FailureClass::BudgetBreach
        } else {
            FailureClass::RenewalTerminate
        }
    }

    fn expired(&self) -> FailureClass {
        let code = error_code::RUN_LEASE_LOST.as_str();
        let lease_id = self.lease_id.as_str();
        let event = EVENT_EXPIRED;
        tracing::warn!(
            error_code = code,
            lease_id,
            event,
            "the lease ran out before a renewal landed"
        );
        FailureClass::RenewalTerminate
    }
}

#[cfg(test)]
#[path = "renew/tests.rs"]
mod tests;
