//! The billing sweeper: usage that committed without its settlement, and holds
//! that outlived their lease, finished on a schedule.

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
    supervisor.spawn(super::BILLING, move |stop| async move {
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            match afd_fleet::lease::billing_sweep::sweep(&database, biller.as_ref()).await {
                Ok(swept) if swept != afd_fleet::lease::billing_sweep::BillingSwept::default() => {
                    tracing::info!(
                        event = COMPLETED,
                        resettled = swept.resettled,
                        settle_failures = swept.settle_failures,
                        released = swept.released,
                        release_failures = swept.release_failures,
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
        }
    });
}
