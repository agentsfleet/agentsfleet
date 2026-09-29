//! What a producer asks the ledger for: who is asking, what the work is keyed
//! on, where its answer goes, and the digest a retry is checked against.

use afd_wire::event::EventType;
use sha2::{Digest as _, Sha256};

/// Who is asking a fleet to run something.
///
/// A closed set rather than a caller-supplied string: the spelling lands in
/// the `producer` column and is half of the dedup key, so two call sites
/// spelling one producer two ways would let a retry through as new work, and
/// two producers sharing a spelling would silently dedup against each other.
/// An enum owned by the table's owner is what makes both impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Producer {
    /// An operator's message, keyed `<fleet_id>:<operation_id>` when the caller
    /// names its operation and minted per call when it does not.
    Steer,
    /// A signed delivery to one fleet's own route, keyed by the sender's
    /// delivery id.
    Webhook,
    /// A provider App's delivery, fanned out to every subscribed fleet and
    /// keyed per fleet by the sender's delivery id.
    WebhookApp,
    /// A schedule fire, keyed by the scheduler's message id.
    ScheduleFire,
    /// The event an approved gate continues with, keyed by the gate's action.
    GateContinuation,
    /// A repair verification, keyed by its intent row.
    RepairVerification,
    /// A chat mention of the bot, keyed by the provider's own event id, which
    /// the provider signs and repeats on every retry.
    SlackMention,
}

impl Producer {
    /// The stored spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Steer => "steer",
            Self::Webhook => "webhook",
            Self::WebhookApp => "webhook_app",
            Self::ScheduleFire => "schedule_fire",
            Self::GateContinuation => "gate_continuation",
            Self::RepairVerification => "repair_verification",
            Self::SlackMention => "slack_mention",
        }
    }

    /// Whether a caller chooses this producer's key.
    ///
    /// Only a steer's is: its operation id is whatever the client sent, so a
    /// different payload under it is that client's mistake, which the steer
    /// refuses and logs as its own. Every other key is repeated by a sender
    /// this daemon trusts, and a payload that changed under one is a deploy
    /// that renders it differently.
    #[must_use]
    pub const fn is_caller_keyed(self) -> bool {
        matches!(self, Self::Steer)
    }
}

/// What a producer is deduplicated on.
///
/// Two states rather than an `Option<&str>` and a comment, because the two
/// mean opposite things and a caller passing the wrong one is a correctness
/// bug either way: `Repeated` promises the value comes back unchanged on a
/// retry, and `Unrepeatable` promises there is no such value. An `Option`
/// would let a caller that HAS a delivery id pass `None` by omission and
/// silently lose deduplication for that producer.
#[derive(Debug, Clone, Copy)]
pub enum Key<'a> {
    /// The value this producer repeats across its retries — a delivery id, a
    /// scheduler message id, a gate action. Composed by the caller, because
    /// which field of which envelope is the sender's idempotency key is the
    /// envelope's to define.
    Repeated(&'a str),
    /// This producer has no value that survives a retry, so the ledger mints
    /// one. A person pressing send twice means two messages; there is nothing
    /// to deduplicate against and pretending otherwise would silently drop
    /// the second.
    Unrepeatable,
}

/// Where the answer to a unit of work goes, as its producer states it.
///
/// Three states rather than an `Option`, for the reason [`Key`] gives: a
/// continuation neither owns a reply surface nor lacks one — it answers where
/// the event it resumes would have — and folding that into `None` would lose a
/// thread's answer the moment a gate parked it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply<'a> {
    /// This producer owns no reply surface, so no answer is ever owed.
    None,
    /// The producer's own reply surface.
    To {
        /// The connector whose poster delivers the answer. `&'static` on
        /// purpose: only a constant such as `afd_connector::Provider::id()`
        /// can supply it, so a string resolved at run time — a lease's model
        /// provider, the defect this field exists to end — cannot be recorded
        /// as a destination.
        connector: &'static str,
        /// An address only that connector's poster reads.
        address: &'a str,
    },
    /// The destination of an earlier event on the same fleet, copied inside
    /// the admission's own statement so no read can race the write.
    Inherit {
        /// The logical id of the event whose destination is copied. An id this
        /// ledger never minted, or an event recorded with none, copies none.
        event_id: &'a str,
    },
}

/// One unit of work a producer asks a fleet to run.
///
/// Borrowed throughout: every field is a slice of something the caller holds
/// and the ledger reads each once.
#[derive(Debug, Clone, Copy)]
pub struct Admission<'a> {
    /// Who is asking.
    pub producer: Producer,
    /// What this unit of work is deduplicated on.
    pub key: Key<'a>,
    /// The fleet to run it.
    pub fleet: &'a str,
    /// The workspace the fleet belongs to.
    pub workspace: &'a str,
    /// Who the history records as having woken the fleet.
    pub actor: &'a str,
    /// How the event entered the system.
    pub event_type: EventType,
    /// The trigger payload, already serialized.
    pub request_json: &'a str,
    /// Where the answer goes. No default: every producer states it, so one
    /// added later cannot forget the question.
    pub reply: Reply<'a>,
}

/// Opens a stated destination's parts in the payload digest.
const DIGEST_REPLY_TO: &str = "reply_to";

/// Opens an inherited destination's parts in the payload digest.
const DIGEST_REPLY_INHERIT: &str = "reply_inherit";

impl Admission<'_> {
    /// The digest a producer key is checked against on a retry.
    ///
    /// Over the fields that make the event what it is — actor, type,
    /// workspace, body and destination — and not the instant, which a retry
    /// legitimately re-stamps. A NUL between parts, so two fields cannot slide
    /// into each other and hash the same.
    ///
    /// [`Reply::None`] adds nothing, so every row admitted before destinations
    /// existed keeps the digest it was stored with and a retry of one is not
    /// logged as drift. The other two open with a distinct tag, so a stated
    /// address can never hash like an inherited event id.
    #[must_use]
    pub fn payload_digest(&self) -> String {
        let mut hasher = Sha256::new();
        let mut absorb = |part: &str| {
            hasher.update(part.as_bytes());
            hasher.update([0u8]);
        };
        for part in [
            self.actor,
            self.event_type.as_str(),
            self.workspace,
            self.request_json,
        ] {
            absorb(part);
        }
        match self.reply {
            Reply::None => {}
            Reply::To { connector, address } => {
                [DIGEST_REPLY_TO, connector, address]
                    .into_iter()
                    .for_each(absorb);
            }
            Reply::Inherit { event_id } => {
                [DIGEST_REPLY_INHERIT, event_id]
                    .into_iter()
                    .for_each(absorb);
            }
        }
        hex::encode(hasher.finalize())
    }
}
