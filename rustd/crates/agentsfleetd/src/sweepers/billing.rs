//! The billing sweeper: usage that committed without its settlement, and holds
//! that outlived their fleet lease or setup chat attempt, finished on a
//! schedule.

use std::sync::Arc;
use std::time::Duration;

use afd_billing::biller::Biller;
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
    let entropy = afd_crypto::entropy::Entropy::new();
    supervisor.spawn(super::BILLING, move |stop| async move {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            let swept = afd_fleet::lease::billing_sweep::sweep(
                &database,
                biller.as_ref(),
                &entropy,
                afd_core::clock::now(),
            )
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
                Err(error) => tracing::warn!(
                    event = FAILED,
                    error_code = error.code().as_str(),
                    "billing sweep waits for its datastore"
                ),
            }
            match afd_authoring::billing_sweep::release_finished(&database, biller.as_ref()).await {
                Ok(pass) if pass != afd_authoring::billing_sweep::Released::default() => {
                    tracing::info!(
                        event = COMPLETED,
                        released = pass.released,
                        release_failures = pass.release_failures,
                        "billing sweep released holds finished setup chats left open"
                    );
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(
                    event = FAILED,
                    error_code = error.code().as_str(),
                    "billing sweep waits for its datastore"
                ),
            }
        }
    });
}
