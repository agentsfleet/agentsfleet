//! One bounded set read restores immutable meaning and original reply receipts.

use crate::{
    Result,
    durable_task::{DurableTaskRefusal, guard::RunRow, row},
    error::query,
};
use af_core::id::Uuid7;
use af_wire::durable_task::{
    DurableTaskAcceptedInterpretation, DurableTaskItemBinding, DurableTaskRecoveredItem,
    DurableTaskState,
};
use sqlx::PgConnection;
use std::collections::BTreeMap;

#[derive(sqlx::FromRow)]
struct InterpretationRow {
    source_turn_id: Uuid7,
    plan_digest: String,
    source_producer: String,
    source_request_json: String,
    item_id: Uuid7,
    durable_task_id: Option<Uuid7>,
    generation: Option<i64>,
    state: Option<String>,
    input_sequence: Option<i64>,
    action: String,
    source_start: i32,
    source_end: i32,
    interpretation: String,
    response_text: Option<String>,
}

pub(super) async fn read(
    connection: &mut PgConnection,
    current: &RunRow,
    sources: &[Uuid7],
) -> Result<BTreeMap<Uuid7, DurableTaskAcceptedInterpretation>> {
    let rows: Vec<InterpretationRow> =
        sqlx::query_as(include_str!("../sql/recovered_interpretations.sql"))
            .bind(sources)
            .bind(&current.fleet_id)
            .bind(&current.workspace_id)
            // A new task's item has no input row; neither carries an interpretation.
            .bind(crate::durable_task::accept::NO_INTERPRETATION)
            .fetch_all(connection)
            .await
            .map_err(query(
                "recover complete accepted source meanings and original replies",
            ))?;
    let mut interpretations: BTreeMap<Uuid7, DurableTaskAcceptedInterpretation> = BTreeMap::new();
    for item in rows {
        let message = crate::durable_task::accept::source::message_for(
            &item.source_producer,
            &item.source_request_json,
        )?;
        let start = usize::try_from(item.source_start)
            .map_err(|_overflow| DurableTaskRefusal::InputAmbiguous)?;
        let end = usize::try_from(item.source_end)
            .map_err(|_overflow| DurableTaskRefusal::InputAmbiguous)?;
        let text = message
            .get(start..end)
            .ok_or(DurableTaskRefusal::InputAmbiguous)?
            .to_owned();
        let action = row::action(&item.action)?;
        let binding = DurableTaskItemBinding {
            item_id: item.item_id.into(),
            durable_task_generation: row::optional_counter(item.generation, "generation")?,
            applies_here: item.durable_task_id == current.durable_task_id
                && item.state.as_deref() == Some(DurableTaskState::Running.as_str()),
            durable_task_id: item.durable_task_id.map(String::from),
            action,
            input_sequence: row::optional_counter(item.input_sequence, "input sequence")?,
        };
        let accepted = interpretations
            .entry(item.source_turn_id.clone())
            .or_insert_with(|| DurableTaskAcceptedInterpretation {
                source_turn_id: item.source_turn_id.into(),
                plan_digest: item.plan_digest,
                message: message.into_owned(),
                items: Vec::new(),
            });
        accepted.items.push(DurableTaskRecoveredItem {
            binding,
            accepted_text: text,
            interpretation: item.interpretation,
            response_text: item.response_text,
        });
    }
    Ok(interpretations)
}
