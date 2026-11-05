//! Fixed page identities, exact replay and native object/path validation.

use super::{
    guard,
    row::{Pin, Plan},
};
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal, Tasks, execution, guard as task_guard, transaction},
};
use afd_core::id::Uuid7;
use afd_wire::task::{
    CheckpointEntry, CheckpointMode, CheckpointPage, CheckpointPageInput, CheckpointReadQuery,
    CheckpointTransferPurpose,
};
use sqlx::PgConnection;
use std::collections::{BTreeMap, BTreeSet};

#[derive(sqlx::FromRow)]
pub(super) struct Page {
    pub id: String,
    pub page_number: i32,
    pub digest: Option<String>,
    pub body: Option<Vec<u8>>,
}

pub(in crate::task) async fn write(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    checkpoint: &Uuid7,
    page: &Uuid7,
    input: &garde::Valid<CheckpointPageInput>,
) -> Result<CheckpointPage> {
    let body = serde_json::to_vec(&input.contents).map_err(afd_core::json::Unreadable::from)?;
    if afd_core::etag::sha256_hex(&body) != input.digest {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let (_owned, plan, pin) = guard::owned(
        &mut tx,
        lease,
        &input.task,
        checkpoint,
        &task_guard::id(&input.pin_id, "pin_id")?,
        CheckpointTransferPurpose::Upload,
    )
    .await?;
    let position = usize::try_from(input.contents.page_number)
        .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?;
    if plan.page_ids.get(position).map(String::as_str) != Some(page.as_str()) {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    let objects = validate(&mut tx, &plan, &input.contents).await?;
    super::transfer::protect(&mut tx, &objects, &pin).await?;
    let existing = selected(&mut tx, &plan, page.as_str()).await?;
    if let Some(saved) = &existing.body {
        if saved != &body || existing.digest.as_deref() != Some(input.digest.as_str()) {
            return Err(TaskRefusal::ReplayConflict.into());
        }
    } else {
        let statement = match plan.mode()? {
            CheckpointMode::RetainBase => include_str!("sql/base_page_write.sql"),
            CheckpointMode::Incremental => include_str!("sql/page_write.sql"),
        };
        sqlx::query(statement)
            .bind(page.as_str())
            .bind(&input.digest)
            .bind(&body)
            .execute(&mut *tx)
            .await
            .map_err(query("freeze complete manifest page"))?;
    }
    tx.commit()
        .await
        .map_err(query("commit immutable manifest page"))?;
    transaction::recycle(connection);
    Ok(input.contents.clone())
}

pub(in crate::task) async fn read(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    checkpoint: &Uuid7,
    page: &Uuid7,
    input: &garde::Valid<CheckpointReadQuery>,
) -> Result<CheckpointPage> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let (_owned, plan, _pin) = read_authority(&mut tx, lease, checkpoint, &input.pin_id).await?;
    let page = selected(&mut tx, &plan, page.as_str()).await?;
    let body = page.body.ok_or(TaskRefusal::CheckpointIncompatible)?;
    if page.digest.as_deref() != Some(afd_core::etag::sha256_hex(&body).as_str()) {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    let contents = serde_json::from_slice(&body).map_err(afd_core::json::Unreadable::from)?;
    tx.commit()
        .await
        .map_err(query("finish bounded immutable manifest page read"))?;
    transaction::recycle(connection);
    Ok(contents)
}

pub(super) async fn read_authority(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    checkpoint: &Uuid7,
    pin: &str,
) -> Result<(execution::Owned, Plan, Pin)> {
    let current = task_guard::lease(connection, lease).await?;
    let owned = execution::attached(connection, current).await?;
    guard::with_execution(
        connection,
        lease,
        owned,
        checkpoint,
        &task_guard::id(pin, "pin_id")?,
        CheckpointTransferPurpose::Restore,
    )
    .await
}

pub(super) async fn selected(
    connection: &mut PgConnection,
    plan: &Plan,
    page: &str,
) -> Result<Page> {
    sqlx::query_as(include_str!("sql/page_read.sql"))
        .bind(page)
        .bind(&plan.id)
        .bind(&plan.base_id)
        .fetch_optional(connection)
        .await
        .map_err(query("read one scoped manifest page"))?
        .ok_or_else(|| TaskRefusal::NotFound.into())
}

pub(super) async fn validate(
    connection: &mut PgConnection,
    plan: &Plan,
    page: &CheckpointPage,
) -> Result<Vec<String>> {
    if plan.mode()? == CheckpointMode::RetainBase && !page.history_objects.is_empty() {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    let mut previous = None;
    let mut ids: BTreeSet<String> = page.history_objects.iter().cloned().collect();
    for entry in &page.entries {
        if previous.is_some_and(|previous: &str| previous >= entry.path()) {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        previous = Some(entry.path());
        match entry {
            CheckpointEntry::File(file) => {
                ids.extend(file.objects.iter().cloned());
            }
            CheckpointEntry::Symlink { path, target } => link(path, target)?,
            CheckpointEntry::Remove { .. } if plan.mode()? == CheckpointMode::RetainBase => {
                return Err(TaskRefusal::CheckpointIncompatible.into());
            }
            _ => {}
        }
    }
    // One set read covers every file/history object in this bounded page.
    let ids: Vec<_> = ids.into_iter().collect();
    let objects = super::transfer::scoped(connection, plan, &ids).await?;
    super::objects::attach(connection, plan, &ids).await?;
    let lengths: BTreeMap<_, _> = objects
        .into_iter()
        .map(|object| (object.id, object.plaintext_size))
        .collect();
    for entry in &page.entries {
        if let CheckpointEntry::File(file) = entry {
            let bytes = file
                .objects
                .iter()
                .try_fold(0_u64, |sum, id| {
                    let size = lengths.get(id).and_then(|size| u64::try_from(*size).ok())?;
                    sum.checked_add(size)
                })
                .ok_or(TaskRefusal::CheckpointIncompatible)?;
            if bytes != file.size {
                return Err(TaskRefusal::CheckpointIncompatible.into());
            }
        }
    }
    Ok(ids)
}

fn link(path: &str, target: &str) -> Result<()> {
    let mut depth = path.split('/').count().saturating_sub(1);
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                depth = depth
                    .checked_sub(1)
                    .ok_or(TaskRefusal::CheckpointIncompatible)?;
            }
            _ => depth += 1,
        }
    }
    Ok(())
}
