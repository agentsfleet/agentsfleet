//! Whether a fleet belongs to the workspace a caller was proven in.
//!
//! Split from [`super`] because it is not a schedule read: it answers the
//! question every schedule route asks before naming a schedule at all. The
//! route proves the caller holds the workspace in its path; the fleet in that
//! same path is the caller's claim, and this is where the claim is checked.

use afd_core::id::Uuid7;

use super::{CONTEXT_READ, Schedules};
use crate::error::{self, Result};
use crate::sql;

impl Schedules {
    /// Whether `fleet` belongs to `workspace`.
    ///
    /// `false` for a fleet that does not exist and for one in another workspace
    /// alike, so a caller cannot tell the two apart.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn holds_fleet(&self, workspace: &Uuid7, fleet: &Uuid7) -> Result<bool> {
        let mut connection = self.database.acquire().await?;
        let held = sqlx::query(sql::FLEET_IN_WORKSPACE)
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_READ))?;
        Ok(held.is_some())
    }
}
