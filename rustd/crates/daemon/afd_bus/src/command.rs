use af_core::id::Uuid7;
use serde::{Deserialize, Serialize};

/// Notifications sent after the durable authority or cancellation transaction commits.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// The next renewal also observes the durable cancellation marker.
    Cancel {
        /// Run whose task must stop.
        run_id: Uuid7,
    },
    /// Reload the runner's status, connection owner and credential digest.
    Recheck,
}

impl Command {
    /// Publish a committed runner mutation. The bus bounds the publish at its
    /// configured `publish_deadline_ms` and fails at once while the broker is down, so a
    /// silent broker never holds the caller.
    ///
    /// # Errors
    /// Retains encoding, broker and publish-deadline failures for the caller's policy.
    pub async fn publish(&self, bus: &crate::Handle, runner: &Uuid7) -> crate::Result<()> {
        let subject = crate::Subject::RunnerCommand(runner.clone());
        bus.publish_frame(&subject, self).await
    }
}
