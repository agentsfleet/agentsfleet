//! Transfer the current request; historical working context stays with the daemon.

use afd_credential::provider::RegisteredModel;
use afd_wire::authoring::{ContextMessage, ContextRole};
use sqlx::PgConnection;

use super::{selection::Selected, sql};
use crate::{Result, candidate, error::raise::query, types::Candidate};

pub(super) struct Context {
    pub(super) candidate: Option<Candidate>,
    pub(super) messages: Vec<ContextMessage>,
}

pub(super) async fn read(
    connection: &mut PgConnection,
    selected: &Selected,
    _model: &RegisteredModel,
) -> Result<Context> {
    let candidate = match &selected.draft.head_candidate_id {
        Some(id) => Some(candidate::read(connection, &selected.draft.id, id).await?),
        None => None,
    };
    let user: String = sqlx::query_scalar(sql::HISTORY)
        .bind(selected.draft.id.as_str())
        .bind(selected.sequence)
        .fetch_one(connection)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(Context {
        candidate,
        messages: vec![ContextMessage {
            role: ContextRole::User,
            text: user,
        }],
    })
}
