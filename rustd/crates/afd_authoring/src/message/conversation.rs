//! A bounded conversation and current review share one creator-checked transaction.

use super::read::{page_on, recent_cursor};
use crate::{
    Access, Result, Setup, access,
    draft::read,
    error::raise::query,
    types::{Draft, Message, MessageQuery},
};
use afd_core::id::Uuid7;
use sqlx::Connection as _;

const CONTEXT: &str = "read editor conversation";

/// Recent private messages and their current editable setup.
#[derive(Debug)]
pub struct Conversation {
    pub(crate) continuation: String,
    after_messages: Vec<String>,
    /// Creator-private current proposal and readiness.
    pub draft: Draft,
    /// At most 25 role rows, including the latest accepted turn.
    pub messages: Vec<Message>,
    /// Boundary to reuse with limit=25 when reconciling these recent rows.
    pub messages_starting_after: Option<String>,
    /// Older messages remain available through the full draft messages resource.
    pub truncated: bool,
}

impl Conversation {
    /// Continuations for a byte-limited response, in the same order as its rows.
    #[must_use]
    pub fn page_boundaries(&self) -> (&str, &[String]) {
        (&self.continuation, &self.after_messages)
    }
}

impl Setup {
    pub(crate) async fn conversation(
        &self,
        access: Access<'_>,
        id: &Uuid7,
    ) -> Result<Conversation> {
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let locked = read::lock(&mut tx, access, &person, id).await?;
        let messages_starting_after = recent_cursor(access, &person.id, &locked)?;
        let request = MessageQuery {
            limit: 25,
            starting_after: messages_starting_after.clone(),
        };
        let messages = page_on(&mut tx, access, &person.id, &locked, &request).await?;
        let after_messages =
            super::read::window_cursors(access, &person.id, &locked, &messages.items)?;
        let draft = self.review_draft_on(&mut tx, locked).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(Conversation {
            continuation: String::new(),
            after_messages,
            draft,
            messages: messages.items,
            truncated: messages_starting_after.is_some(),
            messages_starting_after,
        })
    }
}
