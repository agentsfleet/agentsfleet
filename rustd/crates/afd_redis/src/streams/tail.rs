//! The live tail: what a fleet's `fleet:{id}:activity` channel is told.
//!
//! Split from [`super`] on the seam between the durable stream — append, read
//! as a group, acknowledge — and the ephemeral channel beside it. A publish
//! here is fire-and-forget telemetry: no consumer group, no persistence, and a
//! frame nobody was subscribed for is simply gone.

use serde::Serialize;

use super::{FleetStreams, fleet_activity_channel};
use crate::error::Result;

const CMD_PUBLISH: &str = "PUBLISH";

/// A daemon-authored frame could not be published; the tail loses it and the
/// row it announces does not care.
const EVENT_FRAME_DROPPED: &str = "tail_frame_dropped";

impl FleetStreams {
    /// Publishes on a channel, for the subscription hub's readers.
    ///
    /// # Errors
    /// Returns a command error when the publish fails.
    pub async fn publish(&self, channel: &str, payload: &str) -> Result<i64> {
        let mut cmd = redis::cmd(CMD_PUBLISH);
        cmd.arg(channel).arg(payload);
        self.redis.command(CMD_PUBLISH, channel, &cmd).await
    }

    /// Publishes one payload on `fleet_id`'s live-tail channel.
    ///
    /// The channel is formatted here so every publisher — the runner's
    /// forwarded frames and the daemon's own brackets and gate frames — lands
    /// on the one name the tail subscribes to.
    ///
    /// # Errors
    /// Returns a command error when the publish fails.
    pub async fn publish_tail(&self, fleet_id: &str, payload: &str) -> Result<i64> {
        self.publish(&fleet_activity_channel(fleet_id), payload)
            .await
    }

    /// Publishes one of the daemon's own frames on `fleet_id`'s tail,
    /// best-effort.
    ///
    /// The one place a bracket or a gate frame is serialized, published and,
    /// when the queue would not take it, logged: a frame that does not land
    /// costs the tail a marker and the verb that wrote the row nothing, and
    /// that contract is stated here once rather than at every publisher. The
    /// row the frame announces is already durable, and a lost announcement is
    /// what the client's reconnect backfill recovers.
    pub async fn publish_frame<F: Serialize>(&self, fleet_id: &str, frame: &F) {
        // Strings and integers cannot fail to serialize; the arm is the
        // signature's, not a path a test could reach.
        let Ok(payload) = serde_json::to_string(frame) else {
            return;
        };
        if let Err(unreachable_queue) = self.publish_tail(fleet_id, &payload).await {
            let reason = unreachable_queue.to_string();
            tracing::debug!(
                fleet_id,
                reason,
                event = EVENT_FRAME_DROPPED,
                "the queue would not take a live-tail frame; the row it announces stands"
            );
        }
    }
}
