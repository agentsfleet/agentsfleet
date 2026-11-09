//! Renewal: a held lease bought more time, tick by tick, until the daemon says
//! no or the time it granted runs out.

use std::{sync::Arc, time::Duration};

use af_core::clock::Clock;
use af_core::error_code::{self, RUN_BUDGET_EXCEEDED, RUN_RENEW_BODY_MALFORMED};
use af_core::id::Uuid7;
use af_core::timing::MAX_RUNTIME;
use af_wire::report::{FailureClass, RenewRequest};
use af_wire::usage::LINES_MAX;
use afr_agent::Meter;
use jiff::Timestamp;
use tokio::time::Instant;

use crate::client::{ControlPlane, retrying};
use crate::error::Error;

/// Leave time for an uncooperative engine, process-group termination and reaping before
/// the daemon may hand the expired lease to another runner.
pub(crate) const EXPIRY_MARGIN: Duration = af_core::timing::ENGINE_STOP_GRACE
    .saturating_add(afr_executor::timing::KILL_GRACE)
    .saturating_add(Duration::from_secs(2));
/// The longest a lease can be granted for, which bounds any deadline a reply
/// can set, however far ahead it claims.
const LONGEST_GRANT: Duration = MAX_RUNTIME;
const EVENT_RENEWED: &str = "lease_renewed";
const EVENT_TERMINATED: &str = "lease_terminated_by_renewal";
const EVENT_EXPIRED: &str = "lease_timed_out";
const EVENT_UNFLUSHED: &str = "usage_lines_unflushed";

/// Renews one lease.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Renewal<'a> {
    plane: &'a ControlPlane,
    lease_id: &'a Uuid7,
    expires_at: Timestamp,
    clock: &'a dyn Clock,
    meter: &'a std::sync::Arc<Meter>,
}

impl<'a> Renewal<'a> {
    /// Renews `lease_id`, granted until `expires_at`, with
    /// `clock` reading the wall time the daemon's deadlines are written in and
    /// `meter` the tokens each renewal reports.
    pub(crate) const fn new(
        plane: &'a ControlPlane,
        lease_id: &'a Uuid7,
        expires_at: Timestamp,
        clock: &'a dyn Clock,
        meter: &'a std::sync::Arc<Meter>,
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
        let mut registration = match self.plane.stream.renewing.register(
            self.lease_id,
            Arc::clone(self.meter),
            self.deadline(self.expires_at),
        ) {
            Ok(registration) => registration,
            Err(error) => return self.terminated(&error),
        };
        loop {
            let deadline = registration.lifetime.deadline();
            tokio::select! {
                biased;
                () = tokio::time::sleep_until(deadline) => return self.expired(),
                changed = registration.updates.changed() => {
                    if changed.is_err() { return self.expired(); }
                }
            }
            let response = registration.updates.borrow_and_update().clone();
            match response {
                Some(crate::uplink::renewals::Renewed::Confirmed(response)) => {
                    registration
                        .lifetime
                        .extend(self.deadline(response.lease_expires_at));
                    if let Some(sequence) = response.recorded_through {
                        self.meter.recorded_through(sequence);
                    }
                    let lease_id = self.lease_id.as_str();
                    tracing::debug!(event = EVENT_RENEWED, lease_id);
                }
                Some(crate::uplink::renewals::Renewed::Refused(code)) => {
                    let error = crate::error::ErrorKind::StreamRefused {
                        code,
                        retryable: false,
                    }
                    .into();
                    return self.terminated(&error);
                }
                None => {}
            }
        }
    }

    /// When to give the lease up, for a grant until `expires_at`.
    fn deadline(&self, expires_at: Timestamp) -> Instant {
        let left = af_core::clock::elapsed(self.clock.now(), expires_at).min(LONGEST_GRANT);
        Instant::now() + left.saturating_sub(EXPIRY_MARGIN)
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

/// Renews until the meter's unrecorded lines fit one report body.
///
/// The daemon refuses a report past [`LINES_MAX`] lines, so a run that ends
/// with more sends its oldest through renewals first. Each answer's
/// `recorded_through` forgets what that renewal recorded.
///
/// A renewal that fails after its retries, or records nothing, stops the
/// flush. The report then carries the oldest lines left, and the lines past
/// them are logged unbilled. A line sent twice is still billed once, since the
/// daemon keys lines on `(work, seq)`.
pub(crate) async fn flush(plane: &ControlPlane, lease_id: &Uuid7, meter: &Meter) {
    loop {
        let backlog = meter.backlog();
        if backlog <= LINES_MAX {
            return;
        }
        let spent = RenewRequest {
            requests: meter.unrecorded(),
        };
        let renewed = retrying(|| plane.renew(lease_id, &spent)).await;
        if let Some(seq) = renewed
            .as_ref()
            .ok()
            .and_then(|renewed| renewed.recorded_through)
        {
            meter.recorded_through(seq);
        }
        if meter.backlog() < backlog {
            continue;
        }
        // A renewal that answered yet recorded nothing read a body it could
        // not use, which the daemon logs under the same code.
        let code = renewed
            .err()
            .map_or(RUN_RENEW_BODY_MALFORMED, |failure| failure.code());
        let error_code = code.as_str();
        let lease_id = lease_id.as_str();
        let unbilled = meter.backlog().saturating_sub(LINES_MAX);
        let event = EVENT_UNFLUSHED;
        tracing::error!(
            error_code,
            lease_id,
            unbilled,
            event,
            "a renewal did not take the run's usage lines; the report carries the oldest"
        );
        return;
    }
}

#[cfg(test)]
mod tests;
