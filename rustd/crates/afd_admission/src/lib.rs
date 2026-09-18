//! The admission ledger: the row that IS a producer's acceptance, and the
//! queue entry that is only its receipt.
//!
//! # The row is the acceptance, the entry is a receipt
//!
//! Every producer — a steer, a webhook delivery, a schedule fire, a gate's
//! continuation, a repair verifier — commits a `core.fleet_admissions` row
//! before it is told yes. The row's logical id, `<created_at>-<seq>`, is the
//! event's identity from here to the usage ledger. The `XADD` that follows is
//! recorded back as a receipt, and a row that never got one is re-appended by
//! [`Admissions::replay`] under the same id. Losing the queue therefore loses
//! no accepted work while Postgres survives.
//!
//! # Duplicates are a unique index, not a Dragonfly claim
//!
//! `(producer, producer_key)` is unique. A retry is a conflict that answers
//! the FIRST call's id with `replayed = true`, which is what the two-key
//! `append_once` script used to answer — with no expiry, no second key and
//! no cross-slot Lua. A key reused with a DIFFERENT payload is answered the
//! same way and logged: the key is the identity, and a redelivery whose body
//! this daemon now renders differently is still the same delivery. Refusing
//! it with a 4xx would stop the sender retrying, which is how a delivery is
//! lost across a deploy.
//!
//! # Who appends, when two admit at once
//!
//! Two daemons can take one retry at the same instant. Both commit the same
//! row — one inserts, one conflicts — and only the inserter appends; the other
//! answers `replayed` and leaves the append to the inserter or, should it die
//! first, to the replay sweeper. A queue that refuses the append is a
//! deferral, not a refusal: the row is safe, the caller is answered, and the
//! sweeper delivers when the queue is back. Two things refuse, both with a
//! retryable class and no acceptance recorded: a database that will not
//! commit, and a budget that is spent — see [`budget`].
//!
//! # Why its own crate
//!
//! One owner per table, the way `afd_events` owns `core.fleet_events`. Every
//! producer crate admits through here and none of them may depend on another,
//! which is the shape a shared module in any one of them could not have.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit. Gated on `not(test)` because the test build links
// dev-dependencies into this same target.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]

mod admit;
pub mod budget;
mod cursor;
pub mod error;
mod reconcile;
mod replay;
pub mod sql;

use std::sync::Arc;

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_dragonfly::Dragonfly;
use afd_wire::event::EventType;
use sha2::{Digest as _, Sha256};

use self::budget::Ceiling;
pub use self::budget::{BudgetScope, Budgets};
pub use self::cursor::LedgerBacklog;
pub use self::error::{Error, Result};
pub use self::reconcile::{DEFAULT_REPAIR_CAPACITY, Progress, Reconciled};
pub use self::replay::Replayed;

/// Who is asking a fleet to run something.
///
/// A closed set rather than a caller-supplied string: the spelling lands in
/// the `producer` column and is half of the dedup key, so two call sites
/// spelling one producer two ways would let a retry through as new work, and
/// two producers sharing a spelling would silently dedup against each other.
/// An enum owned by the table's owner is what makes both impossible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Producer {
    /// An operator's message. Its key is minted per call: a steer has no
    /// natural retry identity, and never did.
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
        }
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
    /// envelope's contract.
    Repeated(&'a str),
    /// This producer has no value that survives a retry, so the ledger mints
    /// one. A person pressing send twice means two messages; there is nothing
    /// to deduplicate against and pretending otherwise would silently drop
    /// the second.
    Unrepeatable,
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
}

impl Admission<'_> {
    /// The digest a producer key is checked against on a retry.
    ///
    /// Over the fields that make the event what it is — actor, type,
    /// workspace and body — and not the instant, which a retry legitimately
    /// re-stamps. A NUL between parts, so two fields cannot slide into each
    /// other and hash the same.
    #[must_use]
    pub fn payload_digest(&self) -> String {
        let mut hasher = Sha256::new();
        for part in [
            self.actor,
            self.event_type.as_str(),
            self.workspace,
            self.request_json,
        ] {
            hasher.update(part.as_bytes());
            hasher.update([0u8]);
        }
        hex::encode(hasher.finalize())
    }
}

/// What an admission decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admitted {
    /// The event's logical id — this call's, or the earlier call's.
    pub id: String,
    /// Whether an earlier call already admitted this key.
    pub replayed: bool,
}

/// The admission ledger over the database that holds it and the queue it
/// hands receipts to.
///
/// Cheap to clone: two pool handles, an entropy source, two numbers, and a
/// shared handle on the deployment ceiling's sampled figure. The figure is
/// behind an [`Arc`] on purpose — every clone of one ledger must read the
/// sample the last one published, or each clone would carry its own idea of
/// the backlog and none of them would resample often enough to matter.
#[derive(Debug, Clone)]
pub struct Admissions {
    database: Db,
    queue: Dragonfly,
    entropy: Entropy,
    budgets: Budgets,
    ceiling: Arc<Ceiling>,
}

impl Admissions {
    /// Binds the ledger to an already-connected pool and queue, under the
    /// production budgets.
    #[must_use]
    pub fn new(database: Db, queue: Dragonfly, entropy: Entropy) -> Self {
        Self {
            database,
            queue,
            entropy,
            budgets: Budgets::default(),
            ceiling: Arc::new(Ceiling::default()),
        }
    }

    /// The same ledger under other budgets.
    ///
    /// A builder step rather than a fourth argument to [`Admissions::new`]:
    /// every production caller wants the defaults, and the one caller that
    /// does not — a suite proving a refusal without a hundred thousand rows —
    /// should have to say so by name.
    #[must_use]
    pub const fn with_budgets(mut self, budgets: Budgets) -> Self {
        self.budgets = budgets;
        self
    }

    /// The same ledger, with the host's own entropy supplied for a suite.
    ///
    /// Every crate whose lane drives a producer needs an `Admissions`, and the
    /// only argument any of them has an opinion about is the pool and the
    /// queue their own harness already opened. Handing the third one out here
    /// keeps `afd_crypto` out of eight `[dev-dependencies]` blocks and means a
    /// suite that wants a CONTROLLED entropy source has to say so, by calling
    /// [`Admissions::new`] with one, rather than getting it by accident.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn for_tests(database: Db, queue: Dragonfly) -> Self {
        Self::new(database, queue, Entropy::new())
    }
}

/// The logical event id a row spells: the admission instant and the
/// sequence, in the numeric shape a stream entry id has.
///
/// The shape is load-bearing: every surface that renders, sorts or pages on
/// event ids was written against `<millis>-<n>`, and keeping it means none of
/// them learn that identity moved.
#[must_use]
pub fn logical_id(created_at: i64, seq: i64) -> String {
    format!("{created_at}-{seq}")
}

/// The two integers a logical event id spells, or `None` when the text is not
/// one this ledger minted.
///
/// The inverse of [`logical_id`], and here beside it so the one crate owns both
/// directions: the lease path binds these to `sql::MARK_DELIVERED`, and a
/// second parser elsewhere could drift from the spelling written above.
///
/// `None` is an ordinary answer, not a fault. `core.fleet_events` also holds
/// ids this table never minted — an approval's continuation, and every event
/// that predates the ledger — and a delivery of one of those simply has no
/// admission row to stamp.
#[must_use]
pub fn logical_parts(id: &str) -> Option<(i64, i64)> {
    let (created_at, seq) = id.split_once('-')?;
    Some((created_at.parse().ok()?, seq.parse().ok()?))
}

#[cfg(test)]
mod tests;
