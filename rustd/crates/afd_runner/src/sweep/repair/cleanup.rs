//! Forgetting the append-once keys of intents that are fully recorded.
//!
//! Split from `repair` because it is the sweep's other half: dispatch decides
//! what to start, and this decides what may be forgotten. A failure here is
//! reported and swallowed on every path, which is a posture the dispatch side
//! does not share.

use afd_core::clock::UnixMillis;
use afd_redis::streams::OnceScope;
use sqlx::Row as _;

use super::{CLEANUP_BATCH_LIMIT, CONTEXT_CLEANUP, Repairs};
use crate::error::{Result, query};
use crate::sql;

impl Repairs {
    /// Forgets the append-once keys of intents that are fully recorded.
    ///
    /// Answers whether the page was full, which means more are waiting. A
    /// failure here is reported and swallowed: an un-forgotten key costs one
    /// Redis entry until the next pass, and failing the whole sweep over it
    /// would stop dispatching work that is due.
    pub(super) async fn clean(&self, now: UnixMillis) -> bool {
        let Ok(page) = self.cleanup_page(now).await.inspect_err(|failure| {
            tracing::warn!(
                error = %failure,
                event = "repair_verification_cleanup_lookup_failed",
                "the append-once cleanup page could not be read"
            );
        }) else {
            return false;
        };
        if page.is_empty() {
            return false;
        }

        let mut forgotten = Vec::with_capacity(page.len());
        for id in &page {
            match self.streams.forget_once(OnceScope::FleetIntent, id).await {
                Ok(()) => forgotten.push(id.clone()),
                // Left for the next pass: the row keeps its uncleared marker,
                // so nothing is lost by not recording this one.
                Err(failure) => tracing::warn!(
                    verification_id = id,
                    error = %failure,
                    event = "repair_verification_once_key_uncleared",
                    "an append-once key could not be forgotten"
                ),
            }
        }
        if forgotten.is_empty() {
            return false;
        }

        let full = page.len() >= usize::try_from(CLEANUP_BATCH_LIMIT).unwrap_or(usize::MAX);
        if let Err(failure) = self.record_cleanup(&forgotten, now).await {
            tracing::warn!(
                error = %failure,
                event = "repair_verification_cleanup_update_failed",
                "forgotten append-once keys could not be recorded"
            );
            return false;
        }
        full
    }

    /// The intents whose keys are still in Redis.
    async fn cleanup_page(&self, now: UnixMillis) -> Result<Vec<String>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::sweep::SELECT_REPAIR_VERIFICATION_CLEANUP)
            .bind(now.as_millis())
            .bind(CLEANUP_BATCH_LIMIT)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_CLEANUP))?;
        rows.iter()
            .map(|row| row.try_get::<String, _>(0).map_err(query(CONTEXT_CLEANUP)))
            .collect()
    }

    /// Records that a batch of keys is gone.
    async fn record_cleanup(&self, forgotten: &[String], now: UnixMillis) -> Result<()> {
        // Serialised to TEXT and cast by the statement, because a `jsonb` bind
        // would need a sqlx feature this crate does not take for one array of
        // identifiers.
        let identifiers = serde_json::to_string(forgotten)
            .map_err(|_shape| crate::error::vault_data_invalid())?;
        let mut connection = self.database.acquire().await?;
        sqlx::query(sql::sweep::COMPLETE_REPAIR_VERIFICATION_CLEANUP)
            .bind(identifiers)
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_CLEANUP))?;
        Ok(())
    }
}
