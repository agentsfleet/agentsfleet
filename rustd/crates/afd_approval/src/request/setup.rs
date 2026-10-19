//! Kill and setup grant requests share the fleet row's lifecycle lock.

use super::{CONTEXT_REQUEST, Requested, Wanted, report, settle};
use crate::{Result, error, grant::IntegrationGrants, grant_sql};
use afd_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;
use jiff::Timestamp;
use sqlx::Acquire as _;

impl IntegrationGrants {
    /// Requests declared access only while the retained fleet is installing.
    ///
    /// A concurrent Kill either waits for this transaction then cancels its
    /// pending card, or wins first and this method writes nothing. It never
    /// replaces a revoked or pending grant with an approved one.
    ///
    /// # Errors
    /// Preserves statement and identifier failures; a terminal setup returns `None`.
    pub async fn request_for_setup(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        wanted: Wanted<'_>,
        now: Timestamp,
    ) -> Result<Option<Requested>> {
        let mut connection = self.database().acquire().await?;
        let mut tx = connection
            .begin()
            .await
            .map_err(error::query(CONTEXT_REQUEST))?;
        let status: Option<String> = sqlx::query_scalar(grant_sql::LOCK_FLEET_FOR_SETUP)
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(error::query(CONTEXT_REQUEST))?;
        if status.as_deref() != Some(FleetStatus::Installing.as_str()) {
            return Ok(None);
        }
        let (raised, found) = self
            .request_on(&mut tx, workspace, fleet, &wanted, now)
            .await?;
        tx.commit().await.map_err(error::query(CONTEXT_REQUEST))?;
        let outcome = settle(&found, raised > 0, fleet, wanted.service);
        report(outcome, fleet, &wanted);
        Ok(Some(outcome))
    }
}
