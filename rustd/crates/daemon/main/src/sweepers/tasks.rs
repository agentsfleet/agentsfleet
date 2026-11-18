//! Native due work stays supervised and performs no autonomous queued writes.

use crate::supervisor::Supervisor;
use afd_fleet::task::{TaskLifecycle as _, Tasks};
use std::time::Duration;

pub(super) fn spawn(
    supervisor: &mut Supervisor,
    tasks: Tasks,
    startup: &super::schedule::Startup,
    settings: crate::config::maintenance::DurableTasks,
) {
    let mut interval =
        startup.interval(super::TASKS, Duration::from_secs(settings.interval_seconds));
    let pass_timeout = Duration::from_secs(settings.pass_timeout_seconds);
    supervisor.spawn(super::TASKS, async move |stop| {
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            tokio::select! {
                () = stop.cancelled() => return,
                result = tokio::time::timeout(pass_timeout, async {
                    let (effects, waits) = tokio::join!(tasks.maintain_effects(), tasks.maintain_waits());
                    if let Err(error) = effects {
                        let error_code = error.code();
                        tracing::warn!(event = "dt_effect_maintenance_deferred", %error_code);
                    }
                    if let Err(error) = waits {
                        let error_code = error.code();
                        tracing::warn!(event = "dt_wait_maintenance_deferred", %error_code);
                    }
                }) => {
                    if result.is_err() { tracing::warn!(event = "dt_maintenance_deferred", reason = "pass_deadline"); }
                }
            }
        }
    });
}
