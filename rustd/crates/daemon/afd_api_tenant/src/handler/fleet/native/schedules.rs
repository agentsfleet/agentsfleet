//! External registration follows activation and keeps its own visible outcome.

use crate::{
    handler::Refusal,
    services::{FleetSchedules as _, TenantPlane},
};
use af_core::id::Uuid7;
use afd_cron::Schedule;

const EVENT_READ: &str = "fleet_setup_schedules_read_failed";

/// The fleet's schedules, once an active fleet's trigger schedules have had
/// their one bounded registration; an inactive fleet's are only read.
pub(crate) async fn after_activation<D: TenantPlane>(
    services: &D,
    workspace: &Uuid7,
    fleet: &Uuid7,
    active: bool,
) -> Result<Vec<Schedule>, Refusal> {
    if active {
        services
            .schedules()
            .register_triggers(workspace, fleet, services.now())
            .await
            .map_err(Refusal::at(EVENT_READ))?;
    }
    services
        .schedules()
        .list(workspace, fleet)
        .await
        .map_err(Refusal::at(EVENT_READ))
}
