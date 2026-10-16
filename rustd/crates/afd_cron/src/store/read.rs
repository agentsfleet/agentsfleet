//! Reads share the schedule decoder and never reach the external scheduler.

use super::{CONTEXT_READ, Schedules};
use crate::{Result, Schedule, error, sql};
use afd_core::id::Uuid7;
use sqlx::FromRow as _;

impl Schedules {
    /// This fleet's schedules, oldest first.
    ///
    /// # Errors
    /// Reports a datastore that would not answer and a row this build cannot
    /// read.
    pub async fn list(&self, fleet: &Uuid7) -> Result<Vec<Schedule>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::LIST_FOR_FLEET)
            .bind(fleet.as_str())
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_READ))?;

        rows.iter()
            .map(Schedule::from_row)
            .collect::<sqlx::Result<Vec<_>>>()
            .map_err(error::query(CONTEXT_READ))
    }

    /// One schedule of this fleet's.
    ///
    /// # Errors
    /// As [`Self::list`].
    pub async fn one(&self, fleet: &Uuid7, schedule: &Uuid7) -> Result<Option<Schedule>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::SELECT_ONE)
            .bind(schedule.as_str())
            .bind(fleet.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_READ))?;

        row.as_ref()
            .map(Schedule::from_row)
            .transpose()
            .map_err(error::query(CONTEXT_READ))
    }
}
