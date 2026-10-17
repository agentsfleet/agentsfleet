//! Fail oversized context before admission; never silently drop earlier messages.

use afd_credential::provider::RegisteredModel;
use afd_wire::authoring::{ContextMessage, ContextRole, ENVELOPE_BYTES_MAX};
use sqlx::PgConnection;

use super::{selection::Selected, sql};
use crate::{
    Result, candidate,
    error::{
        ErrorKind,
        raise::{query, stored},
    },
    model::{self, budget},
    types::Candidate,
};

const HISTORY_ROWS_MAX: i64 = 64;
const FIXED_INPUT_TOKENS: i64 = 2048 + 4096;

pub(super) struct Context {
    pub(super) candidate: Option<Candidate>,
    pub(super) messages: Vec<ContextMessage>,
}

#[derive(garde::Validate)]
struct Envelope<'a> {
    #[garde(length(bytes, max = ENVELOPE_BYTES_MAX))]
    text: &'a str,
}

pub(super) async fn read(
    connection: &mut PgConnection,
    selected: &Selected,
    model: &RegisteredModel,
) -> Result<Context> {
    let candidate = match &selected.draft.current_candidate_id {
        Some(id) => Some(candidate::read(connection, &selected.draft.id, id).await?),
        None => None,
    };
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(sql::HISTORY)
        .bind(selected.draft.id.as_str())
        .bind(selected.sequence)
        .bind(HISTORY_ROWS_MAX + 1)
        .fetch_all(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    if i64::try_from(rows.len()).map_err(|_overflow| ErrorKind::Token)? > HISTORY_ROWS_MAX {
        return Err(ErrorKind::Token.into());
    }
    let mut messages = Vec::new();
    for (user, answer) in rows {
        messages.push(ContextMessage {
            role: ContextRole::User,
            text: user,
        });
        if let Some(text) = answer {
            messages.push(ContextMessage {
                role: ContextRole::Assistant,
                text,
            });
        }
    }
    let context = Context {
        candidate,
        messages,
    };
    fits(&context, model)?;
    Ok(context)
}

fn fits(context: &Context, model: &RegisteredModel) -> Result<()> {
    let text = serde_json::to_string(&(&context.messages, &context.candidate)).map_err(stored)?;
    if garde::Unvalidated::new(Envelope { text: &text })
        .validate()
        .is_err()
    {
        return Err(ErrorKind::Token.into());
    }
    let tokens = i64::try_from(model::input_tokens(&model.model, &text))
        .map_err(|_overflow| ErrorKind::Token)?;
    let framed = tokens + FIXED_INPUT_TOKENS;
    if framed > budget::INPUT_PER_CALL || framed >= i64::from(model.context_window_tokens) {
        return Err(ErrorKind::Token.into());
    }
    Ok(())
}
