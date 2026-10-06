//! One actor's events in one fleet, newest first: the read behind a schedule's
//! run list, where the actor is `cron:<schedule_id>`.

use afd_core::id::Uuid7;

use super::statement::{SELECT_FLEET_PAGE_OF_ACTOR, SELECT_FLEET_PAGE_OF_ACTOR_AFTER};
use super::{Cursor, EventRow, History, MAX_LIMIT};
use crate::error::{self, Result};

/// What the read was doing, for the operator's log line.
const CONTEXT_ACTOR_PAGE: &str = "read one actor's events in a fleet";

impl History {
    /// `actor`'s events in one fleet, newest first, matched exactly.
    ///
    /// # Errors
    /// As [`Self::page_for_fleet`].
    pub async fn page_of_actor(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        actor: &str,
        cursor: Option<&Cursor>,
        limit: i64,
    ) -> Result<Vec<EventRow>> {
        let query = match cursor {
            None => sqlx::query(SELECT_FLEET_PAGE_OF_ACTOR)
                .bind(workspace.as_str())
                .bind(fleet.as_str()),
            Some(at) => sqlx::query(SELECT_FLEET_PAGE_OF_ACTOR_AFTER)
                .bind(workspace.as_str())
                .bind(fleet.as_str())
                .bind(at.created_at)
                .bind(at.event_id.as_str()),
        };
        let mut connection = self.database.acquire().await?;
        let rows = query
            .bind(actor)
            .bind(limit.clamp(1, MAX_LIMIT))
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_ACTOR_PAGE))?;
        rows.iter().map(EventRow::read).collect()
    }
}
