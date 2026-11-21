//! Both workspace modes retain exact generations and refuse expired or foreign custody.

use super::{Case, Checked, manifest::Pending, publication::command, storage::Files, valid};
use crate::support::Fixtures;
use af_core::{error_code, id::Uuid7};
use af_wire::{
    run::{
        CheckpointOperationId, TurnCheckpointAction as Action, TurnCheckpointRef,
        TurnCheckpointResponse as Response, TurnResume, TurnWorkspace,
    },
    task::CheckpointEntry,
};
use afd_fleet::{
    run::turn_checkpoint::TurnRun,
    task::{CheckpointMaintenanceCursor, TaskLifecycle as _},
};

fn modes() -> [TurnWorkspace; 2] {
    [
        TurnWorkspace::ApiOnly,
        TurnWorkspace::Files {
            toolbox_digest: "a".repeat(64),
        },
    ]
}

pub(super) async fn isolated() -> Checked<Files> {
    Files::for_case(Case::with_fixtures(Fixtures::create_isolated().await).await?).await
}

async fn generation(
    files: &Files,
    workspace: TurnWorkspace,
    expected: Option<CheckpointOperationId>,
) -> Checked<(Pending, Vec<(String, String)>)> {
    let pages: Vec<Vec<CheckpointEntry>> = match &workspace {
        TurnWorkspace::ApiOnly => Vec::new(),
        TurnWorkspace::Files { .. } => vec![Vec::new()],
    };
    let pending = Pending::allocate(files, &pages, workspace, expected).await?;
    let mut digests = Vec::new();
    for (number, (id, entries)) in pending.plan.page_ids.iter().zip(pages).enumerate() {
        digests.push(
            pending
                .write(files, id.clone(), u32::try_from(number)?, entries)
                .await?,
        );
    }
    Ok((pending, digests))
}

async fn publish(
    files: &Files,
    workspace: TurnWorkspace,
    expected: Option<CheckpointOperationId>,
) -> Checked<(Pending, Box<TurnResume>)> {
    let (pending, digests) = generation(files, workspace, expected).await?;
    let Response::Committed(saved) = pending.commit(files, &digests).await? else {
        return Err("committed generation".into());
    };
    Ok((pending, saved))
}

fn refused(result: Checked<Response>, code: error_code::ErrorCode) -> Checked {
    let Err(failure) = result else {
        return Err("custody or authority must refuse".into());
    };
    assert!(
        failure
            .downcast_ref::<afd_fleet::Error>()
            .is_some_and(|failure| failure.code() == code),
        "{failure}"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn both_workspace_modes_bound_writer_custody_and_preserve_retired_allocation_identity()
-> Checked {
    for workspace in modes() {
        let files = isolated().await?;
        let (pending, _) = generation(&files, workspace.clone(), None).await?;
        renew(&files, &pending.reference, &pending.plan.pin).await?;
        assert!(matches!(
            files
                .execute(Action::PinRelease {
                    checkpoint: pending.reference.clone()
                })
                .await?,
            Response::PinReleased
        ));
        for _ in 0..2 {
            assert!(matches!(
                files
                    .execute(Action::Abandon {
                        checkpoint: pending.reference.clone()
                    })
                    .await?,
                Response::Abandoned
            ));
        }
        let (expired, _) = generation(&files, workspace.clone(), None).await?;
        expire_pending(&files, &expired).await?;
        refused(
            files
                .execute(Action::PinRenew {
                    checkpoint: expired.reference,
                    resource_version: expired.plan.pin.resource_version,
                })
                .await,
            error_code::RUN_CHECKPOINT_CONFLICT,
        )?;
        let result = files
            .case
            .tasks()
            .maintain_checkpoints(&mut CheckpointMaintenanceCursor::default())
            .await?;
        assert_eq!(result.owners, 2);
        assert_eq!(files.case.counts().await?, (0, 2, 3, None));
        let (next, _) = generation(&files, workspace, None).await?;
        assert_eq!(next.plan.generation, 3);
        files.finish().await;
    }
    Ok(())
}

async fn renew(
    files: &Files,
    reference: &TurnCheckpointRef,
    original: &af_wire::task::CheckpointPin,
) -> Checked {
    let action = Action::PinRenew {
        checkpoint: reference.clone(),
        resource_version: original.resource_version,
    };
    let Response::PinRenewed(pin) = files.execute(action.clone()).await? else {
        return Err("renewed pin".into());
    };
    assert_eq!(pin.id, original.id);
    assert_eq!(pin.resource_version, original.resource_version + 1);
    assert_eq!(pin.operation_deadline, original.operation_deadline);
    assert!(pin.expires_at <= original.operation_deadline);
    let Response::PinRenewed(replay) = files.execute(action).await? else {
        return Err("replayed pin".into());
    };
    assert_eq!(replay.resource_version, pin.resource_version);
    assert_eq!(replay.expires_at, pin.expires_at);
    Ok(())
}

async fn expire_pending(files: &Files, pending: &Pending) -> Checked {
    let mut connection = files.case.fixtures.database.acquire().await?;
    sqlx::query("UPDATE runner.turn_checkpoint_pins SET expires_at=clock_timestamp()-interval '1 second',operation_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(Uuid7::parse(&pending.plan.pin.id)?).execute(&mut *connection).await?;
    sqlx::query("UPDATE runner.turn_checkpoints SET expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(Uuid7::parse(&pending.plan.checkpoint_id.0)?).execute(&mut *connection).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn ready_retention_preserves_old_readers_until_release_or_expiry() -> Checked {
    for workspace in modes() {
        for expire in [false, true] {
            reader_retention(workspace.clone(), expire).await?;
        }
    }
    Ok(())
}

async fn reader_retention(workspace: TurnWorkspace, expire: bool) -> Checked {
    let files = isolated().await?;
    let (_, first) = publish(&files, workspace.clone(), None).await?;
    let Response::Read(read) = files
        .execute(Action::Read {
            descriptor: first.clone(),
        })
        .await?
    else {
        return Err("pinned read".into());
    };
    let reference = TurnCheckpointRef {
        checkpoint_id: first.checkpoint_id.clone(),
        pin_id: CheckpointOperationId(read.pin.id.clone()),
    };
    renew(&files, &reference, &read.pin).await?;
    let (_, second) = publish(&files, workspace.clone(), Some(first.checkpoint_id.clone())).await?;
    let blocked = generation(
        &files,
        workspace.clone(),
        Some(second.checkpoint_id.clone()),
    )
    .await;
    assert!(blocked.is_err_and(|failure| {
        failure
            .downcast_ref::<afd_fleet::Error>()
            .is_some_and(|failure| failure.code() == error_code::RUN_CHECKPOINT_LIMIT)
    }));
    end_reader(&files, reference, &read.pin, expire).await?;
    let (_, third) = publish(&files, workspace, Some(second.checkpoint_id.clone())).await?;
    assert_eq!(third.generation, 3);
    assert_eq!(files.case.counts().await?.0, 2);
    assert_eq!(
        files.case.counts().await?.3,
        Some(Uuid7::parse(&third.checkpoint_id.0)?)
    );
    refused(
        files.execute(Action::Read { descriptor: first }).await,
        error_code::RUN_CHECKPOINT_CONFLICT,
    )?;
    files.finish().await;
    Ok(())
}

async fn end_reader(
    files: &Files,
    reference: TurnCheckpointRef,
    original: &af_wire::task::CheckpointPin,
    expire: bool,
) -> Checked {
    if expire {
        let mut connection = files.case.fixtures.database.acquire().await?;
        sqlx::query("UPDATE runner.turn_checkpoint_pins SET expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
            .bind(Uuid7::parse(&original.id)?).execute(&mut *connection).await?;
        drop(connection);
        refused(
            files
                .execute(Action::PinRenew {
                    checkpoint: reference,
                    resource_version: original.resource_version + 1,
                })
                .await,
            error_code::RUN_CHECKPOINT_CONFLICT,
        )?;
    } else {
        for _ in 0..2 {
            assert!(matches!(
                files
                    .execute(Action::PinRelease {
                        checkpoint: reference.clone()
                    })
                    .await?,
                Response::PinReleased
            ));
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn commit_read_and_adopt_refuse_foreign_fences_and_expired_runs() -> Checked {
    for workspace in modes() {
        let files = isolated().await?;
        let (pending, saved) = publish(&files, workspace, None).await?;
        let Response::Read(read) = files
            .execute(Action::Read {
                descriptor: saved.clone(),
            })
            .await?
        else {
            return Err("read pin".into());
        };
        let actions = [
            Action::Commit {
                checkpoint: pending.reference,
                root: pending.root,
                manifest_root: saved.manifest_root.clone(),
                folds: Vec::new(),
            },
            Action::Read {
                descriptor: saved.clone(),
            },
            Action::Adopt {
                checkpoint: TurnCheckpointRef {
                    checkpoint_id: saved.checkpoint_id.clone(),
                    pin_id: CheckpointOperationId(read.pin.id),
                },
            },
        ];
        for action in &actions {
            foreign(&files, action.clone()).await?;
        }
        let mut connection = files.case.fixtures.database.acquire().await?;
        sqlx::query("UPDATE thread.runs SET run_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
            .bind(&files.case.issued.run_id).execute(&mut *connection).await?;
        drop(connection);
        for action in actions {
            refused(files.execute(action).await, error_code::RUN_LOST)?;
        }
        assert_eq!(
            files.case.counts().await?.3,
            Some(Uuid7::parse(&saved.checkpoint_id.0)?)
        );
        files.finish().await;
    }
    Ok(())
}

async fn foreign(files: &Files, action: Action) -> Checked {
    let request = valid(command(&files.case, action.clone()))?;
    let presented = TurnRun {
        runner_id: &files.case.foreign,
        ..files.case.run()
    };
    let result = files
        .store
        .execute(&files.case.runs, presented, &request)
        .await
        .map_err(Into::into);
    refused(result, error_code::RUN_NOT_FOUND)?;
    let mut request = command(&files.case, action);
    request.fencing_token += 1;
    let presented = TurnRun {
        fencing_token: request.fencing_token,
        ..files.case.run()
    };
    let result = files
        .store
        .execute(&files.case.runs, presented, &valid(request)?)
        .await
        .map_err(Into::into);
    refused(result, error_code::RUN_STALE_FENCING_TOKEN)
}
