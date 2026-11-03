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
        .fetch_all(connection)
        .await
        .map_err(query("recover durable interpretation"))?;
    if rows.is_empty() {
        return Ok(None);
    }
    if rows.len() != plan.items.len() || rows.iter().any(|row| row.plan_digest != plan.digest) {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    let items = rows
        .into_iter()
        .map(|row| {
            let task_id = row.task_id.ok_or(TaskRefusal::StaleState)?;
            Ok(TaskBinding {
                item_id: guard::id(&row.item_id, "item_id")?,
                task_id: guard::id(&task_id, "id")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    // Reading a receipt grants no execution and changes no terminal state,
    // generation or allowance. Continuation claim owns attempt attachment.
    Ok(Some(TaskBindings {
        items,
        replayed: true,
    }))
}
