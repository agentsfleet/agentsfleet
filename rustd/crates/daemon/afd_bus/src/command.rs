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

impl Command {
    /// Publish a committed runner mutation without letting a silent broker hold its caller.
    ///
    /// # Errors
    /// Retains encoding, broker and notification-deadline failures for the caller's policy.
    pub async fn publish(&self, bus: &crate::Handle, runner: &Uuid7) -> crate::Result<()> {
        let subject = crate::Subject::RunnerCommand(runner.clone());
        tokio::time::timeout(
            af_core::timing::READY_WAKE_DEADLINE,
            bus.publish_frame(&subject, self),
        )
        .await?
    }
}
