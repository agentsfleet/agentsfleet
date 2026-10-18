//! Capacity is counted under the parent lock before intent and its fence commit.

use super::{CONTEXT_WRITE, NewSchedule, Refused, SYNC_LEASE, Schedules, admit, fire_at};
use crate::{DesiredStatus, Result, Schedule, SyncStatus, error, sql};
use afd_core::id::Uuid7;
use afd_db::Precondition;
use jiff::Timestamp;
use sqlx::Acquire as _;

/// A new schedule's fence version: one, never zero.
///
/// The column's own CHECK refuses zero, because "never synced" and "synced at
/// version zero" would otherwise be the same state to a finalize.
const FIRST_VERSION: i64 = 1;

impl Schedules {
    /// Creates a schedule, already claimed by the caller that will register it.
    ///
    /// The counts, the duplicate check and the insert run inside ONE
    /// transaction that has taken the fleet's row — see [`sql::LOCK_FLEET`] on
    /// why a lock on the parent is what makes a count-then-insert atomic.
    ///
    /// `now` mints the new row's id and nothing else: Postgres stamps the row,
    /// and a `once` schedule's instant is counted from that stamp.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a statement that failed, and
    /// a row this build cannot read. A bound the operator hit is `Ok(Err(..))`.
    pub async fn create(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        token: &Uuid7,
        now: Timestamp,
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
        now: Timestamp,
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

        let created = sqlx::query_as::<_, Schedule>(sql::INSERT)
            .bind(schedule_id.as_str())
            .bind(new.fleet.as_str())
            .bind(new.source.as_str())
            .bind(
                new.provider_schedule_id
                    .unwrap_or_else(|| schedule_id.as_str()),
            )
            .bind(new.cron)
            .bind(new.timezone)
            .bind(new.message)
            .bind(DesiredStatus::Active.as_str())
            .bind(SyncStatus::Syncing.as_str())
            .bind(FIRST_VERSION)
            .bind(token.as_str())
            .bind(SYNC_LEASE)
            .bind(new.once)
            .fetch_one(transaction.as_mut())
            .await
            .map_err(error::query(CONTEXT_WRITE))?;
        let created = if created.once {
            fire_at::stamp(&mut transaction, &created, token, created.created_at).await?
        } else {
            created
        };

        transaction
            .commit()
            .await
            .map_err(error::query(CONTEXT_WRITE))?;
        Ok(Ok(created))
    }
}
