//! An operator's message to a fleet, on the way in.
//!
//! One verb: normalize what a person typed into an event envelope and admit
//! it.
//!
//! # A steer is admitted like every other producer
//!
//! It is not a row this daemon inserts and then hopes a runner notices. It
//! goes through the admission ledger every other producer — webhook, cron,
//! continuation — already goes through, and the `core.fleet_events` row
//! appears when the runner leases it. That is what makes a steer
//! indistinguishable from every other way a run starts, and it is why there
//! is no synthetic-event injection anywhere behind this.
//!
//! # A steer's key is the CALLER's, when the caller has one
//!
//! Every other producer repeats a value across its retries: a delivery id, a
//! scheduler message id, a gate action. A steer has no such value of its own,
//! and for a long time that meant the key was this call's own row identifier —
//! correct for a person pressing send twice, wrong for an API client that
//! never saw its response.
//!
//! Those two are indistinguishable from here. The bytes are identical, so only
//! the caller knows which one it is making, and Dimension 7.5 is the field that
//! lets it say: `operation_id`, repeated across a retry. Present, it is the
//! ledger's `producer_key` and the retry conflicts on
//! `UNIQUE (producer, producer_key)` — answered with the first admission's
//! event, one run, one charge. Absent, the ledger mints one and two identical
//! messages stay two operations.
//!
//! The absent case is a real answer and not a default nobody thought about: a
//! timeout does not prove an operation failed, but neither does it prove one
//! happened, and a human typing in a terminal has no operation to identify.

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_wire::event::EventType;

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
    admissions: Admissions,
}

impl Steer {
    /// Admits through `admissions`.
    #[must_use]
    pub const fn new(admissions: Admissions) -> Self {
        Self { admissions }
    }

    /// Puts one message on the fleet's stream, answering with its event id.
    ///
    /// `request_json` is the already-serialized payload; this layer does not
    /// build it, because the shape a producer sends is the producer's
    /// contract and not the ledger's.
    ///
    /// # Errors
    /// Reports a database that would not record the acceptance. A queue that
    /// would not take the append is NOT an error — the message is already
    /// durable and the replay sweeper delivers it, which is exactly the
    /// failure the ledger exists to absorb.
    pub async fn append(
        &self,
        fleet: &str,
        workspace: &str,
        actor: &str,
        request_json: &str,
        operation_id: Option<&str>,
    ) -> Result<String> {
        let admitted = self
            .admissions
            .admit(Admission {
                producer: Producer::Steer,
                // Mapped explicitly, never by `unwrap_or`-ing into a default:
                // `Key`'s own note warns that an `Option` lets a caller which
                // HAS an identity lose deduplication by omission, and this is
                // the call site that would do it.
                key: match operation_id {
                    Some(operation) => Key::Repeated(operation),
                    None => Key::Unrepeatable,
                },
                fleet,
                workspace,
                actor,
                event_type: EventType::Chat,
                request_json,
                // A steer is read on the event tail that carried it, never posted to a thread.
                reply: Reply::None,
            })
            .await?;

        // Hoisted rather than spelled inside the macro: the log bridge
        // duplicates every field expression, and coverage instrumentation
        // scores the dead copy (`docs/LOGGING_STANDARD.md` §8A).
        let id = admitted.id.as_str();
        tracing::debug!(
            fleet_id = fleet,
            workspace_id = workspace,
            actor,
            event_id = id,
            event = "steer_appended",
        );
        Ok(admitted.id)
    }
}
