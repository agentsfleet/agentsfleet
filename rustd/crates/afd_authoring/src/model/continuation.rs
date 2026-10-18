//! Native provider state belongs to one authorized attempt and unchanged conversation.

use super::{ModelCallInput, ModelCallResult, dispatch::CONTEXT, dispatch_status, sql};
use crate::{
    Result,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    lease::guard::Attempt,
    replay,
};
use afd_wire::authoring::{ENVELOPE_BYTES_MAX, ModelMessage};
use jiff::Timestamp;
use rig_core::{
    completion::{CompletionRequest, CompletionResponse},
    message::{AssistantMessage, Message},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgConnection;

/// One [`sql::CONTINUATIONS`] row, by name.
#[derive(sqlx::FromRow)]
struct Saved {
    native_reply: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    private_expires_at: Timestamp,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Continuation {
    message_index: usize,
    digest: String,
    message: AssistantMessage,
}

#[derive(garde::Validate)]
struct Bounded<'a> {
    #[garde(length(bytes, max = ENVELOPE_BYTES_MAX))]
    json: &'a str,
}

pub(super) fn save(
    response: &CompletionResponse,
    request: &garde::Valid<ModelCallInput>,
    result: &ModelCallResult,
) -> Result<Option<Value>> {
    if result.cut || result.proposal_calls.is_empty() {
        return Ok(None);
    }
    // A validated proposal already proves nonempty assistant content.
    let message = response.head().with_content(response.choice.clone());
    let continuation = Continuation {
        message_index: request.request.messages.len(),
        digest: replay::digest(&(
            &request.request.messages,
            crate::message::normalize(&result.text),
            &result.proposal_calls,
        ))?,
        message,
    };
    let raw = serde_json::to_string(&continuation).map_err(stored)?;
    garde::Unvalidated::new(Bounded { json: &raw }).validate()?;
    Ok(Some(serde_json::from_str(&raw).map_err(stored)?))
}

pub(super) async fn read(
    connection: &mut PgConnection,
    attempt: &Attempt,
    request: &garde::Valid<ModelCallInput>,
    now: Timestamp,
) -> Result<Vec<Continuation>> {
    let rows: Vec<Saved> = sqlx::query_as(sql::CONTINUATIONS)
        .bind(attempt.id.as_str())
        .bind(i64::from(request.call_index))
        .bind(dispatch_status::COMPLETED)
        .bind(super::budget::MAX_CALLS)
        .fetch_all(connection)
        .await
        .map_err(query(CONTEXT))?;
    rows.into_iter()
        .map(|saved| {
            if saved.private_expires_at <= now {
                return Err(ErrorKind::Gone.into());
            }
            garde::Unvalidated::new(Bounded {
                json: &saved.native_reply,
            })
            .validate()?;
            serde_json::from_str(&saved.native_reply).map_err(stored)
        })
        .collect()
}

pub(super) fn restore(
    body: &mut CompletionRequest,
    request: &garde::Valid<ModelCallInput>,
    continuations: Vec<Continuation>,
) -> Result<()> {
    for saved in continuations {
        let index = saved.message_index;
        let Some(ModelMessage::Assistant {
            text,
            proposal_calls,
        }) = request.request.messages.get(index)
        else {
            return Err(ErrorKind::Replay.into());
        };
        let prefix = request
            .request
            .messages
            .get(..index)
            .ok_or(ErrorKind::Replay)?;
        if replay::digest(&(prefix, text, proposal_calls))? != saved.digest {
            return Err(ErrorKind::Replay.into());
        }
        let target = body.chat_history.get_mut(index).ok_or(ErrorKind::Replay)?;
        *target = Message::Assistant(saved.message);
    }
    Ok(())
}
