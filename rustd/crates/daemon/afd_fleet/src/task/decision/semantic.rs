//! Advisory answer usability never prevents original-owner financial recording.

use af_wire::{
    decision::{
        DecisionAssignment, DecisionOutcome, DecisionPurpose, DecisionRequest, DecisionResult,
        answer_matches,
    },
    task::DecisionRecordDisposition,
};
use sqlx::{PgConnection, types::Json};

use super::{
    context::{AssessedContext, TaskStamp},
    row::Call,
    sql,
};
use crate::{
    Result,
    error::query,
    task::{TaskScope, Tasks, transaction},
};

#[derive(sqlx::FromRow)]
struct Owner {
    revision_id: Option<String>,
    task_id: Option<String>,
    decision_snapshot: Option<Json<DecisionAssignment>>,
}

pub(super) async fn disposition(
    tasks: &Tasks,
    call: &Call,
    request: &DecisionRequest,
    result: &DecisionResult,
) -> DecisionRecordDisposition {
    if !call.accounted || !answers_match(call, request, result) {
        return DecisionRecordDisposition::Unusable;
    }
    match current(tasks, call).await {
        Ok(true) => DecisionRecordDisposition::Current,
        Ok(false) => DecisionRecordDisposition::Stale,
        Err(fault) => {
            tracing::warn!(event = "dt_decision_context_unavailable", decision_call_id = call.id.as_str(),
                error_code = %fault.code(), reason = "semantic_lookup_failed");
            DecisionRecordDisposition::Unusable
        }
    }
}

pub(super) fn answers_match(
    call: &Call,
    request: &DecisionRequest,
    result: &DecisionResult,
) -> bool {
    let DecisionOutcome::Answered { answers } = &result.result else {
        return false;
    };
    result
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
        })
}

async fn current(tasks: &Tasks, call: &Call) -> Result<bool> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let result = current_on(&mut connection, call).await;
    transaction::recycle(connection);
    result
}

pub(super) async fn current_on(connection: &mut PgConnection, call: &Call) -> Result<bool> {
    let mut tx = transaction::begin(connection).await?;
    let owner: Option<Owner> = sqlx::query_as(sql::SEMANTIC_OWNER)
        .bind(&call.id)
        .bind(crate::lease::sql::LEASE_STATUS_ACTIVE)
        .bind(crate::lease::sql::FLEET_STATUS_ACTIVE)
        .bind(crate::lease::sql::RUNNER_STATUS_ACTIVE)
        .bind(afd_admission::Producer::SlackMention.as_str())
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
        || owner.task_id.as_deref() != saved.execution.as_ref().map(|task| task.task_id.as_str())
    {
        return Ok(false);
    }
    let execution = execution(&mut tx, call).await?;
    if execution != saved.execution {
        return Ok(false);
    }
    let revision = if saved.assessment.purpose() == DecisionPurpose::Goal {
        execution
            .as_ref()
            .map(|task| task.accepted_revision_id.as_str())
    } else {
        owner.revision_id.as_deref()
    };
    if revision != Some(saved.accepted_revision_id.as_str()) {
        return Ok(false);
    }
    candidates_match(&mut tx, call, saved).await
}

async fn execution(connection: &mut PgConnection, call: &Call) -> Result<Option<TaskStamp>> {
    let Some(task) = &call.context_snapshot.0.execution else {
        return Ok(None);
    };
    sqlx::query_as(sql::SEMANTIC_TASK)
        .bind(&task.task_id)
        .bind(&call.fleet_id)
        .bind(&call.workspace_id)
        .bind(afd_admission::Producer::SlackMention.as_str())
        .bind(afd_connector::Provider::Slack.id())
        .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
        .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
        .fetch_optional(connection)
        .await
        .map_err(query(
            "read the assessed task generation and evidence cursors",
        ))
}

async fn candidates_match(
    connection: &mut PgConnection,
    call: &Call,
    saved: &AssessedContext,
) -> Result<bool> {
    let scope = TaskScope {
        fleet_id: Call::id(&call.fleet_id, "fleet_id")?,
        workspace_id: Call::id(&call.workspace_id, "workspace_id")?,
    };
    let candidates =
        super::prepare::candidates(connection, &scope, saved.assessment.purpose()).await?;
    Ok(candidates
        .iter()
        .map(|candidate| &candidate.stamp)
        .eq(saved.candidates.iter()))
}
