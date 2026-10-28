//! Equality-filtered thread pages walk an event-type index before reading bodies.

use super::{Cursor, EventDetailRow, History, THREAD_MAX_LIMIT};
use crate::{Result, error};
use afd_core::id::Uuid7;
use jiff_sqlx::ToSqlx as _;

impl History {
    /// Reads one event type, or the ordinary mixed thread when absent.
    ///
    /// # Errors
    /// Preserves datastore and stored-row failures.
    pub async fn filtered_thread_page(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        cursor: Option<&Cursor>,
        limit: i64,
        event_type: Option<&str>,
    ) -> Result<Vec<EventDetailRow>> {
        let Some(event_type) = event_type else {
            return self.thread_page(workspace, fleet, cursor, limit).await;
        };
        let scoped = match cursor {
            None => sqlx::query(super::statement::SELECT_THREAD_FILTERED)
                .bind(workspace.as_str())
                .bind(fleet.as_str())
                .bind(event_type),
            Some(at) => sqlx::query(super::statement::SELECT_THREAD_FILTERED_AFTER)
                .bind(workspace.as_str())
                .bind(fleet.as_str())
                .bind(event_type)
                .bind(at.created_at.to_sqlx())
                .bind(at.event_id.as_str()),
        };
        let mut connection = self.database.acquire().await?;
        scoped
            .bind(limit.clamp(1, THREAD_MAX_LIMIT + 1))
            .fetch_all(&mut *connection)
            .await
            .map_err(error::query("read filtered fleet messages"))?
            .iter()
            .map(EventDetailRow::read)
            .collect()
    }
}
