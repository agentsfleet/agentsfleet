//! A signed callback resolves its schedule and fleet together.

use super::{CONTEXT_READ, FireTarget, Schedules};
use crate::error::{self, COLUMN_DESIRED_STATUS, COLUMN_FLEET, COLUMN_WORKSPACE};
use crate::{DesiredStatus, Result, sql};
use afd_core::id::Uuid7;
use sqlx::Row as _;

/// The columns [`Schedules::fire_target`] reads its answer from.
///
/// Named, not positional — see [`decode`] on why an index makes the statement's
/// column order load-bearing.
const COLUMN_MESSAGE: &str = "message";
/// See [`COLUMN_MESSAGE`].
const COLUMN_ONCE: &str = "once";
/// See [`COLUMN_MESSAGE`].
const COLUMN_FLEET_STATUS: &str = "status";

impl Schedules {
    /// What a signed fire resolves to.
    ///
    /// `Ok(None)` for a schedule this daemon has no row for — a callback the
    /// external scheduler kept after the schedule was deleted. Dropped rather
    /// than refused: the sender is correctly configured and acting on what it
    /// was last told.
    ///
    /// # Errors
    /// As [`Self::list`].
    pub async fn fire_target(&self, schedule: &Uuid7) -> Result<Option<FireTarget>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::FIRE_TARGET)
            .bind(schedule.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_READ))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let unreadable = error::query(CONTEXT_READ);
        let fleet: String = row.try_get(COLUMN_FLEET).map_err(&unreadable)?;
        let workspace: String = row.try_get(COLUMN_WORKSPACE).map_err(&unreadable)?;
        let message: String = row.try_get(COLUMN_MESSAGE).map_err(&unreadable)?;
        let once: bool = row.try_get(COLUMN_ONCE).map_err(&unreadable)?;
        let desired: String = row.try_get(COLUMN_DESIRED_STATUS).map_err(&unreadable)?;
        let fleet_status: String = row.try_get(COLUMN_FLEET_STATUS).map_err(&unreadable)?;

        Ok(Some(FireTarget {
            fleet: Uuid7::parse(&fleet).map_err(|_shape| error::row_unreadable(COLUMN_FLEET))?,
            workspace: Uuid7::parse(&workspace)
                .map_err(|_shape| error::row_unreadable(COLUMN_WORKSPACE))?,
            message,
            once,
            desired_status: DesiredStatus::parse(&desired)
                .ok_or_else(|| error::row_unreadable(COLUMN_DESIRED_STATUS))?,
            fleet_status,
        }))
    }
}
