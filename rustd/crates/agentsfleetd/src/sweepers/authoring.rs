//! Private-content cleanup is supervised and joined before its database closes.

use crate::supervisor::Supervisor;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_observability::Analytics;
use std::time::Duration;

const INTERVAL: Duration = Duration::from_secs(10);
const FAILED: &str = "authoring_maintenance_failed";
const COMPLETED: &str = "authoring_maintenance_completed";

pub(super) fn spawn(supervisor: &mut Supervisor, database: &Db, analytics: &Analytics) {
    let drafts = afd_authoring::Drafts::new(database.clone(), Entropy::new())
        .with_analytics(analytics.clone());
    supervisor.spawn(super::AUTHORING, move |stop| async move {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            match drafts.maintain().await {
                Ok(counts) if counts != afd_authoring::Maintenance::default() => {
                    tracing::info!(
                        event = COMPLETED,
                        interrupted = counts.interrupted,
                        purged_drafts = counts.purged_drafts,
                        expired_results = counts.expired_results,
                        expired_receipts = counts.expired_receipts,
                        "authoring recovery advanced"
                    );
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(
                    event = FAILED,
                    error_code = error.code().as_str(),
                    "authoring recovery waits for its durable store"
                ),
            }
        }
    });
}
