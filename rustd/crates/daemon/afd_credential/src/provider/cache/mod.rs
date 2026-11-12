//! The platform default's resolution, held in daemon memory by the witnesses
//! that change it.
//!
//! # What the key is, and why it is enough
//!
//! A platform resolution is the active default's provider, its catalogued
//! model and endpoint, and the decrypted key of the vault row it names. Each of
//! those has a witness a statement can read without opening anything:
//!
//! - the catalogue: every model, price and platform-default write advances
//!   `model.catalogue_version` in its own transaction;
//! - the key and its endpoint: a digest of the stored ciphertext and the
//!   endpoint beside it. A writer cannot skip it, because it is a function of
//!   the bytes: every write re-seals the envelope under a fresh nonce, so even
//!   a replace under the same key-encryption key (KEK) that keeps the row's id
//!   moves it;
//! - a delete and recreate: the new row has a new id.
//!
//! So an entry keyed by the catalogue version, the secret's id and that digest
//! answers for exactly one resolution, and the claim statement reads all three
//! beside the slot it takes. No lookup by provider or tenant compiles: a
//! [`PlatformKeyKey`] is built only from a statement's row.
//!
//! # What no write announces
//!
//! A price window closing is the one change that moves no witness: the
//! catalogue edit that would answer for it never happens, and the read would
//! refuse where the cache would still serve. So an entry also dies with the
//! price window the read matched, measured by the database as a span from its
//! own clock and added here to the instant the read began, and never outlives
//! [`PLATFORM_KEY_TTL`] either.
//!
//! # The key stays a secret
//!
//! The value is a [`Resolved`], whose key is a `SecretString` over
//! `Zeroizing<String>`: it renders as redacted and is zeroed when the last copy
//! drops, eviction included. The time limit is what makes that last clause
//! prompt for a key an operator has replaced: nothing asks for the old entry
//! again, so without it the plaintext would wait for the entry cap.
//!
//! `moka` frees an expired entry only inside its own calls, so every
//! resolution that reads the database first sweeps the cache ([`PlatformKeys::sweep`]).
//! The bound therefore holds while the replica resolves keys; an idle replica
//! keeps an expired entry until its next resolution.

use std::sync::Arc;
use std::time::Duration;

use af_core::clock::{Clock, SystemClock, after};
use af_core::id::Uuid7;
use af_observability::metrics::label::fleet::{CacheOutcome, LeaseCache};
use af_observability::producers::fleet::cache::looked_up;
use jiff::Timestamp;
use moka::Expiry;
use moka::future::Cache;

use super::Resolved;

/// How many platform resolutions one replica holds.
///
/// A performance parameter: a deployment has one active default, so the cap
/// only bounds the versions a busy catalogue leaves behind.
pub const PLATFORM_KEY_ENTRIES: u64 = 1_024;

/// The longest a platform resolution stays in memory, whatever its witnesses
/// say.
///
/// It bounds how long a replaced or deactivated key stays decrypted in the
/// daemon: nothing asks for such an entry again, so the entry cap alone would
/// leave its plaintext in memory indefinitely. The cost of the bound is one
/// resolution read per replica per interval. It holds while the replica
/// resolves keys, because the cache frees an expired entry only inside its own
/// calls and [`PlatformKeys::sweep`] makes one before each read.
pub const PLATFORM_KEY_TTL: Duration = Duration::from_mins(5);

/// The catalogue version, the platform secret's id and the digest of what it
/// holds, exactly as a statement returned them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlatformKeyKey {
    catalogue_version: i64,
    secret_id: Uuid7,
    secret_digest: Box<str>,
}

impl PlatformKeyKey {
    /// The key for the witnesses the caller's statement returned in this pass.
    ///
    /// Only a statement row's decoder calls this: the lease claim and the
    /// platform resolution's own read.
    #[must_use]
    pub fn observed(
        catalogue_version: i64,
        secret_id: Uuid7,
        secret_digest: impl Into<Box<str>>,
    ) -> Self {
        Self {
            catalogue_version,
            secret_id,
            secret_digest: secret_digest.into(),
        }
    }
}

/// What a platform read observed: the key it answers for, and the span the
/// price window it matched stays open, measured on the database's clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    /// The witnesses the read returned.
    pub key: PlatformKeyKey,
    /// How long the matched price window has left; `None` for an open-ended one.
    pub window: Option<Duration>,
}

/// A held resolution and the instant it stops answering.
#[derive(Debug, Clone)]
struct Held {
    resolved: Resolved,
    /// The instant after which a lookup is a miss.
    open_until: Timestamp,
    /// How long `moka` keeps the entry: the same span, so an entry nobody asks
    /// for again is still dropped.
    lifetime: Duration,
}

impl Held {
    /// Whether the entry still answers at `now`.
    ///
    /// Decided against the injected clock rather than left to `moka`'s own
    /// expiry alone, which is what keeps it provable without waiting for
    /// wall-clock time to pass; the cache's expiry is the same span, so the two
    /// agree.
    fn is_open(&self, now: Timestamp) -> bool {
        now < self.open_until
    }
}

/// Expires each entry by its own span: the price window can close long before
/// [`PLATFORM_KEY_TTL`].
struct PerEntry;

impl Expiry<PlatformKeyKey, Held> for PerEntry {
    fn expire_after_create(
        &self,
        _key: &PlatformKeyKey,
        value: &Held,
        _created_at: std::time::Instant,
    ) -> Option<Duration> {
        Some(value.lifetime)
    }
}

/// Platform resolutions by the witnesses they were read at.
#[derive(Debug, Clone)]
pub struct PlatformKeys {
    entries: Cache<PlatformKeyKey, Held>,
    clock: Arc<dyn Clock>,
}

impl Default for PlatformKeys {
    fn default() -> Self {
        Self::new(Arc::new(SystemClock))
    }
}

impl PlatformKeys {
    /// An empty cache that tells time by `clock`.
    #[must_use]
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            entries: Cache::builder()
                .max_capacity(PLATFORM_KEY_ENTRIES)
                .expire_after(PerEntry)
                .build(),
            clock,
        }
    }

    /// The instant now, by the clock the entries are judged against.
    pub(crate) fn now(&self) -> Timestamp {
        self.clock.now()
    }

    /// Frees every entry past its span now, zeroing the keys they held.
    ///
    /// `moka` frees an expired entry only inside its own calls, so a replica that
    /// stops asking for a replaced or deactivated key would otherwise keep its
    /// plaintext until the next lookup of any entry.
    pub(crate) async fn sweep(&self) {
        self.entries.run_pending_tasks().await;
    }

    /// The resolution `key` holds, counted as a hit or a miss.
    ///
    /// An entry past its span is a miss and is dropped on the way, so the
    /// plaintext does not wait for the cache's own housekeeping.
    pub async fn get(&self, key: &PlatformKeyKey) -> Option<Resolved> {
        let now = self.now();
        let held = match self.entries.get(key).await {
            Some(held) if held.is_open(now) => Some(held.resolved),
            Some(_closed) => {
                self.entries.invalidate(key).await;
                None
            }
            None => None,
        };
        let outcome = if held.is_some() {
            CacheOutcome::Hit
        } else {
            CacheOutcome::Miss
        };
        looked_up(LeaseCache::PlatformKey, outcome);
        held
    }

    /// Holds `resolved` under the witnesses its own read returned.
    ///
    /// `started` is the instant the read began: the window's span was measured
    /// after it, so adding the span to it can only end the entry early.
    pub(crate) async fn insert(&self, observed: Observed, started: Timestamp, resolved: Resolved) {
        let lifetime = lifetime(observed.window);
        let held = Held {
            resolved,
            open_until: after(started, lifetime),
            lifetime,
        };
        self.entries.insert(observed.key, held).await;
    }
}

/// How long an entry may be held: the price window the read matched, and never
/// more than [`PLATFORM_KEY_TTL`].
fn lifetime(window: Option<Duration>) -> Duration {
    window.map_or(PLATFORM_KEY_TTL, |span| span.min(PLATFORM_KEY_TTL))
}

#[cfg(test)]
mod tests;
