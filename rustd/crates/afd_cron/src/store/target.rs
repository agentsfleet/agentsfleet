//! A signed callback resolves its schedule and fleet together.

use super::{CONTEXT_READ, FireTarget, Schedules};
use crate::error::{self, COLUMN_DESIRED_STATUS, COLUMN_FLEET, COLUMN_WORKSPACE};
use crate::{DesiredStatus, Result, sql};
use afd_core::id::Uuid7;

/// What [`sql::FIRE_TARGET`] answers, by column name, before its words are
/// read.
#[derive(Debug, sqlx::FromRow)]
struct FireRow {
    /// The fleet to wake, as text.
    fleet_id: String,
    /// The fleet's workspace, as text.
    workspace_id: String,
    /// What the fleet is asked to do.
    message: String,
    /// Whether the schedule retires after this fire.
    once: bool,
    /// The schedule's intent word.
    desired_status: String,
    /// The fleet's own status word.
    status: String,
}

impl FireRow {
    /// The target this row names; ids and the intent word parse here.
    fn parse(self) -> Result<FireTarget> {
        Ok(FireTarget {
            fleet: Uuid7::parse(&self.fleet_id)
                .map_err(|_shape| error::row_unreadable(COLUMN_FLEET))?,
            workspace: Uuid7::parse(&self.workspace_id)
                .map_err(|_shape| error::row_unreadable(COLUMN_WORKSPACE))?,
            message: self.message,
            once: self.once,
            desired_status: DesiredStatus::parse(&self.desired_status)
                .ok_or_else(|| error::row_unreadable(COLUMN_DESIRED_STATUS))?,
            fleet_status: self.status,
        })
    }
}

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
        sqlx::query_as::<_, FireRow>(sql::FIRE_TARGET)
            .bind(schedule.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_READ))?
            .map(FireRow::parse)
            .transpose()
    }
}
