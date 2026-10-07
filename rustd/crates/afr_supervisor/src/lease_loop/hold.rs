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

use super::LeaseRun;
use crate::holds::{HoldKey, Release, Taken};
use crate::report::Ending;

/// How long a thawed sandbox's executor has to answer before the hold is
/// given up for a fresh sandbox.
const THAW_ANSWER_WAIT: Duration = Duration::from_secs(2);
const EVENT_REUSED: &str = "sandbox_reused";
const EVENT_THAW_STARTED: &str = "sandbox_thaw_started";
const EVENT_THAW_COMPLETED: &str = "sandbox_thaw_completed";
const EVENT_THAW_FAILED: &str = "sandbox_thaw_failed";
const EVENT_FREEZE_STARTED: &str = "sandbox_freeze_started";
const EVENT_FREEZE_COMPLETED: &str = "sandbox_freeze_completed";
const EVENT_FREEZE_FAILED: &str = "sandbox_freeze_failed";
/// The event a lease whose policy will not encode, and so neither takes nor
/// leaves a hold, is logged under.
const EVENT_KEY_FAILED: &str = "sandbox_hold_key_failed";
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
    pub(super) key: Option<HoldKey>,
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
    /// The key this lease's sandbox is filed and found under; none when the
    /// policy will not encode, so the lease neither takes nor leaves a hold.
    pub(super) fn hold_key(&self, limits: Limits) -> Option<HoldKey> {
        let policy = &self.lease.policy;
        let built_under = (&policy.network_policy, &policy.repository_binding);
        let encoded = match serde_json::to_string(&built_under) {
            Ok(encoded) => encoded,
            Err(failure) => {
                let failure = crate::error::encode(failure);
                let error_code = failure.code().as_str();
                let lease_id = self.ids.lease.as_str();
                let event = EVENT_KEY_FAILED;
                tracing::warn!(
                    error_code,
                    lease_id,
                    event,
                    "the policy would not encode, so the lease neither takes nor leaves a hold"
                );
                return None;
            }
        };
        Some(HoldKey {
            fleet: self.ids.fleet.clone(),
            workspace: self.lease.event.workspace_id.to_string(),
            limits,
            policy: encoded,
        })
    }

    /// The fleet's held sandbox, thawed and answering, or none. One that will
    /// not thaw, or whose executor stays silent, is destroyed, and the lease
    /// builds a fresh one.
    pub(super) async fn revive(&self, key: &HoldKey) -> Option<Box<dyn Sandbox>> {
        let Taken { sandbox, held_ms } = self.lessee.holds.take(key).await?;
        let lease_id = self.ids.lease.as_str();
        let event = EVENT_THAW_STARTED;
        tracing::debug!(lease_id, event);
        match thawed(sandbox.as_ref()).await {
            Ok(()) => {
                let event = EVENT_THAW_COMPLETED;
                tracing::debug!(lease_id, event);
                let fleet_id = key.fleet.as_str();
                let event = EVENT_REUSED;
                tracing::info!(lease_id, fleet_id, held_ms, event);
                Some(sandbox)
            }
            Err(failure) => {
                let error_code = failure.code().as_str();
                let reason = failure.reason();
                let event = EVENT_THAW_FAILED;
                tracing::warn!(
                    error_code,
                    lease_id,
                    reason,
                    event,
                    "a held sandbox would not resume; the lease gets a fresh one"
                );
                let fleet = key.fleet.clone();
                let holds = &self.lessee.holds;
                holds.discard(fleet, sandbox, Release::ThawFailed).await;
                None
            }
        }
    }

    /// Holds the lease's sandbox, frozen, for the fleet's next lease when the
    /// lease ended processed and ran to its own end; destroys it otherwise.
    /// Answers when the hold lapses, for the report to carry.
    pub(super) async fn keep(&self, kept: Option<Kept>, ending: &Ending) -> Option<UnixMillis> {
        let Kept { sandbox, key } = kept?;
        let key = match key {
            Some(key) if ending.processed() && !self.interrupt.is_cancelled() => key,
            _unheld => {
                self.destroy(sandbox).await;
                return None;
            }
        };
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

/// Why a held sandbox did not come back: it would not thaw, or its executor
/// did not answer once it had. Kept apart so the log names each one's cause.
#[derive(Debug)]
enum Unthawed {
    /// The sandbox would not thaw.
    Thaw(afr_sandbox::Error),
    /// Its executor stayed silent past the wait, or answered with a failure.
    Answer(afr_executor::Error),
}

impl Unthawed {
    /// The registry code the failure is logged under.
    fn code(&self) -> ErrorCode {
        match self {
            Self::Thaw(failure) => failure.code(),
            Self::Answer(failure) => failure.code(),
        }
    }

    /// The log's reason: an executor's failure with its cause, rendered as
    /// the executor logs its own.
    fn reason(&self) -> String {
        match self {
            Self::Thaw(failure) => failure.to_string(),
            Self::Answer(failure) => failure.wire_message(),
        }
    }
}

/// Thaws `sandbox` and waits for its executor to answer.
async fn thawed(sandbox: &dyn Sandbox) -> Result<(), Unthawed> {
    sandbox.thaw().await.map_err(Unthawed::Thaw)?;
    let answer = sandbox.executor().list_dir(WORKSPACE_TOP);
    let answered = tokio::time::timeout(THAW_ANSWER_WAIT, answer)
        .await
        .unwrap_or_else(|_late| Err(silent()));
    answered.map(drop).map_err(Unthawed::Answer)
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
