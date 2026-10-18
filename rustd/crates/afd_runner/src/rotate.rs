//! Atomic runner credential rotation.

use afd_api_wire::admin::AdminState;
use afd_core::id::Uuid7;
use jiff::Timestamp;

use crate::admin::state_wire;
use crate::error::{Result, query, rejected, runner_not_found};
use crate::sql;
use crate::store::Runners;
use afd_auth::credential::CredentialKind;
use afd_auth::minted::Minted;

const CONTEXT_ROTATE: &str = "runner token rotation";
const DETAIL_REVOKED: &str = "revoked runners cannot rotate credentials";
const EVENT_TOKEN_ROTATED: &str = "runner_token_rotated";

impl Runners {
    /// Replaces a runner's stored digest and returns the new credential once.
    ///
    /// The credential value exists only in [`Minted`]; the statement receives
    /// its digest. Rotation and the audit row are one statement, so neither can
    /// become visible without the other. `now` mints the audit row's identifier;
    /// the database stamps both rows.
    ///
    /// # Errors
    /// Refuses a missing or revoked runner and reports entropy or datastore
    /// failures without exposing either credential.
    pub async fn rotate_token(
        &self,
        runner: &Uuid7,
        actor_id: &str,
        now: Timestamp,
    ) -> Result<Minted> {
        let token = Minted::draw(CredentialKind::RunnerToken, self.entropy())?;
        let event_id = self.admin_event_id(now)?;
        let mut connection = self.pool().acquire().await?;
        let changed: bool = sqlx::query_scalar(sql::runner::ROTATE_RUNNER_TOKEN)
            .bind(runner.as_str())
            .bind(token.digest().as_str())
            .bind(state_wire(AdminState::Revoked))
            .bind(event_id.as_str())
            .bind(EVENT_TOKEN_ROTATED)
            .bind(sql::meta::ACTOR_ID)
            .bind(actor_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_ROTATE))?
            .ok_or_else(runner_not_found)?;
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
