//! The fleet's memory window, read for the run just issued.
//!
//! After the issue, so the run row holds the fleet's thread: no other run
//! for the fleet can settle between this read and the runner's first model
//! call, and the last report's memory, written before that report committed,
//! is in the window. A read that fails leaves the run issuable without one;
//! the runner then reads memory itself, as it did before the window rode the
//! run.

use af_core::error_code::{Coded as _, Logged};
use af_wire::memory::MemoryLoadResponse;
use afd_api_wire::fleet::MemoryAccess;

use crate::run::envelope::Acquired;
use crate::run::pull::RunService;

/// The window could not be read; the run issues without one.
pub(super) const EVENT_WINDOW_READ_FAILED: &str = "run_window_read_failed";

impl RunService {
    /// The memory window the run carries, or none when the read failed.
    ///
    /// Scoped by the workspace and grants the assignment statement read, so a grant
    /// change reaches the next run without a read here.
    pub(super) async fn window_for(
        &self,
        acquired: &Acquired,
    ) -> Option<MemoryLoadResponse<'static>> {
        let fleet_id = &acquired.fleet_id;
        let grants = MemoryAccess {
            read: acquired.opened.memory_reads_workspace,
            publish: acquired.opened.memory_publishes_workspace,
        };
        let window = self
            .memories
            .load_scoped(fleet_id, &acquired.workspace_id, grants)
            .await;
        match window {
            Ok(window) => Some(window),
            Err(failure) => {
                let Logged { error_code, reason } = failure.logged();
                let fleet_id = fleet_id.as_str();
                let event = EVENT_WINDOW_READ_FAILED;
                {
                    let (message,) = (format_args!(
                        "the run issues without its memory window; the runner reads memory itself"
                    ),);
                    tracing::warn!(message, error_code, fleet_id, reason, event);
                };
                None
            }
        }
    }
}
