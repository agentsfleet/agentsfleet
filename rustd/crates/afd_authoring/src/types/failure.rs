//! Recovery actions are derived by the daemon from a bounded durable reason.

use afd_core::error_code::{self, ErrorCode};
use afd_wire::authoring::FailureReason;
use serde::Serialize;

/// The owner must add balance before another turn can be admitted.
const ACTION_OWNER_BILLING: &str = "owner_billing";
/// The selected authoring model must be replaced while the draft is idle.
const ACTION_CHOOSE_MODEL: &str = "choose_authoring_model";
/// The conversation or candidate must shrink to fit the model window.
const ACTION_REDUCE_CONTEXT: &str = "reduce_context";
/// A new message with a new key may be sent.
const ACTION_RETRY_MESSAGE: &str = "retry_message";
/// Uncancelled provider work must settle or be explicitly stopped.
const ACTION_AWAIT_ACCOUNTING: &str = "await_accounting";
/// The setup must be reviewed again under the current policy.
const ACTION_REVIEW_SETUP: &str = "review_setup";

/// Safe creator-visible failure without diagnostics, arbitrary URLs or keys.
#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    /// Persisted bounded cause.
    pub reason: FailureReason,
    /// Registry code for this cause.
    pub error_code: ErrorCode,
    /// Stable client action; authorized labels/links are derived on read.
    pub recovery_action: &'static str,
}

impl Failure {
    pub(crate) fn for_message(
        reason: FailureReason,
        sequence: i64,
        cancelled_through: i64,
    ) -> Self {
        let mut failure = Self::from(reason);
        if reason == FailureReason::ProviderUsageUnknown && sequence <= cancelled_through {
            failure.recovery_action = ACTION_RETRY_MESSAGE;
        }
        failure
    }
}

impl From<FailureReason> for Failure {
    fn from(reason: FailureReason) -> Self {
        let (error_code, recovery_action) = match reason {
            FailureReason::InsufficientBalance => {
                (error_code::AUTHORING_BALANCE, ACTION_OWNER_BILLING)
            }
            FailureReason::ModelUnavailable => (error_code::AUTHORING_MODEL, ACTION_CHOOSE_MODEL),
            FailureReason::ContextTooLarge => (error_code::AUTHORING_TOKEN, ACTION_REDUCE_CONTEXT),
            FailureReason::ProviderFailed => (error_code::AUTHORING_PROVIDER, ACTION_RETRY_MESSAGE),
            FailureReason::ProviderUsageUnknown => {
                (error_code::AUTHORING_UNKNOWN, ACTION_AWAIT_ACCOUNTING)
            }
            FailureReason::AttemptTimedOut => (error_code::AUTHORING_TIMEOUT, ACTION_RETRY_MESSAGE),
            FailureReason::PolicyChanged => (error_code::AUTHORING_POLICY, ACTION_REVIEW_SETUP),
        };
        Self {
            reason,
            error_code,
            recovery_action,
        }
    }
}
