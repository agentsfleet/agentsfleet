//! A committed edit already owns the sync fence before any upstream call.

use super::{EditRequest, input, sql};
use crate::store::history::{Fact, Receipt};
use crate::store::{SYNC_LEASE, fire_at};
use crate::{Result, Schedule, Schedules, SyncStatus, error};
use af_core::id::Uuid7;

const CONTEXT: &str = "conditionally edit a schedule";

impl Schedules {
    /// Replaces intent and takes its sync fence on the same transaction.
    ///
    /// A `once` schedule's instant is recounted from the edit's own
    /// `updated_at`, read back after the write — see `store::fire_at`.
    ///
    /// # Errors
    /// Refuses hidden resources, stale/missing tags, invalid edits and live syncs.
    pub async fn edit(&self, request: EditRequest<'_>, token: &Uuid7) -> Result<Schedule> {
        let mut tx = sqlx::Transaction::begin(self.database.acquire().await?, None)
            .await
            .map_err(error::query(CONTEXT))?;
        let row = super::guard::lock(&mut tx, &request).await?;
        let (effective, inherited) = input::replacement(&row, request.patch)?;
        let encoded = inherited.as_ref().map(serde_json::to_string).transpose()?;
        sqlx::query(sql::UPDATE)
            .bind(request.schedule.as_str())
            .bind(effective.cron)
            .bind(effective.timezone)
            .bind(effective.message)
            .bind(effective.status.as_str())
            .bind(encoded)
            .bind(SyncStatus::Syncing.as_str())
            .bind(token.as_str())
            .bind(SYNC_LEASE)
            .execute(&mut *tx)
            .await
            .map_err(error::query(CONTEXT))?;
        let edited = sqlx::query_as::<_, Schedule>(crate::sql::SELECT_ONE)
            .bind(request.schedule.as_str())
            .bind(request.fleet.as_str())
            .fetch_one(&mut *tx)
            .await
            .map_err(error::query(CONTEXT))?;
        let edited = if edited.once {
            let from = edited.updated_at;
            fire_at::stamp(&mut tx, &edited, token, from).await?
        } else {
            edited
        };
        let status = (edited.desired_status != row.desired_status).then_some(edited.desired_status);
        let announcement = Receipt {
            schedule: &edited,
            operation: token.as_str(),
            fact: Fact::mutation(status),
        }
        .record_on(&mut tx, request.workspace)
        .await?;
        self.commit_activity(tx, announcement).await?;
        Ok(edited)
    }
}
