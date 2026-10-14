//! The plane's answer to every verb the schedules seam declares.
//!
//! Every read and write a route makes names the workspace its caller was
//! proven in, and the fleet is checked against it before any schedule of the
//! fleet is read, claimed or pushed — see `afd_cron`'s `Schedules::holds_fleet`.
//! The lease-guarded writes are the exception, because the lease already
//! binds the fleet to its workspace; they live in `guarded`.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_cron::{
    Change, DesiredStatus, FireTarget, Fired, NewSchedule, Reconciled, Refused,
    Result as CronResult, Schedule,
};
use afd_db::Precondition;

use super::{FleetSchedules, SchedulePlane};

/// A `once` schedule retired after its fire.
const EVENT_RETIRED: &str = "schedule_once_retired";

/// A `once` schedule's moment came while another syncer held its row.
const EVENT_RETIRE_HELD: &str = "schedule_once_retire_held";

impl FleetSchedules for SchedulePlane {
    async fn list(&self, workspace: &Uuid7, fleet: &Uuid7) -> CronResult<Vec<Schedule>> {
        if !self.store().holds_fleet(workspace, fleet).await? {
            return Ok(Vec::new());
        }
        self.store().list(fleet).await
    }

    async fn one(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        schedule: &Uuid7,
    ) -> CronResult<Option<Schedule>> {
        if !self.store().holds_fleet(workspace, fleet).await? {
            return Ok(None);
        }
        self.store().one(fleet, schedule).await
    }

    async fn create(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        now: UnixMillis,
    ) -> CronResult<Result<Reconciled, Refused>> {
        let token = self.token(now)?;
        match self.store().create(workspace, new, &token, now).await? {
            Err(refused) => Ok(Err(refused)),
            Ok(created) => Ok(Ok(self.service.reconcile(&created, &token, now).await?)),
        }
    }

    async fn change(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        schedule: &Uuid7,
        change: Change<'_>,
        now: UnixMillis,
    ) -> CronResult<Option<Reconciled>> {
        if !self.store().holds_fleet(workspace, fleet).await? {
            return Ok(None);
        }
        let token = self.token(now)?;
        let Some(held) = self
            .store()
            .claim_change(fleet, schedule, change, &token, now)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(self.service.reconcile(&held, &token, now).await?))
    }

    fn create_guarded(
        &self,
        workspace: &Uuid7,
        new: NewSchedule<'_>,
        now: UnixMillis,
        guard: &dyn Precondition,
    ) -> impl Future<Output = CronResult<Result<Reconciled, Refused>>> + Send {
        self.create_under(workspace, new, now, guard)
    }

    fn change_guarded(
        &self,
        fleet: &Uuid7,
        schedule: &Uuid7,
        change: Change<'_>,
        now: UnixMillis,
        guard: &dyn Precondition,
    ) -> impl Future<Output = CronResult<Result<Option<Reconciled>, Refused>>> + Send {
        self.change_under(fleet, schedule, change, now, guard)
    }

    async fn sync(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        schedule: &Uuid7,
        now: UnixMillis,
    ) -> CronResult<Option<Reconciled>> {
        if !self.store().holds_fleet(workspace, fleet).await? {
            return Ok(None);
        }
        let token = self.token(now)?;
        let Some(held) = self
            .store()
            .claim_current(fleet, schedule, &token, now)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(self.service.reconcile(&held, &token, now).await?))
    }

    fn fire_target(
        &self,
        schedule: &Uuid7,
    ) -> impl Future<Output = CronResult<Option<FireTarget>>> + Send {
        self.store().fire_target(schedule)
    }

    async fn fire(
        &self,
        schedule: &Uuid7,
        target: &FireTarget,
        message_id: &str,
        now: UnixMillis,
    ) -> CronResult<Fired> {
        let fired = self.fire.deliver(schedule, target, message_id).await?;
        if target.once {
            self.retire(&target.workspace, &target.fleet, schedule, now)
                .await?;
        }
        Ok(fired)
    }

    async fn retire(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        schedule: &Uuid7,
        now: UnixMillis,
    ) -> CronResult<()> {
        let retiring = Change {
            desired_status: Some(DesiredStatus::Deleting),
            ..Change::default()
        };
        let claimed = self
            .change(workspace, fleet, schedule, retiring, now)
            .await?
            .is_some();
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let fleet_id = fleet.as_str();
        let schedule_id = schedule.as_str();
        if claimed {
            tracing::info!(fleet_id, schedule_id, event = EVENT_RETIRED);
            return Ok(());
        }
        // Nothing claimed: the row is gone, which is retired, or another
        // syncer holds it, which is not yet. Read straight from the store: the
        // workspace is the one the fire target's own join resolved, so a second
        // ownership read would ask a question already answered.
        if self.store().one(fleet, schedule).await?.is_none() {
            return Ok(());
        }
        let error_code = afd_core::error_code::SCHEDULE_SYNCING.as_str();
        tracing::warn!(error_code, fleet_id, schedule_id, event = EVENT_RETIRE_HELD);
        Err(afd_cron::error::retire_held())
    }
}
