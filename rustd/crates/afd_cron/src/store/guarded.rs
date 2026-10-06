//! The writes a runner's lease guards.
//!
//! A fleet changes its own schedules under the lease it runs. A reclaim can
//! take that lease between the handler's standing check and the write, and an
//! old holder resumed after it would still write. These take the lease as an
//! [`afd_db::Precondition`] and prove it on the write's own transaction; one
//! that no longer holds is [`Refused::Unheld`], and nothing is written.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::Precondition;

use super::{Change, NewSchedule, Refused, Schedules};
use crate::error::Result;
use crate::model::Schedule;

impl Schedules {
    /// [`Self::create`], refused as [`Refused::Unheld`] when `guard` no
    /// longer holds.
    ///
    /// # Errors
    /// As [`Self::create`].
    pub async fn create_guarded(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        token: &Uuid7,
        now: UnixMillis,
        guard: &dyn Precondition,
    ) -> Result<core::result::Result<Schedule, Refused>> {
        self.insert(workspace, new, token, now, Some(guard)).await
    }

    /// [`Self::claim_change`], refused as [`Refused::Unheld`] when `guard` no
    /// longer holds.
    ///
    /// # Errors
    /// As [`Self::claim_change`].
    pub async fn claim_change_guarded(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
        change: Change<'_>,
        token: &Uuid7,
        now: UnixMillis,
        guard: &dyn Precondition,
    ) -> Result<core::result::Result<Option<Schedule>, Refused>> {
        self.claim(fleet, schedule, change, token, now, Some(guard))
            .await
    }
}
