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
//! A key the caller chooses ([`Producer::is_caller_keyed`], a steer's
//! operation id) is the exception. The ledger still answers the first row, and
//! [`Admitted::stored`] carries its digest and fleet, but it does not log the
//! drift: a different payload there is the caller's mistake, not a deploy, and
//! the producer's own crate refuses it and logs it as one.
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

mod admission;
mod admit;
mod admit_receipt;
pub mod budget;
mod cursor;
pub mod error;
mod reconcile;
mod repeat;
mod replay;
pub mod sql;

use std::sync::Arc;

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_dragonfly::{Dragonfly, FleetStreams};

pub use self::admission::{Admission, Key, Producer, Reply};
use self::budget::Ceiling;
pub use self::budget::{BudgetScope, Budgets};
pub use self::cursor::LedgerBacklog;
pub use self::error::{Error, Result};
pub use self::reconcile::{DEFAULT_REPAIR_CAPACITY, Progress, Reconciled};
pub use self::repeat::Repeated;
pub use self::replay::Replayed;

/// What an admission decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admitted {
    /// Whether an earlier call already admitted this key.
    pub replayed: bool,
    /// The row the key holds, as the admitting statement itself returned it:
    /// this call's on a fresh insert, the earlier call's on a replay. Its id is
    /// the event's, and its digest and fleet let a producer tell a retry from a
    /// different payload under one key without a second read.
    pub stored: Repeated,
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

    /// The live tails over this ledger's queue, for a producer announcing
    /// what it admitted — the handle the lease plane's `streams()` is too.
    #[must_use]
    pub fn streams(&self) -> FleetStreams {
        FleetStreams::new(self.queue.clone())
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
