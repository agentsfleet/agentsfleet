//! The reads every money gate runs through.
//!
//! The pool is OWNED here and [`Accounts::pool`] is `pub(crate)`, so nothing
//! outside this crate can run a billing statement that is not in
//! [`crate::sql`] (Invariant 5). Money itself moves only through a
//! [`Biller`](crate::biller::Biller); `Accounts` answers the questions a gate
//! asks before any money moves: what a run's floor costs, and what a fleet has
//! spent.
//!
//! # Rates and the cache
//!
//! [`crate::price::PriceBook::load`] is one indexed lookup on the model's
//! `(provider, model_name)` and its price rows. `Accounts` reads it every time.
//! The lease path holds the book instead, in a cache keyed by the catalogue
//! version, so an edit to a rate moves the key and the next lookup loads the new
//! book; [`Accounts::estimate_from`] prices against a book the caller already
//! holds.

use afd_db::Db;

/// The billing reads a gate makes.
///
/// Cheap to clone: `Db` is a handle over an `Arc`-backed pool, so every clone
/// shares one connection set.
///
/// No bus. Every money decision is Postgres alone, which lets the whole gate
/// pass be proven against a database and nothing else.
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
