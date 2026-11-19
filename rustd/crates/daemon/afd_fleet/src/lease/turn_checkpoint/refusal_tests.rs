//! Checkpoint refusals keep the same public code through the existing fleet error shell.

use super::TurnCheckpointRefusal;
use af_core::error_code;
use std::error::Error as _;

#[test]
fn recovery_refusals_are_safe_nonpermanent_domain_answers() {
    for (reason, code) in [
        (
            TurnCheckpointRefusal::Incompatible,
            error_code::RUN_CHECKPOINT_INCOMPATIBLE,
        ),
        (
            TurnCheckpointRefusal::Conflict,
            error_code::RUN_CHECKPOINT_CONFLICT,
        ),
        (
            TurnCheckpointRefusal::Unavailable,
            error_code::RUN_CHECKPOINT_UNAVAILABLE,
        ),
        (
            TurnCheckpointRefusal::Limit,
            error_code::RUN_CHECKPOINT_LIMIT,
        ),
    ] {
        let fault = crate::Error::from(reason);
        assert_eq!(fault.code(), code);
        assert_eq!(fault.detail(), reason.detail());
        assert!(!fault.is_config_permanent());
        assert!(!fault.is_datastore_unavailable());
        assert_eq!(fault.task_current_state(), None);
        assert_eq!(fault.task_precondition_tag(), None);
        assert_eq!(fault.decision_failure_reason(), None);
        assert_eq!(fault.task_timeout_reason(), None);
        assert_eq!(fault.remote_cooldown(), None);
        assert!(!fault.is_fence_refusal());
        assert!(fault.source().is_none());
    }
}
