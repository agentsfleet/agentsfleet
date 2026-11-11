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
/// resolution read per replica per interval.
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
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the library"
    )]

    use std::sync::Arc;
    use std::time::Duration;

    use af_core::clock::FixedClock;
    use af_crypto::secret::SecretString;
    use afd_billing::Posture;
    use jiff::Timestamp;
    use moka::Expiry;

    use super::{
        Held, Observed, PLATFORM_KEY_TTL, PerEntry, PlatformKeyKey, PlatformKeys, Resolved,
    };

    /// The platform key a fixture caches; it must never render.
    const KEY: &str = "sk-platform-cached-fixture";

    /// When the fixture's read began.
    const STARTED: Timestamp = Timestamp::constant(1_760_000_000, 0);

    /// The digest a fixture's statement returned.
    const DIGEST: &str = "5e3f1a";

    fn key_with(digest: &str) -> PlatformKeyKey {
        PlatformKeyKey::observed(
            7,
            af_core::id::Uuid7::parse("01900000-0000-7000-8000-000000000001")
                .expect("a fixture identifier"),
            digest,
        )
    }

    fn key() -> PlatformKeyKey {
        key_with(DIGEST)
    }

    fn observed(window: Option<Duration>) -> Observed {
        Observed { key: key(), window }
    }

    fn resolved() -> Resolved {
        Resolved::new(
            Posture::Platform,
            "anthropic".into(),
            "claude-fixture".into(),
            200_000,
            None,
            SecretString::new(KEY.to_owned()),
        )
    }

    fn cache_at(clock: &FixedClock) -> PlatformKeys {
        PlatformKeys::new(Arc::new(clock.clone()))
    }

    /// A cached resolution, and the cache holding it, render no byte of the key.
    #[tokio::test]
    async fn cached_platform_key_never_renders() {
        let clock = FixedClock::at(STARTED);
        let cache = cache_at(&clock);
        cache.insert(observed(None), STARTED, resolved()).await;
        let held = cache.get(&key()).await.expect("the entry is held");
        assert_eq!(held.api_key().expose(), KEY, "the entry holds the key");
        for rendered in [format!("{held:?}"), format!("{cache:?}")] {
            assert!(!rendered.contains(KEY), "{rendered}");
            assert!(!rendered.contains("sk-platform"), "{rendered}");
        }
    }

    /// Another digest of the same secret misses: the entry answers for one.
    #[tokio::test]
    async fn another_secret_digest_misses() {
        let clock = FixedClock::at(STARTED);
        let cache = cache_at(&clock);
        cache.insert(observed(None), STARTED, resolved()).await;
        assert!(cache.get(&key_with("a-replaced-key")).await.is_none());
    }

    /// A price window that closes with no successor stops the entry answering
    /// at that instant, though no version moved.
    #[tokio::test]
    async fn an_entry_ends_with_its_price_window() {
        let window = Duration::from_secs(90);
        let clock = FixedClock::at(STARTED);
        let cache = cache_at(&clock);
        cache
            .insert(observed(Some(window)), STARTED, resolved())
            .await;

        clock.advance(window.saturating_sub(Duration::from_millis(1)));
        assert!(
            cache.get(&key()).await.is_some(),
            "a millisecond before the window closes it still answers"
        );
        clock.advance(Duration::from_millis(1));
        assert!(
            cache.get(&key()).await.is_none(),
            "at the instant the window closes it does not"
        );
    }

    /// The window was measured after the read began, so the entry's end is
    /// counted from when the read began, never from when it was stored.
    #[tokio::test]
    async fn the_span_is_counted_from_when_the_read_began() {
        let window = Duration::from_secs(90);
        let read_took = Duration::from_secs(5);
        let clock = FixedClock::at(STARTED);
        let cache = cache_at(&clock);
        clock.advance(read_took);
        cache
            .insert(observed(Some(window)), STARTED, resolved())
            .await;

        clock.advance(
            window
                .saturating_sub(read_took)
                .saturating_sub(Duration::from_millis(1)),
        );
        assert!(cache.get(&key()).await.is_some());
        clock.advance(Duration::from_millis(1));
        assert!(
            cache.get(&key()).await.is_none(),
            "the window ends {window:?} after the read began"
        );
    }

    /// An open-ended window is bounded by the time limit alone, so a key an
    /// operator replaced does not stay decrypted past it.
    #[tokio::test]
    async fn an_open_window_is_bounded_by_the_time_limit() {
        let clock = FixedClock::at(STARTED);
        let cache = cache_at(&clock);
        cache.insert(observed(None), STARTED, resolved()).await;

        clock.advance(PLATFORM_KEY_TTL.saturating_sub(Duration::from_millis(1)));
        assert!(cache.get(&key()).await.is_some());
        clock.advance(Duration::from_millis(1));
        assert!(cache.get(&key()).await.is_none());
    }

    /// A window longer than the time limit does not extend it.
    #[tokio::test]
    async fn a_long_window_does_not_outlive_the_time_limit() {
        let clock = FixedClock::at(STARTED);
        let cache = cache_at(&clock);
        cache
            .insert(observed(Some(PLATFORM_KEY_TTL * 12)), STARTED, resolved())
            .await;

        clock.advance(PLATFORM_KEY_TTL);
        assert!(cache.get(&key()).await.is_none());
    }

    /// The cache is told each entry's own span, so an entry nobody asks for
    /// again (a replaced key) is still dropped when the span ends.
    #[test]
    fn the_cache_is_told_each_entrys_own_span() {
        let span = Duration::from_secs(9);
        let held = Held {
            resolved: resolved(),
            open_until: STARTED,
            lifetime: span,
        };
        assert_eq!(
            PerEntry.expire_after_create(&key(), &held, std::time::Instant::now()),
            Some(span)
        );
    }
}
