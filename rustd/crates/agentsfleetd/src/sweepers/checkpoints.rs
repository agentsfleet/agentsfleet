//! Private storage custody is supervised independently of timely GitHub evidence reads.

use crate::supervisor::Supervisor;
use afd_fleet::task::{CheckpointMaintenanceCursor, TaskLifecycle as _, Tasks};
use std::time::Duration;

const INTERVAL: Duration = Duration::from_mins(5);
const PASS_LIMIT: Duration = Duration::from_mins(4);

pub(super) fn spawn(supervisor: &mut Supervisor, tasks: Tasks) {
    supervisor.spawn(super::TASK_STORAGE, move |stop| async move {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut cursor = CheckpointMaintenanceCursor::default();
        loop {
            tokio::select! { () = stop.cancelled() => return, _ = interval.tick() => {} }
            tokio::select! {
                () = stop.cancelled() => return,
                result = tokio::time::timeout(PASS_LIMIT, tasks.maintain_checkpoints(&mut cursor)) => {
                    match result {
                        Ok(Ok(result)) => tracing::info!(event = "dt_storage_collection_completed", private_generations = result.owners,
                            bases = result.bases, objects = result.objects, purged = result.purged, deferred = result.deferred),
                        Ok(Err(error)) => tracing::warn!(event = "dt_storage_collection_deferred", error_code = %error.code()),
                        Err(_deadline) => tracing::warn!(event = "dt_storage_collection_deadline"),
                    }
                }
            }
        }
    });
}
