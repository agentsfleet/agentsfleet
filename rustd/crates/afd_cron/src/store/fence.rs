//! Taking the fence, and giving it back.
//!
//! Split from [`super`], which owns the table's plain reads and the create. The
//! line is whether a call participates in the single-syncer protocol: a claim
//! takes the token and bumps the version, a finalize gives it back and says
//! whether the push worked, and a delete is a finalize that removes the row
//! instead. Every one of them is conditioned on the fence, and reading them
//! together is how a reader checks that.
//!
//! `Ok(None)` throughout means "not mine, or not there" — see [`super`] on why
//! that is an answer rather than a failure.
//!
//! None of them takes the caller's clock: Postgres stamps the row and judges
//! the lease (see [`sql`] on whose clock).

use afd_core::id::Uuid7;

use afd_db::Precondition;
use sqlx::Acquire as _;

use super::{Change, Refused, SYNC_LEASE, Schedules, fire_at};
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
    ) -> Result<Option<Schedule>> {
        let claimed = self.claim(fleet, schedule, change, token, None).await?;
        // With no guard there is nothing to refuse.
        Ok(claimed.unwrap_or(None))
    }

    /// [`Self::claim_change`] on one transaction, proving `guard` first when
    /// there is one, so a guard that no longer holds writes nothing.
    pub(super) async fn claim(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
        change: Change<'_>,
        token: &Uuid7,
        guard: Option<&dyn Precondition>,
    ) -> Result<core::result::Result<Option<Schedule>, Refused>> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection
            .begin()
            .await
            .map_err(error::query(CONTEXT_CLAIM))?;
        if let Some(guard) = guard
            && !guard
                .holds(&mut transaction)
                .await
                .map_err(error::query(CONTEXT_CLAIM))?
        {
            return Ok(Err(Refused::Unheld));
        }
        let claimed = sqlx::query_as::<_, Schedule>(sql::CLAIM_MUTATION)
            .bind(schedule.as_str())
            .bind(fleet.as_str())
            .bind(change.cron)
            .bind(change.timezone)
            .bind(change.message)
            .bind(change.desired_status.map(DesiredStatus::as_str))
            .bind(SyncStatus::Syncing.as_str())
            .bind(token.as_str())
            .bind(SYNC_LEASE)
            .fetch_optional(transaction.as_mut())
            .await
            .map_err(error::query(CONTEXT_CLAIM))?;
        // Counted from the claim's own stamp; see `fire_at`.
        let claimed = match claimed {
            Some(claimed) if claimed.once && moved(&change) => {
                let from = claimed.updated_at;
                Some(fire_at::stamp(&mut transaction, &claimed, token, from).await?)
            }
            claimed => claimed,
        };
        transaction
            .commit()
            .await
            .map_err(error::query(CONTEXT_CLAIM))?;
        Ok(Ok(claimed))
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
    ) -> Result<Option<Schedule>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_as::<_, Schedule>(sql::CLAIM_CURRENT)
            .bind(schedule.as_str())
            .bind(fleet.as_str())
            .bind(SyncStatus::Syncing.as_str())
            .bind(token.as_str())
            .bind(SYNC_LEASE)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_CLAIM))
    }

    /// Releases a fence a push succeeded under.
    ///
    /// `Ok(None)` when the claim is no longer this caller's: the version
    /// moved on, or the lease expired and another syncer took the row. Not an
    /// error, and not a retry — the newer holder's state is the right one, and
    /// overwriting it is precisely what the fence exists to prevent.
    ///
    /// # Errors
    /// As [`Self::list`].
    ///
    /// `registered` is the key the scheduler filed this schedule under, when
    /// this push learned one. `None` leaves the stored key alone — see
    /// [`sql::FINALIZE_SUCCESS`] on why that is not the same as blanking it.
    pub async fn finalize_synced(
        &self,
        held: &Schedule,
        token: &Uuid7,
        registered: Option<&str>,
    ) -> Result<Option<Schedule>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_as::<_, Schedule>(sql::FINALIZE_SUCCESS)
            .bind(held.schedule_id.as_str())
            .bind(held.resource_version)
            .bind(token.as_str())
            .bind(SyncStatus::Synced.as_str())
            .bind(registered)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_WRITE))
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
    ) -> Result<Option<Schedule>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_as::<_, Schedule>(sql::FINALIZE_FAILURE)
            .bind(held.schedule_id.as_str())
            .bind(held.resource_version)
            .bind(token.as_str())
            .bind(SyncStatus::Failed.as_str())
            .bind(reason)
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_WRITE))
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
            .bind(held.resource_version)
            .bind(token.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_WRITE))?;

        Ok(row.is_some())
    }

    /// Whether `token` still holds `held`.
    ///
    /// `false` is the not-mine state [`Self::finalize_synced`] answers `None`
    /// for, learned before the scheduler is called instead of after.
    ///
    /// # Errors
    /// As [`Self::list`].
    pub async fn holds_fence(&self, held: &Schedule, token: &Uuid7) -> Result<bool> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::HOLDS_FENCE)
            .bind(held.schedule_id.as_str())
            .bind(held.resource_version)
            .bind(token.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_CLAIM))?;

        Ok(row.is_some())
    }
}

/// Whether `change` moves the moment a schedule fires at.
const fn moved(change: &Change<'_>) -> bool {
    change.cron.is_some() || change.timezone.is_some()
}
