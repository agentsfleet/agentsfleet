//! A receipt from another attempt or private root never advances recovery state.

use super::{
    CommittedRecovery, RecoveryReceipt, RecoverySnapshot, RecoveryState, RootRecovery, TaskRecovery,
};
use af_providers::Message;

fn fixture() -> crate::Result<(RecoverySnapshot, RecoveryReceipt)> {
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
        lease_id: "019f0000-0000-7000-8000-000000000010".to_owned(),
        event_id: "019f0000-0000-7000-8000-000000000011".to_owned(),
        fencing_token: 9,
        state: RecoveryState::Task(task),
    };
    let valid = RecoveryReceipt {
        lease_id: snapshot.lease_id.clone(),
        event_id: snapshot.event_id.clone(),
        fencing_token: snapshot.fencing_token,
        checkpoint: CommittedRecovery::Task {
            checkpoint_id: "019f0000-0000-7000-8000-000000000012".to_owned(),
            generation: 8,
            history_digest: digest,
        },
    };
    Ok((snapshot, valid))
}

#[test]
fn committed_receipt_requires_the_exact_attempt() -> crate::Result<()> {
    let (snapshot, valid) = fixture()?;
    let expected = snapshot.witness();
    assert!(valid.matches(&expected));
    for invalid in [
        RecoveryReceipt {
            lease_id: "other".to_owned(),
            ..valid.clone()
        },
        RecoveryReceipt {
            event_id: "other".to_owned(),
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
    let (snapshot, valid) = fixture()?;
    let expected = snapshot.witness();
    let CommittedRecovery::Task { history_digest, .. } = &valid.checkpoint else {
        return Err(crate::decision::unavailable());
    };
    for (id, generation, digest) in [
        (
            "019f0000-0000-7000-8000-000000000012",
            7,
            history_digest.clone(),
        ),
        ("missing", 8, history_digest.clone()),
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
