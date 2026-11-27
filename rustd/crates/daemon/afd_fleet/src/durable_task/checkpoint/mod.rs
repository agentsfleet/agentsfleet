//! Fenced plans, immutable object custody and bounded recovery on existing runs.

mod base;
pub(crate) mod chain;
mod commit;
mod custody;
mod effective;
mod guard;
mod maintenance;
mod object_row;
mod objects;
mod page;
mod pins;
mod plan;
mod retire;
mod row;
pub(crate) mod sandbox;
mod transfer;

pub(super) use commit::publish;
pub(crate) use custody::{accept, supersede};
pub(super) use maintenance::maintain;
pub(crate) use object_row::Object as StoredObject;
pub(super) use objects::register;
pub(super) use page::{read, write};
pub(super) use pins::renew;
pub(super) use plan::create;
pub(crate) use retire::{retire, rotate};
pub(super) use transfer::{authorize, list, uploads};

/// The statements a run's turn checkpoints share with task checkpoints,
/// included once so the two custody paths cannot drift apart.
pub(crate) const OBJECTS_INSERT: &str = include_str!("sql/objects_insert.sql");
pub(crate) const DESCRIPTORS: &str = include_str!("sql/descriptors.sql");
pub(crate) const PLAN_BYTES: &str = include_str!("sql/plan_bytes.sql");

use crate::{Result, error::query};
use af_core::id::Uuid7;
use std::time::Duration;

/// Serializes one workspace's checkpoint storage writes until the transaction ends.
pub(crate) async fn lock_storage(
    connection: &mut sqlx::PgConnection,
    workspace: &Uuid7,
) -> Result<()> {
    sqlx::query_scalar::<_, Uuid7>(include_str!("sql/lock_storage.sql"))
        .bind(workspace)
        .fetch_one(connection)
        .await
        .map_err(query("lock workspace checkpoint storage"))?;
    Ok(())
}

/// The `thread.replay_receipts` scope of a task's checkpoint allocations: one
/// retry key per operation for as long as the receipt stays.
pub(super) fn receipt_scope(workspace: &Uuid7, task: &Uuid7) -> String {
    format!("{workspace}:durable_task:{task}:checkpoint")
}

/// How long a checkpoint's uploads or restore may run in total, by default.
pub(crate) const TRANSFER: Duration = Duration::from_mins(15);
/// How long a restoring checkpoint is pinned, by default.
pub(crate) const PIN: Duration = Duration::from_mins(30);
/// How long a runner's presigned upload or download link stays valid, by default.
pub(crate) const LINK: Duration = Duration::from_mins(5);
/// How long an uncommitted upload is kept, by default.
pub(crate) const ORPHAN: Duration = Duration::from_hours(1);
/// Objects deleted per storage call, by default.
pub(crate) const GC_BATCH: u32 = 256;
/// Newest checkpoints kept per task, by default.
pub(crate) const LIVE_GENERATIONS: u32 = 3;
/// How long a finished task's records are kept, by default.
pub(crate) const TERMINAL: Duration = af_core::timing::DAY.saturating_mul(7);
