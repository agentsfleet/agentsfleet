//! The park sweeper: heads parked behind approval cards that have all lapsed,
//! re-armed on a schedule.
//!
//! A parked head is not claimable: the admission trigger clears its fleet's
//! readiness, and an answer re-arms it. A card a person never answers lapses
//! at its deadline with no write anywhere, so without this pass a head parked
//! behind a lapsed event card would hold its fleet's queue forever. The pass
//! expires no card: a late answer stays possible, and standing access still
//! waits for one (`afd_fleet::lease::Leases::resume_lapsed_parks`).

use std::time::Duration;

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_dragonfly::Dragonfly;
use afd_fleet::lease::Leases;

use crate::supervisor::Supervisor;

/// How often a pass runs: the longest a lapsed card's parked head waits past
/// its deadline before its fleet is claimable again. A pass that finds nothing
/// is one indexed read.
const INTERVAL: Duration = Duration::from_secs(5);

/// A pass's statements failed; the next pass retries them.
const FAILED: &str = "park_sweep_failed";

/// A pass re-armed at least one fleet.
const COMPLETED: &str = "park_sweep_completed";

pub(super) fn spawn(supervisor: &mut Supervisor, database: &Db, queue: &Dragonfly) {
    let leases = Leases::new(database.clone(), queue.clone(), Entropy::new());
    supervisor.spawn(super::PARKS, move |stop| async move {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            sweep_parks(&leases).await;
        }
    });
}

/// One pass, logged: how many fleets it re-armed, or that its datastore did
/// not answer and the next pass retries.
async fn sweep_parks(leases: &Leases) {
    match leases.resume_lapsed_parks().await {
        Ok(0) => {}
        Ok(resumed) => {
            tracing::info!(
                event = COMPLETED,
                resumed,
                "heads parked behind lapsed approval cards are claimable again"
            );
        }
        Err(error) => {
            let error_code = error.code().as_str();
            tracing::warn!(
                event = FAILED,
                error_code,
                "the park sweep waits for its datastore"
            );
        }
    }
}

#[cfg(all(test, feature = "test-util"))]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use afd_core::error_code;
    use afd_core::test_util::trace::Capture;
    use afd_crypto::entropy::Entropy;
    use afd_db::test_util::unreachable_db;
    use afd_dragonfly::{Dragonfly, DragonflyConfig, DragonflyRole};
    use afd_fleet::lease::Leases;

    use super::{FAILED, sweep_parks};

    /// A pass whose datastore is gone says it is waiting, under the outage's
    /// code, and returns for the next tick rather than ending the task.
    #[tokio::test]
    async fn a_park_pass_with_no_datastore_waits_for_it() {
        let config =
            DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:1".to_owned());
        let queue = Dragonfly::unreachable(&config).expect("a lazy manager opens no socket");
        let leases = Leases::new(unreachable_db(), queue, Entropy::new());
        let capture = Capture::install();

        sweep_parks(&leases).await;

        assert_eq!(
            capture.only(FAILED).field("error_code"),
            Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
        );
    }
}
