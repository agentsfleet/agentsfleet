//! Putting a verified delivery on the fleet's stream, at most once.
//!
//! # A Postgres row, and NOT a Dragonfly claim
//!
//! The idempotency boundary for an inbound delivery is the admission ledger's
//! `(producer, producer_key)` unique index. It used to be a Lua script's
//! claim key, which had two problems the index does not: it expired, so a
//! sender retrying past the window ran the fleet twice; and it lived in the
//! queue, so losing the queue lost both the claim and the delivery it was
//! protecting.
//!
//! Nothing is written to `core.fleet_events` here, for the reason
//! `afd_events::steer` states about its own path: the row appears when the
//! runner leases the event, and a daemon that inserted one at ingress would
//! be racing its own runner to describe the same event.
//!
//! # Why the key is `{fleet}:{provider event id}`
//!
//! Per fleet, because one App delivery fans out to every subscribed fleet and
//! each of them must run: a key on the provider's id alone would let the
//! first fleet's admission silence all the others. Per provider event id,
//! because that is the value a sender REPEATS when it retries — a random id
//! minted here would make every retry a new event, which is the duplicate run
//! the key exists to prevent.
//!
//! # The two surfaces are two producers, not two windows
//!
//! [`Surface`] used to choose a claim's expiry: a day for the per-fleet
//! routes and three for the App ingress, because an operator may press
//! Redeliver in a provider's own delivery log for three days. A ledger row
//! does not expire, so there is no window to choose and no Redeliver that can
//! outlive one. What the surface still decides is WHICH producer the row
//! records, which keeps a per-fleet delivery and an App fan-out to the same
//! fleet from deduplicating against each other.

use afd_admission::{Admission, Admitted, Key, Producer};
use afd_wire::event::EventType;

use crate::Ingress;
use crate::binding::Binding;
use crate::error::Result;

/// Which ingress surface took a delivery.
///
/// Carried as an argument rather than a field of [`Delivery`] because it is
/// not part of what the stream records — it decides only which producer the
/// ledger row is attributed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// The per-fleet routes, whose URL named the fleet.
    Fleet,
    /// The App ingress, which fanned one delivery out to its subscribers.
    App,
}

impl Surface {
    /// The producer a delivery on this surface is recorded as.
    const fn producer(self) -> Producer {
        match self {
            Self::Fleet => Producer::Webhook,
            Self::App => Producer::WebhookApp,
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
    /// GitHub's `x-github-delivery`, Svix's `svix-id`, Slack's `event_id`.
    /// Never minted here — see the module note.
    pub event_id: &'d str,
    /// Who the history records as having woken the fleet.
    pub actor: &'d str,
    /// The digest a fleet's authored prose reasons over.
    pub request_json: &'d str,
}

impl Ingress {
    /// Admits one verified delivery, at most once however often it arrives.
    ///
    /// Answers what the admission did: the event's logical id, and whether an
    /// earlier call already admitted it. A caller answers 2xx either way — a
    /// provider redelivering a delivery this daemon already holds has nothing
    /// to fix, and a non-2xx would only earn another retry.
    ///
    /// # Errors
    /// Reports a database that would not record the acceptance. A queue that
    /// would not take the entry is NOT an error: the delivery is durable and
    /// the replay sweeper delivers it, which is the whole reason acceptance
    /// moved to Postgres.
    pub async fn deliver(
        &self,
        surface: Surface,
        binding: &Binding,
        delivery: &Delivery<'_>,
    ) -> Result<Admitted> {
        let fleet = binding.fleet().as_str();
        let workspace = binding.workspace().as_str();
        let key = format!("{fleet}:{}", delivery.event_id);
        let admitted = self
            .admissions
            .admit(Admission {
                producer: surface.producer(),
                key: Key::Repeated(&key),
                fleet,
                workspace,
                actor: delivery.actor,
                event_type: EventType::Webhook,
                request_json: delivery.request_json,
            })
            .await?;

        // Hoisted rather than spelled inside the macro: the log bridge
        // duplicates every field expression and coverage instrumentation
        // scores the dead copy (`docs/LOGGING_STANDARD.md` §8A).
        let event_id = admitted.id.as_str();
        let replayed = admitted.replayed;
        let source = binding.source();
        tracing::info!(
            fleet_id = fleet,
            workspace_id = workspace,
            source,
            event_id,
            replayed,
            event = "webhook_delivery_appended",
        );
        Ok(admitted)
    }
}
