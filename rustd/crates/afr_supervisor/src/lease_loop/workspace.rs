//! The lease's sandbox and its workspace: the lease's egress is resolved, the
//! fleet's held sandbox is taken or a fresh one built, repositories are checked
//! out, and the turn runs with its pinned native instructions.
//! The sandbox goes back to the lease, which holds it for the fleet's next
//! lease or destroys it.

use std::time::Instant;

use afd_core::error_code::{Coded as _, Logged};
use afd_wire::lease::SandboxLimits;
use afd_wire::report::FailureClass;
use afr_memory::Seed;
use afr_sandbox::{Limits, Sandbox, SandboxRequest};
use afr_telemetry::labels::SandboxStart;
use afr_telemetry::record;

use super::hold::{Kept, Worked, mark_reused};
use super::segments::{Segment, Stopwatch};
use super::{LeaseRun, failed};
use crate::activity::ActivitySink;
use crate::egress::Bound;
use crate::holds::{HoldKey, Release};
use crate::report::Ending;

const DETAIL_SANDBOX: &str = "this host could not build a sandbox for the run";
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
const EVENT_SANDBOX_REFUSED: &str = "sandbox_refused";
const EVENT_SIZE_REFUSED: &str = "sandbox_size_refused";

impl LeaseRun<'_> {
    /// Resolves the lease's egress, takes the fleet's held sandbox built to
    /// reach the same, or builds one and checks the bound repositories out
    /// into it, then runs the turn in it and hands it back. A held sandbox
    /// keeps the repositories as its last lease left them. An egress that will
    /// not resolve, a sandbox that cannot be built, or a repository that will
    /// not check out, ends the lease at startup.
    pub(super) async fn sandboxed(&self, memory: Seed<'_>, sink: ActivitySink) -> Worked {
        // Timed from here to the landed workspace, whichever way it ends.
        let setup = self.timeline.start(Segment::Sandbox);
        let limits = match sized(self.lease.limits, self.lessee.limits) {
            Ok(limits) => limits,
            Err(failure) => {
                self.release_hold();
                return self
                    .refuse(&failure, EVENT_SIZE_REFUSED, DETAIL_SIZE)
                    .into();
            }
        };
        let resolving = self.timeline.start(Segment::Egress);
        let bound = self.bind().await;
        resolving.stop();
        let bound = match bound {
            Ok(bound) => bound,
            Err(refused) => {
                self.release_hold();
                return (*refused).into();
            }
        };
        let key = self.hold_key(limits, &bound);
        let (sandbox, revived) = match self.acquire(&key, limits, &bound).await {
            Ok(acquired) => acquired,
            Err(refused) => return (*refused).into(),
        };
        let checked_out = if revived {
            Ok(())
        } else {
            let checking_out = self.timeline.start(Segment::Checkout);
            let checked_out = self.check_out(sandbox.as_ref()).await;
            checking_out.stop();
            checked_out
        };
        let ending = match checked_out {
            Ok(()) => self.in_sandbox(memory, sandbox.as_ref(), sink, setup).await,
            Err(refused) => *refused,
        };
        Worked {
            ending,
            kept: Some(Kept { sandbox, key }),
        }
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

    /// The fleet's held sandbox filed under `key`, or a fresh one, and whether
    /// it was held. Timed as the prepare part either way: the watch records
    /// when it drops, so a refused build is timed too.
    async fn acquire(
        &self,
        key: &HoldKey,
        limits: Limits,
        bound: &Bound,
    ) -> Result<(Box<dyn Sandbox>, bool), Box<Ending>> {
        let _preparing = self.timeline.start(Segment::Prepare);
        let held = self.revive(key, bound).await;
        mark_reused(held.is_some());
        match held {
            Some(sandbox) => Ok((sandbox, true)),
            None => self
                .prepare(limits, bound)
                .await
                .map(|sandbox| (sandbox, false)),
        }
    }

    /// Builds a fresh sandbox enforcing `limits` and reaching what `bound`
    /// admits, or the startup failure that ends the lease when the host
    /// cannot.
    async fn prepare(
        &self,
        limits: Limits,
        bound: &Bound,
    ) -> Result<Box<dyn Sandbox>, Box<Ending>> {
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
        prepared.map_err(|failure| {
            Box::new(self.refuse(&failure, EVENT_SANDBOX_REFUSED, DETAIL_SANDBOX))
        })
    }

    async fn in_sandbox(
        &self,
        memory: Seed<'_>,
        sandbox: &dyn Sandbox,
        sink: ActivitySink,
        setup: Stopwatch<'_>,
    ) -> Ending {
        setup.stop();
        self.drive(memory, Some(sandbox.executor()), sink).await
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
