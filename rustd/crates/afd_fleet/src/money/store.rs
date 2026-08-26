//! The store every money read and write runs through.
//!
//! Same shape as [`crate::runner::Runners`] and [`crate::lease::Leases`], for
//! the same reason: the pool is OWNED here and [`Accounts::pool`] is
//! `pub(crate)`, so nothing outside this crate can run a money statement that
//! is not in [`crate::sql::billing`], and the side-by-side parity read of that
//! module stays meaningful (Invariant 5).
//!
//! # Why there is no rate cache, when the Zig has one
//!
//! `model_rate_cache.zig` is three hundred lines: a fixed-capacity table, a
//! hand-written `hash`/`eql` over the `(provider, model)` pair, an essay
//! proving the key cannot collide, and a generation stored beside every entry
//! so a caller never accepts a rate older than the catalogue revision it
//! observed. That machinery is answering a real question — a stale rate prices
//! a charge wrong, silently — and it exists because the Zig's
//! `resolveRenewSliceRates` reads the generation in ONE statement and then
//! looks the rate up separately, so a cache hit is what saves the second.
//!
//! But the Zig ALSO has `LOAD_RATE_WITH_REVISION`, which returns the rate and
//! the generation it was read at in a single snapshot. Reading through that
//! statement every time costs ONE round trip — fewer than the Zig's cached path
//! — and there is then no generation to compare, no entry to evict, and no
//! window in which a resident rate can be older than the catalogue. The cache's
//! entire purpose is to avoid a read that this shape does not perform.
//!
//! So the issue-time gate reads fresh. §3's renewal path prices a slice per
//! renewal rather than once per lease, and may want a cache for LATENCY — but
//! it would be an optimisation over a correct read, not the coherence mechanism
//! it is upstream, and it should be added with a measurement rather than
//! inherited with a port.

use afd_crypto::entropy::Entropy;
use afd_db::Db;

/// Reads and writes against `billing.*`, plus the catalogue a charge is priced
/// from.
///
/// Cheap to clone: `Db` is a handle over an `Arc`-backed pool, so every clone
/// shares one connection set. The entropy source is the second field because a
/// `billing.usage_ledger` row carries its own identifier and it is drawn
/// through the workspace's one entropy surface rather than a second call to the
/// operating system.
///
/// No queue. Every money decision is Postgres alone — which is worth stating,
/// because it is what lets the whole gate pass be proven against a database
/// with no Redis in the picture.
///
/// # Not `Ledger`
///
/// The obvious name is taken, and by something that is not money:
/// [`afd_db::migrate::Ledger`] is the SCHEMA-MIGRATION ledger — applied
/// versions and failure rows over `audit.schema_migrations`. Two types called
/// `Ledger` in one workspace, one about migrations and one about a tenant's
/// credit, is an ambiguous import and a reader's coin flip. `Accounts` says
/// what this one does in the vocabulary the domain already uses: it resolves
/// who pays, reads what they have, and records what they were charged.
#[derive(Debug, Clone)]
pub struct Accounts {
    database: Db,
    entropy: Entropy,
}

impl Accounts {
    /// Accounts reading and writing through `database`.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// The pool these accounts read through, for the sibling modules that add
    /// verbs to [`Accounts`] in their own files.
    pub(crate) const fn pool(&self) -> &Db {
        &self.database
    }

    /// The entropy a `usage_ledger` row's identifier is minted from.
    pub(crate) const fn entropy(&self) -> &Entropy {
        &self.entropy
    }
}
