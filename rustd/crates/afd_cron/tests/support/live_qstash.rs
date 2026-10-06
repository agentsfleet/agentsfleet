//! The lane's real scheduler, for the suites that reconcile against it.
//!
//! Shared by `integration_sync.rs` and `integration_sync_once.rs`; each
//! self-skips outside `make test-integration-rustd`, which exports the knobs.

use afd_cron::ScheduleService as Reconciler;
use afd_cron::qstash::QStash;

use super::CronLane;

/// Where the lane's scheduler listens.
const LIVE_URL_KNOB: &str = "AGENTSFLEET_QSTASH_LIVE_URL";

/// The credential it authenticates that lane against.
const LIVE_TOKEN_KNOB: &str = "AGENTSFLEET_QSTASH_LIVE_TOKEN";

/// A destination the dev server will accept.
///
/// It resolves the destination for real at create time, so a `.test` host is
/// refused before any of this suite's actual subject is reached.
pub(crate) const LIVE_DESTINATION: &str = "https://example.com";

/// The scheduler this lane talks to, or `None` outside the lane.
///
/// Says so on the way out. A silent `return` reports `ok` in the same words a
/// real pass does, so a lane that stopped exporting these knobs would go on
/// reporting four passes over two tests that never ran — which is how this
/// suite's own base URL stayed wrong long enough to be found by accident.
pub(crate) fn live() -> Option<(String, String)> {
    let url = std::env::var(LIVE_URL_KNOB).ok().filter(|v| !v.is_empty());
    let token = std::env::var(LIVE_TOKEN_KNOB)
        .ok()
        .filter(|v| !v.is_empty());
    match (url, token) {
        (Some(url), Some(token)) => Some((url, token)),
        _unset => {
            eprintln!(
                "SKIPPED: no live scheduler — {LIVE_URL_KNOB} and {LIVE_TOKEN_KNOB} are what \
                 `make test-integration-rustd` exports"
            );
            None
        }
    }
}

/// A reconciler bound to the real scheduler.
pub(crate) fn against_live(lane: &CronLane, url: String, token: String) -> Reconciler {
    Reconciler::new(
        lane.store.clone(),
        QStash::new(
            reqwest::Client::new(),
            afd_crypto::secret::SecretString::new(token),
            LIVE_DESTINATION.to_owned(),
            url,
        ),
    )
}
