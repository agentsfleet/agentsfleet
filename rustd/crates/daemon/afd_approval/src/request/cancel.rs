//! Setup cancellation closes pending grant cards under the caller's fleet lock.

use crate::{Result, error, grant::IntegrationGrants, grant_sql};
use afd_api_wire::approval::status;
use afd_core::id::Uuid7;
use sqlx::PgConnection;

const CONTEXT: &str = "cancel setup approvals";

/// Who a cancelled card records as its resolver.
const CANCELLED_BY: &str = "setup-cancellation";

/// What a cancelled card records as its detail.
const CANCELLED_DETAIL: &str = "Fleet setup was cancelled";

impl IntegrationGrants {
    /// Closes outstanding setup requests in the transaction that kills their fleet.
    ///
    /// The caller holds the fleet row exclusively. `request_for_setup` takes that
    /// same lock before writing, so a late retry cannot reopen these cards. The
    /// cards close on the database clock.
    ///
    /// # Errors
    /// Preserves statement failures so the surrounding Kill rolls back atomically.
    pub async fn cancel_setup_on(
        &self,
        connection: &mut PgConnection,
        fleet: &Uuid7,
    ) -> Result<()> {
        sqlx::query(grant_sql::CANCEL_SETUP_CARDS)
            .bind(fleet.as_str())
            .bind(status::AUTO_KILLED)
            .bind(CANCELLED_BY)
            .bind(CANCELLED_DETAIL)
            .bind(super::KIND_INTEGRATION_GRANT)
            .execute(connection)
            .await
            .map_err(error::query(CONTEXT))?;
        Ok(())
    }
}
