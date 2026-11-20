//! Recovery actions are derived by the daemon from a bounded durable reason.

use af_core::error_code::{self, ErrorCode};
use af_wire::setup::FailureReason;
use serde::Serialize;

/// The owner must add balance before another turn can be admitted.
const ACTION_OWNER_BILLING: &str = "owner_billing";
/// The selected setup model must be replaced while the draft is idle.
const ACTION_CHOOSE_MODEL: &str = "choose_setup_model";
/// The conversation or candidate must shrink to fit the model window.
const ACTION_REDUCE_CONTEXT: &str = "reduce_context";
/// A new message with a new key may be sent.
const ACTION_RETRY_MESSAGE: &str = "retry_message";

/// Safe creator-visible failure without diagnostics, arbitrary URLs or keys.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Failure {
    /// Persisted bounded cause.
    pub reason: FailureReason,
    /// Registry code for this cause.
    pub error_code: ErrorCode,
    /// Stable client action; authorized labels/links are derived on read.
    pub recovery_action: &'static str,
}

impl From<FailureReason> for Failure {
    fn from(reason: FailureReason) -> Self {
        let (error_code, recovery_action) = match reason {
            FailureReason::InsufficientBalance => (error_code::SETUP_BALANCE, ACTION_OWNER_BILLING),
            FailureReason::ModelUnavailable => (error_code::SETUP_MODEL, ACTION_CHOOSE_MODEL),
            FailureReason::ProviderFailed => (error_code::SETUP_PROVIDER, ACTION_RETRY_MESSAGE),
            // The draft no longer waits on accounting, so the person may send again.
            FailureReason::ProviderUsageUnknown => {
                (error_code::SETUP_UNKNOWN, ACTION_RETRY_MESSAGE)
            }
            // The same message would fail again, so the person shortens it.
            FailureReason::FrameTooLarge => {
                (error_code::RUN_FRAME_TOO_LARGE, ACTION_REDUCE_CONTEXT)
            }
        };
        Self {
            reason,
            error_code,
            recovery_action,
        }
    }
}

#[cfg(test)]
mod tests {
    use af_core::error_code;
    use af_wire::setup::FailureReason;

    use super::{ACTION_REDUCE_CONTEXT, Failure};

    /// The same message would not fit again, so the person is asked to
    /// shorten it, under the code the fleet path fails such a run with.
    #[test]
    fn a_frame_too_large_failure_names_the_run_code_and_asks_to_shorten() {
        let failure = Failure::from(FailureReason::FrameTooLarge);
        assert_eq!(failure.error_code, error_code::RUN_FRAME_TOO_LARGE);
        assert_eq!(failure.recovery_action, ACTION_REDUCE_CONTEXT);
        assert_eq!(failure.recovery_action, "reduce_context");
    }
}
