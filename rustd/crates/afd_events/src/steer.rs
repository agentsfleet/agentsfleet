//! An operator's message to a fleet, on the way in.
//!
//! The port of `fleets/messages.zig`. One verb: normalize what a person typed
//! into an event envelope and `XADD` it onto `fleet:{id}:events`.
//!
//! # Nothing is written to Postgres here
//!
//! A steer is not a row this daemon inserts and then hopes a runner notices.
//! It is an append to the SINGLE ingress stream every other producer — webhook,
//! cron, continuation — already writes to, and the row appears when the runner
//! leases it. That is what makes a steer indistinguishable from every other
//! way a run starts, and it is why there is no synthetic-event injection
//! anywhere behind this.
//!
//! # The readiness mark is separate, and its failure is not the caller's
//!
//! `XADD` makes the message durable; the mark is what makes it PROMPTLY
//! leasable rather than waiting for the next poll. So the order is append,
//! then mark — and a mark that fails is logged rather than raised, because by
//! then the message is already in the stream and answering 500 would invite a
//! retry that appends it twice.

use afd_core::error_code;
use afd_redis::{FleetStreams, ReadyIndex, Redis};
use afd_wire::event::{EventType, field};

use crate::error::Result;

/// The prefix every operator-driven message carries in its actor.
///
/// Matched as `steer:%` by the onboarding read and grouped on by the
/// dashboard, so this spelling and that pattern must not drift.
pub const ACTOR_PREFIX: &str = "steer:";

/// The actor a machine-driven steer records.
///
/// Every non-human credential collapses to this one category. An `agt_t`
/// api-key carries its creator in `subject`, so attributing the wake to that
/// person would name an uninvolved human — worse than naming nobody, because
/// an actor-shaped assertion would then certify "a person woke this fleet"
/// while automation did.
pub const ACTOR_MACHINE: &str = "steer:api";

/// The ingress side of the narrative log.
#[derive(Debug, Clone)]
pub struct Steer {
    queue: Redis,
}

impl Steer {
    /// Appends through `queue`.
    #[must_use]
    pub const fn new(queue: Redis) -> Self {
        Self { queue }
    }

    /// Puts one message on the fleet's stream, answering with its event id.
    ///
    /// `request_json` is the already-serialized payload; this layer does not
    /// build it, because the shape a producer sends is the producer's contract
    /// and not the queue's.
    ///
    /// # Errors
    /// Reports a queue that would not take the append. A message Postgres
    /// would have accepted and the queue refused is one a person sent that no
    /// runner will see, which is why it is raised rather than logged.
    pub async fn append(
        &self,
        fleet: &str,
        workspace: &str,
        actor: &str,
        request_json: &str,
    ) -> Result<String> {
        let appended = FleetStreams::new(self.queue.clone())
            .append(
                fleet,
                &[
                    (field::ACTOR, actor),
                    (field::EVENT_TYPE, EventType::Chat.as_str()),
                    (field::WORKSPACE_ID, workspace),
                    (field::REQUEST_JSON, request_json),
                ],
            )
            .await?;

        let event_id = appended.as_str().to_owned();
        // Hoisted rather than spelled inside the macro: the log bridge
        // duplicates every field expression, and coverage instrumentation
        // scores the dead copy (`docs/LOGGING_STANDARD.md` §8A).
        let id = event_id.as_str();
        tracing::debug!(
            fleet_id = fleet,
            workspace_id = workspace,
            actor,
            event_id = id,
            event = "steer_appended",
        );

        // The token is the fleet id, as every producer in this workspace spells
        // it: the clear compares it, so a mark written under another value is
        // one nothing can remove.
        if let Err(unmarked) = ReadyIndex::new(self.queue.clone()).mark(fleet, fleet).await {
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            let reason = unmarked.to_string();
            tracing::warn!(
                error_code = code,
                fleet_id = fleet,
                event_id = id,
                reason,
                event = "steer_ready_mark_failed",
            );
        }
        Ok(event_id)
    }
}
