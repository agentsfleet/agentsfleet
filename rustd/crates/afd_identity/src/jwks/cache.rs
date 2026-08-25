//! Holding a key set, refreshing it once, and serving it stale when refusing
//! would be worse.
//!
//! # Why this is not a cache crate
//!
//! It looks like a one-entry cache and is not one. A cache evicts what has
//! expired; this must do the opposite — when the identity provider is
//! unreachable, an EXPIRED key set is exactly what should still be served,
//! because verifying against keys that are minutes old beats a total
//! authentication outage. `jwks.zig:172-186` calls that the stale-serve path
//! and it is the behaviour any eviction policy would break.
//!
//! So: an `RwLock` over the held set, a `Mutex` as the single-flight gate, and
//! the freshness decision read from an injected [`Clock`] rather than from a
//! crate's internal timer. Every one of those decisions is steerable in a test.
//!
//! # The three refusals to conflate
//!
//! - **Expired** — refresh, then serve whatever we end up holding.
//! - **Key-id miss on a FRESH set** — the issuer probably rotated ahead of
//!   signing. Refresh once, rate-limited, then look again.
//! - **Refresh failed** — keep the previous set and serve from it. A failed
//!   fetch must never empty the cache; that would turn a provider blip into
//!   every token failing at once.

use std::sync::Arc;

use afd_auth::verifier::VerifyError;
use afd_core::clock::{Clock, UnixMillis};

use crate::jwks::key_set::JwkKeySet;
use crate::jwks::source::KeySetSource;

/// How long a fetched key set is served without asking again.
///
/// Six hours, matching `jwks.zig`'s `cache_ttl_ms` and `docs/AUTH.md`'s
/// "Cached for 6 h, refreshed on `kid` miss".
pub const DEFAULT_TTL_MS: i64 = 6 * 60 * 60 * 1_000;

/// Shortest interval between fetch ATTEMPTS, successful or not.
///
/// `jwks.zig`'s `JWKS_REFRESH_MIN_INTERVAL_MS`. It bounds two different storms
/// with one number: key-id misses during a rotation, and retries while the
/// provider is down. The six-hour refresh is always far above it, so it only
/// ever bites the miss path.
pub const REFRESH_MIN_INTERVAL_MS: i64 = 30 * 1_000;

/// Why a refresh was attempted. Carried for the log line, and to decide whether
/// a set that is already fresh makes the attempt unnecessary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The held set is older than its time-to-live, or there is none.
    Expired,
    /// The held set is fresh but does not carry the key a token named.
    KeyIdMiss,
}

/// A held key set and when it was fetched.
#[derive(Debug, Clone)]
struct Held {
    keys: Arc<JwkKeySet>,
    fetched_at: UnixMillis,
}

/// The key set this verifier holds, and the policy for replacing it.
#[derive(Debug)]
pub(crate) struct KeyCache<S> {
    source: S,
    clock: Arc<dyn Clock>,
    ttl_ms: i64,
    held: tokio::sync::RwLock<Option<Held>>,
    /// The single-flight gate. Held across the fetch, which happens with the
    /// `held` lock RELEASED so a cache hit never queues behind a slow provider.
    flight: tokio::sync::Mutex<LastAttempt>,
}

/// When the last fetch was attempted, guarded by the flight gate.
#[derive(Debug, Clone, Copy)]
struct LastAttempt(Option<UnixMillis>);

impl<S: KeySetSource> KeyCache<S> {
    /// Builds a cache over `source`.
    pub(crate) fn new(source: S, clock: Arc<dyn Clock>, ttl_ms: i64) -> Self {
        Self {
            source,
            clock,
            ttl_ms,
            held: tokio::sync::RwLock::new(None),
            flight: tokio::sync::Mutex::new(LastAttempt(None)),
        }
    }

    /// The source, for a test to count fetches against.
    pub(crate) const fn source(&self) -> &S {
        &self.source
    }

    /// Resolves the key `kid` names, refreshing at most once.
    ///
    /// # Errors
    /// [`VerifyError::KeyNotFound`] when a refreshed set still does not carry
    /// the key — a token signed by a key the issuer never published.
    /// [`VerifyError::KeySetUnavailable`] when there is no set to serve at all.
    pub(crate) async fn resolve(&self, kid: &str) -> Result<Arc<JwkKeySet>, VerifyError> {
        let reason = {
            let held = self.held.read().await;
            match held.as_ref() {
                None => Reason::Expired,
                Some(entry) if !self.is_fresh(entry) => Reason::Expired,
                Some(entry) if entry.keys.find(kid).is_some() => {
                    // The overwhelmingly common path: fresh, and it has the
                    // key. Returns under a READ lock, so concurrent
                    // verifications never serialize on each other.
                    return Ok(Arc::clone(&entry.keys));
                }
                // Fresh, and the key is not in it. The issuer publishes a new
                // key before it signs with it, so this usually means our set
                // is simply behind.
                Some(_) => Reason::KeyIdMiss,
            }
        };

        self.refresh(reason).await;

        // Stale-serve: whatever we hold now, even if the refresh failed and
        // even if it is past its time-to-live. Verifying against known keys
        // beats a hard authentication outage while the provider is down.
        let held = self.held.read().await;
        let Some(entry) = held.as_ref() else {
            return Err(VerifyError::KeySetUnavailable);
        };
        if entry.keys.find(kid).is_none() {
            return Err(VerifyError::KeyNotFound);
        }
        Ok(Arc::clone(&entry.keys))
    }

    /// Fetches and installs a key set, or leaves the previous one in place.
    ///
    /// Public so §7 can call it at boot: a key set this daemon cannot verify
    /// against must refuse startup rather than 401 every session token while
    /// `agt_t` and `afc_` keep working.
    ///
    /// # Errors
    /// [`VerifyError::KeySetUnavailable`] when the fetch or the parse failed.
    pub(crate) async fn prime(&self) -> Result<Arc<JwkKeySet>, VerifyError> {
        self.fetch_and_install().await?;
        let held = self.held.read().await;
        held.as_ref()
            .map(|entry| Arc::clone(&entry.keys))
            .ok_or(VerifyError::KeySetUnavailable)
    }

    /// Whether `entry` is within its time-to-live.
    fn is_fresh(&self, entry: &Held) -> bool {
        self.clock.now().saturating_millis_since(entry.fetched_at) <= self.ttl_ms
    }

    /// One flight at a time, rate-limited, and never fatal.
    ///
    /// Returns nothing: every caller's next move is the same — read whatever is
    /// held — so a failure here is a log line and not a decision. That is the
    /// stale-serve policy expressed as a signature.
    async fn refresh(&self, reason: Reason) {
        let entered = self.clock.now();
        let mut last = self.flight.lock().await;

        // Another flight may have refreshed while this one waited for the gate.
        if reason == Reason::Expired && self.held_is_fresh().await {
            return;
        }
        if let LastAttempt(Some(previous)) = *last
            && entered.saturating_millis_since(previous) < REFRESH_MIN_INTERVAL_MS
        {
            // Rate-limited. The caller serves whatever set we still hold, which
            // is the same thing it would do after a failed fetch.
            return;
        }
        *last = LastAttempt(Some(entered));

        if let Err(err) = self.fetch_and_install().await {
            // Hoisted out of the macro: with `tracing`'s `log` feature enabled
            // across this workspace, a call inside an event field compiles
            // twice and llvm-cov reports the dead copy. See the note beside
            // `tracing` in the workspace manifest.
            let cause = err.to_string();
            let reason = format!("{reason:?}");
            tracing::warn!(
                cause,
                reason,
                "jwks_refresh_failed: serving the previously held key set"
            );
        }
    }

    /// Whether the held set is within its time-to-live.
    async fn held_is_fresh(&self) -> bool {
        let held = self.held.read().await;
        held.as_ref().is_some_and(|entry| self.is_fresh(entry))
    }

    /// Reads the source and replaces the held set, or changes nothing.
    ///
    /// A failed fetch or an unparseable document leaves the previous set in
    /// place, deliberately: emptying it would turn a provider blip into every
    /// token failing at once, which is strictly worse than keys that are stale.
    async fn fetch_and_install(&self) -> Result<(), VerifyError> {
        let raw = self.source.fetch().await?;
        let parsed = JwkKeySet::parse(&raw)?;
        let rejected = parsed.rejected();
        if rejected > 0 {
            let usable = parsed.len();
            tracing::warn!(
                rejected,
                usable,
                "jwks_keys_declined: the issuer published keys this daemon cannot verify against"
            );
        }
        let fetched_at = self.clock.now();
        let mut held = self.held.write().await;
        *held = Some(Held {
            keys: Arc::new(parsed),
            fetched_at,
        });
        Ok(())
    }
}
