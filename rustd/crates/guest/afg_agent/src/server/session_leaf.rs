//! What a save asks of the tenant leaf: whether anything still runs, and the
//! pin pause.
//!
//! ```text
//!   fs/recovery_idle  calls    no call runs; a tenant process may
//!                     file_jobs  no call runs and the leaf is empty
//!   env/freeze        every process in the leaf stopped where it stands
//!   env/thaw          every one running on
//! ```
//!
//! The guest agent sits outside the leaf, so it answers every call while the
//! leaf is frozen: the export a pin reads, a listing, a file read.

use std::sync::Arc;

use jsonrpsee_types::Id;

use super::Session;
use crate::server::launch::Placement;
use af_guest_wire::error::Result;
use af_guest_wire::protocol::{Leaf, LeafParams, RecoveryParams, RecoveryProof};

impl Session {
    /// Answers whether the work `params` names is idle. No call of this
    /// session may run for either proof; only the file-jobs proof also
    /// counts a process a call started and left behind, which the pin pause
    /// freezes rather than waits out.
    pub(super) fn recovery_idle(&mut self, id: Id<'static>, params: Result<RecoveryParams>) {
        let idle = params.and_then(|params| {
            while let Some(served) = self.calls.try_join_next() {
                self.forget(served.ok().flatten());
            }
            let leaf_idle = match params.proof {
                RecoveryProof::FileJobs => self.placement.idle()?,
                RecoveryProof::Calls => true,
            };
            Ok(self.calls.is_empty() && leaf_idle)
        });
        self.answer(id, idle);
    }

    /// Freezes or thaws the leaf with `call`, off the runtime's threads: it
    /// blocks until the kernel reports the leaf settled.
    pub(super) fn on_leaf(
        &mut self,
        id: Id<'static>,
        params: Result<LeafParams>,
        call: impl FnOnce(&dyn Placement) -> Result<()> + Send + 'static,
    ) {
        match params {
            Ok(LeafParams { leaf: Leaf::Tenant }) => {
                let placement = Arc::clone(&self.placement);
                self.blocking(id, move |_workspace| (call(placement.as_ref()), None));
            }
            Err(failure) => self.answer::<()>(id, Err(failure)),
        }
    }
}

#[cfg(test)]
#[path = "leaf_tests.rs"]
mod tests;
