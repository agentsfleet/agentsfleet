//! Remote outcomes are recorded even if the original task has since stopped.

use super::row::Effect;
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal, TaskScope, Tasks, execution, transaction},
};
use af_core::id::Uuid7;
use af_wire::task::TaskEffectInput;

pub(in crate::task) struct PublicationPermit<'a> {
    deadline: jiff::Timestamp,
    effect_id: String,
    target: super::super::github::BoundPr,
    input: &'a garde::Valid<TaskEffectInput>,
}

impl<'a> PublicationPermit<'a> {
    pub(in crate::task) const fn deadline(&self) -> jiff::Timestamp {
        self.deadline
    }
    pub(in crate::task) fn into_parts(
        self,
    ) -> (
        String,
        super::super::github::BoundPr,
        &'a garde::Valid<TaskEffectInput>,
    ) {
        (self.effect_id, self.target, self.input)
    }
}

pub(super) async fn issue<'a>(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &'a garde::Valid<TaskEffectInput>,
    effect: &Effect,
    recovery: &super::super::github::recovery::Sealed,
) -> Result<PublicationPermit<'a>> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let owned = execution::load(&mut tx, lease, &input.task).await?;
    let deadline = owned
        .current
        .lease_expires_at
        .min(owned.current.slot_expires_at)
        .min(owned.snapshot.context.expires_at);
    if deadline <= af_core::clock::now() {
        return Err(TaskRefusal::StaleState.into());
    }
    if i64::try_from(owned.snapshot.context.generation)
        .map_err(|_overflow| TaskRefusal::StaleState)?
        != effect.task_generation
        || crate::task::row::sql_counter(owned.snapshot.context.stop_generation, "stop_generation")?
            != effect.stop_generation
        || owned.snapshot.context.covered_input_sequence != input.covered_input_sequence
        || owned.snapshot.context.usage.publications >= owned.snapshot.context.limits.publications
    {
        return Err(TaskRefusal::StaleState.into());
    }
    let (sequence, head): (i64, String) = sqlx::query_as(include_str!("sql/issue_context.sql"))
        .bind(&effect.task_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(query(
            "recheck input and target immediately before publication",
        ))?;
    if super::super::row::counter(sequence, "input sequence")? != input.covered_input_sequence
        || head != effect.expected_head
    {
        return Err(TaskRefusal::StaleState.into());
    }
    let config = crate::lease::Leases::stored_config_on(&mut tx, &owned.current.fleet_id, None)
        .await?
        .ok_or(TaskRefusal::InvalidTarget)?;
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
        return Err(TaskRefusal::EffectUncertain.into());
    }
    recovery
        .insert(&mut tx, &owned.scope.workspace_id, &effect.id)
        .await?;
    sqlx::query(include_str!("sql/debit.sql"))
        .bind(&effect.task_id)
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

pub(super) async fn confirm(tasks: &Tasks, effect: &Effect, commit: &str) -> Result<()> {
    let scope = TaskScope {
        fleet_id: effect.fleet_id.clone(),
        workspace_id: effect.workspace_id.clone(),
    };
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let _parent: Option<Uuid7> = sqlx::query_scalar(super::super::sql::LOCK_SCOPE)
        .bind(&effect.fleet_id)
        .bind(&effect.workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("protect any remaining publication parent"))?;
    let _slot: Option<Uuid7> = sqlx::query_scalar(super::super::sql::LOCK_SCOPE_SLOT)
        .bind(&effect.fleet_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("serialize late publication receipt"))?;
    let held: Option<String> = sqlx::query_scalar(super::super::sql::LOCK_TASK)
        .bind(&effect.task_id)
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
        sqlx::query(include_str!("sql/confirmed_task.sql"))
            .bind(&effect.task_id)
            .bind(effect.ordinal)
            .bind(commit)
            .bind(&effect.tested_tree)
            .bind(&effect.expected_head)
            .execute(&mut *tx)
            .await
            .map_err(query("advance confirmed native effect watermark"))?;
    }
    sqlx::query(include_str!("sql/drop_recovery.sql"))
        .bind(&effect.id)
        .execute(&mut *tx)
        .await
        .map_err(query("erase resolved effect read custody"))?;
    tx.commit()
        .await
        .map_err(query("commit confirmed publication receipt"))?;
    transaction::recycle(connection);
    if held.is_some() {
        let id = effect.task_id.clone();
        if let Ok(snapshot) = super::super::read::read(tasks, &scope, &id).await {
            tasks
                .publish_changed(&scope, &id, snapshot.resource_version)
                .await;
        }
    }
    Ok(())
}

pub(super) async fn refuse(tasks: &Tasks, effect: &Effect) -> Result<()> {
    state(tasks, effect, "refused", "prepared").await
}
pub(super) async fn unknown(tasks: &Tasks, effect: &Effect) -> Result<()> {
    state(tasks, effect, "unknown", "issued").await
}

async fn state(tasks: &Tasks, effect: &Effect, state: &str, previous: &str) -> Result<()> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
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
