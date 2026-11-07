//! A plan is replayed as a whole; changed count or meaning cannot append work.

use sqlx::PgConnection;

use super::super::{
    PreparedPlan, TaskBinding, TaskBindings, TaskRefusal,
    guard::{self, LeaseRow},
    sql,
};
use crate::error::{Result, query};

#[derive(sqlx::FromRow)]
struct Existing {
    item_id: String,
    task_id: Option<String>,
    plan_digest: String,
    action: String,
    task_state: Option<String>,
    task_generation: Option<i64>,
    input_sequence: Option<i64>,
}

pub(super) async fn read(
    connection: &mut PgConnection,
    current: &LeaseRow,
    plan: &PreparedPlan,
) -> Result<Option<TaskBindings>> {
    let rows: Vec<Existing> = sqlx::query_as(sql::REPLAY)
        .bind(plan.source_admission_id.as_str())
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(query("recover durable interpretation"))?;
    if rows.is_empty() {
        return Ok(None);
    }
    let count = plan
        .authorization
        .as_ref()
        .map_or(plan.items.len(), |input| input.items.len());
    if rows.len() != count || rows.iter().any(|row| row.plan_digest != plan.digest) {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    let items = rows
        .into_iter()
        .map(|row| {
            let action: af_wire::task::TaskInputAction = serde::Deserialize::deserialize(
                serde::de::value::StrDeserializer::new(row.action.as_str()),
            )
            .map_err(|source: serde::de::value::Error| {
                crate::error::Error::from(crate::error::ErrorKind::TaskState { source })
            })?;
            Ok(TaskBinding {
                item_id: guard::id(&row.item_id, "item_id")?,
                applies_here: row.task_id == current.task_id
                    && row.task_state.as_deref()
                        == Some(af_wire::task::TaskState::Running.as_str()),
                task_id: row
                    .task_id
                    .map(|task_id| guard::id(&task_id, "id"))
                    .transpose()?,
                task_generation: row
                    .task_generation
                    .map(|generation| super::super::row::counter(generation, "generation"))
                    .transpose()?,
                action,
                input_sequence: row
                    .input_sequence
                    .map(|sequence| super::super::row::counter(sequence, "input sequence"))
                    .transpose()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    // Reading a receipt grants no execution and changes no terminal state,
    // generation or allowance. Continuation claim owns attempt attachment.
    let current_task = guard::context(&mut *connection, current).await?;
    let execution_deadline = crate::lease::deadline::read(connection, &current.lease_id).await?;
    Ok(Some(TaskBindings {
        items,
        current_task,
        execution_deadline,
        replayed: true,
    }))
}
