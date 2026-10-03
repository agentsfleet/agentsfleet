//! The lease's workspace before the turn: the bundle's support files land
//! there, where the fleet's instructions find them.

use afd_wire::memory::MemoryDelta;
use afd_wire::report::FailureClass;
use afr_executor::Executor;
use afr_sandbox::Sandbox;

use super::{DETAIL_RENEWAL, LeaseRun, failed};
use crate::activity::ActivitySink;
use crate::bundles::Bundle;
use crate::report::Ending;

/// What a lease whose support files would not land reports.
const DETAIL_LANDING: &str =
    "the fleet bundle's support files could not be written to the workspace";
const EVENT_LANDING_FAILED: &str = "bundle_landing_failed";

impl LeaseRun<'_> {
    /// Lands the bundle's support files in the workspace, then runs the turn.
    /// A bundle that will not land is a startup failure before the model is
    /// invoked, as a missing one is; a lease that ends while it lands stops
    /// the landing there, so its worker and sandbox are freed at once.
    pub(super) async fn in_sandbox(
        &self,
        memory: &[MemoryDelta<'_>],
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
        self.drive(memory, sandbox, sink).await
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
