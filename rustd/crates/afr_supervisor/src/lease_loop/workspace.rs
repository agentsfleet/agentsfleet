//! The lease's sandbox and its workspace: the sandbox is built, the bundle's
//! support files land where the fleet's instructions find them, the turn runs,
//! and the sandbox is destroyed exactly once.

use std::time::Instant;

use afd_wire::report::FailureClass;
use afr_executor::Executor;
use afr_memory::Seed;
use afr_sandbox::{Sandbox, SandboxRequest};
use afr_telemetry::labels::SandboxStart;
use afr_telemetry::record;

use super::{DETAIL_RENEWAL, LeaseRun, failed};
use crate::activity::ActivitySink;
use crate::bundles::Bundle;
use crate::report::Ending;

/// What a lease whose support files would not land reports.
const DETAIL_LANDING: &str =
    "the fleet bundle's support files could not be written to the workspace";
const DETAIL_SANDBOX: &str = "this host could not build a sandbox for the run";
const EVENT_LANDING_FAILED: &str = "bundle_landing_failed";
const EVENT_SANDBOX_REFUSED: &str = "sandbox_refused";
const EVENT_DESTROY_FAILED: &str = "sandbox_destroy_failed";

impl LeaseRun<'_> {
    /// Builds the lease's sandbox, runs the turn in it, and destroys it. A
    /// sandbox that cannot be built ends the lease at startup.
    pub(super) async fn sandboxed(
        &self,
        memory: Seed<'_>,
        bundle: Option<&Bundle>,
        sink: ActivitySink,
    ) -> Ending {
        let lessee = self.lessee;
        let request = SandboxRequest {
            lease_id: self.ids.lease.as_str(),
            limits: lessee.limits,
        };
        let started = Instant::now();
        let prepared = lessee.engine.prepare(request).await;
        let outcome = if prepared.is_ok() {
            SandboxStart::Ready
        } else {
            SandboxStart::Failed
        };
        record::sandbox_start(outcome, started.elapsed());
        let sandbox = match prepared {
            Ok(sandbox) => sandbox,
            Err(failure) => return self.refuse(&failure, EVENT_SANDBOX_REFUSED, DETAIL_SANDBOX),
        };
        let ending = self
            .in_sandbox(memory, bundle, sandbox.as_ref(), sink)
            .await;
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
        ending
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
