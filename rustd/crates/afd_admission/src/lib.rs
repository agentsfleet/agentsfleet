//! One durable admission per producer key. The committed row is the work queue.
//! Notifications are best-effort hints; polling Postgres always finds accepted work.

// A dependency listed but unused is a supply-chain and compile-time cost with
// no offsetting benefit. Gated on `not(test)` because the test build links
// dev-dependencies into this same target.
#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

mod admission;
mod admit;
mod backlog;
pub mod budget;
mod confirm;
pub mod error;
mod repeat;
pub mod sql;
mod stage;
mod wake;

use std::sync::Arc;

use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_dragonfly::{Dragonfly, FleetStreams};

pub use self::admission::{Admission, Key, Producer, Reply};
pub use self::backlog::LedgerBacklog;
use self::budget::Ceiling;
pub use self::budget::{BudgetScope, Budgets};
pub use self::confirm::Confirmation;
pub use self::error::{Error, Result};
pub use self::repeat::Repeated;
pub use self::stage::PreparedAdmission;

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

/// The durable queue, its entropy source and best-effort notification transport.
/// Clones share the deployment ceiling sample.
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

#[cfg(test)]
mod tests;
