//! Editor history is bounded and always filtered by the authenticated creator.

use super::Creations;
use crate::{
    Access, Conversation, Result,
    types::{ListQuery, Page},
};
use afd_core::id::Uuid7;
use futures_util::future::try_join_all;

impl Creations {
    /// Reads three recent conversations for this creator and fleet, with an opaque continuation.
    ///
    /// # Errors
    /// Refuses foreign continuations, revoked membership and inaccessible drafts.
    pub async fn conversations(
        &self,
        access: Access<'_>,
        fleet: &Uuid7,
        after: Option<String>,
    ) -> Result<Page<Conversation>> {
        let page = self
            .setup
            .drafts
            .list(
                access,
                ListQuery {
                    limit: 3,
                    state: None,
                    linked_fleet_id: Some(fleet.clone()),
                    q: None,
                    starting_after: after,
                },
            )
            .await?;
        let items = try_join_all(page.items.iter().map(|card| async {
            let mut conversation = self.setup.conversation(access, &card.id).await?;
            conversation.continuation.clone_from(&card.continuation);
            Ok::<_, crate::Error>(conversation)
        }))
        .await?;
        Ok(Page {
            items,
            total: None,
            next_cursor: page.next_cursor,
        })
    }
}
