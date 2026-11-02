//! The lease's sandbox build, run beside its turn in the same task.
//!
//! ```text
//!   acquire: thaw the fleet's hold, or prepare ─► check out ─► fill the slot
//!   └── a stop is read between steps, never mid-step: a thaw always finishes ──┘
//! ```
//!
//! The build owns its sandbox until it is ready. A failure or a stop destroys
//! what it made at once, so nothing half-built is ever held, and fills the
//! slot with why, so every waiting call answers. A ready sandbox moves into
//! the lease's cell, which outlives the turn, and the slot lends the turn its
//! executor from there. Restore, when workspace snapshots land, is one more
//! way a repository arrives in place of its checkout.

use std::sync::OnceLock;
use std::time::Instant;

use afd_core::error_code::{Coded as _, Logged};
use afr_executor::{ExecutorSlot, Unavailable};
use afr_sandbox::{Limits, Sandbox, SandboxRequest};
use afr_telemetry::labels::SandboxStart;
use afr_telemetry::record;
use tokio_util::sync::CancellationToken;

use super::LeaseRun;
use super::checkout::Plan;
use super::hold::mark_reused;
use super::segments::Segment;
use crate::egress::Bound;
use crate::holds::HoldKey;

pub(super) const EVENT_BUILD_FAILED: &str = "sandbox_build_failed";

/// What one lease's build works from, every check before the turn passed.
pub(super) struct Build<'b> {
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

impl LeaseRun<'_> {
    /// Builds the lease's sandbox into `cell` and fills `slot`: with its
    /// executor once every repository is in place, or with why not.
    pub(super) async fn build<'c>(
        &self,
        build: &Build<'_>,
        cell: &'c OnceLock<Box<dyn Sandbox>>,
        slot: &ExecutorSlot<'c>,
    ) {
        match self.built(build).await {
            Ok(sandbox) => {
                let sandbox = cell.get_or_init(|| sandbox);
                slot.fill(Ok(sandbox.executor()));
            }
            Err(ended) => {
                if let Ended::Failed(class, logged) = &ended {
                    self.build_failed(*class, logged);
                }
                slot.fill(Err(ended.class()));
            }
        }
    }

    /// The sandbox, with every repository in place, or why there is none; a
    /// sandbox the build made and cannot hand over is destroyed here.
    async fn built(&self, build: &Build<'_>) -> Result<Box<dyn Sandbox>, Ended> {
        if build.stop.is_cancelled() {
            return Err(Ended::Stopped);
        }
        let (sandbox, held) = self.acquire(build).await?;
        let landed = match build.plan {
            // A held sandbox keeps the repositories its last lease left.
            Some(plan) if !held => {
                let _checking_out = self.timeline.start(Segment::Checkout);
                self.check_out(sandbox.as_ref(), plan, build.stop).await
            }
            _nothing_to_land => Ok(()),
        };
        // A stop outranks what the last step answered: the turn already ended.
        let landed = if build.stop.is_cancelled() {
            Err(Ended::Stopped)
        } else {
            landed
        };
        match landed {
            Ok(()) => Ok(sandbox),
            Err(ended) => {
                self.destroy(sandbox).await;
                Err(ended)
            }
        }
    }

    /// The fleet's held sandbox filed under the build's key, or a fresh one,
    /// and whether it was held. Timed as the prepare part either way: the
    /// watch records when it drops, so a refused build is timed too.
    async fn acquire(&self, build: &Build<'_>) -> Result<(Box<dyn Sandbox>, bool), Ended> {
        let _preparing = self.timeline.start(Segment::Prepare);
        let held = self.revive(build.key, build.bound).await;
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
