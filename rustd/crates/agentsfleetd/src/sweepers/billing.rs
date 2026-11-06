//! The billing relay: the only task that moves money. Renewals and reports log
//! their usage lines and return; every 2 s this task settles those lines
//! through the daemon's [`Biller`] (billing §11).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use afd_billing::biller::Biller;
use afd_billing::ledger::Resume;
use afd_crypto::entropy::Entropy;
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
    let entropy = Entropy::new();
    supervisor.spawn(super::BILLING, move |stop| async move {
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
            let _decision_recovery = tasks.maintain_decisions(&mut decision_cursor).await;
            lap(&mut ticks, &resume);
            drain(
                || sweep_leases(&database, biller.as_ref(), &entropy, &resume),
                &stop,
            )
            .await;
        }
    });
}

/// Runs `pass` until one leaves nothing waiting, [`DRAIN_PASSES`] have run, or
/// `stop` fires; answers how many ran.
async fn drain<F: Future<Output = bool>>(
    mut pass: impl FnMut() -> F,
    stop: &CancellationToken,
) -> usize {
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
async fn sweep_leases(
    database: &Db,
    biller: &dyn Biller,
    entropy: &Entropy,
    resume: &Mutex<Option<Resume>>,
) -> bool {
    tracing::debug!(
        event = STARTED,
        pass = "leases",
        "billing relay pass starts"
    );
    let after = resume.lock().map_or(None, |at| at.clone());
    let now = afd_core::clock::now();
    let swept =
        afd_fleet::lease::billing_sweep::sweep(database, biller, entropy, now, after.as_ref())
            .await;
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
fn drains(swept: &BillingSwept) -> bool {
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
mod tests {
    use afd_core::error_code;
    use afd_core::test_util::trace::Capture;
    use afd_crypto::entropy::Entropy;
    use afd_db::test_util::unreachable_db;

    use afd_fleet::lease::billing_sweep::BillingSwept;

    use std::cell::Cell;

    use tokio_util::sync::CancellationToken;

    use super::{
        DRAIN_PASSES, FAILED, LAP_TICKS, REFUSED, drain, drains, lap, log_moved, restarts,
        sweep_leases,
    };

    /// The read position survives every tick of a lap and is cleared on the
    /// lap's last tick, so the next pass starts at the oldest line.
    #[test]
    fn a_lap_clears_the_read_position_on_its_last_tick() {
        let resume = std::sync::Mutex::new(Some(7_u8));
        let mut ticks = 0;
        for _ in 1..LAP_TICKS {
            lap(&mut ticks, &resume);
        }
        assert_eq!(
            resume.lock().map(|at| *at).ok(),
            Some(Some(7)),
            "kept within the lap"
        );
        lap(&mut ticks, &resume);
        assert_eq!(
            resume.lock().map(|at| *at).ok(),
            Some(None),
            "cleared at the lap"
        );
    }

    /// The line read goes back to the oldest line once a lap and only then, so
    /// refused lines are retried and progress through newer lines is kept.
    #[test]
    fn the_read_restarts_at_the_oldest_line_once_a_lap() {
        let restarted: Vec<u64> = (1..=LAP_TICKS * 2).filter(|tick| restarts(*tick)).collect();
        assert_eq!(restarted, [LAP_TICKS, LAP_TICKS * 2]);
    }

    /// Passes reporting more waiting, then nothing: one tick runs all three
    /// back to back rather than one per tick.
    #[tokio::test]
    async fn a_tick_runs_passes_until_one_leaves_nothing_waiting() {
        let answers = [true, true, false];
        let next = Cell::new(0);
        let ran = drain(
            || {
                let answer = answers.get(next.get()).copied().unwrap_or(false);
                next.set(next.get() + 1);
                async move { answer }
            },
            &CancellationToken::new(),
        )
        .await;
        assert_eq!(ran, 3);
    }

    /// A backlog that never empties stops at the cap and yields to the next
    /// tick.
    #[tokio::test]
    async fn a_tick_stops_at_the_drain_cap() {
        let ran = drain(|| async { true }, &CancellationToken::new()).await;
        assert_eq!(ran, DRAIN_PASSES);
    }

    /// A pass the biller refused part of warns once, under the refusal's code.
    #[test]
    fn a_refused_pass_warns_with_its_code() {
        let capture = Capture::install();
        log_moved(&BillingSwept {
            settle_failures: 2,
            refused: Some(error_code::INTERNAL_DB_UNAVAILABLE),
            ..BillingSwept::default()
        });
        assert_eq!(
            capture.only(REFUSED).field("error_code"),
            Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
        );
    }

    /// A pass that read a whole batch runs again at once, settled or refused,
    /// so a batch the biller refused never holds newer lines behind it.
    #[test]
    fn a_full_pass_drains_again_whether_or_not_it_settled() {
        for swept in [
            BillingSwept {
                resettled: 3,
                lines_full: true,
                ..BillingSwept::default()
            },
            BillingSwept {
                settle_failures: 500,
                lines_full: true,
                ..BillingSwept::default()
            },
        ] {
            assert!(drains(&swept), "{swept:?}");
        }
    }

    /// A pass that read less than a batch left nothing waiting: the tick ends.
    #[test]
    fn a_partial_pass_waits_for_the_tick() {
        let partial = BillingSwept {
            resettled: 3,
            ..BillingSwept::default()
        };
        assert!(!drains(&partial));
    }

    /// A lease pass whose datastore is gone says it is waiting, under the
    /// outage's code, and returns for the next tick rather than ending the task.
    #[tokio::test]
    async fn a_lease_pass_with_no_datastore_waits_for_it() {
        let database = unreachable_db();
        let biller = crate::billing::biller(database.clone());
        let capture = Capture::install();

        sweep_leases(
            &database,
            biller.as_ref(),
            &Entropy::new(),
            &std::sync::Mutex::new(None),
        )
        .await;

        assert_eq!(
            capture.only(FAILED).field("error_code"),
            Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())
        );
    }
}
