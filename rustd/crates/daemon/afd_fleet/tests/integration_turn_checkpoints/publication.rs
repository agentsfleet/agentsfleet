//! API-only publication needs encrypted PostgreSQL state and no file-storage backend.

use std::sync::Arc;

use af_core::id::Uuid7;
use af_crypto::secret::Kek;
use af_wire::run::{
    TurnCheckpointAction, TurnCheckpointCommand, TurnCheckpointRef, TurnCheckpointResponse, TurnId,
    TurnPrivateRoot, TurnResume, TurnScope, TurnWorkspace,
};
use afd_fleet::run::turn_checkpoint::TurnCheckpoints;
use serde_json::json;

use super::{Case, Checked, valid};

pub(super) fn root(case: &Case, workspace: TurnWorkspace) -> TurnPrivateRoot {
    let scope = TurnScope {
        workspace_id: TurnId(case.acquired.workspace_id.as_str().to_owned()),
        fleet_id: TurnId(case.acquired.fleet_id.as_str().to_owned()),
        event_id: TurnId(case.acquired.event_id.as_str().to_owned()),
        setup_revision_id: TurnId(case.setup.as_str().to_owned()),
        workspace,
    };
    TurnPrivateRoot {
        history: json!({"format":1,"scope":scope,"model":crate::seed::MODEL,
        "provider":crate::seed::PROVIDER,"root":{"messages":[{"User":"accepted-prefix"}],
        "driver":{"answers":["accepted-prefix"],"completed_turns":1},
        "children":{"next":0,"started":0,"children":[]},
        "memory":{"pending":[],"superseded":[],"misses":0},"pinned":0,"folds":[]}})
        .to_string(),
    }
}

pub(super) fn command(case: &Case, action: TurnCheckpointAction) -> TurnCheckpointCommand {
    TurnCheckpointCommand {
        fencing_token: case.acquired.fence.as_u64(),
        operation_id: TurnId(afd_db::test_util::mint_id().as_str().to_owned()),
        action,
    }
}

pub(super) async fn publish(
    case: &Case,
    store: &TurnCheckpoints,
    root: &TurnPrivateRoot,
) -> Checked<Box<TurnResume>> {
    let mut begin = case.command();
    if let TurnCheckpointAction::Begin(input) = &mut begin.action {
        input.history_digest = af_core::etag::sha256_hex(root.history.as_bytes());
    }
    let plan = case.begin(begin).await?;
    let checkpoint = TurnCheckpointRef {
        checkpoint_id: plan.checkpoint_id.clone(),
        pin_id: TurnId(plan.pin.id.clone()),
    };
    let commit = valid(command(
        case,
        TurnCheckpointAction::Commit {
            checkpoint,
            root: root.clone(),
            manifest_root: af_crypto::checkpoint_digest::manifest_root(std::iter::empty::<(
                &str,
                &str,
            )>()),
            folds: Vec::new(),
        },
    ))?;
    let TurnCheckpointResponse::Committed(descriptor) =
        store.execute(&case.runs, case.run(), &commit).await?
    else {
        return Err("publication must acknowledge a committed generation".into());
    };
    let TurnCheckpointResponse::Committed(replay) =
        store.execute(&case.runs, case.run(), &commit).await?
    else {
        return Err("lost acknowledgement must return the same committed generation".into());
    };
    assert_eq!(replay, descriptor);
    Ok(descriptor)
}

async fn assert_sealed(case: &Case, root: &TurnPrivateRoot, descriptor: &TurnResume) -> Checked {
    let mut connection = case.fixtures.database.acquire().await?;
    let (head, ready, sealed): (Uuid7, bool, Vec<u8>) = sqlx::query_as("SELECT h.head_turn_checkpoint_id,p.ready_at IS NOT NULL,p.root_secret_ciphertext
        FROM runner.turn_recovery h JOIN runner.turn_checkpoints p ON p.id=h.head_turn_checkpoint_id WHERE h.event_id=$1")
        .bind(&case.acquired.event_id).fetch_one(&mut *connection).await?;
    assert_eq!(head.as_str(), descriptor.checkpoint_id.0);
    assert!(ready);
    assert_ne!(sealed.as_slice(), root.history.as_bytes());
    Ok(())
}

async fn read_and_adopt(
    case: &Case,
    store: &TurnCheckpoints,
    root: &TurnPrivateRoot,
    descriptor: &TurnResume,
) -> Checked {
    let read = valid(command(
        case,
        TurnCheckpointAction::Read {
            descriptor: Box::new(descriptor.clone()),
        },
    ))?;
    let TurnCheckpointResponse::Read(saved) = store.execute(&case.runs, case.run(), &read).await?
    else {
        return Err("reader must return the exact committed root".into());
    };
    assert_eq!(saved.descriptor, *descriptor);
    assert_eq!(saved.root.history, root.history);
    assert_eq!(saved.folds, Vec::new());
    let reference = TurnCheckpointRef {
        checkpoint_id: descriptor.checkpoint_id.clone(),
        pin_id: TurnId(saved.pin.id),
    };
    let adopt = valid(command(
        case,
        TurnCheckpointAction::Adopt {
            checkpoint: reference.clone(),
        },
    ))?;
    assert!(
        matches!(store.execute(&case.runs, case.run(), &adopt).await?, TurnCheckpointResponse::Adopted(covered) if covered.is_empty())
    );
    release_and_refuse_replayed_read(case, store, reference, read).await
}

async fn release_and_refuse_replayed_read(
    case: &Case,
    store: &TurnCheckpoints,
    reference: TurnCheckpointRef,
    read: garde::Valid<TurnCheckpointCommand>,
) -> Checked {
    let release = valid(command(
        case,
        TurnCheckpointAction::PinRelease {
            checkpoint: reference,
        },
    ))?;
    assert!(matches!(
        store.execute(&case.runs, case.run(), &release).await?,
        TurnCheckpointResponse::PinReleased
    ));
    assert!(
        store
            .execute(&case.runs, case.run(), &read)
            .await
            .is_err_and(|fault| fault.code() == af_core::error_code::RUN_CHECKPOINT_CONFLICT)
    );
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn api_only_publication_is_sealed_atomic_replayable_and_readable_without_file_storage()
-> Checked {
    let case = Case::new().await?;
    let root = root(&case, TurnWorkspace::ApiOnly);
    let store = TurnCheckpoints::new(Arc::new(Kek::from_hex(&hex::encode([17_u8; 32]))?));
    let descriptor = publish(&case, &store, &root).await?;
    assert_sealed(&case, &root, &descriptor).await?;
    read_and_adopt(&case, &store, &root, &descriptor).await
}
