//! Taking the fence, and giving it back.
//!
//! Split from [`super`], which owns the table's plain reads and the create. The
//! line is whether a call participates in the single-syncer protocol: a claim
//! takes the token and bumps the generation, a finalize gives it back and says
//! whether the push worked, and a delete is a finalize that removes the row
//! instead. Every one of them is conditioned on the fence, and reading them
//! together is how a reader checks that.
//!
//! `Ok(None)` throughout means "not mine, or not there" — see [`super`] on why
//! that is an answer rather than a failure.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

use sqlx::FromRow as _;

use super::{Change, SYNC_LEASE_MS, Schedules};
use crate::error::{self, Result};
use crate::model::{DesiredStatus, Schedule, SyncStatus};
use crate::sql;

/// The context a failed claim reports under.
const CONTEXT_CLAIM: &str = "claim a schedule";

/// The context a failed finalize reports under.
const CONTEXT_WRITE: &str = "write a schedule";

impl Schedules {
    /// Applies a change and takes the fence, in one statement.
    ///
    /// `Ok(None)` for a schedule that is absent, belongs to another fleet, or
    /// is held by a syncer whose lease has not run out — three states the
    /// caller renders differently and none of which is a failure.
    ///
    /// # Errors
    /// As [`Self::list`].
    pub async fn claim_change(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
        change: Change<'_>,
        token: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<Schedule>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::CLAIM_MUTATION)
            .bind(schedule.as_str())
            .bind(fleet.as_str())
            .bind(change.cron)
            .bind(change.timezone)
            .bind(change.message)
            .bind(change.desired_status.map(DesiredStatus::as_str))
            .bind(SyncStatus::Syncing.as_str())
            .bind(token.as_str())
            .bind(now.as_millis() + SYNC_LEASE_MS)
            .bind(now.as_millis())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_CLAIM))?;

        row.as_ref()
            .map(Schedule::from_row)
            .transpose()
            .map_err(error::query(CONTEXT_CLAIM))
    }

    /// Takes the fence over the row's current state, changing nothing.
    ///
    /// What `:sync` runs — see [`sql::CLAIM_CURRENT`].
    ///
    /// # Errors
    /// As [`Self::list`].
    pub async fn claim_current(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
        token: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<Schedule>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::CLAIM_CURRENT)
            .bind(schedule.as_str())
            .bind(fleet.as_str())
            .bind(SyncStatus::Syncing.as_str())
            .bind(token.as_str())
            .bind(now.as_millis() + SYNC_LEASE_MS)
            .bind(now.as_millis())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_CLAIM))?;

        row.as_ref()
            .map(Schedule::from_row)
            .transpose()
            .map_err(error::query(CONTEXT_CLAIM))
    }

    /// Releases a fence a push succeeded under.
    ///
    /// `Ok(None)` when the claim is no longer this caller's: the generation
    /// moved on, or the lease expired and another syncer took the row. Not an
    /// error, and not a retry — the newer holder's state is the right one, and
    /// overwriting it is precisely what the fence exists to prevent.
    ///
    /// # Errors
    /// As [`Self::list`].
    pub async fn finalize_synced(
        &self,
        held: &Schedule,
        token: &Uuid7,
        now: UnixMillis,
    ) -> Result<Option<Schedule>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::FINALIZE_SUCCESS)
            .bind(held.schedule_id.as_str())
            .bind(held.generation)
            .bind(token.as_str())
            .bind(SyncStatus::Synced.as_str())
            .bind(now.as_millis())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_WRITE))?;

        row.as_ref()
            .map(Schedule::from_row)
            .transpose()
            .map_err(error::query(CONTEXT_CLAIM))
    }

    /// Releases a fence a push failed under, keeping why on the row.
    ///
    /// # Errors
    /// As [`Self::list`].
    pub async fn finalize_failed(
        &self,
        held: &Schedule,
        token: &Uuid7,
        reason: &str,
        now: UnixMillis,
    ) -> Result<Option<Schedule>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::FINALIZE_FAILURE)
            .bind(held.schedule_id.as_str())
            .bind(held.generation)
            .bind(token.as_str())
            .bind(SyncStatus::Failed.as_str())
            .bind(reason)
            .bind(now.as_millis())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_WRITE))?;

        row.as_ref()
            .map(Schedule::from_row)
            .transpose()
            .map_err(error::query(CONTEXT_CLAIM))
    }

    /// Removes a row whose upstream schedule is confirmed gone.
    ///
    /// Answers whether it removed one. `false` is the same not-mine state
    /// [`Self::finalize_synced`] answers `None` for.
    ///
    /// # Errors
    /// As [`Self::list`].
    pub async fn delete_claimed(&self, held: &Schedule, token: &Uuid7) -> Result<bool> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::DELETE_CLAIMED)
            .bind(held.schedule_id.as_str())
            .bind(held.generation)
            .bind(token.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_WRITE))?;

        Ok(row.is_some())
    }
}
