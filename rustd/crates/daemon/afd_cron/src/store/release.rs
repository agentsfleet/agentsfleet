//! The reads a fleet's release makes on its caller's transaction.
//!
//! Split from [`super`]'s pooled reads because the fleet's delete holds the
//! fleet's row `FOR UPDATE` on one connection while its schedules go, and a
//! request holds one connection at a time.

use af_core::id::Uuid7;
use sqlx::PgConnection;

use super::{CONTEXT_READ, Schedules};
use crate::error::{self, Result};
use crate::model::Schedule;
use crate::sql;

impl Schedules {
    /// Whether `fleet` belongs to `workspace`, on the caller's connection.
    ///
    /// # Errors
    /// Reports a statement the datastore refused.
    pub(crate) async fn holds_fleet_on(
        connection: &mut PgConnection,
        workspace: &Uuid7,
        fleet: &Uuid7,
    ) -> Result<bool> {
        let held = sqlx::query(sql::FLEET_IN_WORKSPACE)
            .bind(fleet)
            .bind(workspace)
            .fetch_optional(connection)
            .await
            .map_err(error::query(CONTEXT_READ))?;
        Ok(held.is_some())
    }

    /// Every schedule of `fleet`, oldest first, on the caller's connection.
    ///
    /// # Errors
    /// As [`Self::holds_fleet_on`].
    pub(crate) async fn list_on(
        connection: &mut PgConnection,
        fleet: &Uuid7,
    ) -> Result<Vec<Schedule>> {
        sqlx::query_as::<_, Schedule>(sql::LIST_FOR_FLEET)
            .bind(fleet)
            .fetch_all(connection)
            .await
            .map_err(error::query(CONTEXT_READ))
    }
}
