//! Replacement workers discover effects by native task ordinal and exact scope.

use super::{record, row::Effect};
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTaskRun, DurableTasks, github::Reconciled, guard, transaction,
    },
    error::{query, rejected},
};
use af_core::id::Uuid7;
use af_wire::durable_task::{DurableTaskEffect, DurableTaskEffectsPage, DurableTaskEffectsQuery};

pub(super) async fn one(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    id: &Uuid7,
) -> Result<DurableTaskEffect> {
    let mut effect = load(tasks, run, id).await?;
    if effect.state == "prepared" && super::recover::abandon(tasks, run, &effect).await? {
        effect = load(tasks, run, id).await?;
    }
    if matches!(effect.state.as_str(), "issued" | "unknown") {
        let github = tasks
            .dependencies
            .github
            .as_ref()
            .ok_or(DurableTaskRefusal::InvalidTarget)?;
        let session = github
            .read_session(run, &effect.target()?.repository)
            .await?;
        match session
            .reconcile(
                effect.target()?,
                effect.id.as_str(),
                &effect.expected_head,
                &effect.tested_tree,
            )
            .await?
        {
            Reconciled::Confirmed(commit) => {
                record::confirm(tasks, &effect, &commit).await?;
                return load(tasks, run, id).await?.wire();
            }
            Reconciled::Unknown => {}
        }
    }
    effect.wire()
}

async fn load(tasks: &DurableTasks, run: DurableTaskRun<'_>, id: &Uuid7) -> Result<Effect> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::run(&mut tx, run).await?;
    let effect = sqlx::query_as(include_str!("sql/read.sql"))
        .bind(id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(&current.durable_task_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("read scoped publication receipt"))?
        .ok_or(DurableTaskRefusal::NotFound)?;
    tx.commit()
        .await
        .map_err(query("finish scoped publication read"))?;
    transaction::recycle(connection);
    Ok(effect)
}

pub(super) async fn list(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &garde::Valid<DurableTaskEffectsQuery>,
) -> Result<DurableTaskEffectsPage> {
    let after = input
        .starting_after
        .as_deref()
        .map(str::parse::<i64>)
        .transpose()
        .map_err(|_invalid| rejected("Invalid effect cursor"))?
        .unwrap_or(0);
    if after < 0 {
        return Err(rejected("Invalid effect cursor"));
    }
    let watermark = i64::try_from(input.ordinal_after)
        .map_err(|_overflow| rejected("Invalid effect watermark"))?;
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::run(&mut tx, run).await?;
    if current.durable_task_id.as_ref().map(Uuid7::as_str) != Some(input.durable_task_id.as_str()) {
        return Err(DurableTaskRefusal::NotFound.into());
    }
    let mut effects: Vec<Effect> = sqlx::query_as(include_str!("sql/list.sql"))
        .bind(guard::wire(&input.durable_task_id)?)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(after.max(watermark))
        .bind(i64::from(input.limit) + 1)
        .fetch_all(&mut *tx)
        .await
        .map_err(query("page recoverable task effects"))?;
    let more = effects.len() > usize::try_from(input.limit).unwrap_or(usize::MAX);
    if more {
        effects.pop();
    }
    let next_cursor = more
        .then(|| effects.last())
        .flatten()
        .map(|last| last.ordinal.to_string());
    let effects = effects
        .iter()
        .map(Effect::wire)
        .collect::<Result<Vec<_>>>()?;
    tx.commit()
        .await
        .map_err(query("finish effect recovery page"))?;
    transaction::recycle(connection);
    Ok(DurableTaskEffectsPage {
        effects,
        next_cursor,
    })
}
