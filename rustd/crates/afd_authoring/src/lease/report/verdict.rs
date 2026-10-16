//! Unknown obligations take precedence over any ordinary retryable outcome.

use afd_wire::authoring::{FailureReason, Knowledge, Outcome};

use super::prepare::Prepared;
use crate::{
    Result,
    error::raise::invalid,
    lease::{Status, guard, meter},
};

pub(super) struct Verdict {
    pub(super) outcome: Outcome,
    pub(super) reason: Option<FailureReason>,
    pub(super) answer_allowed: bool,
}

pub(super) fn derive(
    attempt: &guard::Attempt,
    prepared: &Prepared,
    verified: &meter::Verified,
    private: Option<&guard::Guarded>,
    cancelled: bool,
    now: i64,
) -> Result<Verdict> {
    if verified.usage.knowledge == Knowledge::Unknown {
        return Ok(Verdict {
            outcome: Outcome::Failed,
            reason: Some(FailureReason::ProviderUsageUnknown),
            answer_allowed: false,
        });
    }
    let input = prepared.request.accounting();
    if let Some(reason) = attempt.daemon_failure {
        return Ok(Verdict {
            outcome: Outcome::Failed,
            reason: Some(reason),
            answer_allowed: false,
        });
    }
    validate_known(attempt, input, verified, cancelled, now)?;
    let answer_allowed =
        private.is_some() && attempt.status == Status::Active && !attempt.usage_unknown;
    let recovered = input.failure_reason == Some(FailureReason::ProviderUsageUnknown)
        || attempt.status == Status::AwaitingAccounting;
    let reason = if recovered {
        Some(FailureReason::ProviderFailed)
    } else {
        input.failure_reason
    };
    Ok(Verdict {
        outcome: if recovered {
            Outcome::Failed
        } else {
            input.outcome
        },
        reason,
        answer_allowed,
    })
}

fn validate_known(
    attempt: &guard::Attempt,
    input: &afd_wire::authoring::Accounting,
    verified: &meter::Verified,
    cancelled: bool,
    now: i64,
) -> Result<()> {
    if input.outcome == Outcome::TimedOut
        && (now < attempt.deadline || input.failure_reason != Some(FailureReason::AttemptTimedOut))
    {
        return Err(invalid("A timed_out report requires the attempt deadline"));
    }
    if input.outcome == Outcome::Cancelled && !cancelled {
        return Err(invalid(
            "A cancelled report requires an existing cancellation fence",
        ));
    }
    if matches!(
        input.failure_reason,
        Some(
            FailureReason::ModelUnavailable
                | FailureReason::InsufficientBalance
                | FailureReason::ContextTooLarge
                | FailureReason::PolicyChanged
        )
    ) {
        return Err(invalid(
            "This failure reason requires a saved daemon observation",
        ));
    }
    if input.outcome == Outcome::Completed && verified.failed_calls != 0 {
        return Err(invalid(
            "A failed provider dispatch cannot be reported completed",
        ));
    }
    Ok(())
}

pub(super) fn outcome(value: Outcome) -> &'static str {
    match value {
        Outcome::Completed => "completed",
        Outcome::Failed => "failed",
        Outcome::TimedOut => "timed_out",
        Outcome::Cancelled => "cancelled",
    }
}

pub(crate) fn reason(value: FailureReason) -> &'static str {
    match value {
        FailureReason::InsufficientBalance => "insufficient_balance",
        FailureReason::ModelUnavailable => "model_unavailable",
        FailureReason::ContextTooLarge => "context_too_large",
        FailureReason::ProviderFailed => "provider_failed",
        FailureReason::ProviderUsageUnknown => "provider_usage_unknown",
        FailureReason::AttemptTimedOut => "attempt_timed_out",
        FailureReason::PolicyChanged => "policy_changed",
    }
}
