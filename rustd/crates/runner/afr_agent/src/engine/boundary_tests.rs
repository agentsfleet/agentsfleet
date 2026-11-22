//! A receipt from another attempt or private root never advances recovery state.

use super::{
    CommittedRecovery, RecoveryReceipt, RecoverySnapshot, RecoveryState, RootRecovery, TaskRecovery,
};
use guffbolt::Message;

/// A task snapshot, the receipt that commits it, and its history's digest.
fn fixture() -> crate::Result<(RecoverySnapshot, RecoveryReceipt, String)> {
    let mut task = super::recovery_tests::task();
    task.checkpoint_generation = Some(7);
    let task = TaskRecovery::capture(
        task,
        "model",
        "provider",
        RootRecovery {
            messages: vec![Message::User("accepted input".to_owned())],
            ..RootRecovery::default()
        },
    )?;
    let digest = af_core::etag::sha256_hex(&task.history);
    let snapshot = RecoverySnapshot {
        run_id: "019f0000-0000-7000-8000-000000000010".to_owned(),
        turn_id: "019f0000-0000-7000-8000-000000000011".to_owned(),
        fencing_token: 9,
        file_permit: None,
        state: RecoveryState::Task(task),
    };
    let valid = RecoveryReceipt {
        run_id: snapshot.run_id.clone(),
        turn_id: snapshot.turn_id.clone(),
        fencing_token: snapshot.fencing_token,
        checkpoint: CommittedRecovery::Task {
            checkpoint_id: "019f0000-0000-7000-8000-000000000012".to_owned(),
            generation: 8,
            history_digest: digest.clone(),
        },
    };
    Ok((snapshot, valid, digest))
}

#[test]
fn committed_receipt_requires_the_exact_attempt() -> crate::Result<()> {
    let (snapshot, valid, _digest) = fixture()?;
    let expected = snapshot.witness();
    assert!(valid.matches(&expected));
    for invalid in [
        RecoveryReceipt {
            run_id: "other".to_owned(),
            ..valid.clone()
        },
        RecoveryReceipt {
            turn_id: "other".to_owned(),
            ..valid.clone()
        },
        RecoveryReceipt {
            fencing_token: 8,
            ..valid
        },
    ] {
        assert!(!invalid.matches(&expected));
    }
    Ok(())
}

#[test]
fn committed_receipt_requires_new_identity_generation_and_exact_history() -> crate::Result<()> {
    let (snapshot, valid, history_digest) = fixture()?;
    let expected = snapshot.witness();
    for (id, generation, digest) in [
        (
            "019f0000-0000-7000-8000-000000000012",
            7,
            history_digest.clone(),
        ),
        ("missing", 8, history_digest),
        ("019f0000-0000-7000-8000-000000000012", 8, "0".repeat(64)),
    ] {
        let invalid = RecoveryReceipt {
            checkpoint: CommittedRecovery::Task {
                checkpoint_id: id.to_owned(),
                generation,
                history_digest: digest,
            },
            ..valid.clone()
        };
        assert!(!invalid.matches(&expected));
    }
    Ok(())
}

#[test]
fn an_ordinary_receipt_never_answers_a_task_snapshot() -> crate::Result<()> {
    use af_wire::run::{CheckpointOperationId, TurnResume, TurnScope, TurnWorkspace};

    let (snapshot, valid, digest) = fixture()?;
    let expected = snapshot.witness();
    let id = |number: u32| CheckpointOperationId(format!("019f0000-0000-7000-8000-{number:012x}"));
    let ordinary = RecoveryReceipt {
        checkpoint: CommittedRecovery::Turn(TurnResume {
            checkpoint_id: id(0x12),
            generation: 8,
            format_version: 1,
            scope: TurnScope {
                workspace_id: id(1),
                fleet_id: id(2),
                turn_id: id(0x11),
                setup_revision_id: id(4),
                workspace: TurnWorkspace::ApiOnly,
            },
            manifest_root: af_core::etag::sha256_hex(b"manifest"),
            history_digest: digest,
        }),
        ..valid.clone()
    };

    assert!(valid.matches(&expected));
    assert!(!ordinary.matches(&expected));
    Ok(())
}
