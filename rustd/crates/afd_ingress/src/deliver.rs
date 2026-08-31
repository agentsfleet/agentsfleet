//! Putting a verified delivery on the fleet's stream, at most once.
//!
//! # Redis, and NOT an `INSERT … ON CONFLICT`
//!
//! The idempotency boundary for an inbound delivery is the `append_once` Lua
//! script's claim key, not a Postgres row. Nothing is written to Postgres here,
//! for the reason `afd_events::steer` states about its own path: the row
//! appears when the runner leases the event, and a daemon that inserted one at
//! ingress would be racing its own runner to describe the same event.
//!
//! `INSERT_FLEET_EVENT` has exactly two callers — `afd_fleet::lease::event` and
//! `afd_approval::inbox` — the lease and the continuation. Ingress is neither.
//!
//! # Why the claim is `{fleet}:{provider event id}`
//!
//! Per fleet, because one App delivery fans out to every subscribed fleet and
//! each of them must run: a claim keyed on the provider's id alone would let
//! the first fleet's append silence all the others. Per provider event id,
//! because that is the value a sender REPEATS when it retries — a random id
//! minted here would make every retry a new event, which is the duplicate run
//! the claim exists to prevent.
//!
//! [`afd_redis::streams::OnceScope`] owns both the key prefix and the retention
//! window. This module composes the id and names neither, which is the split
//! that module's own header asks for: *which field of which envelope is the
//! sender's idempotency key is the envelope's contract*.
//!
//! There are two windows, chosen by [`Surface`]: a day for the per-fleet routes
//! and three for the App ingress, because an operator may press Redeliver in a
//! provider's own delivery log for three days after the event.

use afd_redis::FleetStreams;
use afd_redis::streams::{Appended, OnceScope};
use afd_wire::event::{EventType, field};

use crate::Ingress;
use crate::binding::Binding;
use crate::error::Result;

/// Which ingress surface took a delivery.
///
/// Carried as an argument rather than a field of [`Delivery`] because it is not
/// part of what the stream records — it decides only how long the at-most-once
/// claim outlives the delivery, and the two surfaces answer that differently.
/// See [`afd_redis::streams::OnceScope`] for the two windows and why they
/// differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The per-fleet routes, whose URL named the fleet.
    Fleet,
    /// The App ingress, which fanned one delivery out to its subscribers.
    App,
}

impl Surface {
    /// The claim this surface's deliveries are remembered under.
    const fn scope(self) -> OnceScope {
        match self {
            Self::Fleet => OnceScope::WebhookDelivery,
            Self::App => OnceScope::AppDelivery,
        }
    }
}

/// One verified delivery, reduced to what the stream carries.
///
/// Borrowed rather than owned: every field is a slice of something the handler
/// already holds, and the append reads them once. A struct rather than four
/// positional arguments because two of them are `&str` that a call site could
/// swap without the compiler noticing.
#[derive(Debug, Clone, Copy)]
pub struct Delivery<'d> {
    /// The sender's own identifier for this delivery, repeated across retries.
    ///
    /// GitHub's `x-github-delivery`, Svix's `svix-id`, Slack's `event_id`. Never
    /// minted here — see the module note.
    pub event_id: &'d str,
    /// Who the history records as having woken the fleet.
    pub actor: &'d str,
    /// The digest a fleet's authored prose reasons over.
    pub request_json: &'d str,
}

impl Ingress {
    /// Appends one verified delivery, at most once however often it arrives.
    ///
    /// Answers what the append did: the event's id, and whether an earlier
    /// call already wrote it. A caller answers 2xx either way — a provider
    /// redelivering a delivery this daemon already ran has nothing to fix, and
    /// a non-2xx would only earn another retry.
    ///
    /// The readiness mark is [`afd_redis::ReadyIndex`]'s and rides on the
    /// steer path rather than here: a delivery that has been claimed is already
    /// durable, and a mark that failed would be a 500 inviting the retry that
    /// the claim would then suppress.
    ///
    /// # Errors
    /// Reports a queue that would not take the append. A verified delivery this
    /// daemon accepted and could not enqueue is one no runner will ever see,
    /// which is why it is raised rather than logged.
    pub async fn deliver(
        &self,
        surface: Surface,
        binding: &Binding,
        delivery: &Delivery<'_>,
    ) -> Result<Appended> {
        let fleet = binding.fleet().as_str();
        let workspace = binding.workspace().as_str();
        let once_id = format!("{fleet}:{}", delivery.event_id);
        let kind = EventType::Webhook.as_str();

        let appended = FleetStreams::new(self.queue.clone())
            .append_once(
                surface.scope(),
                &once_id,
                fleet,
                &[
                    (field::ACTOR, delivery.actor),
                    (field::EVENT_TYPE, kind),
                    (field::WORKSPACE_ID, workspace),
                    (field::REQUEST_JSON, delivery.request_json),
                ],
            )
            .await?;

        // Hoisted rather than spelled inside the macro: the log bridge
        // duplicates every field expression and coverage instrumentation scores
        // the dead copy (`docs/LOGGING_STANDARD.md` §8A).
        let event_id = appended.id.as_str();
        let replayed = appended.replayed;
        let source = binding.source();
        tracing::info!(
            fleet_id = fleet,
            workspace_id = workspace,
            source,
            event_id,
            replayed,
            event = "webhook_delivery_appended",
        );
        Ok(appended)
    }
}
