//! Advisory answer usability never prevents original-owner financial recording.

use af_core::id::Uuid7;
use af_wire::{
    decision::{
        DecisionAnswer, DecisionAssignment, DecisionOutcome, DecisionRequest, DecisionResult,
        answer_matches,
    },
    durable_task::DecisionRecordDisposition,
};
use sqlx::{PgConnection, types::Json};
use std::collections::BTreeMap;

use super::{context::DurableTaskStamp, row::Call, sql};
use crate::{
    Result,
    durable_task::{DurableTasks, transaction},
    error::query,
};

#[derive(sqlx::FromRow)]
struct Owner {
    durable_task_id: Option<Uuid7>,
    decision_snapshot: Option<Json<DecisionAssignment>>,
}

pub(super) async fn disposition(
    tasks: &DurableTasks,
    call: &Call,
    request: &DecisionRequest,
    result: &DecisionResult,
) -> DecisionRecordDisposition {
    if !call.accounted || answered(call, request, result).is_none() {
        return DecisionRecordDisposition::Unusable;
    }
    match current(tasks, call).await {
        Ok(true) => DecisionRecordDisposition::Current,
        Ok(false) => DecisionRecordDisposition::Stale,
        Err(fault) => {
            let (decision_call_id, error_code) = (call.id.as_str(), fault.code());
            {
                let (event, reason) = (
                    &"dt_decision_context_unavailable",
                    &"semantic_lookup_failed",
                );
                tracing::warn!(event, decision_call_id, %error_code, reason);
            };
            DecisionRecordDisposition::Unusable
        }
    }
}

/// `result`'s answers, when it answered every question `request` asked, in the
/// question's kind, from a model `call` accepts, with known usage.
pub(super) fn answered<'a>(
    call: &Call,
    request: &DecisionRequest,
    result: &'a DecisionResult,
) -> Option<&'a BTreeMap<String, DecisionAnswer>> {
    let DecisionOutcome::Answered { answers } = &result.result else {
        return None;
    };
    let matched = result
        .usage
        .is_some_and(|usage| usage.uncached_input().is_some())
        && result.reported_model.as_ref().is_some_and(|model| {
            call.profile_snapshot
                .0
                .profile
                .accepted_models
                .contains(model)
        })
        && answers.len() == request.questions.len()
        && request.questions.iter().all(|named| {
            answers
                .get(&named.name)
                .is_some_and(|answer| answer_matches(&named.question, answer))
        });
    matched.then_some(answers)
}

async fn current(tasks: &DurableTasks, call: &Call) -> Result<bool> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let result = current_on(&mut connection, call).await;
    transaction::recycle(connection);
    result
}

pub(super) async fn current_on(connection: &mut PgConnection, call: &Call) -> Result<bool> {
    let mut tx = transaction::begin(connection).await?;
    let owner: Option<Owner> = sqlx::query_as(sql::SEMANTIC_OWNER)
        .bind(&call.id)
        .bind(crate::run::sql::RUN_STATUS_ACTIVE)
        .bind(crate::run::sql::FLEET_STATUS_ACTIVE)
        .bind(crate::run::sql::RUNNER_STATUS_ACTIVE)
        .bind(afd_turns::Producer::SlackMention.as_str())
        .bind(afd_connector::Provider::Slack.id())
        .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
        .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("read current decision owner without financial locks"))?;
    let Some(owner) = owner else {
        return Ok(false);
    };
    let saved = &call.context_snapshot.0;
    if owner
        .decision_snapshot
        .as_ref()
        .map(|assignment| &assignment.0)
        != Some(&call.profile_snapshot.0)
        || owner.durable_task_id.as_ref()
            != saved.execution.as_ref().map(|task| &task.durable_task_id)
    {
        return Ok(false);
    }
    let execution = execution(&mut tx, call).await?;
    Ok(execution == saved.execution)
}

async fn execution(connection: &mut PgConnection, call: &Call) -> Result<Option<DurableTaskStamp>> {
    let Some(task) = &call.context_snapshot.0.execution else {
        return Ok(None);
    };
    sqlx::query_as(sql::SEMANTIC_DURABLE_TASK)
        .bind(&task.durable_task_id)
        .bind(&call.fleet_id)
        .bind(&call.workspace_id)
        .bind(afd_turns::Producer::SlackMention.as_str())
        .bind(afd_connector::Provider::Slack.id())
        .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
        .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
        .fetch_optional(connection)
        .await
        .map_err(query(
            "read the assessed task generation and evidence cursors",
        ))
}
