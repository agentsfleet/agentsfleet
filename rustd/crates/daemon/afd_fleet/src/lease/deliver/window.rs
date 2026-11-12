//! The fleet's memory window, read for the lease just issued.
//!
//! After the issue, so the lease row holds the fleet's slot: no other lease
//! for the fleet can settle between this read and the runner's first model
//! call, and the last report's memory, written before that report committed,
//! is in the window. A read that fails leaves the lease issuable without one;
//! the runner then reads memory itself, as it did before the window rode the
//! lease.

use af_core::error_code::{Coded as _, Logged};
use af_wire::memory::MemoryHydrateResponse;
use afd_api_wire::fleet::MemoryAccess;

use crate::lease::envelope::Acquired;
use crate::lease::pull::LeaseService;

/// The window could not be read; the lease issues without one.
pub(super) const EVENT_WINDOW_READ_FAILED: &str = "lease_window_read_failed";

impl LeaseService {
    /// The memory window the lease carries, or none when the read failed.
    ///
    /// Scoped by the workspace and grants the claim statement read, so a grant
    /// change reaches the next lease without a read here.
    pub(super) async fn window_for(
        &self,
        acquired: &Acquired,
    ) -> Option<MemoryHydrateResponse<'static>> {
        let fleet_id = &acquired.fleet_id;
        let grants = MemoryAccess {
            read: acquired.opened.memory_reads_workspace,
            publish: acquired.opened.memory_publishes_workspace,
        };
        let window = self
            .memories
            .hydrate_scoped(fleet_id, &acquired.workspace_id, grants)
            .await;
        match window {
            Ok(window) => Some(window),
            Err(failure) => {
                let Logged { error_code, reason } = failure.logged();
                let fleet_id = fleet_id.as_str();
                let event = EVENT_WINDOW_READ_FAILED;
                tracing::warn!(
                    error_code,
                    fleet_id,
                    reason,
                    event,
                    "the lease issues without its memory window; the runner reads memory itself"
                );
                None
            }
        }
    }
}
