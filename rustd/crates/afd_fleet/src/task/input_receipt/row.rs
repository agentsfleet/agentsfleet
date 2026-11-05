//! Ownership comes from immutable native items and their complete plan digest.

use crate::{
    Result,
    error::query,
    task::{TaskRefusal, guard::LeaseRow, row::counter},
};
use afd_wire::task::{TaskInputReceiptInput, TaskInputReceiptResult};
use sqlx::PgConnection;

#[derive(sqlx::FromRow)]
pub(super) struct Item {
    pub task_id: Option<String>,
    pub generation: Option<i64>,
    pub title: Option<String>,
    pub sequence: Option<i64>,
    pub covered_input_sequence: Option<i64>,
    pub state: Option<String>,
}

pub(super) async fn item(
    connection: &mut PgConnection,
    current: &LeaseRow,
    input: &TaskInputReceiptInput,
) -> Result<Item> {
    let item: Item = sqlx::query_as(include_str!("sql/item.sql"))
        .bind(&input.item_id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(&input.source_admission_id)
        .bind(&input.plan_digest)
        .fetch_optional(connection)
        .await
        .map_err(query("read accepted scoped input identity"))?
        .ok_or(TaskRefusal::NotFound)?;
    if item.task_id.as_deref() != input.task.as_ref().map(|task| task.task_id.as_str()) {
        return Err(TaskRefusal::InvalidTarget.into());
    }
    Ok(item)
}

pub(super) fn current(item: &Item, input: &TaskInputReceiptInput) -> Result<()> {
    if let Some(task) = &input.task {
        if item
            .generation
            .map(|generation| counter(generation, "generation"))
            .transpose()?
            != Some(task.generation)
        {
            return Err(TaskRefusal::StaleState.into());
        }
    }
    Ok(())
}

pub(super) async fn replay(
    connection: &mut PgConnection,
    current: &LeaseRow,
    input: &TaskInputReceiptInput,
) -> Result<Option<String>> {
    let mut saved: Vec<String> = sqlx::query_scalar(include_str!("sql/replay.sql"))
        .bind(&input.item_id)
        .bind(input.mode.as_str())
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(&input.operation_id)
        .fetch_all(connection)
        .await
        .map_err(query("read item/mode and operation replay keys"))?;
    if saved.len() > 1 {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    Ok(saved.pop())
}

pub(super) async fn has_answer(
    connection: &mut PgConnection,
    input: &TaskInputReceiptInput,
) -> Result<bool> {
    sqlx::query_scalar(include_str!("sql/has_answer.sql"))
        .bind(&input.item_id)
        .fetch_one(connection)
        .await
        .map_err(query("protect original input answer"))
}

pub(super) fn result(
    input: &TaskInputReceiptInput,
    item: &Item,
    replayed: bool,
) -> Result<TaskInputReceiptResult> {
    Ok(TaskInputReceiptResult {
        item_id: input.item_id.clone(),
        mode: input.mode,
        covered_input_sequence: item
            .covered_input_sequence
            .map(|sequence| counter(sequence, "covered input sequence"))
            .transpose()?,
        replayed,
    })
}
