//! Capacity is counted under the parent lock before intent and its fence commit.

use super::{CONTEXT_WRITE, NewSchedule, Refused, SYNC_LEASE_MS, Schedules, admit};
use crate::{DesiredStatus, Result, Schedule, SyncStatus, error, sql};
use afd_core::{clock::UnixMillis, id::Uuid7};
use afd_db::Precondition;
use sqlx::{Acquire as _, FromRow as _};

impl Schedules {
    /// Creates a schedule, already claimed by the caller that will register it.
    ///
    /// The counts, the duplicate check and the insert run inside ONE
    /// transaction that has taken the fleet's row — see [`sql::LOCK_FLEET`] on
    /// why a lock on the parent is what makes a count-then-insert atomic.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a statement that failed, and
    /// a row this build cannot read. A bound the operator hit is `Ok(Err(..))`.
    pub async fn create(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        token: &Uuid7,
        now: UnixMillis,
    ) -> Result<core::result::Result<Schedule, Refused>> {
        self.insert(workspace, new, token, now, None).await
    }

    /// [`Self::create`], proving `guard` after the fleet's row is taken and
    /// before the insert, so a guard that no longer holds writes nothing.
    pub(super) async fn insert(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        token: &Uuid7,
        now: UnixMillis,
        guard: Option<&dyn Precondition>,
    ) -> Result<core::result::Result<Schedule, Refused>> {
        let schedule_id = self.mint(now)?;
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection
            .begin()
            .await
            .map_err(error::query(CONTEXT_WRITE))?;

        if let Some(refused) = admit::refusal(&mut transaction, workspace, &new).await? {
            return Ok(Err(refused));
        }
        if let Some(guard) = guard
            && !guard
                .holds(&mut transaction)
                .await
                .map_err(error::query(CONTEXT_WRITE))?
        {
            return Ok(Err(Refused::Unheld));
        }

        let row = sqlx::query(sql::INSERT)
            .bind(schedule_id.as_str())
            .bind(new.fleet.as_str())
            .bind(new.source.as_str())
            .bind(new.source_key.unwrap_or_else(|| schedule_id.as_str()))
            .bind(new.cron)
            .bind(new.timezone)
            .bind(new.message)
            .bind(DesiredStatus::Active.as_str())
            .bind(SyncStatus::Syncing.as_str())
            // Generation ONE, never zero: the column's own CHECK refuses zero,
            // because "never synced" and "synced at generation zero" would
            // otherwise be the same state to a finalize.
            .bind(1_i64)
            .bind(token.as_str())
            .bind(now.as_millis() + SYNC_LEASE_MS)
            .bind(now.as_millis())
            .bind(new.once)
            .bind(
                new.once
                    .then(|| crate::next::next_fire(new.cron, new.timezone, now))
                    .flatten(),
            )
            .fetch_one(transaction.as_mut())
            .await
            .map_err(error::query(CONTEXT_WRITE))?;

        let created = Schedule::from_row(&row).map_err(error::query(CONTEXT_WRITE))?;
        transaction
            .commit()
            .await
            .map_err(error::query(CONTEXT_WRITE))?;
        Ok(Ok(created))
    }
}
