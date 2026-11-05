//! The billing relay: the only task that moves money. Renewals and reports log
//! their usage lines and return; every 2 s this task settles those lines and
//! releases the holds of finished fleet leases and setup chat attempts,
//! through the daemon's [`Biller`] (billing §11).

use std::sync::Arc;
use std::time::Duration;

use afd_billing::biller::Biller;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_fleet::lease::billing_sweep::BillingSwept;

use crate::supervisor::Supervisor;

/// How often a pass runs: how long a committed line waits to be charged, and
/// how far a balance can trail the charges a person reads.
const INTERVAL: Duration = Duration::from_secs(2);

/// A pass's read failed; the next pass retries it.
const FAILED: &str = "billing_sweep_failed";

/// A pass settled or released something.
const COMPLETED: &str = "billing_sweep_completed";

pub(super) fn spawn(supervisor: &mut Supervisor, database: &Db) {
    let database = database.clone();
    let biller: Arc<dyn Biller> = crate::billing::biller(database.clone());
    let entropy = Entropy::new();
    supervisor.spawn(super::BILLING, move |stop| async move {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            while sweep_leases(&database, biller.as_ref(), &entropy).await
                && !stop.is_cancelled()
            {}
            release_attempts(&database, biller.as_ref()).await;
        }
    });
}

/// One pass over fleet leases, logged: what it moved, or that its read failed
/// and the next tick retries. Answers whether to run again at once.
async fn sweep_leases(database: &Db, biller: &dyn Biller, entropy: &Entropy) -> bool {
    let swept =
        afd_fleet::lease::billing_sweep::sweep(database, biller, entropy, afd_core::clock::now())
            .await;
    match swept {
        Ok(swept) => {
            log_moved(&swept);
            drains(&swept)
        }
        Err(error) => {
            let error_code = error.code().as_str();
            tracing::warn!(
                event = FAILED,
                error_code,
                "billing sweep waits for its datastore"
            );
            false
        }
    }
}

/// A pass that read a whole batch of lines and settled some runs again at once,
/// so a backlog drains at the biller's pace rather than one batch per tick. A
/// full batch that settled nothing waits for the tick: the biller is refusing.
fn drains(swept: &BillingSwept) -> bool {
    swept.lines_full && swept.resettled > 0
}

/// Logs what a pass moved. Every pass is one iteration of a 2 s poll, so a
/// clean pass logs at `debug` (LOG-HOT); a pass the biller refused part of
/// logs at `info`, and the next pass retries what it refused.
fn log_moved(swept: &BillingSwept) {
    if *swept == BillingSwept::default() {
        return;
    }
    let refused = swept.settle_failures + swept.release_failures + swept.expired_settle_failures;
    macro_rules! moved {
        ($level:ident) => {
            tracing::$level!(
                event = COMPLETED,
                resettled = swept.resettled,
                settle_failures = swept.settle_failures,
                released = swept.released,
                release_failures = swept.release_failures,
                expired_billed = swept.expired_billed,
                expired_settle_failures = swept.expired_settle_failures,
                "billing relay moved usage the lease path logged"
            )
        };
    }
    if refused > 0 {
        moved!(info);
    } else {
        moved!(debug);
    }
}

/// One pass over the holds of settled setup chats, logged the same way.
async fn release_attempts(database: &Db, biller: &dyn Biller) {
    match afd_authoring::billing_sweep::release_finished(database, biller).await {
        Ok(pass) if pass.release_failures > 0 => tracing::info!(
            event = COMPLETED,
            released = pass.released,
            release_failures = pass.release_failures,
            "billing relay released settled setup chats' holds"
        ),
        Ok(pass) if pass != afd_authoring::billing_sweep::Released::default() => tracing::debug!(
            event = COMPLETED,
            released = pass.released,
            release_failures = pass.release_failures,
            "billing relay released settled setup chats' holds"
        ),
        Ok(_) => {}
        Err(error) => {
            let error_code = error.code().as_str();
            tracing::warn!(
                event = FAILED,
                error_code,
                "billing sweep waits for its datastore"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use afd_core::error_code;
    use afd_core::test_util::trace::Capture;
    use afd_crypto::entropy::Entropy;
    use afd_db::test_util::unreachable_db;

    use afd_fleet::lease::billing_sweep::BillingSwept;

    use super::{FAILED, drains, release_attempts, sweep_leases};

    /// A pass that read a whole batch and settled some of it runs again at
    /// once, so a backlog drains without waiting a tick per batch.
    #[test]
    fn a_full_pass_that_settled_something_drains_again() {
        let swept = BillingSwept {
            resettled: 3,
            lines_full: true,
            ..BillingSwept::default()
        };
        assert!(drains(&swept));
    }

    /// A full batch the biller refused waits for the tick: running again at
    /// once would only hammer a biller that is not answering.
    #[test]
    fn a_full_pass_that_settled_nothing_waits_for_the_tick() {
        let refused = BillingSwept {
            settle_failures: 500,
            lines_full: true,
            ..BillingSwept::default()
        };
        assert!(!drains(&refused));
        let partial = BillingSwept {
            resettled: 3,
            ..BillingSwept::default()
        };
        assert!(!drains(&partial), "a partial batch left nothing waiting");
    }

    /// A lease pass whose datastore is gone says it is waiting, under the
    /// outage's code, and returns for the next tick rather than ending the task.
    #[tokio::test]
    async fn a_lease_pass_with_no_datastore_waits_for_it() {
        let database = unreachable_db();
        let biller = crate::billing::biller(database.clone());
        let capture = Capture::install();

        sweep_leases(&database, biller.as_ref(), &Entropy::new()).await;

        assert_eq!(
            capture.only(FAILED).field("error_code"),
            Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
        );
    }

    /// The setup-chat pass reports the same outage the same way.
    #[tokio::test]
    async fn an_attempt_pass_with_no_datastore_waits_for_it() {
        let database = unreachable_db();
        let biller = crate::billing::biller(database.clone());
        let capture = Capture::install();

        release_attempts(&database, biller.as_ref()).await;

        assert_eq!(
            capture.only(FAILED).field("error_code"),
            Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
        );
    }
}
