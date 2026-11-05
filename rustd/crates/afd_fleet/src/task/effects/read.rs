//! Replacement workers discover effects by native task ordinal and exact scope.

use afd_core::id::Uuid7;
use afd_wire::task::{TaskEffect, TaskEffectsPage, TaskEffectsQuery};
use super::{record, row::Effect};
use crate::{Result, error::{query, rejected}, task::{TaskLease, TaskRefusal, Tasks, github::Reconciled, guard, transaction}};

pub(super) async fn one(tasks: &Tasks, lease: TaskLease<'_>, id: &Uuid7) -> Result<TaskEffect> {
    let mut effect = load(tasks, lease, id).await?;
    if effect.state == "prepared" && super::recover::abandon(tasks, lease, &effect).await? {
        effect = load(tasks, lease, id).await?;
    }
    if matches!(effect.state.as_str(), "issued" | "unknown") {
        let github = tasks.dependencies.github.as_ref().ok_or(TaskRefusal::InvalidTarget)?;
        let session = github.read_session(lease, &effect.target()?.repository).await?;
        match session.reconcile(effect.target()?, &effect.id, &effect.expected_head, &effect.tested_tree).await? {
            Reconciled::Confirmed(commit) => {
                record::confirm(tasks, &effect, &commit).await?;
                return load(tasks, lease, id).await?.wire();
            }
            Reconciled::Unknown => {}
        }
    }
    effect.wire()
}

async fn load(tasks: &Tasks, lease: TaskLease<'_>, id: &Uuid7) -> Result<Effect> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::lease(&mut tx, lease).await?;
    let effect = sqlx::query_as(include_str!("sql/read.sql")).bind(id.as_str())
        .bind(&current.fleet_id).bind(&current.workspace_id).bind(&current.task_id)
        .fetch_optional(&mut *tx).await.map_err(query("read scoped publication receipt"))?.ok_or(TaskRefusal::NotFound)?;
    tx.commit().await.map_err(query("finish scoped publication read"))?;
    transaction::recycle(connection);
    Ok(effect)
}

pub(super) async fn list(tasks: &Tasks, lease: TaskLease<'_>, input: &garde::Valid<TaskEffectsQuery>) -> Result<TaskEffectsPage> {
    let after = input.starting_after.as_deref().map(str::parse::<i64>).transpose()
        .map_err(|_invalid| rejected("Invalid effect cursor"))?.unwrap_or(0);
    if after < 0 { return Err(rejected("Invalid effect cursor")); }
    let watermark = i64::try_from(input.ordinal_after).map_err(|_overflow| rejected("Invalid effect watermark"))?;
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::lease(&mut tx, lease).await?;
    if current.task_id.as_deref() != Some(input.task_id.as_str()) { return Err(TaskRefusal::NotFound.into()); }
    let mut effects: Vec<Effect> = sqlx::query_as(include_str!("sql/list.sql"))
        .bind(&input.task_id).bind(&current.fleet_id).bind(&current.workspace_id).bind(after.max(watermark))
        .bind(i64::from(input.limit) + 1).fetch_all(&mut *tx).await.map_err(query("page recoverable task effects"))?;
    let more = effects.len() > input.limit as usize;
    if more { effects.pop(); }
    let next_cursor = more.then(|| effects.last()).flatten().map(|last| last.ordinal.to_string());
    let effects = effects.iter().map(Effect::wire).collect::<Result<Vec<_>>>()?;
    tx.commit().await.map_err(query("finish effect recovery page"))?;
    transaction::recycle(connection);
    Ok(TaskEffectsPage { effects, next_cursor })
}
