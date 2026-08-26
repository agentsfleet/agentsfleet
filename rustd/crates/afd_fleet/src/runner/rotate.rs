//! Atomic runner credential rotation.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;

use crate::error::{Result, query, rejected, runner_not_found};
use crate::runner::Runners;
use crate::runner::token::Minted;
use crate::sql;

const CONTEXT_ROTATE: &str = "runner token rotation";
const DETAIL_REVOKED: &str = "revoked runners cannot rotate credentials";
const STATE_REVOKED: &str = "revoked";
const EVENT_TOKEN_ROTATED: &str = "runner_token_rotated";

impl Runners {
    /// Replaces a runner's stored digest and returns the new credential once.
    ///
    /// The credential value exists only in [`Minted`]; the statement receives
    /// its digest. Rotation and the audit row are one statement, so neither can
    /// become visible without the other.
    ///
    /// # Errors
    /// Refuses a missing or revoked runner and reports entropy or datastore
    /// failures without exposing either credential.
    pub async fn rotate_token(&self, runner: &Uuid7, now: UnixMillis) -> Result<Minted> {
        let token = Minted::draw(self.entropy())?;
        let event_id = self.admin_event_id(now)?;
        let mut connection = self.pool().acquire().await?;
        let row = sqlx::query(sql::runner::ROTATE_RUNNER_TOKEN)
            .bind(runner.as_str())
            .bind(token.digest().as_str())
            .bind(now.as_millis())
            .bind(STATE_REVOKED)
            .bind(event_id.as_str())
            .bind(EVENT_TOKEN_ROTATED)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_ROTATE))?
            .ok_or_else(runner_not_found)?;
        let changed: bool = row.try_get("changed").map_err(query(CONTEXT_ROTATE))?;
        if !changed {
            return Err(rejected(DETAIL_REVOKED));
        }
        tracing::debug!(
            runner_id = runner.as_str(),
            event = "runner_token_rotated",
            "runner credential rotated"
        );
        Ok(token)
    }
}
