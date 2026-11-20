//! One clock for the deadline pass: every
//! `maintenance.expired_work.interval_seconds` this task bills the last slice
//! of each run that expired unreported, a fleet's or a setup draft's.

use std::sync::Arc;

use af_crypto::entropy::Entropy;
use afd_billing::biller::Biller;
use afd_db::Db;

use crate::supervisor::Supervisor;

/// A pass could not reach its store; the next tick runs it again.
const FAILED: &str = "expiry_sweep_failed";

/// A pass moved something.
const COMPLETED: &str = "expiry_sweep_completed";

/// The biller refused an expired run's last slice; the next tick retries.
const REFUSED: &str = "expiry_sweep_refused";

pub(super) fn spawn(
    supervisor: &mut Supervisor,
    database: &Db,
    biller: Arc<dyn Biller>,
    startup: &super::schedule::Startup,
    settings: crate::config::maintenance::ExpiredWork,
) {
    let passes = Passes {
        database: database.clone(),
        biller,
        entropy: Entropy::new(),
    };
    let mut interval = startup.interval(
        super::EXPIRY,
        std::time::Duration::from_secs(settings.interval_seconds),
    );
    supervisor.spawn(super::EXPIRY, async move |stop| {
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            tokio::select! { () = stop.cancelled() => return, _ticked = passes.tick() => {} }
        }
    });
}

/// What one tick runs, over the stores it reads.
struct Passes {
    database: Db,
    biller: Arc<dyn Biller>,
    entropy: Entropy,
}

impl Passes {
    /// The deadline pass; a failed one is logged and the next tick retries.
    async fn tick(&self) -> Ticked {
        let runs = afd_fleet::run::billing_sweep::bill_expired_runs(
            &self.database,
            self.biller.as_ref(),
            &self.entropy,
            af_core::clock::now(),
        )
        .await;
        let billed = match runs {
            Ok(swept) => {
                if let (true, Some(code)) = (swept.settle_failures > 0, swept.refused) {
                    let error_code = code.as_str();
                    let refused = swept.settle_failures;
                    tracing::warn!(
                        event = REFUSED,
                        error_code,
                        refused,
                        "the biller refused an expired run's last slice; the next tick retries"
                    );
                }
                Some(swept.billed)
            }
            Err(fault) => {
                let error_code = fault.code().as_str();
                {
                    let (event, pass, message) = (
                        &FAILED,
                        &"runs",
                        format_args!("expired runs wait for the next tick"),
                    );
                    tracing::warn!(message, event, pass, error_code);
                };
                None
            }
        };
        if let Some(billed) = billed.filter(|billed| *billed > 0) {
            tracing::info!(
                event = COMPLETED,
                expired_runs = billed,
                "deadline passes advanced"
            );
        }
        Ticked { billed }
    }
}

/// What a tick's pass did: `None` when it could not reach its store.
#[derive(Debug, PartialEq, Eq)]
struct Ticked {
    billed: Option<u64>,
}

#[cfg(test)]
mod tests {
    use af_core::error_code;
    use af_core::test_util::trace::Capture;
    use af_crypto::entropy::Entropy;
    use afd_db::test_util::unreachable_db;

    use super::{FAILED, Passes, Ticked};

    // One tick runs the deadline pass, and a store it cannot reach is logged
    // under its code and waits for the next tick, the task still running.
    #[tokio::test]
    async fn a_tick_runs_its_pass_and_logs_a_store_it_cannot_reach() {
        let log = Capture::install();
        let database = unreachable_db();
        let biller =
            crate::billing::biller(database.clone(), af_observability::Analytics::silent());
        let passes = Passes {
            biller,
            entropy: Entropy::new(),
            database,
        };
        assert_eq!(
            passes.tick().await,
            Ticked { billed: None },
            "the pass ran and did not reach its store"
        );
        let failed: Vec<_> = log
            .events()
            .into_iter()
            .filter(|event| event.field("event") == Some(FAILED))
            .collect();
        let passes_logged: Vec<&str> = failed
            .iter()
            .filter_map(|event| event.field("pass"))
            .collect();
        assert_eq!(passes_logged, ["runs"]);
        assert!(
            failed.iter().all(|event| event.field("error_code")
                == Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str())),
            "each failure names the unreachable store's code"
        );
        // A second tick runs again: a failure is never the task's end.
        assert_eq!(passes.tick().await, Ticked { billed: None });
    }
}
