//! A lease's side of a held sandbox: the key it files and finds a hold under,
//! taking the fleet's hold before a fresh sandbox is built, and parking the
//! sandbox when the lease ends processed instead of destroying it.
//!
//! A held sandbox keeps the repositories the last lease left exactly as they
//! are, its uncommitted edits and local commits with them. The host never
//! writes into, or reads, a working copy a tenant has had write access to:
//! the first checkout is safe only because the host made the directory it
//! writes into (`crate::workspace_clone`).

use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::error_code::ErrorCode;
use afd_observability::semconv::ATTR_SANDBOX_REUSED;
use afr_sandbox::{Limits, Sandbox};
use afr_telemetry::labels::SandboxHold;
use afr_telemetry::record;

use super::LeaseRun;
use crate::egress::Bound;
use crate::holds::{BuiltUnder, HoldKey, Release, Taken};
use crate::report::Ending;

/// How long a thawed sandbox's executor has to answer before the hold is
/// given up for a fresh sandbox.
const THAW_ANSWER_WAIT: Duration = Duration::from_secs(2);
const EVENT_REUSED: &str = "sandbox_reused";
const EVENT_THAW_STARTED: &str = "sandbox_thaw_started";
const EVENT_THAW_COMPLETED: &str = "sandbox_thaw_completed";
const EVENT_THAW_FAILED: &str = "sandbox_thaw_failed";
const EVENT_REFILL_FAILED: &str = "sandbox_refill_failed";
const EVENT_FREEZE_STARTED: &str = "sandbox_freeze_started";
const EVENT_FREEZE_COMPLETED: &str = "sandbox_freeze_completed";
const EVENT_FREEZE_FAILED: &str = "sandbox_freeze_failed";
const EVENT_DESTROY_FAILED: &str = "sandbox_destroy_failed";
/// The workspace's own top, as the executor reads a relative path whatever
/// its root is mounted at.
const WORKSPACE_TOP: &str = ".";
/// Why a thawed sandbox is given up when its executor does not answer.
const DETAIL_SILENT: &str = "the thawed sandbox's executor did not answer";

/// The sandbox a lease ran in, and the key it would be held under.
#[derive(Debug)]
pub(super) struct Kept {
    pub(super) sandbox: Box<dyn Sandbox>,
    pub(super) key: HoldKey,
}

/// What a lease's work hands back: how it ended, and the sandbox it ran in,
/// for the lease to hold or destroy once its cut and its report decide.
#[derive(Debug)]
pub(super) struct Worked {
    pub(super) ending: Ending,
    pub(super) kept: Option<Kept>,
}

impl From<Ending> for Worked {
    fn from(ending: Ending) -> Self {
        Self { ending, kept: None }
    }
}

impl LeaseRun<'_> {
    /// The key this lease's sandbox, enforcing `limits` and reaching what
    /// `bound` admits, is filed and found under.
    pub(super) fn hold_key(&self, limits: Limits, bound: &Bound) -> HoldKey {
        HoldKey {
            fleet: self.ids.fleet.clone(),
            workspace: self.lease.event.workspace_id.to_string(),
            limits,
            policy: BuiltUnder::of(&self.lease.policy, bound),
        }
    }

    /// The fleet's held sandbox, holding to what `bound` admits, thawed and
    /// answering, or none. A hold the lease was not told to resume is ended,
    /// and so is one that will not take `bound`'s addresses, will not thaw,
    /// or whose executor stays silent; the lease builds a fresh one.
    pub(super) async fn revive(&self, key: &HoldKey, bound: &Bound) -> Option<Box<dyn Sandbox>> {
        let holds = &self.lessee.holds;
        if !self.lease.resume_hold {
            // The daemon does not record this runner's hold as the fleet's
            // latest run: another runner ran the fleet since, or this event
            // is a redelivery whose first attempt the hold may carry.
            holds.release(key.fleet.clone(), Release::Superseded);
            return None;
        }
        let Taken {
            mut sandbox,
            held_ms,
        } = holds.take(key).await?;
        let lease_id = self.ids.lease.as_str();
        let given_up = resume(sandbox.as_mut(), bound, lease_id).await.err();
        let Some(GivenUp {
            event,
            code,
            reason,
        }) = given_up
        else {
            let event = EVENT_THAW_COMPLETED;
            tracing::debug!(lease_id, event);
            let fleet_id = key.fleet.as_str();
            let event = EVENT_REUSED;
            tracing::info!(lease_id, fleet_id, held_ms, event);
            record::sandbox_hold(SandboxHold::Reused);
            return Some(sandbox);
        };
        let error_code = code.as_str();
        tracing::warn!(
            error_code,
            lease_id,
            reason,
            event,
            "a held sandbox would not resume; the lease gets a fresh one"
        );
        let fleet = key.fleet.clone();
        holds.discard(fleet, sandbox, Release::ThawFailed).await;
        None
    }

    /// Holds the lease's sandbox, frozen, for the fleet's next lease when the
    /// lease ended processed and ran to its own end, the runner still takes
    /// leases and the sandbox still runs; destroys it otherwise. Answers when
    /// the hold lapses, for the report to carry.
    pub(super) async fn keep(&self, kept: Option<Kept>, ending: &Ending) -> Option<UnixMillis> {
        let Kept { mut sandbox, key } = kept?;
        // Leasing is a child of serving, so a runner shutting down or stopped
        // counts too: it takes no lease that could take the hold.
        let leasing = !self.lessee.halt.leasing().is_cancelled();
        let ran_out = ending.processed() && !self.interrupt.is_cancelled();
        if !(ran_out && leasing && sandbox.is_running()) {
            self.destroy(sandbox).await;
            return None;
        }
        let lease_id = self.ids.lease.as_str();
        let event = EVENT_FREEZE_STARTED;
        tracing::debug!(lease_id, event);
        if let Err(failure) = sandbox.freeze().await {
            let error_code = failure.code().as_str();
            let event = EVENT_FREEZE_FAILED;
            tracing::warn!(
                error_code,
                lease_id,
                event,
                "a sandbox would not freeze, so it is destroyed rather than held"
            );
            self.destroy(sandbox).await;
            return None;
        }
        let event = EVENT_FREEZE_COMPLETED;
        tracing::debug!(lease_id, event);
        let lease = self.ids.lease.clone();
        self.lessee.holds.park(key, lease, sandbox).await
    }

    /// Ends every process in `sandbox` and removes it.
    pub(super) async fn destroy(&self, sandbox: Box<dyn Sandbox>) {
        if let Err(failure) = sandbox.destroy().await {
            let code = failure.code().as_str();
            let lease_id = self.ids.lease.as_str();
            let event = EVENT_DESTROY_FAILED;
            tracing::warn!(
                error_code = code,
                lease_id,
                event,
                "a sandbox did not tear down cleanly"
            );
        }
    }
}

/// Why a held sandbox could not serve its fleet's next lease: the event its
/// failed step is logged under, the failure's code, and why.
struct GivenUp {
    event: &'static str,
    code: ErrorCode,
    reason: String,
}

/// Holds a frozen sandbox to what `bound` admits, when that is an allowlist,
/// then thaws it and waits for its executor to answer. The addresses change
/// while it is frozen, so no process in it runs against the old set once its
/// names name the new one.
async fn resume(sandbox: &mut dyn Sandbox, bound: &Bound, lease_id: &str) -> Result<(), GivenUp> {
    let refused = |event, failure: afr_sandbox::Error| GivenUp {
        event,
        code: failure.code(),
        reason: failure.told(),
    };
    if let Bound::Allowed(allowlist) = bound {
        sandbox
            .reallow(allowlist)
            .await
            .map_err(|failure| refused(EVENT_REFILL_FAILED, failure))?;
    }
    let event = EVENT_THAW_STARTED;
    tracing::debug!(lease_id, event);
    sandbox
        .thaw()
        .await
        .map_err(|failure| refused(EVENT_THAW_FAILED, failure))?;
    answered(sandbox).await.map_err(|silent| GivenUp {
        event: EVENT_THAW_FAILED,
        code: silent.code(),
        reason: silent.wire_message(),
    })
}

/// Waits for a thawed sandbox's executor to answer.
async fn answered(sandbox: &dyn Sandbox) -> afr_executor::Result<()> {
    let answer = sandbox.executor().list_dir(WORKSPACE_TOP);
    tokio::time::timeout(THAW_ANSWER_WAIT, answer)
        .await
        .unwrap_or_else(|_late| Err(silent()))
        .map(drop)
}

/// The failure a thawed executor that never answered is logged as.
fn silent() -> afr_executor::Error {
    std::io::Error::new(std::io::ErrorKind::TimedOut, DETAIL_SILENT).into()
}

/// Records on the lease's span whether its sandbox was a held one.
pub(super) fn mark_reused(reused: bool) {
    tracing::Span::current().record(ATTR_SANDBOX_REUSED, reused);
}

#[cfg(test)]
#[path = "hold_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "revive_tests.rs"]
mod revive_tests;

#[cfg(test)]
#[path = "unheld_tests.rs"]
mod unheld_tests;

#[cfg(test)]
#[path = "reassign_tests.rs"]
mod reassign_tests;

#[cfg(test)]
#[path = "refill_tests.rs"]
mod refill_tests;
