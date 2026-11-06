//! Persist before dispatch; duplicates and restart never manufacture an unsent proof.

use super::*;

fn id(suffix: u8) -> Result<Uuid7> {
    Ok(Uuid7::parse(&format!(
        "019f0000-0000-7000-8000-{suffix:012}"
    ))?)
}

#[tokio::test]
async fn a_duplicate_cannot_release_another_dispatch_and_completed_receipts_keep_the_original_owner()
-> Result<()> {
    let root = tempfile::tempdir()?;
    let spool = ReportSpool {
        dir: root.path().into(),
    };
    let (lease, call) = (id(1)?, id(2)?);
    let pending = spool.begin_decision(&lease, &call, 17).await?;
    let directory = root.path().join(DIRECTORY);
    let receipt = load(&pending.directory, call.as_str())?;
    assert_eq!(receipt.lease_id, lease.as_str());
    assert!(matches!(
        receipt.input.receipt,
        DecisionReceipt::Unknown { .. }
    ));
    assert!(
        scan(&directory)?.pending.is_empty(),
        "an active call cannot be drained online"
    );
    spool
        .begin_decision(&lease, &call, 17)
        .await
        .err()
        .ok_or_else(|| crate::error::config("unsafe operation was accepted"))?;
    assert!(
        pending.directory.join(ACTIVE).exists(),
        "a duplicate owns no active marker"
    );
    let (pending, saved) = pending
        .finish(DecisionReceipt::NotSent {
            cause: afd_wire::task::DecisionNotSentCause::Preflight,
        })
        .await?;
    saved?;
    let bytes = fs::read(pending.directory.join(cipher::CONTENT))?;
    assert!(!String::from_utf8_lossy(&bytes).contains(lease.as_str()));
    let scanned = scan(&directory)?;
    assert_eq!(scanned.pending.len(), 1);
    let recovered = load(&pending.directory, call.as_str())?;
    assert_eq!(recovered.input.fencing_token, 17);
    assert!(matches!(
        recovered.input.receipt,
        DecisionReceipt::NotSent { .. }
    ));
    assert!(
        cipher::read(&pending.directory, id(3)?.as_str()).is_err(),
        "ciphertext is bound to its original call"
    );
    Ok(())
}

#[tokio::test]
async fn startup_exposes_only_the_saved_uncertainty_after_an_interrupted_dispatch() -> Result<()> {
    let root = tempfile::tempdir()?;
    let spool = ReportSpool {
        dir: root.path().into(),
    };
    let call = id(5)?;
    let mut pending = spool.begin_decision(&id(4)?, &call, 23).await?;
    // Model process loss without running Drop's online-release callback.
    pending.active = false;
    drop(pending);
    spool.recover_decisions().await?;
    let recovered = scan(&root.path().join(DIRECTORY))?;
    assert_eq!(recovered.pending.len(), 1);
    for pending in recovered.pending {
        let pending = pending?;
        assert_eq!(pending.input.fencing_token, 23);
        assert!(matches!(
            pending.input.receipt,
            DecisionReceipt::Unknown { .. }
        ));
    }
    assert!(
        spool.begin_decision(&id(4)?, &call, 24).await.is_err(),
        "replacement cannot resend a retained identity"
    );
    Ok(())
}
