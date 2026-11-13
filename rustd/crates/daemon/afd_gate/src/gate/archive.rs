//! Archiving a fleet, when the gate decides no further work of its may run.
//!
//! # One gate-driven archive
//!
//! A tripped anomaly rule, one action repeated past its threshold, is the only
//! way the gate archives a fleet. A matching gate rule always asks a person;
//! no rule can archive on its own, so a fleet stops without a human's answer
//! only when it is visibly running away.

use af_core::id::Uuid7;
use afd_fleet_lifecycle::FleetStatus;

use super::sql;
use crate::error::{Result, query};
use crate::gate::store::Gates;

/// Statement name, for the context a query failure carries.
const CONTEXT_ARCHIVE: &str = "fleet archive";

/// A fleet was archived by the gate.
const EVENT_FLEET_ARCHIVED: &str = "gate_fleet_archived";

impl Gates {
    /// Archive `fleet_id` so candidate discovery excludes it for good.
    ///
    /// Archive is terminal: nothing returns the fleet to `active`, so a false
    /// positive is permanent, the price of a trip that the runaway it stopped
    /// cannot undo. Live leases are not cancelled here: the candidate query
    /// admits no new one, and the archive path that cancels leases lives in
    /// `afd_authoring`, which this crate does not reach.
    ///
    /// # Errors
    /// Reports a datastore that would not answer the archive. The caller absorbs
    /// it — the EVENT that tripped the rule ends whether or not the fleet's own
    /// row could be flipped — but absorbing it is the caller's visible decision
    /// rather than something swallowed here.
    ///
    /// The row is stamped on the database clock.
    pub async fn archive(&self, fleet_id: &Uuid7) -> Result<()> {
        let mut connection = self.database().acquire().await?;
        sqlx::query(sql::ARCHIVE_FLEET)
            .bind(FleetStatus::Archived.as_str())
            .bind(fleet_id)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_ARCHIVE))?;

        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let fleet = fleet_id.as_str();
        tracing::warn!(
            event = EVENT_FLEET_ARCHIVED,
            fleet_id = fleet,
            "the fleet ran away and is archived; no further work of its is admitted"
        );

        Ok(())
    }
}
