//! The fleet's memory window, read for the lease just issued.
//!
//! After the issue, so the lease row holds the fleet's slot: no other lease
//! for the fleet can settle between this read and the runner's first model
//! call, and the last report's memory, written before that report committed,
//! is in the window. A read that fails leaves the lease issuable without one;
//! the runner then reads memory itself, as it did before the window rode the
//! lease.

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use af_wire::memory::MemoryHydrateResponse;

use crate::lease::pull::LeaseService;

/// The window could not be read; the lease issues without one.
pub(super) const EVENT_WINDOW_READ_FAILED: &str = "lease_window_read_failed";

impl LeaseService {
    /// The memory window the lease carries, or none when the read failed.
    pub(super) async fn window_for(
        &self,
        fleet_id: &Uuid7,
    ) -> Option<MemoryHydrateResponse<'static>> {
        match self.memories.hydrate(fleet_id).await {
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
