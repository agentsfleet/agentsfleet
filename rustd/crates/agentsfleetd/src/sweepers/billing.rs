//! The billing sweeper: usage that committed without its settlement, and holds
//! that outlived their fleet lease or setup chat attempt, finished on a
//! schedule.

use std::sync::Arc;
use std::time::Duration;

use afd_billing::biller::Biller;
use afd_crypto::entropy::Entropy;
use afd_db::Db;

use crate::supervisor::Supervisor;

/// How often a pass runs. Lines wait at least the sweep's grace before it
/// touches them, so this only bounds how late a repair can be.
const INTERVAL: Duration = Duration::from_secs(30);

/// A pass's read failed; the next pass retries it.
const FAILED: &str = "billing_sweep_failed";

/// A pass settled or released something the request path left behind.
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
            sweep_leases(&database, biller.as_ref(), &entropy).await;
            release_attempts(&database, biller.as_ref()).await;
        }
    });
}

/// One pass over fleet leases, logged: what it repaired, or that its read
/// failed and the next pass retries.
async fn sweep_leases(database: &Db, biller: &dyn Biller, entropy: &Entropy) {
    let swept =
        afd_fleet::lease::billing_sweep::sweep(database, biller, entropy, afd_core::clock::now())
            .await;
    match swept {
        Ok(swept) if swept != afd_fleet::lease::billing_sweep::BillingSwept::default() => {
            tracing::info!(
                event = COMPLETED,
                resettled = swept.resettled,
                settle_failures = swept.settle_failures,
                released = swept.released,
                release_failures = swept.release_failures,
                expired_billed = swept.expired_billed,
                expired_settle_failures = swept.expired_settle_failures,
                "billing sweep repaired usage the request path left unsettled"
            );
        }
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

/// One pass over the holds finished setup chats left open, logged the same way.
async fn release_attempts(database: &Db, biller: &dyn Biller) {
    match afd_authoring::billing_sweep::release_finished(database, biller).await {
        Ok(pass) if pass != afd_authoring::billing_sweep::Released::default() => {
            tracing::info!(
                event = COMPLETED,
                released = pass.released,
                release_failures = pass.release_failures,
                "billing sweep released holds finished setup chats left open"
            );
        }
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

    use super::{FAILED, release_attempts, sweep_leases};

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
