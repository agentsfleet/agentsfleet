//! The lease's workspace before the turn: the bundle's support files land
//! there, where the fleet's instructions find them.

use afd_wire::lease::LeasePayload;
use afd_wire::memory::MemoryDelta;
use afd_wire::report::FailureClass;
use afr_executor::Executor;
use afr_sandbox::Sandbox;
use tokio_util::sync::CancellationToken;

use super::{Ids, Lessee, failed};
use crate::activity::ActivitySink;
use crate::bundles::Bundle;
use crate::report::Ending;

/// What a lease whose support files would not land reports.
const DETAIL_LANDING: &str =
    "the fleet bundle's support files could not be written to the workspace";
const EVENT_LANDING_FAILED: &str = "bundle_landing_failed";

impl Lessee {
    /// Lands the bundle's support files in the workspace, then runs the turn.
    /// A bundle that will not land is a startup failure before the model is
    /// invoked, as a missing one is.
    #[expect(
        clippy::too_many_arguments,
        reason = "the lease, its identities, memory, bundle, sandbox, sink and interrupt are each the run's own"
    )]
    pub(super) async fn in_sandbox(
        &self,
        lease: &LeasePayload<'_>,
        ids: &Ids,
        memory: &[MemoryDelta<'_>],
        bundle: Option<&Bundle>,
        sandbox: &dyn Sandbox,
        sink: ActivitySink,
        interrupt: &CancellationToken,
    ) -> Ending {
        if let Err(failure) = materialize(sandbox.executor(), bundle).await {
            let code = failure.code().as_str();
            let lease_id = ids.lease.as_str();
            let event = EVENT_LANDING_FAILED;
            let detail = DETAIL_LANDING;
            tracing::warn!(error_code = code, lease_id, event, detail);
            return failed(FailureClass::StartupPosture, DETAIL_LANDING);
        }
        self.drive(lease, ids, memory, sandbox, sink, interrupt)
            .await
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
