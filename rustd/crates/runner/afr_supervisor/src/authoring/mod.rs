//! Private authoring runs in the supervisor with no runtime tools or sandbox.

mod meter;
mod proposal;
mod renew;
mod turn;

#[cfg(test)]
pub(crate) mod tests;

use self::{meter::Observed, turn::Turn};
use crate::{Result, lease_loop::Lessee};
use crate::{client::ControlPlane, halt::Halt, report::delivery::until_acknowledged};
use af_core::{id::Uuid7, timing::DAY};
use af_crypto::entropy::Entropy;
use af_wire::authoring::{Accounting, FailureReason, Lease, Outcome, Report, Reported};
use serde_json::Value;
use tokio::time::Instant;
use zeroize::{Zeroize, Zeroizing};

struct Ending {
    outcome: Outcome,
    reason: Option<FailureReason>,
    /// The model's private text; its wrapper wipes the buffer when the run drops it unsent.
    answer: Option<Zeroizing<String>>,
}

impl Ending {
    fn completed(answer: Option<String>) -> Self {
        Self {
            outcome: Outcome::Completed,
            reason: None,
            answer: answer.map(Zeroizing::new),
        }
    }

    /// Hand the answer's buffer to the report unchanged; the report's owner wipes it next.
    fn take_answer(&mut self) -> Option<String> {
        self.answer
            .take()
            .map(|mut answer| std::mem::take(&mut *answer))
    }

    const fn unknown() -> Self {
        Self {
            outcome: Outcome::Failed,
            reason: Some(FailureReason::ProviderUsageUnknown),
            answer: None,
        }
    }

    const fn incomplete() -> Self {
        Self {
            outcome: Outcome::Failed,
            reason: Some(FailureReason::ProviderFailed),
            answer: None,
        }
    }

    const fn timed_out() -> Self {
        Self {
            outcome: Outcome::TimedOut,
            reason: Some(FailureReason::AttemptTimedOut),
            answer: None,
        }
    }
}

impl Lessee {
    pub(crate) async fn author(
        &self,
        lease: &Lease<Value>,
        interrupt: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
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
        // Polled in order: renewal stops at the same attempt end, so the
        // deadline is checked first and a turn that reaches it reports
        // `timed_out`, never a lost slot.
        let mut ending = tokio::select! {
            biased;
            () = tokio::time::sleep_until(deadline) => { observed.uncertain()?; Ending::timed_out() },
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
            () = interrupt.cancelled() => { observed.uncertain()?; Ending::unknown() },
        };
        let accounting = Accounting {
            fence: lease.fence.clone(),
            report_id: Entropy::new().uuid7(self.clock.now())?.to_string(),
            outcome: ending.outcome,
            failure_reason: ending.reason,
            usage: observed.snapshot()?,
        };
        deliver_report(
            &self.plane,
            &self.halt,
            &id,
            Report::Terminal {
                accounting,
                answer: ending.take_answer(),
            },
        )
        .await
    }
}

/// Zeroes the owned private answer when delivery completes or its future is dropped.
struct Pending(Report);

impl Drop for Pending {
    fn drop(&mut self) {
        if let Report::Terminal {
            answer: Some(answer),
            ..
        } = &mut self.0
        {
            answer.zeroize();
        }
    }
}

async fn deliver_report(
    plane: &ControlPlane,
    halt: &Halt,
    lease: &Uuid7,
    report: Report,
) -> Result<()> {
    let mut pending = Pending(report);
    garde::Unvalidated::new(&pending.0).validate()?;
    let private_until = Instant::now() + DAY;
    let sending = acknowledged_report(plane, halt, lease, &pending.0);
    match tokio::time::timeout_at(private_until, sending).await {
        Ok(result) => result.map(drop),
        Err(_expired) => {
            // Privacy expiry changes an authoring report to its current
            // accounting-only form; no earlier wire format is accepted or retried.
            if let Report::Terminal {
                answer: Some(answer),
                ..
            } = &mut pending.0
            {
                answer.zeroize();
            }
            pending.0 = Report::Accounting {
                accounting: pending.0.accounting().clone(),
            };
            acknowledged_report(plane, halt, lease, &pending.0)
                .await
                .map(drop)
        }
    }
}

async fn acknowledged_report(
    plane: &ControlPlane,
    halt: &Halt,
    lease: &Uuid7,
    report: &Report,
) -> Result<Reported> {
    until_acknowledged(halt, || async {
        let reply = plane.authoring_report(lease, report).await?;
        Ok(reply.settled.then_some(reply))
    })
    .await
}

#[cfg(test)]
mod ownership_tests;
