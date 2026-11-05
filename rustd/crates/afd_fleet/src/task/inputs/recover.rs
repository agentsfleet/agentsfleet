//! One bounded set read restores immutable meaning and original reply receipts.

use crate::{
    Result,
    error::query,
    task::{TaskRefusal, guard::LeaseRow, row},
};
use afd_wire::task::{TaskAcceptedInterpretation, TaskItemBinding, TaskRecoveredItem, TaskState};
use sqlx::PgConnection;
use std::collections::BTreeMap;

#[derive(sqlx::FromRow)]
struct Item {
    source_admission_id: String,
    plan_digest: String,
    message: String,
    item_id: String,
    task_id: Option<String>,
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
    current: &LeaseRow,
    sources: &[String],
) -> Result<BTreeMap<String, TaskAcceptedInterpretation>> {
    let rows: Vec<Item> = sqlx::query_as(include_str!("../sql/recovered_interpretations.sql"))
        .bind(sources)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind("items")
        .bind("proposed")
        .bind("interpretation")
        .fetch_all(connection)
        .await
        .map_err(query(
            "recover complete accepted source meanings and original replies",
        ))?;
    let mut interpretations: BTreeMap<String, TaskAcceptedInterpretation> = BTreeMap::new();
    for item in rows {
        let start =
            usize::try_from(item.source_start).map_err(|_overflow| TaskRefusal::InputAmbiguous)?;
        let end =
            usize::try_from(item.source_end).map_err(|_overflow| TaskRefusal::InputAmbiguous)?;
        let text = item
            .message
            .get(start..end)
            .ok_or(TaskRefusal::InputAmbiguous)?
            .to_owned();
        let action = serde::Deserialize::deserialize(serde::de::value::StrDeserializer::new(
            item.action.as_str(),
        ))
        .map_err(|source: serde::de::value::Error| {
            crate::error::Error::from(crate::error::ErrorKind::TaskState { source })
        })?;
        let binding = TaskItemBinding {
            item_id: item.item_id,
            task_generation: item
                .generation
                .map(|generation| row::counter(generation, "generation"))
                .transpose()?,
            applies_here: item.task_id == current.task_id
                && item.state.as_deref() == Some(TaskState::Running.as_str()),
            task_id: item.task_id,
            action,
            input_sequence: item
                .input_sequence
                .map(|sequence| row::counter(sequence, "input sequence"))
                .transpose()?,
        };
        let accepted = interpretations
            .entry(item.source_admission_id.clone())
            .or_insert_with(|| TaskAcceptedInterpretation {
                source_admission_id: item.source_admission_id,
                plan_digest: item.plan_digest,
                message: item.message,
                items: Vec::new(),
            });
        accepted.items.push(TaskRecoveredItem {
            binding,
            accepted_text: text,
            interpretation: item.interpretation,
            response_text: item.response_text,
        });
    }
    Ok(interpretations)
}
