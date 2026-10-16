//! Private authoring runs in the supervisor with no runtime tools or sandbox.

mod meter;
mod proposal;
mod renew;
mod turn;

#[cfg(test)]
mod tests;

use self::{meter::Observed, turn::Turn};
use crate::{Result, lease_loop::Lessee};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_wire::authoring::{Accounting, FailureReason, Lease, Outcome, Report};
use serde_json::Value;

struct Ending {
    outcome: Outcome,
    reason: Option<FailureReason>,
    answer: Option<String>,
}

impl Ending {
    fn completed(answer: Option<String>) -> Self {
        Self {
            outcome: Outcome::Completed,
            reason: None,
            answer,
        }
    }

    fn unknown() -> Self {
        Self {
            outcome: Outcome::Failed,
            reason: Some(FailureReason::ProviderUsageUnknown),
            answer: None,
        }
    }

    fn timed_out() -> Self {
        Self {
            outcome: Outcome::TimedOut,
            reason: Some(FailureReason::AttemptTimedOut),
            answer: None,
        }
    }
}

impl Lessee {
    pub(crate) async fn author(&self, lease: &Lease<Value>) -> Result<()> {
        let id = Uuid7::parse(&lease.lease_id)?;
        let observed = Observed::new();
        let turn = Turn {
            plane: &self.plane,
            lease,
            id: &id,
            clock: self.clock.as_ref(),
            observed: &observed,
        };
        let deadline = renew::deadline(lease.deadline, self.clock.as_ref())
            .min(tokio::time::Instant::now() + std::time::Duration::from_secs(300));
        let ending = tokio::select! {
            result = turn.run() => match result {
                Ok(ending) => ending,
                Err(failure) => { self.halt.stops_on(&failure); observed.uncertain()?; Ending::unknown() },
            },
            result = renew::keep(&self.plane, &id, lease, self.clock.as_ref(), &observed) => {
                if let Err(failure) = result { self.halt.stops_on(&failure); }
                observed.uncertain()?;
                Ending::unknown()
            },
            () = self.halt.running().cancelled() => { observed.uncertain()?; Ending::unknown() },
            () = tokio::time::sleep_until(deadline) => { observed.uncertain()?; Ending::timed_out() },
        };
        let accounting = Accounting {
            fence: lease.fence.clone(),
            deletion_generation: lease.deletion_generation.clone(),
            report_id: Entropy::new().uuid7(self.clock.now())?.to_string(),
            outcome: ending.outcome,
            failure_reason: ending.reason,
            usage: observed.snapshot()?,
        };
        self.authoring_spool
            .hold(
                &id,
                &lease.attempt_id,
                Report::Terminal {
                    accounting,
                    answer: ending.answer,
                },
            )
            .await
    }
}
