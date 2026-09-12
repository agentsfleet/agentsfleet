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

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]

    use std::collections::HashMap;
    use std::time::Duration;

    use super::super::FleetStreams;
    use crate::Redis;
    use crate::config::{RedisConfig, RedisRole};

    /// A loopback port nobody listens on.
    const NOWHERE: &str = "redis://127.0.0.1:1";

    /// A frame that will not serialize is dropped before the queue is asked.
    ///
    /// `serde_json` refuses a map keyed by anything but a string; the daemon's
    /// own frames are structs of strings and integers and never reach this
    /// arm, which is why it is proven with a shape they cannot take. The verb
    /// answers within the budget over a queue that is not there, because it
    /// never got as far as asking.
    #[tokio::test]
    async fn should_drop_a_frame_that_will_not_serialize_before_asking_the_queue() {
        let queue = Redis::unreachable(&RedisConfig::from_url(
            RedisRole::Default,
            NOWHERE.to_owned(),
        ))
        .expect("a lazy handle opens no socket");
        let streams = FleetStreams::new(queue);
        let unserializable: HashMap<(u8, u8), u8> = HashMap::from([((1, 2), 3)]);
        tokio::time::timeout(
            Duration::from_secs(5),
            streams.publish_frame("fleet-1", &unserializable),
        )
        .await
        .expect("the frame is dropped at serialization, not after a connect attempt");
    }
}
