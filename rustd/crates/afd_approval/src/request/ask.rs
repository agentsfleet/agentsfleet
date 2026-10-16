//! One grant and one decision card, on the caller's existing transaction.

use super::{
    CONTEXT_REQUEST, GRANT_DECISION_WINDOW_MS, KIND_INTEGRATION_GRANT, PROPOSED_ACTION_PREFIX,
    RADIUS_PREFIX, RADIUS_SUFFIX, Wanted,
};
use crate::{Result, error, grant::IntegrationGrants, grant_sql};
use afd_api_wire::grant::status;
use afd_core::{clock::UnixMillis, id::Uuid7};
use sqlx::{PgConnection, Row as _};

impl IntegrationGrants {
    pub(super) async fn request_on(
        &self,
        connection: &mut PgConnection,
        workspace: &Uuid7,
        fleet: &Uuid7,
        wanted: &Wanted<'_>,
        now: UnixMillis,
    ) -> Result<(i64, String)> {
        // Hoisted, not drawn inside the chain: three identifiers are minted
        // here and each is fallible, so naming them says which row is which and
        // keeps the failure ahead of the statement rather than inside it.
        let grant_id = self.mint(now)?;
        let gate_id = self.mint(now)?;
        let action_id = self.mint(now)?;
        sqlx::query(grant_sql::ENSURE_GRANT)
            .bind(grant_id.as_str())
            .bind(fleet.as_str())
            .bind(wanted.service)
            .bind(status::PENDING)
            .bind(wanted.origin.reason())
            .bind(now.as_millis())
            .execute(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_REQUEST))?;
        let row = sqlx::query(grant_sql::REQUEST_GRANT)
            .bind(fleet.as_str())
            .bind(wanted.service)
            .bind(status::PENDING)
            .bind(now.as_millis())
            .bind(gate_id.as_str())
            .bind(workspace.as_str())
            .bind(action_id.as_str())
            .bind(wanted.service)
            .bind(wanted.credential)
            .bind(KIND_INTEGRATION_GRANT)
            .bind(format!("{PROPOSED_ACTION_PREFIX}{}", wanted.service))
            .bind(wanted.evidence())
            .bind(format!("{RADIUS_PREFIX}{}{RADIUS_SUFFIX}", wanted.service))
            .bind(
                now.saturating_add_millis(GRANT_DECISION_WINDOW_MS)
                    .as_millis(),
            )
            .bind(afd_api_wire::approval::status::PENDING)
            .fetch_one(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_REQUEST))?;

        let unreadable = error::query(CONTEXT_REQUEST);
        let raised: i64 = row.try_get(0).map_err(&unreadable)?;
        let found: String = row.try_get(1).map_err(&unreadable)?;

        Ok((raised, found))
    }
}
