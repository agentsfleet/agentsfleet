//! A run's side of a held sandbox: the key it files and finds a hold under,
//! taking the fleet's hold before a fresh sandbox is built, and parking the
//! sandbox when the run ends processed instead of destroying it.
//!
//! A held sandbox keeps the repositories the last run left exactly as they
//! are, its uncommitted edits and local commits with them. The host never
//! writes into, or reads, a working copy a tenant has had write access to:
//! the first checkout is safe only because the host made the directory it
//! writes into (`crate::workspace_clone`).

use std::time::{Duration, Instant};

use af_core::clock::saturating_millis;
use af_core::error_code::{Coded as _, ErrorCode, Logged};
use af_observability::semconv::ATTR_SANDBOX_REUSED;
use afr_sandbox::{Killed, Limits, Parked, Sandbox, SandboxState};
use afr_telemetry::labels::{SandboxHold, SandboxRelease};
use afr_telemetry::record;
use jiff::Timestamp;

use super::Run;
use crate::egress::Bound;
use crate::holds::{BuiltUnder, HoldKey, Holds, Release, Stowed, Taken};
use crate::report::Ending;

/// How long a resumed sandbox's executor has to answer before the hold is
/// given up for a fresh sandbox.
const RESUME_ANSWER_WAIT: Duration = Duration::from_secs(2);
const EVENT_REUSED: &str = "sandbox_reused";
const EVENT_RESUME_STARTED: &str = "sandbox_resume_started";
const EVENT_RESUME_COMPLETED: &str = "sandbox_resume_completed";
const EVENT_RESUME_FAILED: &str = "sandbox_resume_failed";
const EVENT_REFILL_FAILED: &str = "sandbox_refill_failed";
const EVENT_PARK_STARTED: &str = "sandbox_park_started";
const EVENT_PARK_COMPLETED: &str = "sandbox_park_completed";
const EVENT_PARK_FAILED: &str = "sandbox_park_failed";
const EVENT_DESTROY_FAILED: &str = "sandbox_destroy_failed";
const EVENT_KILL_STARTED: &str = "sandbox_kill_started";
const EVENT_KILL_COMPLETED: &str = "sandbox_kill_completed";
const EVENT_KILL_UNSETTLED: &str = "sandbox_kill_unsettled";
/// The workspace's own top, as the executor reads a relative path whatever
/// its root is mounted at.
const WORKSPACE_TOP: &str = ".";
/// Why a resumed sandbox is given up when its executor does not answer.
const DETAIL_SILENT: &str = "the resumed sandbox's executor did not answer";

/// The sandbox a run ran in, and the key it would be held under.
#[derive(Debug)]
pub(super) struct Kept {
    pub(super) recovery: Option<super::recovery::State>,
    pub(super) sandbox: Box<dyn Sandbox>,
    pub(super) key: HoldKey,
}

/// What a run's work hands back: how it ended, and the sandbox it ran in,
/// for the run to hold or destroy once its cut and its report decide.
#[derive(Debug)]
pub(super) struct Worked {
    pub(super) ending: Ending,
    pub(super) kept: Option<Kept>,
}

/// What becomes of a run's sandbox at its report: parked until a time, or
/// killed now and released once the report is in. Only `keep` makes one, so
/// the report cannot be settled before the kill.
#[derive(Debug)]
#[must_use]
pub(super) enum Fate {
    /// No sandbox to hold or release.
    Gone,
    /// Parked for the fleet's next run, until then.
    Held(Timestamp),
    /// Killed, its disk, cgroup and egress waiting for the release.
    Killed(Box<dyn Sandbox>),
    /// Parked, but the keeper had no room for it; destroyed at the release.
    Unfiled(Box<dyn Parked>),
}

impl Fate {
    /// When the hold lapses, for the report to carry.
    pub(super) const fn held_until(&self) -> Option<Timestamp> {
        match self {
            Self::Held(until) => Some(*until),
            Self::Gone | Self::Killed(_) | Self::Unfiled(_) => None,
        }
    }
}

impl From<Ending> for Worked {
    fn from(ending: Ending) -> Self {
        Self { ending, kept: None }
    }
}

impl Run<'_> {
    /// The key this run's sandbox, enforcing `limits` and reaching what
    /// `bound` admits, is filed and found under.
    pub(super) fn hold_key(&self, limits: Limits, bound: &Bound) -> HoldKey {
        HoldKey {
            fleet: self.ids.fleet.clone(),
            workspace: self.payload.event.workspace_id.to_string(),
            limits,
            policy: BuiltUnder::of(&self.payload.policy, bound),
        }
    }

    /// The fleet's parked sandbox, resumed, confined to what `bound` admits
    /// and answering, or none. A hold the run was not told to resume is
    /// ended, and so is one that will not resume, will not take `bound`'s
    /// addresses, or whose executor stays silent; the run builds a fresh one.
    pub(super) async fn revive(&self, key: &HoldKey, bound: &Bound) -> Option<Box<dyn Sandbox>> {
        let holds = &self.lessee.holds;
        if !self.payload.resume_hold {
            // The daemon does not record this runner's hold as the fleet's
            // latest run: another runner ran the fleet since, or this event
            // is a redelivery whose first attempt the hold may carry.
            holds.release(key.fleet.clone(), Release::Superseded);
            return None;
        }
        let Taken { sandbox, held } = holds.take(key).await?;
        let held_ms = saturating_millis(held);
        let run_id = self.ids.run.as_str();
        let (
            GivenUp {
                event,
                code,
                reason,
            },
            resumed,
        ) = match resume(sandbox, bound, run_id).await {
            Ok(sandbox) => {
                let event = EVENT_RESUME_COMPLETED;
                tracing::debug!(run_id, event);
                let fleet_id = key.fleet.as_str();
                let event = EVENT_REUSED;
                tracing::info!(run_id, fleet_id, held_ms, event);
                record::sandbox_hold(SandboxHold::Reused);
                return Some(sandbox);
            }
            Err(given_up) => given_up,
        };
        let error_code = code.as_str();
        {
            let (message,) = (format_args!(
                "a parked sandbox would not resume; the run gets a fresh one"
            ),);
            tracing::warn!(message, error_code, run_id, reason, event);
        };
        Holds::count_release(&key.fleet, Release::ThawFailed);
        if let Some(sandbox) = resumed {
            let _logged = self.destroy(sandbox).await;
        }
        None
    }

    /// Parks the run's sandbox for the fleet's next run when the run ended
    /// processed and ran to its own end, the runner still takes runs, the
    /// sandbox still runs and its engine parks; kills it otherwise, for its
    /// release after the report. A task's final capture runs first, while
    /// the sandbox still serves.
    pub(super) async fn keep(&self, kept: Option<Kept>, ending: &mut Ending) -> Fate {
        let Some(Kept {
            mut sandbox,
            key,
            recovery,
        }) = kept
        else {
            self.capture_without_sandbox(ending).await;
            return Fate::Gone;
        };
        // RunPlane is a child of serving, so a runner shutting down or stopped
        // counts too: it takes no run that could take the hold.
        let taking_runs = !self.lessee.halt.taking_runs().is_cancelled();
        let durable = recovery.is_some();
        let recoverable = matches!(ending, Ending::Ran { output, .. } if output.recovery.is_some());
        let ran_out = (ending.processed() || recoverable) && !self.interrupt.is_cancelled();
        if !(ran_out && taking_runs && sandbox.state() == SandboxState::Running) {
            if durable {
                super::recovery::capture_unavailable(ending);
            }
            return self.kill(sandbox).await;
        }
        let Ok(_reservation) = self
            .reserve_terminal_capture(sandbox.as_ref(), recoverable)
            .await
        else {
            super::recovery::capture_unavailable(ending);
            return self.kill(sandbox).await;
        };
        self.capture_final(sandbox.as_ref(), recovery.as_ref(), ending)
            .await;
        // Durable tasks restore a fresh workspace under their native checkpoint.
        if durable || !ending.processed() {
            return self.kill(sandbox).await;
        }
        let Some(&mode) = self.lessee.engine.capabilities().parks.first() else {
            return self.kill(sandbox).await;
        };
        let run_id = self.ids.run.as_str();
        let event = EVENT_PARK_STARTED;
        tracing::debug!(run_id, event);
        let parked = match sandbox.park(mode).await {
            Ok(parked) => parked,
            Err(failure) => {
                let error_code = failure.code().as_str();
                let event = EVENT_PARK_FAILED;
                {
                    let (message,) = (format_args!(
                        "a sandbox would not park, so its engine destroyed it"
                    ),);
                    tracing::warn!(message, error_code, run_id, event);
                };
                return Fate::Gone;
            }
        };
        let event = EVENT_PARK_COMPLETED;
        tracing::debug!(run_id, event);
        let run = self.ids.run.clone();
        match self.lessee.holds.park(key, run, parked).await {
            Stowed::Held(until) => Fate::Held(until),
            Stowed::Refused(parked) => Fate::Unfiled(parked),
            Stowed::Gone => Fate::Gone,
        }
    }

    /// Kills `sandbox` now, for its release after the report. A kill that
    /// cannot prove the tree empty still lets the report post, and warns.
    async fn kill(&self, sandbox: Box<dyn Sandbox>) -> Fate {
        let run_id = self.ids.run.as_str();
        let event = EVENT_KILL_STARTED;
        tracing::debug!(run_id, event);
        match sandbox.kill().await {
            Killed::Settled => {
                let event = EVENT_KILL_COMPLETED;
                tracing::debug!(run_id, event);
            }
            Killed::Unsettled(failure) => {
                let Logged { error_code, reason } = failure.logged();
                let event = EVENT_KILL_UNSETTLED;
                {
                    let (message,) = (format_args!(
                        "a sandbox's processes were not proved gone; the report posts anyway"
                    ),);
                    tracing::warn!(message, error_code, run_id, reason, event);
                };
            }
        }
        Fate::Killed(sandbox)
    }

    /// Releases what a killed sandbox held, after the report: its egress
    /// rules, cgroup, mount, image and directory. Timed; a failure leaves its
    /// directory for the boot sweep.
    pub(super) async fn release(&self, fate: Fate) {
        let started = Instant::now();
        let destroyed = match fate {
            Fate::Gone | Fate::Held(_) => return,
            Fate::Killed(sandbox) => self.destroy(sandbox).await,
            Fate::Unfiled(parked) => self.logged_destroy(parked.destroy().await),
        };
        let outcome = match destroyed {
            Ok(()) => SandboxRelease::Released,
            Err(_logged) => SandboxRelease::Failed,
        };
        record::sandbox_release(outcome, started.elapsed());
    }

    /// Ends every process in `sandbox` and removes it, logging a teardown
    /// that left something behind.
    ///
    /// # Errors
    /// The engine's own, once logged: what is left is the boot sweep's.
    pub(super) async fn destroy(&self, sandbox: Box<dyn Sandbox>) -> afr_sandbox::Result<()> {
        self.logged_destroy(sandbox.destroy().await)
    }

    /// `destroyed`, logged when its teardown left something behind.
    fn logged_destroy(&self, destroyed: afr_sandbox::Result<()>) -> afr_sandbox::Result<()> {
        if let Err(failure) = &destroyed {
            let code = failure.code().as_str();
            let run_id = self.ids.run.as_str();
            let event = EVENT_DESTROY_FAILED;
            {
                let (error_code, message) =
                    (&code, format_args!("a sandbox did not tear down cleanly"));
                tracing::warn!(message, error_code, run_id, event);
            };
        }
        destroyed
    }
}

/// Why a held sandbox could not serve its fleet's next run: the event its
/// failed step is logged under, the failure's code, and why.
struct GivenUp {
    event: &'static str,
    code: ErrorCode,
    reason: String,
}

/// Resumes a parked sandbox, confines it to what `bound` admits when that is
/// an allowlist, and waits for its executor to answer. The addresses change
/// before the run's first command, so nothing runs against the old set once
/// its names name the new one. A failure answers why, with the sandbox when
/// it resumed and still needs destroying.
async fn resume(
    parked: Box<dyn Parked>,
    bound: &Bound,
    run_id: &str,
) -> Result<Box<dyn Sandbox>, (GivenUp, Option<Box<dyn Sandbox>>)> {
    let refused = |event, failure: afr_sandbox::Error| GivenUp {
        event,
        code: failure.code(),
        reason: failure.told(),
    };
    let event = EVENT_RESUME_STARTED;
    tracing::debug!(run_id, event);
    let mut sandbox = parked
        .resume()
        .await
        .map_err(|failure| (refused(EVENT_RESUME_FAILED, failure), None))?;
    if let Bound::Allowed(allowlist) = bound
        && let Err(failure) = sandbox.reallow(allowlist).await
    {
        return Err((refused(EVENT_REFILL_FAILED, failure), Some(sandbox)));
    }
    match answered(sandbox.as_ref()).await {
        Ok(()) => Ok(sandbox),
        Err(silent) => Err((
            GivenUp {
                event: EVENT_RESUME_FAILED,
                code: silent.code(),
                reason: silent.wire_message(),
            },
            Some(sandbox),
        )),
    }
}

/// Waits for a resumed sandbox's executor to answer.
async fn answered(sandbox: &dyn Sandbox) -> afr_executor::Result<()> {
    let answer = sandbox.executor().list_dir(WORKSPACE_TOP);
    tokio::time::timeout(RESUME_ANSWER_WAIT, answer)
        .await
        .unwrap_or_else(|_late| Err(silent()))
        .map(drop)
}

/// The failure a resumed executor that never answered is logged as.
fn silent() -> afr_executor::Error {
    std::io::Error::new(std::io::ErrorKind::TimedOut, DETAIL_SILENT).into()
}

/// Records on the run's span whether its sandbox was a held one.
pub(super) fn mark_reused(reused: bool) {
    tracing::Span::current().record(ATTR_SANDBOX_REUSED, reused);
}
#[cfg(test)]
#[path = "tests/holds/acknowledgement.rs"]
mod acknowledgement_tests;
#[cfg(test)]
#[path = "tests/holds/reassignment.rs"]
mod reassign_tests;
#[cfg(test)]
#[path = "tests/holds/refill.rs"]
mod refill_tests;
#[cfg(test)]
#[path = "tests/release.rs"]
mod release_tests;
#[cfg(test)]
#[path = "tests/holds/revive.rs"]
mod revive_tests;
#[cfg(test)]
#[path = "tests/holds/reuse.rs"]
mod tests;
#[cfg(test)]
#[path = "tests/holds/release.rs"]
mod unheld_tests;
