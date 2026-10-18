//! The reads every money gate runs through.
//!
//! The pool is OWNED here and [`Accounts::pool`] is `pub(crate)`, so nothing
//! outside this crate can run a billing statement that is not in
//! [`crate::sql`] (Invariant 5). Money itself moves only through a
//! [`Biller`](crate::biller::Biller); `Accounts` answers the questions a gate
//! asks before any money moves: who pays, what a run's floor costs, and what a
//! fleet has spent.
//!
//! # Why there is no rate cache
//!
//! [`crate::price::PriceBook::load`] is one indexed lookup on the model's
//! `(provider, model_name)` and its price rows. Reading it every time costs ONE
//! round trip, and there is then no version to compare, no entry to evict, and
//! no window in which a resident rate can be older than the catalogue. A cache
//! for latency would be an optimisation over a correct read, added with a
//! measurement.

use afd_db::Db;

/// The billing reads a gate makes.
///
/// Cheap to clone: `Db` is a handle over an `Arc`-backed pool, so every clone
/// shares one connection set.
///
/// No queue. Every money decision is Postgres alone — which is worth stating,
/// because it is what lets the whole gate pass be proven against a database
/// with no Dragonfly in the picture.
#[derive(Debug, Clone)]
pub struct Accounts {
    database: Db,
}

impl Accounts {
    /// Accounts reading through `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// The pool these accounts read through, for the sibling modules that add
    /// verbs to [`Accounts`] in their own files.
    pub(crate) const fn pool(&self) -> &Db {
        &self.database
    }
}
