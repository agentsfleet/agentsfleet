//! The plane's writes under a runner's lease.
//!
//! The guard is proved on the write's own transaction (`afd_cron`'s
//! `store::guarded`), and only a write that landed is reconciled.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_cron::{Change, NewSchedule, Reconciled, Refused, Result as CronResult};
use afd_db::Precondition;

use super::SchedulePlane;

impl SchedulePlane {
    /// Creates a fleet's schedule under `guard` and registers it upstream.
    pub(super) async fn create_under(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        now: UnixMillis,
        guard: &dyn Precondition,
    ) -> CronResult<Result<Reconciled, Refused>> {
        let token = self.token(now)?;
        match self
            .store()
            .create_guarded(workspace, new, &token, now, guard)
            .await?
        {
            Err(refused) => Ok(Err(refused)),
            Ok(created) => Ok(Ok(self.service.reconcile(&created, &token, now).await?)),
        }
    }

    /// Changes a fleet's schedule under `guard` and pushes the result.
    pub(super) async fn change_under(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
        change: Change<'_>,
        now: UnixMillis,
        guard: &dyn Precondition,
    ) -> CronResult<Result<Option<Reconciled>, Refused>> {
        let token = self.token(now)?;
        let held = match self
            .store()
            .claim_change_guarded(fleet, schedule, change, &token, now, guard)
            .await?
        {
            Err(refused) => return Ok(Err(refused)),
            Ok(None) => return Ok(Ok(None)),
            Ok(Some(held)) => held,
        };
        Ok(Ok(Some(self.service.reconcile(&held, &token, now).await?)))
    }
}
