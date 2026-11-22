//! Ownership comes from immutable native items and their complete plan digest.

use crate::{
    Result,
    durable_task::{DurableTaskRefusal, guard::RunRow, row::optional_counter},
    error::query,
};
use af_core::id::Uuid7;
use af_wire::durable_task::{DurableTaskInputReceiptInput, DurableTaskInputReceiptResult};
use sqlx::PgConnection;

#[derive(sqlx::FromRow)]
pub(super) struct Item {
    pub durable_task_id: Option<Uuid7>,
    pub generation: Option<i64>,
    pub title: Option<String>,
    pub sequence: Option<i64>,
    pub covered_input_sequence: Option<i64>,
    pub state: Option<String>,
}

pub(super) async fn item(
    connection: &mut PgConnection,
    current: &RunRow,
    input: &DurableTaskInputReceiptInput,
) -> Result<Item> {
    let item: Item = sqlx::query_as(include_str!("sql/item.sql"))
        .bind(crate::durable_task::guard::wire(&input.item_id)?)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(crate::durable_task::guard::wire(&input.source_turn_id)?)
        .bind(&input.plan_digest)
        .bind(afd_turns::Producer::SlackMention.as_str())
        .bind(afd_connector::Provider::Slack.id())
        .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
        .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
        .fetch_optional(connection)
        .await
        .map_err(query("read accepted scoped input identity"))?
        .ok_or(DurableTaskRefusal::NotFound)?;
    if item.durable_task_id.as_ref().map(Uuid7::as_str)
        != input
            .durable_task
            .as_ref()
            .map(|task| task.durable_task_id.as_str())
    {
        return Err(DurableTaskRefusal::InvalidTarget.into());
    }
    Ok(item)
}

pub(super) fn current(item: &Item, input: &DurableTaskInputReceiptInput) -> Result<()> {
    if let Some(task) = &input.durable_task
        && optional_counter(item.generation, "generation")? != Some(task.generation)
    {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    Ok(())
}

pub(super) async fn replay(
    connection: &mut PgConnection,
    current: &RunRow,
    input: &DurableTaskInputReceiptInput,
) -> Result<Option<String>> {
    let mut saved: Vec<String> = sqlx::query_scalar(include_str!("sql/replay.sql"))
        .bind(crate::durable_task::guard::wire(&input.item_id)?)
        .bind(input.mode.as_str())
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(crate::durable_task::guard::wire(&input.operation_id)?)
        .fetch_all(connection)
        .await
        .map_err(query("read item/mode and operation replay keys"))?;
    if saved.len() > 1 {
        return Err(DurableTaskRefusal::ReplayConflict.into());
    }
    Ok(saved.pop())
}

pub(super) async fn has_answer(
    connection: &mut PgConnection,
    input: &DurableTaskInputReceiptInput,
) -> Result<bool> {
    sqlx::query_scalar(include_str!("sql/has_answer.sql"))
        .bind(crate::durable_task::guard::wire(&input.item_id)?)
        .fetch_one(connection)
        .await
        .map_err(query("protect original input answer"))
}

pub(super) fn result(
    input: &DurableTaskInputReceiptInput,
    item: &Item,
    replayed: bool,
) -> Result<DurableTaskInputReceiptResult> {
    Ok(DurableTaskInputReceiptResult {
        item_id: input.item_id.clone(),
        mode: input.mode,
        covered_input_sequence: optional_counter(
            item.covered_input_sequence,
            "covered input sequence",
        )?,
        replayed,
    })
}
