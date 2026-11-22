//! A private file-custody failure keeps its cause and answers with the
//! checkpoint code a runner acts on.

use std::error::Error as _;

use af_core::error_code;

use super::files::public_failure;
use crate::durable_task::DurableTaskRefusal;

// Value: protects=a runner sees checkpoint codes, never task-plane codes, for a
// file-custody failure; fails_when=any arm of public_failure is dropped or
// mapped to another refusal; why_new=four arms had no case; seam=the mapping
// on the task refusals file custody raises.
#[test]
fn file_custody_failures_answer_with_checkpoint_codes() {
    for (refusal, code) in [
        (
            DurableTaskRefusal::LimitExhausted,
            error_code::RUN_CHECKPOINT_LIMIT,
        ),
        (
            DurableTaskRefusal::StorageUnavailable,
            error_code::RUN_CHECKPOINT_UNAVAILABLE,
        ),
        (
            DurableTaskRefusal::CheckpointIncompatible,
            error_code::RUN_CHECKPOINT_INCOMPATIBLE,
        ),
        (
            DurableTaskRefusal::ReplayConflict,
            error_code::RUN_CHECKPOINT_CONFLICT,
        ),
        (
            DurableTaskRefusal::StaleState,
            error_code::RUN_CHECKPOINT_CONFLICT,
        ),
    ] {
        let mapped = public_failure(crate::Error::from(refusal));
        assert_eq!(mapped.code(), code, "{refusal:?}");
        let cause = mapped
            .source()
            .and_then(|source| source.downcast_ref::<crate::Error>());
        assert_eq!(
            cause.map(crate::Error::code),
            Some(refusal.code()),
            "{refusal:?}"
        );
    }
}

// Value: protects=a failure file custody does not own reaches the runner
// unchanged; fails_when=the fallthrough arm rewrites every failure; why_new=the
// passthrough had no case beside the mapped arms; seam=public_failure on a
// refusal outside its table.
#[test]
fn a_failure_outside_file_custody_passes_through() {
    let passed = public_failure(crate::Error::from(DurableTaskRefusal::NotFound));
    assert_eq!(passed.code(), error_code::DURABLE_TASK_NOT_FOUND);
}
