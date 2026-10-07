//! The lease's sandbox and its workspace: the fleet's held sandbox is taken
//! or a fresh one built, the bundle's support files land where the fleet's
//! instructions find them, and the turn runs. The sandbox goes back to the
//! lease, which holds it for the fleet's next lease or destroys it.

use std::time::Instant;

use afd_wire::lease::SandboxLimits;
use afd_wire::report::FailureClass;
use afr_executor::Executor;
use afr_memory::Seed;
use afr_sandbox::{Limits, Sandbox, SandboxRequest};
use afr_telemetry::labels::SandboxStart;
use afr_telemetry::record;

use super::hold::{Kept, Worked, mark_reused};
use super::{DETAIL_RENEWAL, LeaseRun, failed};
use crate::activity::ActivitySink;
use crate::bundles::Bundle;
use crate::report::Ending;

/// What a lease whose support files would not land reports.
const DETAIL_LANDING: &str =
    "the fleet bundle's support files could not be written to the workspace";
const DETAIL_SANDBOX: &str = "this host could not build a sandbox for the run";
const DETAIL_SIZE: &str = "the lease asked for a sandbox size outside the bounds a runner builds";
const EVENT_LANDING_FAILED: &str = "bundle_landing_failed";
const EVENT_SANDBOX_REFUSED: &str = "sandbox_refused";
const EVENT_SIZE_REFUSED: &str = "sandbox_size_refused";

impl LeaseRun<'_> {
    /// Takes the fleet's held sandbox, or builds one and checks the bound
    /// repositories out into it, then runs the turn in it and hands it back.
    /// A held sandbox keeps the repositories as its last lease left them. A
    /// sandbox that cannot be built, or a repository that will not check out,
    /// ends the lease at startup.
    pub(super) async fn sandboxed(
        &self,
        memory: Seed<'_>,
        bundle: Option<&Bundle>,
        sink: ActivitySink,
    ) -> Worked {
        let limits = match sized(self.lease.limits, self.lessee.limits) {
            Ok(limits) => limits,
            Err(failure) => {
                return self
                    .refuse(&failure, EVENT_SIZE_REFUSED, DETAIL_SIZE)
                    .into();
            }
        };
        let key = self.hold_key(limits);
        let held = match &key {
            Some(key) => self.revive(key).await,
            None => None,
        };
        let revived = held.is_some();
        mark_reused(revived);
        let sandbox = match held {
            Some(sandbox) => sandbox,
            None => match self.prepare(limits).await {
                Ok(sandbox) => sandbox,
                Err(refused) => return (*refused).into(),
            },
        };
        let checked_out = if revived {
            Ok(())
        } else {
            self.check_out(sandbox.as_ref()).await
        };
        let ending = match checked_out {
            Ok(()) => {
                self.in_sandbox(memory, bundle, sandbox.as_ref(), sink)
                    .await
            }
            Err(refused) => *refused,
        };
        Worked {
            ending,
            kept: Some(Kept { sandbox, key }),
        }
    }

    /// Builds a fresh sandbox enforcing `limits`, or the startup failure that
    /// ends the lease when the host cannot.
    async fn prepare(&self, limits: Limits) -> Result<Box<dyn Sandbox>, Box<Ending>> {
        let request = SandboxRequest {
            lease_id: self.ids.lease.as_str(),
            limits,
        };
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

    /// Lands the bundle's support files in the workspace, then runs the turn.
    /// A bundle that will not land is a startup failure before the model is
    /// invoked, as a missing one is; a lease that ends while it lands stops
    /// the landing there, so its worker and sandbox are freed at once.
    async fn in_sandbox(
        &self,
        memory: Seed<'_>,
        bundle: Option<&Bundle>,
        sandbox: &dyn Sandbox,
        sink: ActivitySink,
    ) -> Ending {
        let landed = tokio::select! {
            landed = materialize(sandbox.executor(), bundle) => landed,
            () = self.interrupt.cancelled() => {
                return failed(FailureClass::RenewalTerminate, DETAIL_RENEWAL);
            }
        };
        if let Err(failure) = landed {
            return self.refuse(&failure, EVENT_LANDING_FAILED, DETAIL_LANDING);
        }
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

/// Writes a bundle's support files into the lease's workspace, where the
/// fleet's instructions find them; a lease without a bundle has none.
async fn materialize(executor: &dyn Executor, bundle: Option<&Bundle>) -> afr_executor::Result<()> {
    for (path, content) in bundle.map(Bundle::support_files).unwrap_or_default() {
        executor.write_file(path, content.clone()).await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
