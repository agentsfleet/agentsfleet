//! The lease's sandbox build, run beside its turn in the same task.
//!
//! ```text
//!   acquire: thaw the fleet's hold, or prepare ─► check out ─► fill the slot
//!   └── a stop is read between steps, never mid-step: a thaw always finishes ──┘
//! ```
//!
//! The build owns its sandbox until it is ready. A failure or a stop fills the
//! slot with why first, so every waiting call answers, then destroys what it
//! made, so nothing half-built is ever held. A ready sandbox moves into
//! the lease's cell, which outlives the turn, and the slot lends the turn its
//! executor from there. Restore, when workspace snapshots land, is one more
//! way a repository arrives in place of its checkout.

use std::sync::OnceLock;
use std::time::Instant;

use afd_core::error_code::{Coded as _, Logged};
use afr_executor::{ExecutorSlot, Unavailable};
use afr_sandbox::{Limits, Sandbox, SandboxRequest};
use afr_telemetry::labels::{SandboxBuild, SandboxStart};
use afr_telemetry::record;
use tokio_util::sync::CancellationToken;

use super::LeaseRun;
use super::checkout::Plan;
use super::hold::mark_reused;
use super::segments::Segment;
use crate::egress::Bound;
use crate::holds::{HoldKey, Release};

pub(super) const EVENT_BUILD_FAILED: &str = "sandbox_build_failed";

/// What one lease's build works from, every check before the turn passed.
pub(super) struct Build<'b> {
    pub(super) prepared_task: Option<&'b afr_agent::PreparedTask>,
    pub(super) recovery: &'b OnceLock<super::recovery::State>,
    /// The key a held sandbox is found under, and the new one filed under.
    pub(super) key: &'b HoldKey,
    /// The size the sandbox enforces.
    pub(super) limits: Limits,
    /// What the sandbox may reach.
    pub(super) bound: &'b Bound,
    /// The repositories to check out, when a sandbox tool works in one.
    pub(super) plan: Option<&'b Plan<'b>>,
    /// Cancelled when the turn ended without its answer: the build stops at
    /// its next step.
    pub(super) stop: &'b CancellationToken,
}

/// Why a build ended with no sandbox the turn can use.
pub(super) enum Ended {
    /// Its stop was cancelled: the turn ended first.
    Stopped,
    /// A step failed: which, and the failure as its log names it.
    Failed(Unavailable, Logged),
}

impl Ended {
    /// What a sandbox-side call reads back.
    const fn class(&self) -> Unavailable {
        match self {
            Self::Stopped => Unavailable::Stopped,
            Self::Failed(class, _logged) => *class,
        }
    }
}

/// A build that ended without a sandbox: why, and the sandbox it made and
/// could not hand over, which is destroyed once the slot says why.
struct Unbuilt {
    ended: Ended,
    leftover: Option<Box<dyn Sandbox>>,
}

impl From<Ended> for Unbuilt {
    fn from(ended: Ended) -> Self {
        Self {
            ended,
            leftover: None,
        }
    }
}

/// The label a build's end is counted under. A build fails only at its
/// prepare or its checkout.
fn counted(built: &Result<Box<dyn Sandbox>, Unbuilt>) -> SandboxBuild {
    match built {
        Ok(_sandbox) => SandboxBuild::Ready,
        Err(Unbuilt {
            ended: Ended::Stopped,
            ..
        }) => SandboxBuild::Stopped,
        Err(Unbuilt {
            ended: Ended::Failed(Unavailable::Checkout, _logged),
            ..
        }) => SandboxBuild::Checkout,
        Err(Unbuilt {
            ended: Ended::Failed(_prepare, _logged),
            ..
        }) => SandboxBuild::Prepare,
    }
}

impl LeaseRun<'_> {
    /// Builds the lease's sandbox into `cell` and fills `slot`: with its
    /// executor once every repository is in place, or with why not.
    pub(super) async fn build<'c>(
        &self,
        build: &Build<'_>,
        cell: &'c OnceLock<Box<dyn Sandbox>>,
        slot: &ExecutorSlot<'c>,
    ) {
        let made = self.built(build).await;
        record::sandbox_build(counted(&made));
        match made {
            Ok(sandbox) => {
                let sandbox = cell.get_or_init(|| sandbox);
                slot.fill(Ok(sandbox.executor()));
            }
            Err(Unbuilt { ended, leftover }) => {
                if let Ended::Failed(class, logged) = &ended {
                    self.build_failed(*class, logged);
                }
                // Waiting calls hear why before this teardown, which can
                // take seconds while the cgroup drains. A prepare that failed
                // has already torn its own parts down inside the engine.
                slot.fill(Err(ended.class()));
                if let Some(sandbox) = leftover {
                    // Logged where it failed; what is left is the boot sweep's.
                    let _left_for_the_sweep = self.destroy(sandbox).await;
                }
            }
        }
    }

    /// The sandbox, with every repository in place, or why there is none and
    /// the sandbox the build made and cannot hand over.
    async fn built(&self, build: &Build<'_>) -> Result<Box<dyn Sandbox>, Unbuilt> {
        if build.stop.is_cancelled() {
            // Stopped before it took the fleet's hold: the lease settles with
            // no hold, so the daemon tells the fleet's next lease not to
            // resume it. Released now, it does not wait out its idle window.
            self.lessee
                .holds
                .release(self.ids.fleet.clone(), Release::Superseded);
            return Err(Ended::Stopped.into());
        }
        let (sandbox, held) = self.acquire(build).await?;
        let landed = match build.plan {
            // A held sandbox keeps the repositories its last lease left.
            Some(plan) if !held && !build.prepared_task.and_then(|prepared| prepared.inputs.current_task.as_ref())
                .is_some_and(|task| task.checkpoint_id.is_some()) => {
                let _checking_out = self.timeline.start(Segment::Checkout);
                self.check_out(sandbox.as_ref(), plan, build.stop).await
            }
            _nothing_to_land => Ok(()),
        };
        let landed = match landed {
            Ok(()) => self.prepare_recovery(sandbox.as_ref(), build).await
                .map_err(|failure| Ended::Failed(Unavailable::Prepare, failure.logged())),
            Err(failure) => Err(failure),
        };
        // A stop outranks what the last step answered: the turn already ended.
        let landed = if build.stop.is_cancelled() {
            Err(Ended::Stopped)
        } else {
            landed
        };
        match landed {
            Ok(()) => Ok(sandbox),
            Err(ended) => Err(Unbuilt {
                ended,
                leftover: Some(sandbox),
            }),
        }
    }

    /// The fleet's held sandbox filed under the build's key, or a fresh one,
    /// and whether it was held. Timed as the prepare part either way: the
    /// watch records when it drops, so a refused build is timed too.
    async fn acquire(&self, build: &Build<'_>) -> Result<(Box<dyn Sandbox>, bool), Ended> {
        let _preparing = self.timeline.start(Segment::Prepare);
        let held = if build.prepared_task.is_some() {
            // Durable state has one restore owner; an unrelated warm workspace
            // cannot silently replace the native checkpoint generation.
            self.lessee.holds.release(self.ids.fleet.clone(), Release::Superseded);
            None
        } else { self.revive(build.key, build.bound).await };
        mark_reused(held.is_some());
        match held {
            Some(sandbox) => Ok((sandbox, true)),
            None => self
                .prepare(build.limits, build.bound)
                .await
                .map(|sandbox| (sandbox, false)),
        }
    }

    /// Builds a fresh sandbox enforcing `limits` and reaching what `bound`
    /// admits.
    async fn prepare(&self, limits: Limits, bound: &Bound) -> Result<Box<dyn Sandbox>, Ended> {
        let request =
            SandboxRequest::new(self.ids.lease.as_str(), limits).with_network(bound.network());
        let started = Instant::now();
        let prepared = self.lessee.engine.prepare(request).await;
        let outcome = if prepared.is_ok() {
            SandboxStart::Ready
        } else {
            SandboxStart::Failed
        };
        record::sandbox_start(outcome, started.elapsed());
        prepared.map_err(|failure| Ended::Failed(Unavailable::Prepare, failure.logged()))
    }

    /// Logs a build that failed. The turn goes on without a sandbox, and its
    /// sandbox-side calls answer with `class`.
    fn build_failed(&self, class: Unavailable, logged: &Logged) {
        let Logged { error_code, reason } = logged;
        let lease_id = self.ids.lease.as_str();
        let class = class.as_str();
        let event = EVENT_BUILD_FAILED;
        tracing::warn!(
            error_code,
            lease_id,
            class,
            reason,
            event,
            "the sandbox could not be built; the run goes on without it"
        );
    }
}
