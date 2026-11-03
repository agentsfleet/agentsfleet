//! The lease's sandbox and its workspace, built beside the turn.
//!
//! ```text
//!   size ─► egress ─► checkout plan            refused here: startup_posture
//!     ├─► build: thaw or prepare ─► check out ─► fill the slot
//!     └─► turn:  model call ─► sandbox call waits on the slot ─► …
//! ```
//!
//! Checks that would fail on every lease run before any model request, and
//! refuse it. Then the build and the turn run in one task: the turn's
//! first model call never waits for the sandbox, and only a sandbox-side call
//! does. A build that fails leaves the turn going; its sandbox-side calls
//! answer `sandbox_unavailable`. The sandbox goes back to the lease, which
//! holds it for the fleet's next lease or destroys it.

use std::sync::OnceLock;
use std::time::Duration;

use afd_core::error_code::{Coded as _, Logged};
use afd_wire::lease::SandboxLimits;
use afd_wire::report::FailureClass;
use afr_executor::ExecutorSlot;
use afr_memory::Seed;
use afr_sandbox::Limits;
use tokio_util::sync::CancellationToken;

use super::build::Build;
use super::checkout::Plan;
use super::hold::{Kept, Worked};
use super::segments::Segment;
use super::{LeaseRun, failed};
use crate::activity::ActivitySink;
use crate::egress::Bound;
use crate::holds::Release;
use crate::report::Ending;

/// How long a turn that ended processed waits for its build, so the
/// complete sandbox can be held: long enough for a checkout from a warm
/// mirror, short against the fleet's next turn, which waits on this lease's
/// report. The answer is already out; past this, the build is stopped and
/// the report goes, so a stalled token mint, a wait on another lease's
/// mirror or a slow fetch does not hold the lease.
pub(super) const BUILD_AFTER_ANSWER: Duration = Duration::from_secs(20);
/// The event a build stopped because it outlasted its turn is logged under.
pub(super) const EVENT_BUILD_OUTLASTED: &str = "sandbox_build_outlasted_turn";
const DETAIL_SIZE: &str = "the lease asked for a sandbox size outside the bounds a runner builds";
/// What a lease whose egress allowlist could not be resolved reports: a host
/// that does not resolve, resolves to IPv6 alone, or an allowlist past its cap.
const DETAIL_EGRESS: &str =
    "the egress allowlist could not be resolved into addresses this runner can admit";
/// What a lease reports when a host the fleet's own `network.allow` names
/// resolves to a private or reserved address: the fleet's owner fixes this,
/// so it is told apart from the runner-side failures above.
const DETAIL_EGRESS_BLOCKED: &str =
    "the fleet allows an egress host at a private or reserved address";
const EVENT_EGRESS_REFUSED: &str = "egress_bind_refused";
const EVENT_SIZE_REFUSED: &str = "sandbox_size_refused";

/// What a lease proved before its first model request: the size its sandbox
/// enforces, what it may reach, and what it checks out.
struct Admitted<'l> {
    limits: Limits,
    bound: Bound,
    plan: Option<Plan<'l>>,
}

impl LeaseRun<'_> {
    /// Admits the lease's sandbox, then runs its build and its turn together
    /// and hands the sandbox back with the turn's ending. A held sandbox keeps
    /// the repositories as its last lease left them.
    pub(super) async fn sandboxed(&self, memory: Seed<'_>, sink: ActivitySink) -> Worked {
        // Timed from here to the build's end, whichever way it ends.
        let setup = self.timeline.start(Segment::Sandbox);
        let admitted = match self.admitted().await {
            Ok(admitted) => admitted,
            Err(refused) => return (*refused).into(),
        };
        let key = self.hold_key(admitted.limits, &admitted.bound);
        let stop = self.interrupt.child_token();
        let build = Build {
            key: &key,
            limits: admitted.limits,
            bound: &admitted.bound,
            plan: admitted.plan.as_ref(),
            stop: &stop,
        };
        // The cell outlives the build that fills it and the turn that borrows
        // its executor through the slot; the slot's scope ends the borrow.
        let cell = OnceLock::new();
        let (ending, held) = {
            let slot = ExecutorSlot::pending();
            // Both boxed: side by side, the build's state and the turn's would
            // sit in the lease's own future together, past the large-future
            // bound.
            let building = Box::pin(async {
                self.build(&build, &cell, &slot).await;
                setup.stop();
            });
            let driving = Box::pin(self.drive(memory, &slot, sink));
            let ending = self.beside(building, driving, &stop).await;
            (ending, slot.held())
        };
        self.timeline.waited(held);
        Worked {
            ending,
            kept: cell.into_inner().map(|sandbox| Kept { sandbox, key }),
        }
    }

    /// Runs the build and the turn together, and answers the turn's ending
    /// once both are done. A turn that ended processed and uncut waits up to
    /// [`BUILD_AFTER_ANSWER`] for the build, so a complete sandbox can be
    /// held; any other ending, or a build that outlasts the wait, stops the
    /// build first. Either way the build runs to its end, never dropped
    /// mid-step.
    async fn beside(
        &self,
        building: impl Future<Output = ()>,
        driving: impl Future<Output = Ending>,
        stop: &CancellationToken,
    ) -> Ending {
        tokio::pin!(building, driving);
        tokio::select! {
            biased;
            () = &mut building => driving.await,
            ending = &mut driving => {
                // Only a sandbox `keep` would hold is worth waiting for.
                let waits = ending.processed()
                    && !self.interrupt.is_cancelled()
                    && !self.lessee.halt.leasing().is_cancelled();
                if !waits || tokio::time::timeout(BUILD_AFTER_ANSWER, &mut building).await.is_err() {
                    if waits {
                        let lease_id = self.ids.lease.as_str();
                        let event = EVENT_BUILD_OUTLASTED;
                        tracing::info!(lease_id, event, "the build outlasted its answered turn");
                    }
                    stop.cancel();
                    building.await;
                }
                ending
            }
        }
    }

    /// Every check whose failure would repeat on each lease, made before any
    /// model request: the size, the egress and the checkout plan. A refusal
    /// ends the lease at startup and releases the fleet's hold.
    async fn admitted(&self) -> Result<Admitted<'_>, Box<Ending>> {
        let refused = |ending: Box<Ending>| {
            self.release_hold();
            ending
        };
        let limits = sized(self.lease.limits, self.lessee.limits).map_err(|failure| {
            refused(Box::new(self.refuse(
                &failure,
                EVENT_SIZE_REFUSED,
                DETAIL_SIZE,
            )))
        })?;
        let resolving = self.timeline.start(Segment::Egress);
        let bound = self.bind().await;
        resolving.stop();
        let bound = bound.map_err(refused)?;
        let plan = self.plan().map_err(refused)?;
        Ok(Admitted {
            limits,
            bound,
            plan,
        })
    }

    /// Releases the fleet's held sandbox for a lease refused before a sandbox
    /// was chosen. Such a lease settles with no hold, so the daemon records
    /// none and answers the fleet's next claim with `resume_hold` false: the
    /// held sandbox could never be resumed, only wait out its idle window in
    /// a worker's hold slot.
    fn release_hold(&self) {
        self.lessee
            .holds
            .release(self.ids.fleet.clone(), Release::Mismatch);
    }

    /// The lease's egress, resolved, or the startup failure that ends the
    /// lease when a host it names cannot be admitted.
    async fn bind(&self) -> Result<Bound, Box<Ending>> {
        let fleet = &self.lease.policy.network_policy;
        let resolver = self.lessee.resolver.as_ref();
        self.egress
            .bind(fleet, resolver)
            .await
            .map_err(|failure| Box::new(self.egress_refused(&failure)))
    }

    /// Logs why the lease's egress could not be bound, and ends the lease at
    /// startup. The log names the reason and how many hosts were asked for,
    /// never an address: the reason names a host, and its addresses stay on
    /// the host that resolved them.
    fn egress_refused(&self, failure: &crate::error::Error) -> Ending {
        let blocked = failure.is_egress_blocked();
        let Logged { error_code, reason } = failure.logged();
        let lease_id = self.ids.lease.as_str();
        let hosts = self.egress.hosts(&self.lease.policy.network_policy).len();
        let event = EVENT_EGRESS_REFUSED;
        let detail = if blocked {
            DETAIL_EGRESS_BLOCKED
        } else {
            DETAIL_EGRESS
        };
        tracing::warn!(error_code, lease_id, reason, hosts, event, detail);
        failed(FailureClass::StartupPosture, detail)
    }
}

/// The limits a lease's sandbox enforces: the size the lease asked for once
/// it is proved within the wire's bounds, or this host's own when it asked for
/// none.
///
/// # Errors
/// The size breaks one of the bounds `SandboxLimits` declares.
fn sized(asked: Option<SandboxLimits>, host: Limits) -> crate::error::Result<Limits> {
    match asked {
        Some(asked) => Ok(within(&garde::Unvalidated::new(asked).validate()?, host)),
        None => Ok(host),
    }
}

/// A proved size as limits. Processes and threads are not on the wire, so the
/// host's cap holds for every lease.
fn within(asked: &garde::Valid<SandboxLimits>, host: Limits) -> Limits {
    Limits {
        memory_bytes: asked.memory_bytes,
        cpu_millis: asked.cpu_millis,
        pids: host.pids,
        disk_bytes: asked.disk_bytes,
    }
}

#[cfg(test)]
#[path = "tests/sandbox.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/egress.rs"]
mod egress_tests;

#[cfg(test)]
#[path = "tests/setup_parts.rs"]
mod parts_tests;
