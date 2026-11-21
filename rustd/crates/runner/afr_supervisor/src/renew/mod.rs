//! Renewal: a held run bought more time, tick by tick, until the daemon says
//! no or the time it granted runs out.

use std::{sync::Arc, time::Duration};

use af_core::clock::Clock;
use af_core::error_code::{self, RUN_BUDGET_EXCEEDED, RUN_CANCELLED};
use af_core::id::Uuid7;
use af_core::timing::MAX_RUN_SECONDS;
use af_wire::report::FailureClass;
use afr_agent::Meter;
use jiff::Timestamp;
use tokio::time::Instant;

use crate::client::ControlPlane;
use crate::error::Error;

/// Leave time for an uncooperative engine, process-group termination and reaping before
/// the daemon may hand the expired run to another runner.
pub(crate) const EXPIRY_MARGIN: Duration = af_core::timing::ENGINE_STOP_GRACE
    .saturating_add(afr_executor::timing::KILL_GRACE)
    .saturating_add(Duration::from_secs(2));

const _: () = assert!(
    EXPIRY_MARGIN.as_nanos() + af_core::timing::STREAM_ANSWER_TIMEOUT.as_nanos()
        < af_core::timing::RENEWAL_WINDOW.as_nanos(),
    "recovery must leave a complete renewal answer and physical cleanup window"
);
/// The longest one grant can be: the longest run lifetime a daemon may be
/// configured with. It bounds any deadline a reply can set, however far ahead
/// it claims; a run itself renews grant after grant with no ceiling.
const LONGEST_GRANT: Duration = Duration::from_secs(MAX_RUN_SECONDS);
const EVENT_RENEWED: &str = "run_renewed";
const EVENT_TERMINATED: &str = "run_terminated_by_renewal";
const EVENT_EXPIRED: &str = "run_timed_out";

/// Renews one run.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Renewal<'a> {
    plane: &'a ControlPlane,
    run_id: &'a Uuid7,
    expires_at: Timestamp,
    clock: &'a dyn Clock,
    meter: &'a Arc<Meter>,
}

impl<'a> Renewal<'a> {
    /// Renews `run_id`, granted until `expires_at`, with
    /// `clock` reading the wall time the daemon's deadlines are written in and
    /// `meter` the tokens each renewal reports.
    pub(crate) const fn new(
        plane: &'a ControlPlane,
        run_id: &'a Uuid7,
        expires_at: Timestamp,
        clock: &'a dyn Clock,
        meter: &'a Arc<Meter>,
    ) -> Self {
        Self {
            plane,
            run_id,
            expires_at,
            clock,
            meter,
        }
    }

    /// Renews every [`RENEWAL_TICK`] and returns why the run ended.
    ///
    /// A transport failure or a 5xx keeps the run: the daemon may be briefly
    /// unwell and the next tick may land. A 4xx ends it — revoked, lapsed, out
    /// of budget. And whatever the failures were, the run ends once the last
    /// granted deadline, less [`EXPIRY_MARGIN`], has passed: past it the daemon
    /// may already have given the event to another runner.
    pub(crate) async fn keep(self) -> FailureClass {
        let mut registration = match self.plane.stream.renewing.register(
            self.run_id,
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
                        .extend(self.deadline(response.run_expires_at));
                    if let Some(sequence) = response.recorded_through {
                        self.meter.recorded_through(sequence);
                    }
                    let run_id = self.run_id.as_str();
                    tracing::debug!(event = EVENT_RENEWED, run_id);
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

    /// When to give the run up, for a grant until `expires_at`.
    fn deadline(&self, expires_at: Timestamp) -> Instant {
        let left = af_core::clock::elapsed(self.clock.now(), expires_at).min(LONGEST_GRANT);
        Instant::now() + left.saturating_sub(EXPIRY_MARGIN)
    }

    fn terminated(&self, refusal: &Error) -> FailureClass {
        let code = refusal.code().as_str();
        let run_id = self.run_id.as_str();
        let event = EVENT_TERMINATED;
        {
            let (error_code, message) = (&code, format_args!("the daemon ended the run"));
            tracing::warn!(message, error_code, run_id, event);
        };
        // A renewal refused for the run's cancel mark is the Stop whose hint
        // was lost: the run ends as the Cancel would have ended it.
        if refusal.refusal_code() == Some(RUN_CANCELLED) {
            self.plane.stream.running.cancel(self.run_id, true);
        }
        if refusal.refusal_code() == Some(RUN_BUDGET_EXCEEDED) {
            FailureClass::BudgetBreach
        } else {
            FailureClass::RenewalTerminate
        }
    }

    fn expired(&self) -> FailureClass {
        let code = error_code::RUN_LOST.as_str();
        let run_id = self.run_id.as_str();
        let event = EVENT_EXPIRED;
        {
            let (error_code, message) = (
                &code,
                format_args!("the run ran out before a renewal landed"),
            );
            tracing::warn!(message, error_code, run_id, event);
        };
        FailureClass::RenewalTerminate
    }
}

#[cfg(test)]
mod tests;
