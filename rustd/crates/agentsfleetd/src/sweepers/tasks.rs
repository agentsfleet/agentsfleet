//! Native due work stays supervised and performs no autonomous queued writes.

use std::time::Duration;
use afd_fleet::task::{TaskLifecycle as _, Tasks};
use crate::supervisor::Supervisor;

const INTERVAL: Duration = Duration::from_secs(10);
const PASS_DEADLINE: Duration = Duration::from_secs(30);

pub(super) fn spawn(supervisor: &mut Supervisor, tasks: Tasks) {
    supervisor.spawn(super::TASKS, move |stop| async move {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            tokio::select! {
                () = stop.cancelled() => return,
                result = tokio::time::timeout(PASS_DEADLINE, async {
                    let (effects, waits) = tokio::join!(tasks.maintain_effects(), tasks.maintain_waits());
                    if let Err(error) = effects { tracing::warn!(event = "dt_effect_maintenance_deferred", error_code = %error.code()); }
                    if let Err(error) = waits { tracing::warn!(event = "dt_wait_maintenance_deferred", error_code = %error.code()); }
                }) => {
                    if result.is_err() { tracing::warn!(event = "dt_maintenance_deferred", reason = "pass_deadline"); }
                }
            }
        }
    });
}
