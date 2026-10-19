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
    now: jiff::Timestamp,
) -> Result<Verdict> {
    if verified.usage.knowledge == Knowledge::Unknown {
        return Ok(Verdict {
            outcome: Outcome::Failed,
            reason: Some(FailureReason::ProviderUsageUnknown),
            answer_allowed: false,
        });
    }
    let input = prepared.request.accounting();
    if let Some(reason) = attempt.failure_reason {
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
    now: jiff::Timestamp,
) -> Result<()> {
    if input.outcome == Outcome::TimedOut
        && (now < attempt.expires_at
            || input.failure_reason != Some(FailureReason::AttemptTimedOut))
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

#[cfg(test)]
mod tests {
    use super::{outcome, reason};
    use afd_wire::authoring::{FailureReason, Outcome};

    /// Attempts and messages store these words, and readers parse them back
    /// through serde: every arm must spell its wire form.
    #[test]
    fn every_stored_verdict_word_is_its_wire_spelling() {
        let wire = |value: serde_json::Value| value.as_str().map(str::to_owned);
        for value in [
            Outcome::Completed,
            Outcome::Failed,
            Outcome::TimedOut,
            Outcome::Cancelled,
        ] {
            assert_eq!(
                Some(outcome(value).to_owned()),
                serde_json::to_value(value).ok().and_then(wire)
            );
        }
        for value in [
            FailureReason::InsufficientBalance,
            FailureReason::ModelUnavailable,
            FailureReason::ContextTooLarge,
            FailureReason::ProviderFailed,
            FailureReason::ProviderUsageUnknown,
            FailureReason::AttemptTimedOut,
            FailureReason::PolicyChanged,
        ] {
            assert_eq!(
                Some(reason(value).to_owned()),
                serde_json::to_value(value).ok().and_then(wire)
            );
        }
    }
}
