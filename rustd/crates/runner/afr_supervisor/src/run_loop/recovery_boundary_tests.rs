//! One publication runs per owner: while a save publishes, every boundary
//! that arrives skips as busy without pausing.

use std::sync::Arc;

use af_wire::run::{CheckpointOperationId, TurnResume, TurnScope, TurnWorkspace};
use afr_agent::{
    CommittedRecovery, Publication, RecoveryBoundarySink as _, RecoveryReceipt, RecoverySave,
    RecoverySkip, RecoverySnapshot, RecoveryState, TurnSnapshot,
};
use tokio::sync::{Barrier, Semaphore, mpsc};
use zeroize::Zeroizing;

use super::{Job, Sink, Workspace};
use crate::recovery::RecoveryResources;

type Checked<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// The boundaries released together while the first save publishes.
const BOUNDARIES: usize = 50;

fn id(number: u32) -> CheckpointOperationId {
    CheckpointOperationId(format!("019f0000-0000-7000-8000-{number:012x}"))
}

/// An API-only turn boundary of one run.
fn boundary() -> RecoverySnapshot {
    RecoverySnapshot {
        run_id: id(5).0,
        turn_id: id(3).0,
        fencing_token: 1,
        state: RecoveryState::Turn(TurnSnapshot {
            scope: TurnScope {
                workspace_id: id(1),
                fleet_id: id(2),
                turn_id: id(3),
                workspace: TurnWorkspace::ApiOnly,
            },
            previous: None,
            history: Zeroizing::new(b"saved conversation".to_vec()),
            folds: Vec::new(),
        }),
        file_permit: None,
    }
}

/// The receipt a committed save answers with.
fn committed() -> RecoveryReceipt {
    RecoveryReceipt {
        run_id: id(5).0,
        turn_id: id(3).0,
        fencing_token: 1,
        checkpoint: CommittedRecovery::Turn(TurnResume {
            checkpoint_id: id(10),
            generation: 1,
            format_version: af_wire::run::TURN_CHECKPOINT_FORMAT,
            scope: TurnScope {
                workspace_id: id(1),
                fleet_id: id(2),
                turn_id: id(3),
                workspace: TurnWorkspace::ApiOnly,
            },
            manifest_root: af_core::etag::sha256_hex(b"manifest"),
            history_digest: af_core::etag::sha256_hex(b"saved conversation"),
        }),
    }
}

#[tokio::test]
async fn a_boundary_finding_a_publication_running_skips() -> Checked {
    let resources = RecoveryResources::new()?;
    let (jobs, mut paused) = mpsc::channel::<Job>(1);
    let sink = Sink {
        jobs,
        publishing: Arc::new(Semaphore::new(1)),
        workspace: Workspace::ApiOnly,
        resources: &resources,
    };

    // The owner pauses for the first save and answers once its pin is taken;
    // its upload is held, so its publication keeps the owner's one permit.
    let (first, held) = tokio::join!(sink.save(boundary()), async {
        let job = paused
            .recv()
            .await
            .ok_or("the first save never reached its owner")?;
        let (receipt, publication) = Publication::channel();
        job.reply
            .send(Ok(RecoverySave::Publishing(publication)))
            .map_err(|_gone| "the first save stopped waiting")?;
        Checked::Ok((receipt, job.publishing))
    });
    let RecoverySave::Publishing(publication) = first? else {
        return Err("the first save did not resume its run at the pin".into());
    };
    let (receipt, permit) = held?;

    let barrier = Barrier::new(BOUNDARIES);
    let skipped = futures_util::future::join_all((0..BOUNDARIES).map(async |_| {
        barrier.wait().await;
        sink.save(boundary()).await
    }))
    .await;

    for saved in skipped {
        assert!(matches!(
            saved?,
            RecoverySave::Skipped(RecoverySkip::BusyPublication)
        ));
    }
    assert!(paused.try_recv().is_err(), "no boundary paused");
    receipt
        .send(Ok(RecoverySave::Committed(Box::new(committed()))))
        .map_err(|_gone| "the run dropped its publication")?;
    drop(permit);
    let mut commits = 0;
    if let RecoverySave::Committed(_receipt) = publication_settled(publication).await? {
        commits += 1;
    }
    assert_eq!(commits, 1, "exactly one generation committed");
    Ok(())
}

/// The receipt `publication` ends with.
async fn publication_settled(publication: Publication) -> Checked<RecoverySave> {
    Ok(publication.settled().await?)
}
