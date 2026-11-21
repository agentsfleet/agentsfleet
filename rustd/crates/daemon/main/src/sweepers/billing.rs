//! The billing relay: the task that settles logged usage. Renewals and reports
//! log their usage lines and return; every 5 s this task settles those lines
//! through the daemon's [`Biller`]. The expiry sweeper bills the
//! last slice of a run that ended unreported, and its lines settle here.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use afd_billing::biller::Biller;
use afd_billing::ledger::Resume;
use afd_db::Db;
use afd_fleet::run::billing_sweep::LinesSwept;
use afd_fleet::task::{DecisionMaintenanceCursor, TaskLifecycle as _, Tasks};
use tokio_util::sync::CancellationToken;

use crate::supervisor::Supervisor;

/// How often a tick runs: how long a committed line waits to be charged, and
/// how far a balance can trail the charges a person reads.
#[cfg(test)]
const INTERVAL: Duration = Duration::from_secs(5);

/// Ticks between restarts of the line read at the oldest unsettled line: a
/// line the biller refused is retried at least this often, once a minute.
#[cfg(test)]
const LAP_TICKS: u64 = 12;
#[cfg(test)]
const _: () = assert!(
    INTERVAL.as_secs() * LAP_TICKS <= 60,
    "a refused line waits at most a minute"
);

/// Passes one tick may run back to back. A backlog drains at the biller's
/// pace, and a sustained one still yields to the next tick.
#[cfg(test)]
const DRAIN_PASSES: usize = 10;

/// A pass began.
const STARTED: &str = "billing_sweep_started";

/// A pass's read failed; the next tick retries it.
const FAILED: &str = "billing_sweep_failed";

/// A pass finished.
const COMPLETED: &str = "billing_sweep_completed";

/// The biller refused part of a pass; the next pass retries it.
const REFUSED: &str = "billing_sweep_refused";

pub(super) fn spawn(
    supervisor: &mut Supervisor,
    database: &Db,
    tasks: Tasks,
    biller: Arc<dyn Biller>,
    startup: &super::schedule::Startup,
    settings: crate::config::maintenance::Billing,
) {
    let database = database.clone();
    let mut interval = startup.interval(
        super::BILLING,
        Duration::from_secs(settings.interval_seconds),
    );
    let lap_ticks = settings
        .retry_oldest_after_seconds
        .div_ceil(settings.interval_seconds);
    supervisor.spawn(super::BILLING, async move |stop| {
        let mut decision_cursor = DecisionMaintenanceCursor::default();
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
            lap(&mut ticks, &resume, lap_ticks);
            drain(
                || sweep_runs(&database, biller.as_ref(), &resume),
                &stop,
                settings.max_drain_passes,
            )
            .await;
        }
    });
}

/// Runs `pass` until one leaves nothing waiting, the configured pass ceiling have run, or
/// `stop` fires; answers how many ran.
async fn drain<F: Future<Output = bool>>(
    mut pass: impl FnMut() -> F,
    stop: &CancellationToken,
    maximum: usize,
) -> usize {
    if stop.is_cancelled() {
        return 0;
    }
    let mut passes = 0;
    loop {
        passes += 1;
        if !pass().await || passes >= maximum || stop.is_cancelled() {
            return passes;
        }
    }
}

/// One pass over fleet runs from where the last one stopped, logged at both
/// ends. Answers whether to run again at once.
async fn sweep_runs(database: &Db, biller: &dyn Biller, resume: &Mutex<Option<Resume>>) -> bool {
    {
        let (event, pass, message) = (&STARTED, &"runs", format_args!("billing relay pass starts"));
        tracing::debug!(message, event, pass);
    };
    let after = resume.lock().map_or(None, |at| at.clone());
    let swept = afd_fleet::run::billing_sweep::settle_lines(database, biller, after.as_ref()).await;
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
            {
                let (event, message) = (
                    &FAILED,
                    format_args!("billing sweep waits for its datastore"),
                );
                tracing::warn!(message, event, error_code);
            };
            false
        }
    }
}

/// Whether tick `tick` sends the line read back to the oldest unsettled line.
const fn restarts(tick: u64, lap_ticks: u64) -> bool {
    tick.is_multiple_of(lap_ticks)
}

/// Counts one tick and, once a lap, sends the line read back to the oldest
/// unsettled line by clearing `resume`.
fn lap<T>(ticks: &mut u64, resume: &Mutex<Option<T>>, lap_ticks: u64) {
    *ticks = ticks.wrapping_add(1);
    if restarts(*ticks, lap_ticks)
        && let Ok(mut at) = resume.lock()
    {
        *at = None;
    }
}

/// A pass that read a whole batch runs again at once, resuming after it, so a
/// backlog drains at the biller's pace and a batch the biller refused never
/// holds newer lines. the configured pass ceiling bounds the passes one tick makes.
const fn drains(swept: &LinesSwept) -> bool {
    swept.full
}

/// Logs a run pass's end. Every pass is one iteration of a 5 s poll, so its
/// counts log at `debug` (LOG-HOT); a pass the biller refused part of also
/// warns once, with the first refusal's code, and the next pass retries.
fn log_moved(swept: &LinesSwept) {
    let refused = swept.settle_failures;
    if let (true, Some(code)) = (refused > 0, swept.refused) {
        let error_code = code.as_str();
        {
            let (event, message) = (
                &REFUSED,
                format_args!(
                    "the biller refused part of a billing relay pass; the next pass retries"
                ),
            );
            tracing::warn!(message, event, error_code, refused);
        };
    }
    {
        let (event, pass, resettled, settle_failures, message) = (
            &COMPLETED,
            &"runs",
            &swept.resettled,
            &swept.settle_failures,
            format_args!("billing relay pass finished"),
        );
        tracing::debug!(message, event, pass, resettled, settle_failures);
    };
}

#[cfg(test)]
#[path = "billing_test.rs"]
mod tests;
