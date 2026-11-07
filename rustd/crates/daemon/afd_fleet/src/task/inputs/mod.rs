//! Safe-boundary reads use the source conversation and the existing fleet slot.

mod recover;

use af_core::paging::Cursor;
use af_wire::task::{
    DecisionTaskVersion, TaskAcceptedInput, TaskHumanInput, TaskInputCandidate, TaskInputsQuery,
    TaskInputsResponse,
};

use super::{TaskLease, Tasks, guard, row, transaction};
use crate::{
    Result,
    error::{query, rejected},
};

#[derive(sqlx::FromRow)]
struct Human {
    id: String,
    sequence: i64,
    request_json: String,
    producer: String,
}

#[derive(sqlx::FromRow)]
struct Candidate {
    id: String,
    title: String,
    state: String,
    generation: i64,
}

#[derive(sqlx::FromRow)]
struct InputRow {
    source_admission_id: String,
    plan_digest: String,
    item_id: String,
    task_id: String,
    generation: i64,
    sequence: i64,
    action: String,
    original_message: String,
    accepted_text: String,
    interpretation: String,
    applied: bool,
    response_text: Option<String>,
}

pub(super) async fn read(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &garde::Valid<TaskInputsQuery>,
) -> Result<TaskInputsResponse<'static>> {
    let after = cursor(input.starting_after.as_deref())?;
    let limit =
        usize::try_from(input.limit).map_err(|_overflow| rejected("Invalid task input limit"))?;
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::lease(&mut tx, lease).await?;
    let current_task = guard::context(&mut tx, &current).await?;
    let source = super::admit::source::load(
        &mut tx,
        &current,
        &guard::id(&current.source_admission_id, "source_admission_id")?,
    )
    .await?;
    let current_message = if super::admit::source::check_human(&source).is_ok() {
        Some(source.message()?.into_owned())
    } else {
        None
    };
    let accepted_inputs = accepted(&mut tx, &current, input).await?;
    let mut humans: Vec<Human> = sqlx::query_as(include_str!("../sql/pending_inputs.sql"))
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(&current.source_admission_id)
        .bind(afd_admission::Producer::Steer.as_str())
        .bind(afd_events::ACTOR_MACHINE)
        .bind(format!("{}%", afd_events::ACTOR_PREFIX))
        .bind(after)
        .bind(i64::from(input.limit) + 1)
        .bind(lease.lease_id.as_str())
        .bind(current.fence)
        .bind(crate::lease::sql::LEASE_STATUS_ACTIVE)
        .bind(None::<&str>)
        .bind(true)
        .bind(afd_admission::Producer::SlackMention.as_str())
        .bind(afd_connector::Provider::Slack.id())
        .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
        .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
        .fetch_all(&mut *tx)
        .await
        .map_err(query("read complete eligible human inputs"))?;
    let more = humans.len() > limit;
    if more {
        humans.pop();
    }
    let next_cursor = more.then(|| humans.last()).flatten().map(|last| {
        Cursor::Text {
            value: last.sequence.to_string(),
            id: last.id.clone(),
        }
        .to_string()
    });
    let sources: Vec<_> = humans
        .iter()
        .map(|human| human.id.clone())
        .chain(std::iter::once(current.source_admission_id.clone()))
        .collect();
    let mut recovered = recover::read(&mut tx, &current, &sources).await?;
    let current_interpretation = recovered.remove(&current.source_admission_id);
    let candidates = candidates(&mut tx, &current).await?;
    let inputs = humans
        .into_iter()
        .map(|source| {
            Ok(TaskHumanInput {
                accepted: recovered.remove(&source.id),
                source_admission_id: source.id,
                sequence: row::counter(source.sequence, "input sequence")?,
                message: super::admit::source::message_for(&source.producer, &source.request_json)?
                    .into_owned(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let execution_deadline = crate::lease::deadline::read(&mut tx, lease.lease_id.as_str()).await?;
    tx.commit().await.map_err(query("finish task input read"))?;
    transaction::recycle(connection);
    Ok(TaskInputsResponse {
        source_admission_id: current.source_admission_id,
        accepted_revision_id: current.revision_id,
        current_message,
        inputs,
        accepted_inputs,
        current_task,
        execution_deadline: Some(execution_deadline),
        current_interpretation,
        tasks: candidates,
        next_cursor,
    })
}

fn cursor(value: Option<&str>) -> Result<i64> {
    let Some(value) = value else {
        return Ok(0);
    };
    let Cursor::Text { value, id } =
        Cursor::parse(value).map_err(|_invalid| rejected("Invalid task input cursor"))?
    else {
        return Err(rejected("Invalid task input cursor"));
    };
    af_core::id::Uuid7::parse(&id).map_err(|_invalid| rejected("Invalid task input cursor"))?;
    value
        .parse::<i64>()
        .ok()
        .filter(|value| *value >= 0)
        .ok_or_else(|| rejected("Invalid task input cursor"))
}

async fn accepted(
    connection: &mut sqlx::PgConnection,
    current: &guard::LeaseRow,
    input: &TaskInputsQuery,
) -> Result<Vec<TaskAcceptedInput>> {
    let accepted: Vec<InputRow> = sqlx::query_as(include_str!("../sql/accepted_inputs.sql"))
        .bind(&current.task_id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(row::sql_counter(
            af_wire::task::INPUT_BATCH_MAX,
            "input batch limit",
        )?)
        .bind(
            input
                .after_input_sequence
                .map(i64::try_from)
                .transpose()
                .map_err(|_overflow| super::TaskRefusal::StaleState)?,
        )
        .fetch_all(&mut *connection)
        .await
        .map_err(query(
            "recover accepted inputs beyond the incorporated cursor",
        ))?;
    let accepted_inputs = accepted
        .into_iter()
        .map(|item| {
            Ok(TaskAcceptedInput {
                source_admission_id: item.source_admission_id,
                plan_digest: item.plan_digest,
                item_id: item.item_id,
                task: DecisionTaskVersion {
                    task_id: item.task_id,
                    generation: row::counter(item.generation, "task generation")?,
                },
                sequence: row::counter(item.sequence, "input sequence")?,
                action: serde::Deserialize::deserialize(serde::de::value::StrDeserializer::new(
                    item.action.as_str(),
                ))
                .map_err(|source: serde::de::value::Error| {
                    crate::error::Error::from(crate::error::ErrorKind::TaskState { source })
                })?,
                original_message: item.original_message,
                accepted_text: item.accepted_text,
                interpretation: item.interpretation,
                applied: item.applied,
                response_text: item.response_text,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(accepted_inputs)
}

async fn candidates(
    connection: &mut sqlx::PgConnection,
    current: &guard::LeaseRow,
) -> Result<Vec<TaskInputCandidate>> {
    let candidates: Vec<Candidate> = sqlx::query_as(include_str!("../sql/input_candidates.sql"))
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(i64::from(af_wire::task::UNFINISHED_MAX) + 1)
        .fetch_all(&mut *connection)
        .await
        .map_err(query("read native input association candidates"))?;
    if candidates.len() > af_wire::task::UNFINISHED_MAX as usize {
        return Err(super::TaskRefusal::LimitExhausted.into());
    }
    let candidates = candidates
        .into_iter()
        .map(|candidate| {
            Ok(TaskInputCandidate {
                task_id: candidate.id,
                title: candidate.title,
                state: row::state(&candidate.state)?,
                generation: row::counter(candidate.generation, "generation")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(candidates)
}
