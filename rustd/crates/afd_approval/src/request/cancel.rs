//! Setup cancellation closes pending grant cards under the caller's fleet lock.

use crate::{Result, error, grant::IntegrationGrants};
use afd_api_wire::approval::status;
use afd_core::{clock::UnixMillis, id::Uuid7};
use sqlx::PgConnection;

const CONTEXT: &str = "cancel setup approvals";
const CANCEL: &str = "UPDATE core.fleet_approval_gates SET status=$2,resolved_by=$3,detail=$4,updated_at=$5,active_grant_id=NULL WHERE fleet_id=$1::uuid AND status=$6 AND gate_kind=$7";

impl IntegrationGrants {
    /// Closes outstanding setup requests in the transaction that kills their fleet.
    ///
    /// The caller holds the fleet row exclusively. `request_for_setup` takes that
    /// same lock before writing, so a late retry cannot reopen these cards.
    ///
    /// # Errors
    /// Preserves statement failures so the surrounding Kill rolls back atomically.
    pub async fn cancel_setup_on(
        &self,
        connection: &mut PgConnection,
        fleet: &Uuid7,
        now: UnixMillis,
    ) -> Result<()> {
        sqlx::query(CANCEL)
            .bind(fleet.as_str())
            .bind(status::AUTO_KILLED)
            .bind("setup-cancellation")
            .bind("Fleet setup was cancelled")
            .bind(now.as_millis())
            .bind(status::PENDING)
            .bind(super::KIND_INTEGRATION_GRANT)
            .execute(connection)
            .await
            .map_err(error::query(CONTEXT))?;
        Ok(())
    }
}
