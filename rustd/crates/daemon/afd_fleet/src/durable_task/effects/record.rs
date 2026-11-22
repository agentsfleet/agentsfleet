//! Remote outcomes are recorded even if the original task has since stopped.

use super::row::Effect;
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTaskRun, DurableTaskScope, DurableTasks, execution, transaction,
    },
    error::query,
};
use af_core::id::Uuid7;
use af_wire::durable_task::DurableTaskEffectInput;

pub(in crate::durable_task) struct PublicationPermit<'a> {
    deadline: jiff::Timestamp,
    effect_id: String,
    target: super::super::github::BoundPr,
    input: &'a garde::Valid<DurableTaskEffectInput>,
}

impl<'a> PublicationPermit<'a> {
    pub(in crate::durable_task) const fn deadline(&self) -> jiff::Timestamp {
        self.deadline
    }
    pub(in crate::durable_task) fn into_parts(
        self,
    ) -> (
        String,
        super::super::github::BoundPr,
        &'a garde::Valid<DurableTaskEffectInput>,
    ) {
        (self.effect_id, self.target, self.input)
    }
}

pub(super) async fn issue<'a>(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &'a garde::Valid<DurableTaskEffectInput>,
    effect: &Effect,
) -> Result<PublicationPermit<'a>> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let owned = execution::load(&mut tx, run, &input.durable_task).await?;
    let deadline = owned
        .current
        .run_expires_at
        .min(owned.current.thread_expires_at)
        .min(owned.snapshot.context.expires_at);
    if deadline <= af_core::clock::now() {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    if i64::try_from(owned.snapshot.context.generation)
        .map_err(|_overflow| DurableTaskRefusal::StaleState)?
        != effect.durable_task_generation
        || crate::durable_task::row::sql_counter(
            owned.snapshot.context.stop_generation,
            "stop_generation",
        )? != effect.stop_generation
        || owned.snapshot.context.covered_input_sequence != input.covered_input_sequence
        || owned.snapshot.context.usage.publications >= owned.snapshot.context.limits.publications
    {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    let (sequence, head): (i64, String) = sqlx::query_as(include_str!("sql/issue_context.sql"))
        .bind(&effect.durable_task_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(query(
            "recheck input and target immediately before publication",
        ))?;
    if super::super::row::counter(sequence, "input sequence")? != input.covered_input_sequence
        || head != effect.expected_head
    {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    let config = crate::run::Runs::stored_config_on(&mut tx, &owned.current.fleet_id, None)
        .await?
        .ok_or(DurableTaskRefusal::InvalidTarget)?;
    effect.target()?.within(&config)?;
    super::super::Github::hold_authority(&mut tx, &owned.scope, effect.target()?.installation_id)
        .await?;
    let updated = sqlx::query(include_str!("sql/issue.sql"))
        .bind(&effect.id)
        .bind("issued")
        .bind("prepared")
        .execute(&mut *tx)
        .await
        .map_err(query("authorize single native ref mutation"))?;
    if updated.rows_affected() != 1 {
        return Err(DurableTaskRefusal::EffectUncertain.into());
    }
    sqlx::query(include_str!("sql/debit.sql"))
        .bind(&effect.durable_task_id)
        .execute(&mut *tx)
        .await
        .map_err(query("count issued task publication"))?;
    tx.commit()
        .await
        .map_err(query("commit publication stop cutoff"))?;
    transaction::recycle(connection);
    Ok(PublicationPermit {
        deadline,
        effect_id: effect.id.clone().into(),
        target: effect.target()?.clone(),
        input,
    })
}

pub(super) async fn confirm(tasks: &DurableTasks, effect: &Effect, commit: &str) -> Result<()> {
    let scope = DurableTaskScope {
        fleet_id: effect.fleet_id.clone(),
        workspace_id: effect.workspace_id.clone(),
    };
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let _parent: Option<Uuid7> = sqlx::query_scalar(super::super::sql::LOCK_SCOPE)
        .bind(&effect.fleet_id)
        .bind(&effect.workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("protect any remaining publication parent"))?;
    let _thread: Option<Uuid7> = sqlx::query_scalar(super::super::sql::LOCK_SCOPE_THREAD)
        .bind(&effect.fleet_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("serialize late publication receipt"))?;
    let held: Option<String> = sqlx::query_scalar(super::super::sql::LOCK_DURABLE_TASK)
        .bind(&effect.durable_task_id)
        .bind(&effect.fleet_id)
        .bind(&effect.workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("lock confirmed publication owner"))?;
    let updated = sqlx::query(include_str!("sql/confirm.sql"))
        .bind(&effect.id)
        .bind(commit)
        .bind("confirmed")
        .bind("issued")
        .bind("unknown")
        .execute(&mut *tx)
        .await
        .map_err(query("retain attributable GitHub receipt"))?;
    if updated.rows_affected() == 1 {
        sqlx::query(include_str!("sql/confirmed_durable_task.sql"))
            .bind(&effect.durable_task_id)
            .bind(effect.ordinal)
            .bind(commit)
            .bind(&effect.tested_tree)
            .bind(&effect.expected_head)
            .execute(&mut *tx)
            .await
            .map_err(query("advance confirmed native effect watermark"))?;
    }
    tx.commit()
        .await
        .map_err(query("commit confirmed publication receipt"))?;
    transaction::recycle(connection);
    if held.is_some() {
        let id = effect.durable_task_id.clone();
        if let Ok(snapshot) = super::super::read::read(tasks, &scope, &id).await {
            tasks
                .publish_changed(&scope, &id, snapshot.resource_version)
                .await;
        }
    }
    Ok(())
}

pub(super) async fn refuse(tasks: &DurableTasks, effect: &Effect) -> Result<()> {
    state(tasks, effect, "refused", "prepared").await
}
pub(super) async fn unknown(tasks: &DurableTasks, effect: &Effect) -> Result<()> {
    state(tasks, effect, "unknown", "issued").await
}

async fn state(tasks: &DurableTasks, effect: &Effect, state: &str, previous: &str) -> Result<()> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    sqlx::query(include_str!("sql/state.sql"))
        .bind(&effect.id)
        .bind(state)
        .bind(previous)
        .execute(&mut *tx)
        .await
        .map_err(query("retain native effect disposition"))?;
    tx.commit()
        .await
        .map_err(query("commit native effect disposition"))?;
    transaction::recycle(connection);
    Ok(())
}
