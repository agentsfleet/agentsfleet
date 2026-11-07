//! Schedule facts share their owning transaction and the existing history path.

mod transaction;

use af_core::id::Uuid7;
use afd_api_wire::activity::{FleetActivity, ScheduleChange, ScheduleSnapshot};
use afd_events::activity::Announcement;
use sqlx::PgConnection;

use crate::{DesiredStatus, Result, Schedule, SyncStatus, error};

/// A native mutation's confirmed fact; external responses never supply its text.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Fact<'a> {
    Changed(ScheduleChange),
    Synced,
    Failed(&'a str),
    Removed,
}

/// Display facts and identity proven by one native mutation or sync claim.
#[derive(Debug)]
pub(crate) struct Receipt<'a> {
    pub(crate) schedule: &'a Schedule,
    pub(crate) operation: &'a str,
    pub(crate) fact: Fact<'a>,
}

impl Receipt<'_> {
    pub(crate) async fn record_on(
        self,
        connection: &mut PgConnection,
        workspace: &Uuid7,
    ) -> Result<Announcement> {
        let identity = format!(
            "schedule:{}:{}:{}:{}",
            self.fact.key(),
            self.schedule.schedule_id,
            self.schedule.resource_version,
            self.operation
        );
        afd_events::activity::record_on(
            connection,
            workspace,
            &self.schedule.fleet_id,
            &identity,
            self.payload(),
        )
        .await
        .map_err(error::query(super::CONTEXT_WRITE))
    }

    fn payload(&self) -> FleetActivity<'_> {
        let mut schedule = self.snapshot();
        match self.fact {
            Fact::Changed(change) => FleetActivity::ScheduleChanged { change, schedule },
            Fact::Synced if self.schedule.desired_status == DesiredStatus::Active => {
                FleetActivity::ScheduleRegistered { schedule }
            }
            Fact::Synced => FleetActivity::ScheduleUnregistered { schedule },
            Fact::Failed(reason) => FleetActivity::ScheduleSyncFailed {
                schedule,
                last_error: reason.into(),
            },
            Fact::Removed => {
                schedule.sync = SyncStatus::Synced.as_str().into();
                if self.schedule.once && self.schedule.desired_status == DesiredStatus::Active {
                    schedule.status = "expired".into();
                    FleetActivity::ScheduleExpired { schedule }
                } else {
                    schedule.status = "removed".into();
                    FleetActivity::ScheduleRemoved { schedule }
                }
            }
        }
    }

    fn snapshot(&self) -> ScheduleSnapshot<'_> {
        ScheduleSnapshot {
            schedule_id: self.schedule.schedule_id.as_str().into(),
            description: crate::run::describe(&self.schedule.cron, &self.schedule.timezone).into(),
            message: self.schedule.message.as_str().into(),
            status: self.schedule.desired_status.as_str().into(),
            sync: self.schedule.sync_status.as_str().into(),
        }
    }
}

impl Fact<'_> {
    pub(crate) const fn mutation(status: Option<DesiredStatus>) -> Self {
        Self::Changed(match status {
            Some(DesiredStatus::Paused) => ScheduleChange::Paused,
            Some(DesiredStatus::Deleting) => ScheduleChange::RemovalRequested,
            Some(DesiredStatus::Active) | None => ScheduleChange::Changed,
        })
    }

    const fn key(self) -> &'static str {
        match self {
            Self::Changed(ScheduleChange::Created) => "created",
            Self::Changed(ScheduleChange::Changed) => "changed",
            Self::Changed(ScheduleChange::Paused) => "paused",
            Self::Changed(ScheduleChange::RemovalRequested) => "removal_requested",
            Self::Synced => "synced",
            Self::Failed(_) => "failed",
            Self::Removed => "removed",
        }
    }
}
