//! External registration follows activation and keeps its own visible outcome.

use crate::{
    handler::Refusal,
    services::{FleetSchedules as _, Services},
};
use afd_core::id::Uuid7;
use afd_cron::{Schedule, Source, SyncStatus};
use std::time::Duration;

const LIMIT: Duration = Duration::from_secs(5);
const EVENT_READ: &str = "fleet_setup_schedules_read_failed";
/// A setup's schedules saved locally while their upstream sync is still owed.
const EVENT_SYNC_PENDING: &str = "fleet_setup_schedule_sync_pending";

pub(super) async fn after_activation<D: Services>(
    services: &D,
    workspace: &Uuid7,
    fleet: &Uuid7,
    active: bool,
) -> Result<Vec<Schedule>, Refusal> {
    let schedules = services
        .schedules()
        .list(workspace, fleet)
        .await
        .map_err(Refusal::at(EVENT_READ))?;
    if !active {
        return Ok(schedules);
    }
    let sync = async {
        for schedule in schedules.iter().filter(|schedule| {
            schedule.source == Source::Trigger && schedule.sync_status != SyncStatus::Synced
        }) {
            services
                .schedules()
                .sync(workspace, fleet, &schedule.schedule_id, services.now())
                .await?;
        }
        Ok::<_, afd_cron::Error>(())
    };
    match tokio::time::timeout(LIMIT, sync).await {
        Ok(Ok(())) => {}
        Ok(Err(cause)) => {
            tracing::warn!(event=EVENT_SYNC_PENDING,error_code=%cause.code());
        }
        Err(_elapsed) => {
            tracing::warn!(event=EVENT_SYNC_PENDING,error_code=%afd_core::error_code::SCHEDULE_SYNCING);
        }
    }
    services
        .schedules()
        .list(workspace, fleet)
        .await
        .map_err(Refusal::at(EVENT_READ))
}
