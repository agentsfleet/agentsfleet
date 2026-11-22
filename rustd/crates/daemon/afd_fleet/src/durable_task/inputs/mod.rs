//! Safe-boundary reads use the source conversation and the existing fleet thread.

mod recover;

use af_core::id::Uuid7;
use af_core::paging::Cursor;
use af_wire::durable_task::{
    DecisionDurableTaskVersion, DurableTaskAcceptedInput, DurableTaskHumanInput,
    DurableTaskInputCandidate, DurableTaskInputsQuery, DurableTaskInputsResponse,
};

use super::{DurableTaskRun, DurableTasks, guard, row, transaction};
use crate::{
    Result,
    error::{query, rejected},
};

#[derive(sqlx::FromRow)]
struct Human {
    id: Uuid7,
    sequence: i64,
    request_json: String,
    producer: String,
}

#[derive(sqlx::FromRow)]
struct Candidate {
    id: Uuid7,
    title: String,
    state: String,
    generation: i64,
}

#[derive(sqlx::FromRow)]
struct InputRow {
    source_turn_id: Uuid7,
    plan_digest: String,
    item_id: Uuid7,
    durable_task_id: Uuid7,
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
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &garde::Valid<DurableTaskInputsQuery>,
) -> Result<DurableTaskInputsResponse<'static>> {
    let after = cursor(input.starting_after.as_deref())?;
    // A `u32` limit always fits a 64-bit `usize`; the policy batch caps it.
    let limit = usize::try_from(input.limit)
        .map_err(|_overflow| rejected("Invalid task input limit"))?
        .min(tasks.dependencies.policy.input_batch());
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::run(&mut tx, run).await?;
    let current_durable_task = guard::context(&mut tx, &current).await?;
    let current_message = current_message(&mut tx, &current).await?;
    let accepted_inputs = accepted(
        &mut tx,
        &current,
        input,
        tasks.dependencies.policy.input_batch(),
    )
    .await?;
    let humans = pending_humans(&mut tx, &current, after, limit, run).await?;
    let (humans, next_cursor) = human_page(humans, limit);
    let sources: Vec<_> = humans
        .iter()
        .map(|human| human.id.clone())
        .chain(std::iter::once(current.source_turn_id.clone()))
        .collect();
    let mut recovered = recover::read(&mut tx, &current, &sources).await?;
    let current_interpretation = recovered.remove(&current.source_turn_id);
    let candidates = candidates(&mut tx, &current).await?;
    let inputs = human_inputs(humans, &mut recovered)?;
    let execution_deadline = crate::run::deadline::read(&mut tx, run.run_id).await?;
    tx.commit().await.map_err(query("finish task input read"))?;
    transaction::recycle(connection);
    Ok(DurableTaskInputsResponse {
        source_turn_id: current.source_turn_id.into(),
        accepted_revision_id: current.revision_id.map(String::from),
        current_message,
        inputs,
        accepted_inputs,
        current_durable_task,
        execution_deadline,
        current_interpretation,
        durable_tasks: candidates,
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
    Uuid7::parse(&id).map_err(|_invalid| rejected("Invalid task input cursor"))?;
    value
        .parse::<i64>()
        .ok()
        .filter(|value| *value >= 0)
        .ok_or_else(|| rejected("Invalid task input cursor"))
}

async fn accepted(
    connection: &mut sqlx::PgConnection,
    current: &guard::RunRow,
    input: &garde::Valid<DurableTaskInputsQuery>,
    batch: usize,
) -> Result<Vec<DurableTaskAcceptedInput>> {
    let accepted: Vec<InputRow> = sqlx::query_as(include_str!("../sql/accepted_inputs.sql"))
        .bind(&current.durable_task_id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        // An unset batch knob is `usize::MAX`, which reads every input.
        .bind(i64::try_from(batch).unwrap_or(i64::MAX))
        // `garde` bounds the cursor at `i64::MAX`, so the cast is exact.
        .bind(input.after_input_sequence.map(u64::cast_signed))
        .fetch_all(&mut *connection)
        .await
        .map_err(query(
            "recover accepted inputs beyond the incorporated cursor",
        ))?;
    accepted
        .into_iter()
        .map(DurableTaskAcceptedInput::try_from)
        .collect()
}

impl TryFrom<InputRow> for DurableTaskAcceptedInput {
    type Error = crate::Error;

    fn try_from(item: InputRow) -> Result<Self> {
        Ok(Self {
            source_turn_id: item.source_turn_id.into(),
            plan_digest: item.plan_digest,
            item_id: item.item_id.into(),
            durable_task: DecisionDurableTaskVersion {
                durable_task_id: item.durable_task_id.into(),
                generation: row::counter(item.generation, "task generation")?,
            },
            sequence: row::counter(item.sequence, "input sequence")?,
            action: row::action(&item.action)?,
            original_message: item.original_message,
            accepted_text: item.accepted_text,
            interpretation: item.interpretation,
            applied: item.applied,
            response_text: item.response_text,
        })
    }
}

async fn candidates(
    connection: &mut sqlx::PgConnection,
    current: &guard::RunRow,
) -> Result<Vec<DurableTaskInputCandidate>> {
    let candidates: Vec<Candidate> = sqlx::query_as(include_str!("../sql/input_candidates.sql"))
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(query("read native input association candidates"))?;
    let candidates = candidates
        .into_iter()
        .map(|candidate| {
            Ok(DurableTaskInputCandidate {
                durable_task_id: candidate.id.into(),
                title: candidate.title,
                state: row::state(&candidate.state)?,
                generation: row::counter(candidate.generation, "generation")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(candidates)
}

async fn pending_humans(
    connection: &mut sqlx::PgConnection,
    current: &guard::RunRow,
    after: i64,
    limit: usize,
    run: DurableTaskRun<'_>,
) -> Result<Vec<Human>> {
    let humans: Vec<Human> = sqlx::query_as(include_str!("../sql/pending_inputs.sql"))
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(&current.source_turn_id)
        .bind(afd_turns::Producer::Steer.as_str())
        .bind(afd_turns::view::ACTOR_MACHINE)
        .bind(format!("{}%", afd_turns::view::ACTOR_PREFIX))
        .bind(after)
        .bind(row::sql_counter(limit, "input batch limit")? + 1)
        .bind(run.run_id)
        .bind(current.fence)
        .bind(crate::run::sql::RUN_STATUS_ACTIVE)
        .bind(None::<&str>.map(guard::wire).transpose()?)
        .bind(true)
        .bind(afd_turns::Producer::SlackMention.as_str())
        .bind(afd_connector::Provider::Slack.id())
        .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
        .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
        .fetch_all(connection)
        .await
        .map_err(query("read complete eligible human inputs"))?;
    Ok(humans)
}

fn human_page(mut humans: Vec<Human>, limit: usize) -> (Vec<Human>, Option<String>) {
    let more = humans.len() > limit;
    if more {
        humans.pop();
    }
    let next_cursor = more.then(|| humans.last()).flatten().map(|last| {
        Cursor::Text {
            value: last.sequence.to_string(),
            id: last.id.clone().into(),
        }
        .to_string()
    });
    (humans, next_cursor)
}

fn human_inputs(
    humans: Vec<Human>,
    recovered: &mut std::collections::BTreeMap<
        Uuid7,
        af_wire::durable_task::DurableTaskAcceptedInterpretation,
    >,
) -> Result<Vec<DurableTaskHumanInput>> {
    let inputs = humans
        .into_iter()
        .map(|source| {
            Ok(DurableTaskHumanInput {
                accepted: recovered.remove(&source.id),
                source_turn_id: source.id.into(),
                sequence: row::counter(source.sequence, "input sequence")?,
                message: super::admit::source::message_for(&source.producer, &source.request_json)?
                    .into_owned(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(inputs)
}

async fn current_message(
    connection: &mut sqlx::PgConnection,
    current: &guard::RunRow,
) -> Result<Option<String>> {
    let source = super::admit::source::load(connection, current, &current.source_turn_id).await?;
    let message = if super::admit::source::check_human(&source).is_ok() {
        Some(source.message()?.into_owned())
    } else {
        None
    };
    Ok(message)
}
