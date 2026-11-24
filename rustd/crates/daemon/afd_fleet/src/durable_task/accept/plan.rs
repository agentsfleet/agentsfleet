//! The source turn's plan: each accepted item's action, span and the task it
//! binds, written once before any item changes task state.

use af_core::id::Uuid7;
use af_wire::durable_task::DurableTaskInputAction;
use serde::Serialize;
use sqlx::{PgConnection, types::Json};

use super::super::{DurableTaskBinding, PreparedPlan, guard::RunRow};
use crate::error::{Result, query};

#[derive(Serialize)]
struct Plan<'a> {
    items: Vec<Item<'a>>,
}

#[derive(Serialize)]
struct Item<'a> {
    action: DurableTaskInputAction,
    source_start: i32,
    source_end: i32,
    durable_task_id: Option<&'a Uuid7>,
}

/// Writes `plan` and its digest on the source turn, and marks the turn
/// interpreted so an answer receipt may complete it.
///
/// # Errors
/// A statement that would not run, including the trigger's refusal of a turn
/// that already holds a plan.
pub(super) async fn write(
    connection: &mut PgConnection,
    current: &RunRow,
    plan: &PreparedPlan,
    identities: &[DurableTaskBinding],
) -> Result<()> {
    let items = plan
        .items
        .iter()
        .zip(identities)
        .map(|(item, binding)| Item {
            action: item.action(),
            source_start: item.span().start,
            source_end: item.span().end,
            durable_task_id: binding.durable_task_id.as_ref(),
        })
        .collect();
    sqlx::query(include_str!("sql/plan.sql"))
        .bind(&plan.source_turn_id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(Json(Plan { items }))
        .bind(&plan.digest)
        .execute(connection)
        .await
        .map_err(query("write the source turn's plan"))?;
    Ok(())
}
