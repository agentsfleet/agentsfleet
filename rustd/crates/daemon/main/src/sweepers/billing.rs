//! The billing relay: the only task that moves money. Renewals and reports log
//! their usage lines and return; every 2 s this task settles those lines
//! through the daemon's [`Biller`] (billing §11).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use afd_billing::biller::Biller;
use afd_billing::ledger::Resume;
use afd_db::Db;
use afd_fleet::lease::billing_sweep::BillingSwept;
use afd_fleet::task::{DecisionMaintenanceCursor, TaskLifecycle as _, Tasks};
use tokio_util::sync::CancellationToken;

use crate::supervisor::Supervisor;

/// How often a tick runs: how long a committed line waits to be charged, and
/// how far a balance can trail the charges a person reads.
const INTERVAL: Duration = Duration::from_secs(2);

/// Ticks between restarts of the line read at the oldest unsettled line: a
/// line the biller refused is retried at least this often.
const LAP_TICKS: u64 = 30;

/// Passes one tick may run back to back. A backlog drains at the biller's
/// pace, and a sustained one still yields to the next tick.
const DRAIN_PASSES: usize = 10;

/// A pass began.
const STARTED: &str = "billing_sweep_started";

/// A pass's read failed; the next tick retries it.
const FAILED: &str = "billing_sweep_failed";

/// A pass finished.
const COMPLETED: &str = "billing_sweep_completed";

/// The biller refused part of a pass; the next pass retries it.
const REFUSED: &str = "billing_sweep_refused";

pub(super) fn spawn(supervisor: &mut Supervisor, database: &Db, tasks: Tasks) {
    let database = database.clone();
    let biller: Arc<dyn Biller> = crate::billing::biller(database.clone());
    supervisor.spawn(super::BILLING, async move |stop| {
        let mut decision_cursor = DecisionMaintenanceCursor::default();
        let mut interval = tokio::time::interval(INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Where the next pass's line read resumes. It outlives the tick, so a
        // refused backlog of any size never holds newer lines; it goes back to
        // the oldest line every LAP_TICKS, so a refused line is retried.
        let resume = Mutex::new(None);
        let mut ticks: u64 = 0;
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            // The owner records bounded failure diagnostics itself. A failed
            // recovery pass must not prevent the remaining financial repairs.
            tokio::select! {
                biased;
                () = stop.cancelled() => return,
                _recovery = tasks.maintain_decisions(&mut decision_cursor) => {},
            }
            lap(&mut ticks, &resume);
            drain(|| sweep_leases(&database, biller.as_ref(), &resume), &stop).await;
        }
    });
}

/// Runs `pass` until one leaves nothing waiting, [`DRAIN_PASSES`] have run, or
/// `stop` fires; answers how many ran.
async fn drain<F: Future<Output = bool>>(
    mut pass: impl FnMut() -> F,
    stop: &CancellationToken,
) -> usize {
    if stop.is_cancelled() {
        return 0;
    }
    let mut passes = 0;
    loop {
        passes += 1;
        if !pass().await || passes >= DRAIN_PASSES || stop.is_cancelled() {
            return passes;
        }
    }
}

/// One pass over fleet leases from where the last one stopped, logged at both
/// ends. Answers whether to run again at once.
async fn sweep_leases(database: &Db, biller: &dyn Biller, resume: &Mutex<Option<Resume>>) -> bool {
    tracing::debug!(
        event = STARTED,
        pass = "leases",
        "billing relay pass starts"
    );
    let after = resume.lock().map_or(None, |at| at.clone());
    let swept =
        afd_fleet::lease::billing_sweep::settle_lines(database, biller, after.as_ref()).await;
    match swept {
        Ok(swept) => {
            if let Ok(mut at) = resume.lock() {
                at.clone_from(&swept.resume);
            }
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

/// Whether tick `tick` sends the line read back to the oldest unsettled line.
const fn restarts(tick: u64) -> bool {
    tick.is_multiple_of(LAP_TICKS)
}

/// Counts one tick and, once a lap, sends the line read back to the oldest
/// unsettled line by clearing `resume`.
fn lap<T>(ticks: &mut u64, resume: &Mutex<Option<T>>) {
    *ticks = ticks.wrapping_add(1);
    if restarts(*ticks)
        && let Ok(mut at) = resume.lock()
    {
        *at = None;
    }
}

/// A pass that read a whole batch runs again at once, resuming after it, so a
/// backlog drains at the biller's pace and a batch the biller refused never
/// holds newer lines. [`DRAIN_PASSES`] bounds the passes one tick makes.
const fn drains(swept: &BillingSwept) -> bool {
    swept.lines_full
}

/// Logs a lease pass's end. Every pass is one iteration of a 2 s poll, so its
/// counts log at `debug` (LOG-HOT); a pass the biller refused part of also
/// warns once, with the first refusal's code, and the next pass retries.
fn log_moved(swept: &BillingSwept) {
    let refused = swept.settle_failures + swept.expired_settle_failures;
    if let (true, Some(code)) = (refused > 0, swept.refused) {
        let error_code = code.as_str();
        tracing::warn!(
            event = REFUSED,
            error_code,
            refused,
            "the biller refused part of a billing relay pass; the next pass retries"
        );
    }
    tracing::debug!(
        event = COMPLETED,
        pass = "leases",
        resettled = swept.resettled,
        settle_failures = swept.settle_failures,
        expired_billed = swept.expired_billed,
        expired_settle_failures = swept.expired_settle_failures,
        "billing relay pass finished"
    );
}

#[cfg(test)]
#[path = "billing_test.rs"]
mod tests;
