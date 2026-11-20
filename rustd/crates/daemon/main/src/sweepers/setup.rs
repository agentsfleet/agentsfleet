//! Private-content cleanup is supervised and joined before its database closes.

use crate::supervisor::Supervisor;
use af_crypto::entropy::Entropy;
use af_observability::Analytics;
use afd_db::Db;
use std::time::Duration;

const FAILED: &str = "setup_maintenance_failed";
const COMPLETED: &str = "setup_maintenance_completed";

pub(super) fn spawn(
    supervisor: &mut Supervisor,
    database: &Db,
    analytics: &Analytics,
    startup: &super::schedule::Startup,
    settings: crate::config::maintenance::SetupCleanup,
) {
    let drafts =
        afd_setup::Drafts::new(database.clone(), Entropy::new()).with_analytics(analytics.clone());
    let mut interval =
        startup.interval(super::SETUP, Duration::from_secs(settings.interval_seconds));
    supervisor.spawn(super::SETUP, async move |stop| {
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            tokio::select! { () = stop.cancelled() => return, () = maintain(&drafts) => {} }
        }
    });
}

/// One maintenance pass, logged: what it advanced, or that its store is gone
/// and the next tick retries.
async fn maintain(drafts: &afd_setup::Drafts) {
    // The privacy purges and the replay receipts' expiry.
    match drafts.housekeep().await {
        Ok(counts) if counts != afd_setup::Maintenance::default() => {
            {
                let (event, purged_drafts, expired_receipts, message) = (
                    &COMPLETED,
                    &counts.purged_drafts,
                    &counts.expired_receipts,
                    format_args!("setup recovery advanced"),
                );
                tracing::info!(message, event, purged_drafts, expired_receipts);
            };
        }
        Ok(_) => {}
        Err(error) => {
            let error_code = error.code().as_str();
            {
                let (event, message) = (
                    &FAILED,
                    format_args!("setup recovery waits for its durable store"),
                );
                tracing::warn!(message, event, error_code);
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use af_core::error_code;
    use af_core::test_util::trace::Capture;
    use af_crypto::entropy::Entropy;
    use afd_db::test_util::unreachable_db;

    use super::{FAILED, maintain};

    /// A pass whose store is gone says it is waiting, under the outage's code,
    /// and returns for the next tick rather than ending the task.
    #[tokio::test]
    async fn a_pass_with_no_store_waits_for_it() {
        let drafts = afd_setup::Drafts::new(unreachable_db(), Entropy::new());
        let capture = Capture::install();

        maintain(&drafts).await;

        assert_eq!(
            capture.only(FAILED).field("error_code"),
            Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
        );
    }
}
