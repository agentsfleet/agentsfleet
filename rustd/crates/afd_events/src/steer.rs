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
//!
//! # The key is scoped to the fleet, and a repeat is answered, never re-run
//!
//! `UNIQUE (producer, producer_key)` is global, so the key is
//! `<fleet_id>:<operation_id>` — the composition the webhook producer already
//! uses — and one client's id can never answer with another fleet's event.
//!
//! The happy path costs no extra read: the ledger's insert conflicts on a key
//! already held and answers the first row. Only two edges look the key up.
//! When the insert met an existing row, the row is read back so its payload
//! can be checked — that is also what closes the race where two sends with
//! one id both reached the insert. And when a spent budget refuses the insert,
//! a repeat is still answered: a message already admitted is not new work.
//!
//! The same id with a different payload — another message, or another sender
//! — is refused rather than answered: the first message's event would tell the
//! sender the second one landed.

use afd_admission::{Admission, Admissions, Key, Producer, Reply};
use afd_core::error_code;
use afd_wire::event::EventType;

use crate::error::{Result, operation_conflict};

/// Joins the fleet to the caller's operation id in the ledger key.
const KEY_SEPARATOR: &str = ":";

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
    /// Refuses an operation id already admitted with a different message, and
    /// reports a database that would not record the acceptance. A queue that
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
        let key = operation_id.map(|operation| scoped_key(fleet, operation));
        let admission = steer_admission(fleet, workspace, actor, request_json, key.as_deref());
        let admitted = match self.admissions.admit(admission).await {
            Ok(admitted) => admitted,
            Err(refused) if refused.is_over_capacity() => {
                return self.repeat_despite(&admission, refused).await;
            }
            Err(failed) => return Err(failed.into()),
        };
        if admitted.replayed {
            // The insert met a row already under this key: a retry, or a send
            // with the same id that won the race to the insert. Its event is
            // this caller's answer only if the payload is this caller's too.
            return Ok(self.repeat_of(&admission).await?.unwrap_or(admitted.id));
        }

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

    /// The event a caller's operation already became on `fleet`, or `None`
    /// when this operation id is new.
    ///
    /// The handler asks this when the fleet will not take work: a retry of a
    /// message admitted before the fleet stopped is answered with that
    /// message's event, not refused as if it never landed.
    ///
    /// # Errors
    /// Refuses an id already admitted with a different message, and reports a
    /// ledger that would not answer.
    pub async fn replayed(
        &self,
        fleet: &str,
        workspace: &str,
        actor: &str,
        request_json: &str,
        operation_id: &str,
    ) -> Result<Option<String>> {
        let key = scoped_key(fleet, operation_id);
        self.repeat_of(&steer_admission(
            fleet,
            workspace,
            actor,
            request_json,
            Some(&key),
        ))
        .await
    }

    /// A capacity refusal, unless the key is a repeat: then its event.
    async fn repeat_despite(
        &self,
        admission: &Admission<'_>,
        refused: afd_admission::Error,
    ) -> Result<String> {
        match self.repeat_of(admission).await {
            Ok(Some(id)) => Ok(id),
            Err(conflict) if conflict.is_operation_conflict() => Err(conflict),
            // New work, an unkeyed send, or a lookup that failed too: the
            // refusal the caller retries against stands.
            _new_work => Err(refused.into()),
        }
    }

    /// The first admission's event for a keyed steer with the same payload.
    ///
    /// The digest covers actor, workspace and body (`Admission::payload_digest`),
    /// so a different person reusing the id is a conflict too, never a read of
    /// somebody else's event; and a row some other fleet holds under this key
    /// is refused the same way rather than answered.
    async fn repeat_of(&self, admission: &Admission<'_>) -> Result<Option<String>> {
        let Key::Repeated(key) = admission.key else {
            return Ok(None);
        };
        let Some(repeated) = self
            .admissions
            .find_repeated(admission.producer, key)
            .await?
        else {
            return Ok(None);
        };
        if repeated.fleet == admission.fleet && repeated.digest == admission.payload_digest() {
            return Ok(Some(repeated.id));
        }
        // A client that reused its own id is a defect somebody should see; the
        // refusal itself logs at debug like every other caller fault.
        let code = error_code::AGENTSFLEET_OPERATION_CONFLICT.as_str();
        let fleet_id = admission.fleet;
        let event_id = repeated.id.as_str();
        tracing::warn!(
            error_code = code,
            fleet_id,
            event_id,
            event = "steer_operation_conflict",
        );
        Err(operation_conflict())
    }
}

/// The ledger key a caller's operation id becomes on one fleet.
fn scoped_key(fleet: &str, operation: &str) -> String {
    [fleet, operation].join(KEY_SEPARATOR)
}

/// The admission a steer asks for: the same fields on the first send and on
/// every repeat, which is what makes the digest comparison mean anything.
fn steer_admission<'a>(
    fleet: &'a str,
    workspace: &'a str,
    actor: &'a str,
    request_json: &'a str,
    key: Option<&'a str>,
) -> Admission<'a> {
    Admission {
        producer: Producer::Steer,
        // Mapped explicitly, never by `unwrap_or`-ing into a default: `Key`'s
        // own note warns that an `Option` lets a caller which HAS an identity
        // lose deduplication by omission, and this is the call site that would
        // do it.
        key: match key {
            Some(repeated) => Key::Repeated(repeated),
            None => Key::Unrepeatable,
        },
        fleet,
        workspace,
        actor,
        event_type: EventType::Chat,
        request_json,
        // A steer is read on the event tail that carried it, never posted to a thread.
        reply: Reply::None,
    }
}
