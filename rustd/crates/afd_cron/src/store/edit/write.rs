//! A committed edit already owns the sync fence before any upstream call.

use super::{EditRequest, input, sql};
use crate::{Result, Schedule, Schedules, SyncStatus, error};
use afd_core::id::Uuid7;
use sqlx::{Acquire as _, FromRow as _};

const CONTEXT: &str = "conditionally edit a schedule";

impl Schedules {
    /// Replaces intent and takes its sync fence on the same transaction.
    ///
    /// # Errors
    /// Refuses hidden resources, stale/missing tags, invalid edits and live syncs.
    pub async fn edit(&self, request: EditRequest<'_>, token: &Uuid7) -> Result<Schedule> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(error::query(CONTEXT))?;
        let row = super::guard::lock(&mut tx, &request).await?;
        let (effective, inherited) = input::replacement(&row, request.patch)?;
        let encoded = inherited.as_ref().map(serde_json::to_string).transpose()?;
        let fire_at = row
            .once
            .then(|| crate::next::next_fire(&effective.cron, &effective.timezone, request.now))
            .flatten();
        sqlx::query(sql::UPDATE)
            .bind(request.schedule.as_str())
            .bind(effective.cron)
            .bind(effective.timezone)
            .bind(effective.message)
            .bind(effective.status.as_str())
            .bind(encoded)
            .bind(SyncStatus::Syncing.as_str())
            .bind(token.as_str())
            .bind(crate::SYNC_LEASE_MS)
            .bind(request.now.as_millis())
            .bind(fire_at)
            .execute(&mut *tx)
            .await
            .map_err(error::query(CONTEXT))?;
        let row = sqlx::query(crate::sql::SELECT_ONE)
            .bind(request.schedule.as_str())
            .bind(request.fleet.as_str())
            .fetch_one(&mut *tx)
            .await
            .map_err(error::query(CONTEXT))?;
        let row = Schedule::from_row(&row).map_err(error::query(CONTEXT))?;
        tx.commit().await.map_err(error::query(CONTEXT))?;
        Ok(row)
    }
}
