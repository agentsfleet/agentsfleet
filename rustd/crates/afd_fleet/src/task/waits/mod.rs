//! Durable dependency ownership; every wake still uses ordinary admissions.

mod claim;
mod cleanup;
mod fanout;
mod finish;
mod maintenance;
mod read;
mod row;
pub(super) use maintenance::maintain;
mod completion;
mod types;
pub(super) use completion::record as complete;
pub(crate) use read::ReadPermit;
pub use types::{CompletionScope, VerifiedCompletion, WaitMaintenance};

use super::{
    Tasks,
    github::{BoundPr, status::Snapshot},
};
use crate::{Result, error::query};
use sqlx::{PgConnection, types::Json};

pub(in crate::task) async fn arm(
    tasks: &Tasks,
    connection: &mut PgConnection,
    task: &str,
    lease: &afd_core::id::Uuid7,
    target: &BoundPr,
    dependency: &Snapshot,
) -> Result<()> {
    let id = tasks.leases.entropy().uuid7(afd_core::clock::now())?;
    sqlx::query(include_str!("sql/arm.sql"))
        .bind(id.as_str())
        .bind(task)
        .bind(lease.as_str())
        .bind(i64::try_from(target.installation_id).map_err(|_| super::TaskRefusal::InvalidTarget)?)
        .bind(i64::try_from(target.repository_id).map_err(|_| super::TaskRefusal::InvalidTarget)?)
        .bind(i32::try_from(target.number).map_err(|_| super::TaskRefusal::InvalidTarget)?)
        .bind(&target.head)
        .bind(Json(&dependency.checks))
        .bind(dependency.digest()?)
        .execute(connection)
        .await
        .map_err(query("arm exact-head wait before releasing execution"))?;
    tracing::info!(
        event = "dt_wait_armed",
        task_id = task,
        wait_id = id.as_str()
    );
    Ok(())
}

pub(in crate::task) async fn cancel(connection: &mut PgConnection, task: &str) -> Result<()> {
    sqlx::query(include_str!("sql/cancel.sql"))
        .bind(task)
        .execute(connection)
        .await
        .map_err(query(
            "invalidate previous task wait in the owning transition",
        ))?;
    Ok(())
}
