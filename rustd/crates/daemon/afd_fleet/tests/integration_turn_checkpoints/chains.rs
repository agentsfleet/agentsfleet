//! A delta is applied over the head it was walked against: its chain is read
//! back in order, its totals are the resolved tree's, and Postgres keeps every
//! ancestor a remaining delta names.
//!
//! ```text
//!   full {a, b} ◄─ delta {Remove b, c}      resolves to {a, c}
//! ```

use super::{Checked, lifecycle::isolated, manifest::Pending, publication::root, storage::Files};
use af_core::{error_code, id::Uuid7};
use af_wire::{
    durable_task::{CheckpointEntry, CheckpointLink},
    run::{
        CheckpointOperationId, TurnCheckpointAbandon, TurnCheckpointAction as Action,
        TurnCheckpointPageRead, TurnCheckpointRead, TurnCheckpointRef,
        TurnCheckpointResponse as Response, TurnResume, TurnWorkspace,
    },
};
use afd_fleet::durable_task::{CheckpointMaintenanceCursor, DurableTaskLifecycle as _};

/// The key a delta names its parent by.
const PARENT_KEY: &str = "fk_checkpoints_parent";

fn directory(path: &str) -> CheckpointEntry {
    CheckpointEntry::Directory {
        path: path.to_owned(),
    }
}

fn remove(path: &str) -> CheckpointEntry {
    CheckpointEntry::Remove {
        path: path.to_owned(),
    }
}

/// A file workspace's pending plan over `head`, a delta against `parent`,
/// whose one page resolves to `entries` entries.
async fn allocate(
    files: &Files,
    head: Option<&TurnResume>,
    parent: Option<&TurnResume>,
    entries: u32,
) -> Checked<Pending> {
    let workspace = TurnWorkspace::Files {
        toolbox_digest: "a".repeat(64),
    };
    let history = root(&files.case, workspace.clone());
    let mut command = files.case.command();
    let Action::Begin(begin) = &mut command.action else {
        return Err("begin command".into());
    };
    begin.workspace = workspace;
    begin.expected_head = head.map(|head| head.checkpoint_id.clone());
    begin.parent = parent.map(|parent| parent.checkpoint_id.clone());
    begin.history_digest = af_core::etag::sha256_hex(history.history.as_bytes());
    begin.page_count = 1;
    begin.entry_count = entries;
    let Response::Begun(plan) = files.execute(command.action).await? else {
        return Err("allocated plan".into());
    };
    Ok(Pending {
        reference: TurnCheckpointRef {
            checkpoint_id: plan.checkpoint_id.clone(),
            pin_id: CheckpointOperationId(plan.pin.id.clone()),
        },
        plan: *plan,
        root: history,
    })
}

/// Writes `pending`'s one page of `entries` and commits it.
async fn commit(
    files: &Files,
    pending: &Pending,
    entries: Vec<CheckpointEntry>,
) -> Checked<Response> {
    let page = pending.plan.page_ids.first().ok_or("page")?.clone();
    let written = pending.write(files, page, 0, entries).await?;
    pending.commit(files, &[written]).await
}

/// One committed generation: `entries` over `head`, a delta when `delta`.
async fn generation(
    files: &Files,
    head: Option<&TurnResume>,
    delta: bool,
    (entries, resolved): (Vec<CheckpointEntry>, u32),
) -> Checked<TurnResume> {
    let parent = head.filter(|_| delta);
    let pending = allocate(files, head, parent, resolved).await?;
    let Response::Committed(saved) = commit(files, &pending, entries).await? else {
        return Err("committed generation".into());
    };
    Ok(*saved)
}

/// A full generation of `a` and `b`, then a delta over it that removes `b`
/// and adds `c`.
async fn chain(files: &Files) -> Checked<(TurnResume, TurnResume)> {
    let full = generation(
        files,
        None,
        false,
        (vec![directory("a"), directory("b")], 2),
    )
    .await?;
    let delta = (vec![remove("b"), directory("c")], 2);
    let head = generation(files, Some(&full), true, delta).await?;
    Ok((full, head))
}

fn refused(result: Checked<Response>, code: error_code::ErrorCode) -> Checked {
    let Err(failure) = result else {
        return Err("a refused plan was answered".into());
    };
    assert!(
        failure
            .downcast_ref::<afd_fleet::Error>()
            .is_some_and(|failure| failure.code() == code),
        "{failure}"
    );
    Ok(())
}

async fn rows(files: &Files) -> Checked<Vec<Uuid7>> {
    let mut connection = files.case.fixtures.database.acquire().await?;
    Ok(
        sqlx::query_scalar(
            "SELECT id FROM runner.checkpoints WHERE turn_id=$1 ORDER BY generation",
        )
        .bind(&files.case.acquired.turn_id)
        .fetch_all(&mut *connection)
        .await?,
    )
}

fn id(resume: &TurnResume) -> Checked<Uuid7> {
    Ok(Uuid7::parse(&resume.checkpoint_id.0)?)
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_delta_names_only_the_head_and_commits_its_resolved_totals() -> Checked {
    let files = Files::new().await?;
    let (full, head) = chain(&files).await?;
    let other = allocate(&files, Some(&head), Some(&full), 2).await;
    refused(
        other.map(|_| Response::Abandoned),
        error_code::RUN_CHECKPOINT_INCOMPATIBLE,
    )?;
    // Five entries is what the chain's pages list, not the three they
    // resolve to.
    let pending = allocate(&files, Some(&head), Some(&head), 5).await?;
    refused(
        commit(&files, &pending, vec![directory("d")]).await,
        error_code::RUN_CHECKPOINT_INCOMPATIBLE,
    )?;
    files.finish().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_restore_reads_the_chain_full_manifest_first() -> Checked {
    let files = Files::new().await?;
    let (full, middle) = chain(&files).await?;
    let head = generation(&files, Some(&middle), true, (vec![directory("d")], 3)).await?;
    let Response::Read(saved) = files
        .execute(Action::Read(TurnCheckpointRead {
            descriptor: Box::new(head.clone()),
        }))
        .await?
    else {
        return Err("reader custody".into());
    };
    let links: Vec<_> = saved
        .chain
        .iter()
        .map(|link| link.checkpoint_id.as_str())
        .collect();
    assert_eq!(
        links,
        [
            full.checkpoint_id.0.as_str(),
            middle.checkpoint_id.0.as_str()
        ]
    );
    let CheckpointLink {
        page_ids,
        manifest_root,
        ..
    } = saved.chain.first().ok_or("full link")?;
    assert_eq!(*manifest_root, full.manifest_root);
    let Response::PageRead(page) = files
        .execute(Action::PageRead(TurnCheckpointPageRead {
            checkpoint: TurnCheckpointRef {
                checkpoint_id: head.checkpoint_id.clone(),
                pin_id: CheckpointOperationId(saved.pin.id.clone()),
            },
            page_id: CheckpointOperationId(page_ids.first().ok_or("full page")?.clone()),
        }))
        .await?
    else {
        return Err("an ancestor's page under the head's pin".into());
    };
    assert_eq!(page.entries, [directory("a"), directory("b")]);
    files.finish().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_ancestor_of_a_ready_delta_is_never_purged() -> Checked {
    let files = isolated().await?;
    let (full, head) = chain(&files).await?;
    let mut connection = files.case.fixtures.database.acquire().await?;
    let deleted = sqlx::query("DELETE FROM runner.checkpoints WHERE id=$1")
        .bind(id(&full)?)
        .execute(&mut *connection)
        .await;
    assert!(
        deleted
            .is_err_and(|failure| afd_db::constraint::violates_foreign_key(&failure, PARENT_KEY))
    );
    // The head is pinned by a restore, and an abandoned delta over it makes
    // the settled turn due: purge takes the abandoned plan and skips the
    // ancestor the pinned head names.
    let Response::Read(saved) = files
        .execute(Action::Read(TurnCheckpointRead {
            descriptor: Box::new(head.clone()),
        }))
        .await?
    else {
        return Err("reader custody".into());
    };
    let abandoned = allocate(&files, Some(&head), Some(&head), 2).await?;
    files
        .execute(Action::Abandon(TurnCheckpointAbandon {
            checkpoint: abandoned.reference,
        }))
        .await?;
    sqlx::query(
        "UPDATE thread.turns SET settled_at=clock_timestamp()-interval '8 days' WHERE id=$1",
    )
    .bind(&files.case.acquired.turn_id)
    .execute(&mut *connection)
    .await?;
    drop(connection);
    let tasks = files.case.tasks();
    let mut cursor = CheckpointMaintenanceCursor::default();
    let pass = tasks.maintain_checkpoints(&mut cursor).await?;
    assert_eq!(pass.deferred, 0);
    assert_eq!(rows(&files).await?, [id(&full)?, id(&head)?]);
    // The settled turn's run can no longer release its pin; its custody ends.
    let mut connection = files.case.fixtures.database.acquire().await?;
    sqlx::query("UPDATE runner.checkpoint_pins SET released_at=clock_timestamp() WHERE id=$1")
        .bind(Uuid7::parse(&saved.pin.id)?)
        .execute(&mut *connection)
        .await?;
    drop(connection);
    let pass = tasks.maintain_checkpoints(&mut cursor).await?;
    assert_eq!(pass.deferred, 0);
    assert_eq!(rows(&files).await?, Vec::<Uuid7>::new());
    drop(tasks);
    files.finish().await;
    Ok(())
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_chain_a_full_manifest_superseded_is_retired_newest_first() -> Checked {
    let files = Files::new().await?;
    let (full, middle) = chain(&files).await?;
    let delta = generation(&files, Some(&middle), true, (vec![directory("d")], 3)).await?;
    let compacted = (vec![directory("a"), directory("c"), directory("d")], 3);
    let superseding = generation(&files, Some(&delta), false, compacted).await?;
    assert_eq!(
        rows(&files).await?,
        [id(&full)?, id(&middle)?, id(&delta)?, id(&superseding)?]
    );
    // The next save retires every generation outside the head's chain, a
    // leaf at a time; a parent goes only once its delta has.
    let next = allocate(&files, Some(&superseding), Some(&superseding), 3).await?;
    assert_eq!(
        rows(&files).await?,
        [id(&superseding)?, Uuid7::parse(&next.plan.checkpoint_id.0)?]
    );
    files.finish().await;
    Ok(())
}
