//! An operator's message to a fleet, on the way in.
//!
//! The port of `fleets/messages.zig`. One verb: normalize what a person typed
//! into an event envelope and admit it.
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
//! # A steer's key is minted, because a steer has no retry identity
//!
//! Every other producer repeats a value across its retries: a delivery id, a
//! scheduler message id, a gate action. A person pressing send twice means
//! two messages, so there is nothing to deduplicate against and the key is
//! this call's own row identifier. The ledger still records the acceptance,
//! which is the half that matters — the message survives queue loss.

use afd_admission::{Admission, Admissions, Key, Producer};
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
    ) -> Result<String> {
        let admitted = self
            .admissions
            .admit(Admission {
                producer: Producer::Steer,
                // See the module note: a steer has no value that survives a
                // retry, so the ledger keys it on its own row.
                key: Key::Unrepeatable,
                fleet,
                workspace,
                actor,
                event_type: EventType::Chat,
                request_json,
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
