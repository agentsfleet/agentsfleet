//! A pin is custody only for the run that holds it, at the version it last
//! saw, within its counter and reader bounds; file verbs also need the capture
//! state their purpose names.

use af_core::{error_code, error_code::ErrorCode, id::Uuid7};
use af_wire::{
    run::{
        CheckpointOperationId, TurnCheckpointAction as Action, TurnCheckpointRef,
        TurnCheckpointResponse as Response, TurnResume, TurnWorkspace,
    },
    task::CheckpointEntry,
};
use afd_db::test_util::mint_id;

use super::{Checked, manifest::Pending, publication::command, storage::Files, valid};

/// The fleet code a refused checkpoint call answered with.
pub(super) fn code<T>(result: Checked<T>) -> Option<ErrorCode> {
    let failure = result.err()?;
    failure
        .downcast_ref::<afd_fleet::Error>()
        .map(afd_fleet::Error::code)
}

fn files_mode() -> TurnWorkspace {
    TurnWorkspace::Files {
        toolbox_digest: "a".repeat(64),
    }
}

/// A capture of `workspace` committed on the head `expected` names, every
/// page written empty.
pub(super) async fn committed(
    files: &Files,
    workspace: TurnWorkspace,
    expected: Option<CheckpointOperationId>,
) -> Checked<(Pending, Box<TurnResume>)> {
    let pages: Vec<Vec<CheckpointEntry>> = match &workspace {
        TurnWorkspace::ApiOnly => Vec::new(),
        TurnWorkspace::Files { .. } => vec![Vec::new()],
    };
    let pending = Pending::allocate(files, &pages, workspace, expected).await?;
    let mut digests = Vec::new();
    for (number, id) in pending.plan.page_ids.iter().enumerate() {
        digests.push(
            pending
                .write(files, id.clone(), u32::try_from(number)?, Vec::new())
                .await?,
        );
    }
    let Response::Committed(saved) = pending.commit(files, &digests).await? else {
        return Err("committed generation".into());
    };
    Ok((pending, saved))
}

/// The reader pin a Read of `descriptor` grants.
pub(super) async fn reader(files: &Files, descriptor: &TurnResume) -> Checked<TurnCheckpointRef> {
    let Response::Read(read) = files
        .execute(Action::Read {
            descriptor: Box::new(descriptor.clone()),
        })
        .await?
    else {
        return Err("reader pin".into());
    };
    Ok(TurnCheckpointRef {
        checkpoint_id: descriptor.checkpoint_id.clone(),
        pin_id: CheckpointOperationId(read.pin.id),
    })
}

async fn release(files: &Files, checkpoint: TurnCheckpointRef) -> Checked {
    assert!(matches!(
        files.execute(Action::PinRelease { checkpoint }).await?,
        Response::PinReleased
    ));
    Ok(())
}

// Value: protects=a pin update names a pin that exists and the version its
// holder last saw; fails_when=the missing-pin or skipped-version refusal is
// dropped; why_new=both arms had no case; seam=the production PinRenew over
// live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_pin_update_naming_an_unknown_pin_or_a_skipped_version_conflicts() -> Checked {
    let files = Files::new().await?;
    let pending = Pending::allocate(&files, &[], TurnWorkspace::ApiOnly, None).await?;
    let version = pending.plan.pin.resource_version;
    let unknown = TurnCheckpointRef {
        pin_id: CheckpointOperationId(mint_id().as_str().to_owned()),
        ..pending.reference.clone()
    };
    assert_eq!(
        code(
            files
                .execute(Action::PinRenew {
                    checkpoint: unknown,
                    resource_version: version,
                })
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(
        code(
            files
                .execute(Action::PinRenew {
                    checkpoint: pending.reference.clone(),
                    resource_version: version + 2,
                })
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    let Response::PinRenewed(renewed) = files
        .execute(Action::PinRenew {
            checkpoint: pending.reference,
            resource_version: version,
        })
        .await?
    else {
        return Err("the holder's own version renews".into());
    };
    assert_eq!(renewed.resource_version, version + 1);
    files.finish().await;
    Ok(())
}

// Value: protects=a pin's renewal counter never wraps; fails_when=the renewal
// statement's counter ceiling is dropped or its empty answer stops being a
// limit; why_new=the exhausted counter had no case; seam=the production
// PinRenew over live Postgres, the pin's counter at its column's ceiling.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_pin_at_its_last_resource_version_cannot_renew() -> Checked {
    let files = Files::new().await?;
    let pending = Pending::allocate(&files, &[], TurnWorkspace::ApiOnly, None).await?;
    let mut connection = files.case.fixtures.database.acquire().await?;
    sqlx::query("UPDATE runner.turn_checkpoint_pins SET resource_version=$2 WHERE id=$1")
        .bind(Uuid7::parse(&pending.plan.pin.id)?)
        .bind(i64::MAX)
        .execute(&mut *connection)
        .await?;
    drop(connection);
    assert_eq!(
        code(
            files
                .execute(Action::PinRenew {
                    checkpoint: pending.reference,
                    resource_version: i64::MAX.unsigned_abs(),
                })
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_LIMIT)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=a replacement run never inherits the custody its predecessor
// held; fails_when=a pin update stops proving the pin belongs to the current
// run and fence; why_new=the foreign-owner refusal had no case; seam=the
// production reassignment, then PinRenew of the old writer pin.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_pin_of_the_replaced_run_is_refused() -> Checked {
    let mut files = Files::new().await?;
    let pending = Pending::allocate(&files, &[], TurnWorkspace::ApiOnly, None).await?;
    files.case.replace().await?;
    assert_eq!(
        code(
            files
                .execute(Action::PinRenew {
                    checkpoint: pending.reference,
                    resource_version: pending.plan.pin.resource_version,
                })
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=a committed head is never abandoned; fails_when=Abandon drops
// its ready check; why_new=the committed-abandon refusal had no case; seam=the
// production Abandon after the production Commit.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_committed_capture_cannot_be_abandoned() -> Checked {
    let files = Files::new().await?;
    let (pending, saved) = committed(&files, TurnWorkspace::ApiOnly, None).await?;
    assert_eq!(
        code(
            files
                .execute(Action::Abandon {
                    checkpoint: pending.reference,
                })
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    assert_eq!(
        files.case.counts().await?.3,
        Some(Uuid7::parse(&saved.checkpoint_id.0)?)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=a lost Read acknowledgement returns the same reader pin, and
// one generation never holds more than 64 reader operations; fails_when=a
// replayed Read mints a second pin, or the reader bound is dropped;
// why_new=reader replay and the reader bound had no case; seam=the production
// Read and PinRelease over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_read_replays_its_pin_and_a_generation_admits_sixty_four_readers() -> Checked {
    let files = Files::new().await?;
    let (_, saved) = committed(&files, TurnWorkspace::ApiOnly, None).await?;
    let read = valid(command(
        &files.case,
        Action::Read {
            descriptor: saved.clone(),
        },
    ))?;
    let mut pins = Vec::new();
    for _ in 0..2 {
        let Response::Read(answer) = files
            .store
            .execute(&files.case.runs, files.case.run(), &read)
            .await?
        else {
            return Err("reader pin".into());
        };
        pins.push(answer.pin);
    }
    let [first, replayed] = pins.as_slice() else {
        return Err("two reads".into());
    };
    assert_eq!(
        (&replayed.id, replayed.resource_version, replayed.expires_at),
        (&first.id, first.resource_version, first.expires_at),
        "a replayed Read keeps its pin"
    );
    let held = TurnCheckpointRef {
        checkpoint_id: saved.checkpoint_id.clone(),
        pin_id: CheckpointOperationId(first.id.clone()),
    };
    release(&files, held).await?;
    for _ in 1..64 {
        let next = reader(&files, &saved).await?;
        release(&files, next).await?;
    }
    assert_eq!(
        code(reader(&files, &saved).await),
        Some(error_code::RUN_CHECKPOINT_LIMIT)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=file verbs never run against an API-only capture;
// fails_when=file custody drops its workspace check; why_new=the API-only
// refusal had no case; seam=the production PageRead over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn file_verbs_refuse_an_api_only_capture() -> Checked {
    let files = Files::new().await?;
    let pending = Pending::allocate(&files, &[], TurnWorkspace::ApiOnly, None).await?;
    assert_eq!(
        code(
            files
                .execute(Action::PageRead {
                    checkpoint: pending.reference,
                    page_id: CheckpointOperationId(mint_id().as_str().to_owned()),
                })
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=a committed generation's pages are immutable;
// fails_when=upload custody drops its ready check; why_new=the committed-write
// refusal had no case; seam=the production PageWrite after the production
// Commit, with the writer's own pin.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_committed_file_capture_takes_no_page_write() -> Checked {
    let files = Files::new().await?;
    let (pending, _) = committed(&files, files_mode(), None).await?;
    let page_id = pending.plan.page_ids.first().ok_or("page")?.clone();
    assert_eq!(
        code(pending.write(&files, page_id, 0, Vec::new()).await),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=a restorer reads pages only of the current head;
// fails_when=restore custody drops its head check; why_new=the replaced-head
// refusal had no case; seam=the production PageRead with a reader pin of a
// generation the next Commit replaced.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_reader_of_a_replaced_head_reads_none_of_its_pages() -> Checked {
    let files = Files::new().await?;
    let (first, saved) = committed(&files, files_mode(), None).await?;
    let held = reader(&files, &saved).await?;
    committed(&files, files_mode(), Some(saved.checkpoint_id.clone())).await?;
    assert_eq!(
        code(
            files
                .execute(Action::PageRead {
                    checkpoint: held,
                    page_id: first.plan.page_ids.first().ok_or("page")?.clone(),
                })
                .await
        ),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    files.finish().await;
    Ok(())
}

// Value: protects=a writer that released its pin writes nothing more;
// fails_when=file custody stops proving the pin live; why_new=the released-pin
// refusal had no case; seam=the production PageWrite after the production
// PinRelease.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_released_writer_writes_no_page() -> Checked {
    let files = Files::new().await?;
    let pending = Pending::new(&files, &[Vec::new()]).await?;
    release(&files, pending.reference.clone()).await?;
    let page_id = pending.plan.page_ids.first().ok_or("page")?.clone();
    assert_eq!(
        code(pending.write(&files, page_id, 0, Vec::new()).await),
        Some(error_code::RUN_CHECKPOINT_CONFLICT)
    );
    files.finish().await;
    Ok(())
}
