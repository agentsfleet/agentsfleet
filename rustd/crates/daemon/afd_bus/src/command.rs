use af_core::id::Uuid7;
use serde::{Deserialize, Serialize};

/// Notifications sent after the durable authority or cancellation transaction commits.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// The next renewal also observes the durable cancellation marker.
    Cancel {
        /// Lease whose task must stop.
        lease_id: Uuid7,
    },
    /// Reload the runner's status, connection owner and credential digest.
    Recheck,
}
