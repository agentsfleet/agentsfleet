//! Private storage custody is supervised independently of timely GitHub evidence reads.

use crate::supervisor::Supervisor;
use afd_fleet::task::{CheckpointMaintenanceCursor, TaskLifecycle as _, Tasks};
use std::time::Duration;

pub(super) fn spawn(
    supervisor: &mut Supervisor,
    tasks: Tasks,
    startup: &super::schedule::Startup,
    settings: crate::config::maintenance::CheckpointStorage,
) {
    let mut interval = startup.interval(
        super::TASK_STORAGE,
        Duration::from_secs(settings.interval_seconds),
    );
    let pass_timeout = Duration::from_secs(settings.pass_timeout_seconds);
    supervisor.spawn(super::TASK_STORAGE, async move |stop| {
        let mut cursor = CheckpointMaintenanceCursor::default();
        loop {
            tokio::select! { () = stop.cancelled() => return, _ = interval.tick() => {} }
            tokio::select! {
                () = stop.cancelled() => return,
                result = tokio::time::timeout(pass_timeout, tasks.maintain_checkpoints(&mut cursor)) => {
                    match result {
                        Ok(Ok(result)) => { let (event, private_generations, bases, objects, purged, identity_keys, deferred,) = (&"dt_storage_collection_completed", &result.owners, &result.bases, &result.objects, &result.purged, &result.identity_keys, &result.deferred,); tracing::info!(event, private_generations, bases, objects, purged, identity_keys, deferred); },
                        Ok(Err(error)) => {
                            let error_code = error.code();
                            tracing::warn!(event = "dt_storage_collection_deferred", %error_code);
                        }
                        Err(_deadline) => tracing::warn!(event = "dt_storage_collection_deadline"),
                    }
                }
            }
        }
    });
}
