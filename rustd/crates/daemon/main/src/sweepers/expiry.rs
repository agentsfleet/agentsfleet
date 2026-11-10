//! One clock for every deadline pass (M245 §7): every
//! [`af_core::timing::EXPIRY_TICK`] this task bills the last slice of each
//! fleet lease that expired unreported, and moves each setup attempt past its
//! deadline to accounting recovery. Both were riders on other clocks: the
//! billing relay's and authoring's privacy pass.

use std::sync::Arc;

use af_crypto::entropy::Entropy;
use af_observability::Analytics;
use afd_billing::biller::Biller;
use afd_db::Db;

use crate::supervisor::Supervisor;

/// A pass could not reach its store; the next tick runs it again.
const FAILED: &str = "expiry_sweep_failed";

/// A pass moved something.
const COMPLETED: &str = "expiry_sweep_completed";

pub(super) fn spawn(supervisor: &mut Supervisor, database: &Db, analytics: &Analytics) {
    let passes = Passes {
        database: database.clone(),
        biller: crate::billing::biller(database.clone()),
        entropy: Entropy::new(),
        drafts: afd_authoring::Drafts::new(
            database.clone(),
            Entropy::new(),
            crate::billing::biller(database.clone()),
        )
        .with_analytics(analytics.clone()),
    };
    supervisor.spawn(super::EXPIRY, async move |stop| {
        let mut interval = tokio::time::interval(af_core::timing::EXPIRY_TICK);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                () = stop.cancelled() => return,
                _ = interval.tick() => {},
            }
            passes.tick().await;
        }
    });
}

/// What one tick runs, over the stores it reads.
struct Passes {
    database: Db,
    biller: Arc<dyn Biller>,
    entropy: Entropy,
    drafts: afd_authoring::Drafts,
}

impl Passes {
    /// Both deadline passes; a failed one is logged and the other still runs.
    async fn tick(&self) -> Ticked {
        let leases = afd_fleet::lease::billing_sweep::bill_expired_leases(
            &self.database,
            self.biller.as_ref(),
            &self.entropy,
            af_core::clock::now(),
        )
        .await;
        let billed = match leases {
            Ok(swept) => Some(swept.expired_billed),
            Err(fault) => {
                let error_code = fault.code().as_str();
                tracing::warn!(
                    event = FAILED,
                    pass = "leases",
                    error_code,
                    "expired leases wait for the next tick"
                );
                None
            }
        };
        let expired = match self.drafts.expire_attempts().await {
            Ok(expired) => Some(expired),
            Err(fault) => {
                let error_code = fault.code().as_str();
                tracing::warn!(
                    event = FAILED,
                    pass = "attempts",
                    error_code,
                    "lapsed attempts wait for the next tick"
                );
                None
            }
        };
        if billed.unwrap_or(0) + expired.unwrap_or(0) > 0 {
            tracing::info!(
                event = COMPLETED,
                expired_leases = billed.unwrap_or(0),
                expired_attempts = expired.unwrap_or(0),
                "deadline passes advanced"
            );
        }
        Ticked { billed, expired }
    }
}

/// What a tick's two passes did: `None` where a pass could not reach its store.
#[derive(Debug, PartialEq, Eq)]
struct Ticked {
    billed: Option<u64>,
    expired: Option<u64>,
}

#[cfg(test)]
mod tests {
    use af_core::test_util::trace::Capture;
    use af_crypto::entropy::Entropy;
    use afd_db::test_util::unreachable_db;

    use super::{FAILED, Passes, Ticked};

    // 7.7 and 7.8: one tick runs both deadline passes, and a store neither can
    // reach is logged under its code and waits for the next tick, the task
    // still running.
    #[tokio::test]
    async fn a_tick_runs_both_passes_and_logs_a_store_it_cannot_reach() {
        let log = Capture::install();
        let database = unreachable_db();
        let passes = Passes {
            biller: crate::billing::biller(database.clone()),
            entropy: Entropy::new(),
            drafts: afd_authoring::Drafts::new(
                database.clone(),
                Entropy::new(),
                crate::billing::biller(database.clone()),
            ),
            database,
        };
        assert_eq!(
            passes.tick().await,
            Ticked {
                billed: None,
                expired: None
            },
            "both passes ran and neither reached its store"
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
        assert_eq!(passes_logged, ["leases", "attempts"]);
        assert!(
            failed
                .iter()
                .all(|event| event.field("error_code").is_some()),
            "each failure names its code"
        );
        // A second tick runs again: a failure is never the task's end.
        assert_eq!(
            passes.tick().await,
            Ticked {
                billed: None,
                expired: None
            }
        );
    }
}
