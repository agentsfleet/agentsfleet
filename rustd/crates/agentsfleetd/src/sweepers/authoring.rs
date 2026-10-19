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
    let drafts = afd_authoring::Drafts::new(
        database.clone(),
        Entropy::new(),
        crate::billing::biller(database.clone()),
    )
    .with_analytics(analytics.clone());
    supervisor.spawn(super::AUTHORING, move |stop| async move {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            maintain(&drafts).await;
        }
    });
}

/// One maintenance pass, logged: what it advanced, or that its store is gone
/// and the next tick retries.
async fn maintain(drafts: &afd_authoring::Drafts) {
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

#[cfg(test)]
mod tests {
    use afd_core::error_code;
    use afd_core::test_util::trace::Capture;
    use afd_crypto::entropy::Entropy;
    use afd_db::test_util::unreachable_db;

    use super::{FAILED, maintain};

    /// A pass whose store is gone says it is waiting, under the outage's code,
    /// and returns for the next tick rather than ending the task.
    #[tokio::test]
    async fn a_pass_with_no_store_waits_for_it() {
        let database = unreachable_db();
        let drafts = afd_authoring::Drafts::new(
            database.clone(),
            Entropy::new(),
            crate::billing::biller(database),
        );
        let capture = Capture::install();

        maintain(&drafts).await;

        assert_eq!(
            capture.only(FAILED).field("error_code"),
            Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
        );
    }
}
