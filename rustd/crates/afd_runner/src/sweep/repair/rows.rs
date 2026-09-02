//! The rows this sweeper reads, the payloads it writes, and what one pass
//! concluded about when to come back.
//!
//! Split from `repair` because these are the shapes crossing the boundary —
//! a claimed intent, the three nested objects a synthetic event carries, and
//! the tally that sets the next interval — and none of them changes when the
//! dispatch sequence does.

use std::time::Duration;

use serde::Serialize;

use super::{CLAIM_STALE, DUE_BATCH_LIMIT, NOTHING_DUE_INTERVAL};

#[derive(Debug)]
pub(super) struct Due {
    /// The verification row.
    pub(super) id: String,
    /// The fleet that will RUN the verification.
    pub(super) verifier_fleet_id: String,
    /// The workspace both fleets belong to.
    pub(super) workspace_id: String,
    /// The incident's own fleet and event, which the payload names so the
    /// verification knows what it is checking.
    pub(super) incident: Incident,
    /// The repair that was merged.
    pub(super) repair: Repair,
    /// The deployment that carried it.
    pub(super) production: Production,
    /// When this became due, for the pacing decision.
    pub(super) verify_after: i64,
}

/// The incident a verification is checking.
#[derive(Debug, Serialize)]
pub(super) struct Incident {
    /// The fleet the incident belonged to.
    pub(super) fleet_id: String,
    /// The event that recorded it.
    pub(super) event_id: String,
    /// What was asked, verbatim.
    pub(super) request_json: String,
    /// What was answered.
    pub(super) response_text: String,
}

/// The merged repair.
#[derive(Debug, Serialize)]
pub(super) struct Repair {
    /// The pull request's number.
    pub(super) pr_number: i64,
    /// Its address.
    pub(super) pr_url: String,
    /// The commit the merge produced.
    pub(super) merged_commit_sha: String,
    /// When it merged.
    pub(super) merged_at: i64,
}

/// The deployment that carried the repair to production.
#[derive(Debug, Serialize)]
pub(super) struct Production {
    /// Which deployment provider reported it.
    pub(super) provider: String,
    /// Its identifier there.
    pub(super) deployment_id: String,
    /// How it ended.
    pub(super) conclusion: String,
    /// When it ended.
    pub(super) completed_at: i64,
}

/// The payload a verification run is handed.
#[derive(Debug, Serialize)]
pub(super) struct Synthetic<'a> {
    /// What this payload is.
    pub(super) event_type: &'static str,
    /// The incident under verification.
    pub(super) incident: &'a Incident,
    /// The repair that was supposed to fix it.
    pub(super) repair: &'a Repair,
    /// The deployment that shipped it.
    pub(super) production: &'a Production,
}

/// What one pass did, and what it implies about when to come back.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Dispatched {
    /// Intents claimed.
    pub(super) due: usize,
    /// Intents that produced an event and were recorded.
    pub(super) completed: usize,
    /// Intents that did not, and will be retried once their claim lapses.
    pub(super) failed: usize,
    /// Whether the cleanup page was full, meaning more keys are waiting.
    pub(super) cleanup_pending: bool,
}

impl Dispatched {
    /// How long to wait before the next pass.
    pub(super) fn pacing(self) -> Duration {
        // A full cleanup page means more keys are waiting, and forgetting them
        // costs one round trip each — so the next pass follows immediately
        // rather than leaving keys in Redis for a minute at a time.
        if self.cleanup_pending {
            return Duration::ZERO;
        }
        let full_batch = self.due >= usize::try_from(DUE_BATCH_LIMIT).unwrap_or(usize::MAX);
        if self.failed > 0 && (!full_batch || self.completed == 0) {
            // Coming back sooner would find the failed rows still claimed by
            // this very pass, so the wait is exactly a claim's life.
            return CLAIM_STALE;
        }
        if full_batch {
            // A backlog: the next batch is already waiting.
            return Duration::ZERO;
        }
        NOTHING_DUE_INTERVAL
    }
}
