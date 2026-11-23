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
        assert_eq!(fault.durable_task_current_state(), None);
        assert_eq!(fault.durable_task_precondition_tag(), None);
        assert_eq!(fault.decision_failure_reason(), None);
        assert_eq!(fault.durable_task_timeout_reason(), None);
        assert_eq!(fault.remote_cooldown(), None);
        assert!(!fault.is_fence_refusal());
        assert!(fault.source().is_none());
    }
}

// Value: protects=a refusal's message is its public sentence and nothing else;
// fails_when=Display quotes another field or drops the detail; why_new=the
// rendered message had no case; seam=the fleet error's Display.
#[test]
fn a_refusal_renders_as_its_public_detail() {
    for reason in [
        TurnCheckpointRefusal::Incompatible,
        TurnCheckpointRefusal::Conflict,
        TurnCheckpointRefusal::Unavailable,
        TurnCheckpointRefusal::Limit,
    ] {
        assert_eq!(reason.to_string(), reason.detail());
        let rendered = crate::Error::from(reason).to_string();
        assert!(
            rendered.ends_with(reason.detail()) && rendered.contains(reason.code().as_str()),
            "{rendered}"
        );
    }
}
