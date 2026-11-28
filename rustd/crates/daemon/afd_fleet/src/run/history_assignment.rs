//! Select the conversation an assignment carries. A host decision without a checkpoint
//! resumes from the asking turn and its refused reads.

use std::borrow::Cow;

use super::{EVENT_HISTORY_UNAVAILABLE, Thread, read};
use af_core::id::Uuid7;
use af_observability::producers::fleet::history as metrics;
use af_wire::{
    run::Turn,
    turn::{QuestionAnswer, TurnKind, message_of},
};
use afd_api_wire::grant::grant_type;
use afd_turns::view::Cursor;

use crate::{
    Result,
    error::query,
    run::{
        RunService,
        envelope::Acquired,
        pull::CheckedFleetWork,
        sql::history::{SELECT_ANSWER_KEY, SELECT_HOST_ASKING_TURN},
    },
};

impl RunService {
    pub(in crate::run) async fn assignment_history(
        &self,
        checked: &CheckedFleetWork,
        checkpointed: bool,
    ) -> Vec<Turn<'static>> {
        let acquired = &checked.acquired;
        if checked.event_type != TurnKind::Continuation || checkpointed {
            let at = Cursor {
                created_at: acquired.turn_created_at,
                turn_id: acquired.turn_id.clone(),
            };
            return turns_before(
                self.thread.as_ref(),
                &acquired.workspace_id,
                &acquired.fleet_id,
                &at,
                checked.event_type,
            )
            .await;
        }
        self.host_asking_turn(acquired)
            .await
            .unwrap_or_else(|failure| unavailable(&failure, &acquired.fleet_id, &acquired.turn_id))
    }

    async fn host_asking_turn(&self, acquired: &Acquired) -> Result<Vec<Turn<'static>>> {
        let mut connection = self.runs.pool().acquire().await?;
        let Some((action, asked)) = answer_ids(&mut connection, acquired).await? else {
            return Ok(Vec::new());
        };
        let asking: Option<(Uuid7, i64, String, String, String, String)> =
            sqlx::query_as(SELECT_HOST_ASKING_TURN)
                .bind(&acquired.workspace_id)
                .bind(&acquired.fleet_id)
                .bind(&action)
                .bind(&asked)
                .bind(afd_approval::KIND_INTEGRATION_GRANT)
                .bind(grant_type::HOST)
                .fetch_optional(&mut *connection)
                .await
                .map_err(query("read the host approval's asking turn"))?;
        drop(connection);
        let Some((turn, sequence, actor, kind, request, answer)) = asking else {
            return Ok(Vec::new());
        };
        let calls = self
            .thread
            .calls_of(&acquired.workspace_id, &acquired.fleet_id, &turn)
            .await?
            .into_iter()
            .filter(|call| {
                call.tool == "web_fetch"
                    && call.status == af_wire::tool_detail::ItemStatus::Failed.as_str()
            })
            .filter_map(super::call)
            .collect();
        Ok(vec![Turn {
            sequence: u64::try_from(sequence).unwrap_or_default(),
            actor: Cow::Owned(actor),
            event_type: TurnKind::parse(&kind),
            message: Cow::Owned(message_of(&request).into_owned()),
            calls,
            answer: Cow::Owned(answer),
        }])
    }
}

async fn answer_ids(
    connection: &mut sqlx::PgConnection,
    acquired: &Acquired,
) -> Result<Option<(Uuid7, Uuid7)>> {
    let asked = serde_json::from_str::<QuestionAnswer<'_>>(&acquired.request_json)
        .ok()
        .and_then(|answer| Uuid7::parse(&answer.asked_turn_id).ok());
    let Some(asked) = asked else { return Ok(None) };
    let key: Option<String> = sqlx::query_scalar(SELECT_ANSWER_KEY)
        .bind(&acquired.workspace_id)
        .bind(&acquired.fleet_id)
        .bind(&acquired.turn_id)
        .bind(afd_turns::Producer::QuestionAnswer.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query("read the continuation producer key"))?;
    Ok(key
        .and_then(|key| Uuid7::parse(&key).ok())
        .map(|action| (action, asked)))
}

/// Read the ordinary history of a chat. Other event kinds carry none.
pub async fn turns_before(
    thread: &dyn Thread,
    workspace: &Uuid7,
    fleet: &Uuid7,
    at: &Cursor,
    event_type: TurnKind,
) -> Vec<Turn<'static>> {
    if event_type != TurnKind::Chat {
        return Vec::new();
    }
    // Boxed: the page walk holds a row across each call read, and every
    // issue path awaits this.
    match Box::pin(read(thread, workspace, fleet, at)).await {
        Ok((turns, bytes)) => {
            metrics::carried(bytes);
            turns
        }
        Err(failure) => unavailable(&failure, fleet, &at.turn_id),
    }
}

fn unavailable(failure: &crate::Error, fleet: &Uuid7, turn: &Uuid7) -> Vec<Turn<'static>> {
    metrics::read_failed();
    let error_code = failure.code().as_str();
    let fleet_id = fleet.as_str();
    let agentsfleet_turn_id = turn.as_str();
    let event = EVENT_HISTORY_UNAVAILABLE;
    tracing::warn!(error_code, fleet_id, agentsfleet_turn_id, event);
    Vec::new()
}
