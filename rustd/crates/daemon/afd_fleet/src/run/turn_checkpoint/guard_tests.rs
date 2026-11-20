//! Bounds and post-wait ownership predicates; live lock contention is a database proof.

use super::{guard, plan};
use af_core::error_code;
use af_wire::run::{TURN_CHECKPOINT_FORMAT, TurnCheckpointBegin, TurnWorkspace};

#[test]
fn a_checkpoint_fence_must_equal_both_current_owners() {
    assert!(guard::exact_fence(7, 7, 7));
    for (run, thread, presented) in [
        (6, 7, 6),
        (8, 7, 8),
        (7, 7, 6),
        (0, 0, 0),
        (-1, -1, u64::MAX),
    ] {
        assert!(!guard::exact_fence(run, thread, presented));
    }
}

#[test]
fn a_lock_wait_cannot_reuse_a_pre_wait_deadline() {
    let expires = jiff::Timestamp::constant(35, 0);
    let before = jiff::Timestamp::constant(34, 0);
    let after = jiff::Timestamp::constant(36, 0);
    assert!(guard::live(expires, expires, before));
    assert!(!guard::live(expires, expires, expires));
    assert!(!guard::live(expires, expires, after));
    assert!(!guard::live(after, expires, expires));
    assert!(!guard::live(expires, after, expires));
}

#[test]
fn api_only_cannot_discard_the_files_declared_by_a_capture() -> crate::Result<()> {
    let mut input = begin();
    plan::shape(&input)?;
    input.logical_bytes = 1;
    assert!(
        plan::shape(&input)
            .is_err_and(|fault| fault.code() == error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    input.logical_bytes = 0;
    input.page_count = 1;
    assert!(
        plan::shape(&input)
            .is_err_and(|fault| fault.code() == error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    input.page_count = 0;
    input.workspace = TurnWorkspace::Files {
        toolbox_digest: "a".repeat(64),
    };
    assert!(
        plan::shape(&input)
            .is_err_and(|fault| fault.code() == error_code::RUN_CHECKPOINT_INCOMPATIBLE)
    );
    input.page_count = 1;
    plan::shape(&input)?;
    Ok(())
}

fn begin() -> TurnCheckpointBegin {
    TurnCheckpointBegin {
        expected_head: None,
        format_version: TURN_CHECKPOINT_FORMAT,
        workspace: TurnWorkspace::ApiOnly,
        history_digest: "a".repeat(64),
        page_count: 0,
        upload_budget_bytes: 0,
        logical_bytes: 0,
        entry_count: 0,
    }
}
