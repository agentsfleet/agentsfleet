//! A run's side of a parked sandbox: the key it files and finds a park under,
//! taking the fleet's park before a fresh sandbox is built, and parking the
//! sandbox when the run ends processed instead of destroying it.
//!
//! A parked sandbox keeps the repositories the last run left exactly as they
//! are, its uncommitted edits and local commits with them. The host never
//! writes into, or reads, a working copy a tenant has had write access to:
//! the first checkout is safe only because the host made the directory it
//! writes into (`crate::workspace_clone`).

use std::time::Duration;

use af_core::clock::saturating_millis;
use af_core::error_code::ErrorCode;
use af_observability::semconv::ATTR_SANDBOX_REUSED;
use afr_engine::{Limits, Parked, Sandbox, SandboxState};
use afr_telemetry::labels::SandboxPark;
use afr_telemetry::record;
use jiff::Timestamp;

use super::Run;
use crate::egress::Bound;
use crate::parks::{BuiltUnder, ParkKey, Parks, Release, Stowed, Taken};
use crate::report::Ending;

/// How long a resumed sandbox's executor has to answer before the park is
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
/// The workspace's own top, as the executor reads a relative path whatever
/// its root is mounted at.
const WORKSPACE_TOP: &str = ".";
/// Why a resumed sandbox is given up when its executor does not answer.
const DETAIL_SILENT: &str = "the resumed sandbox's executor did not answer";

/// The sandbox a run ran in, and the key it would be parked under.
#[derive(Debug)]
pub(super) struct Kept {
    pub(super) recovery: Option<super::recovery::State>,
    pub(super) sandbox: Box<dyn Sandbox>,
    pub(super) key: ParkKey,
}

/// What a run's work hands back: how it ended, and the sandbox it ran in,
/// for the run to park or destroy once its cut and its report decide.
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
    /// No sandbox to park or release.
    Gone,
    /// Parked for the fleet's next run, until then.
    Parked(Timestamp),
    /// Killed, its disk, cgroup and egress waiting for the release.
    Killed(Box<dyn Sandbox>),
    /// Parked, but the keeper had no room for it; destroyed at the release.
    Unfiled(Box<dyn Parked>),
}

impl Fate {
    /// When the park lapses, for the report to carry.
    pub(super) const fn parked_until(&self) -> Option<Timestamp> {
        match self {
            Self::Parked(until) => Some(*until),
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
    pub(super) fn park_key(&self, limits: Limits, bound: &Bound) -> ParkKey {
        ParkKey {
            fleet: self.ids.fleet.clone(),
            workspace: self.payload.event.workspace_id.to_string(),
            limits,
            policy: BuiltUnder::of(&self.payload.policy, bound),
        }
    }

    /// The fleet's parked sandbox, resumed, confined to what `bound` admits
    /// and answering, or none. A park the run was not told to resume is
    /// ended, and so is one that will not resume, will not take `bound`'s
    /// addresses, or whose executor stays silent; the run builds a fresh one.
    pub(super) async fn revive(&self, key: &ParkKey, bound: &Bound) -> Option<Box<dyn Sandbox>> {
        let parks = &self.lessee.parks;
        if !self.payload.resume_park {
            // The daemon does not record this runner's park as the fleet's
            // latest run: another runner ran the fleet since, or this event
            // is a redelivery whose first attempt the park may carry.
            parks.release(key.fleet.clone(), Release::Superseded);
            return None;
        }
        let Taken { sandbox, parked } = parks.take(key).await?;
        let parked_ms = saturating_millis(parked);
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
                tracing::info!(run_id, fleet_id, parked_ms, event);
                record::sandbox_park(SandboxPark::Reused);
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
        Parks::count_release(&key.fleet, Release::ThawFailed);
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
            sandbox,
            key,
            recovery,
        }) = kept
        else {
            self.capture_without_sandbox(ending).await;
            return Fate::Gone;
        };
        // RunPlane is a child of serving, so a runner shutting down or stopped
        // counts too: it takes no run that could take the park.
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
        match self.lessee.parks.park(key, run, parked).await {
            Stowed::Parked(until) => Fate::Parked(until),
            Stowed::Refused(parked) => Fate::Unfiled(parked),
            Stowed::Gone => Fate::Gone,
        }
    }
}

/// Why a parked sandbox could not serve its fleet's next run: the event its
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
    let refused = |event, failure: afr_engine::Error| GivenUp {
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
    // The next run's plan, numbered above the one the sandbox was parked
    // under, reads back in force before its first command.
    if let Err(failure) = sandbox.egress(bound.plan()).await {
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

/// Records on the run's span whether its sandbox was a parked one.
pub(super) fn mark_reused(reused: bool) {
    tracing::Span::current().record(ATTR_SANDBOX_REUSED, reused);
}
#[cfg(test)]
#[path = "tests/parks/acknowledgement.rs"]
mod acknowledgement_tests;
#[cfg(test)]
#[path = "tests/parks/reassignment.rs"]
mod reassign_tests;
#[cfg(test)]
#[path = "tests/parks/refill.rs"]
mod refill_tests;
#[cfg(test)]
#[path = "tests/parks/revive.rs"]
mod revive_tests;
#[cfg(test)]
#[path = "tests/parks/reuse.rs"]
mod tests;
#[cfg(test)]
#[path = "tests/parks/release.rs"]
mod unparked_tests;
