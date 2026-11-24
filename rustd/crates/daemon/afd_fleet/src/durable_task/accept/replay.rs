//! A plan is replayed as a whole: one source turn takes one plan, and a
//! repeat with another digest is refused.

use af_core::id::Uuid7;
use sqlx::PgConnection;

use super::super::{
    DurableTaskBinding, DurableTaskBindings, DurableTaskRefusal, PreparedPlan,
    guard::{self, RunRow},
    sql,
};
use crate::error::{Result, query};

#[derive(sqlx::FromRow)]
struct Existing {
    item_id: Uuid7,
    durable_task_id: Option<Uuid7>,
    plan_digest: String,
    action: String,
    durable_task_state: Option<String>,
    durable_task_generation: Option<i64>,
    input_sequence: Option<i64>,
}

pub(super) async fn read(
    connection: &mut PgConnection,
    current: &RunRow,
    plan: &PreparedPlan,
) -> Result<Option<DurableTaskBindings>> {
    let rows: Vec<Existing> = sqlx::query_as(sql::REPLAY)
        .bind(&plan.source_turn_id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(query("recover durable interpretation"))?;
    if rows.is_empty() {
        return Ok(None);
    }
    if rows.iter().any(|row| row.plan_digest != plan.digest) {
        return Err(DurableTaskRefusal::ReplayConflict.into());
    }
    let items = rows
        .into_iter()
        .map(|row| {
            let action = super::super::row::action(&row.action)?;
            Ok(DurableTaskBinding {
                item_id: row.item_id.clone(),
                applies_here: row.durable_task_id == current.durable_task_id
                    && row.durable_task_state.as_deref()
                        == Some(af_wire::durable_task::DurableTaskState::Running.as_str()),
                durable_task_id: row.durable_task_id,
                durable_task_generation: super::super::row::optional_counter(
                    row.durable_task_generation,
                    "generation",
                )?,
                action,
                input_sequence: super::super::row::optional_counter(
                    row.input_sequence,
                    "input sequence",
                )?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    // Reading a receipt grants no execution and changes no terminal state,
    // generation or allowance. A continuation's assignment owns attempt attachment.
    let current_durable_task = guard::context(&mut *connection, current).await?;
    let execution_deadline = crate::run::deadline::read(connection, &current.run_id).await?;
    Ok(Some(DurableTaskBindings {
        items,
        current_durable_task,
        execution_deadline,
        replayed: true,
    }))
}
