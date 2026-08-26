//! What a person may do, asked of the identity provider and cached briefly.
//!
//! The port of `auth/clerk_scope_resolver.zig`, and the one place this
//! milestone takes a dependency to REPLACE hand-written concurrency rather than
//! to save typing.
//!
//! # What the cache is, and what it is not
//!
//! A latency optimisation and nothing else: in-memory, never outliving the
//! process, no projection to backfill and nothing to reconcile. Every entry
//! self-heals toward the provider within the freshness window.
//!
//! # Three windows, and why they are not one
//!
//! - **Fresh** (60 s) — served without asking. The same order as the
//!   dashboard's own session-token refresh, which is the parity that makes a
//!   terminal and a browser agree about a person's capabilities.
//! - **Stale but within the ceiling** (15 min) — served ONLY when the provider
//!   is unreachable. Refusing every terminal during a vendor blip is worse than
//!   acting on capabilities that are minutes old.
//! - **Past the ceiling, or cold** — refused as an outage. Never an empty set:
//!   an empty set reads to an operator as a demotion they never received, and
//!   would be indistinguishable from a person the provider has forgotten.
//!
//! # What `moka` bought, precisely
//!
//! `try_get_with` coalesces concurrent loads for the same key. The Zig resolver
//! says it is not single-flighted and names the consequence:
//!
//! > *"tenant keys ride ONE creator subject at machine rates, so at expiry
//! > their in-flight requests fetch concurrently; that is the first place to
//! > add per-subject single-flight if provider-call volume ever shows up in
//! > ops."*
//!
//! That is closed here. It also RETIRES the `seq` counter, which existed only
//! so a slow out-of-order response could not overwrite a newer one and
//! resurrect a pre-revocation claim — with one flight per subject there is no
//! second response to be out of order with. A hand-written ordering rule on a
//! security-relevant path disappears rather than being ported.
//!
//! And the bound behaves better: the Zig cache drops the WHOLE map when it
//! reaches its limit, costing every live operator a cold fetch. `moka` evicts
//! the coldest entry.
//!
//! # The one thing `moka` is deliberately not trusted with
//!
//! Its expiry runs on an internal `Instant` that `afd_core`'s `FixedClock`
//! cannot steer. So the entry carries its own `fetched_at` and BOTH windows are
//! decided against the injected clock — every decision stays deterministic in a
//! test. `time_to_live` is set to the ceiling as well, purely so an entry that
//! can never be served is eventually reclaimed; nothing reads it as a decision.

use crate::error::ClaimUnavailable;
use std::sync::Arc;
use std::time::Duration;

use afd_auth::capability::CapabilitySource;
use afd_auth::error::Unavailable;
use afd_auth::principal::Subject;
use afd_auth::scope::{ScopeSet, parse_claim};
use afd_core::clock::{Clock, UnixMillis};

/// How long a fetched claim is served without asking again.
///
/// `clerk_scope_resolver.zig`'s `DEFAULT_TTL_MS`.
pub const DEFAULT_TTL_MS: i64 = 60 * 1_000;

/// How long a claim may be served after the provider stops answering.
///
/// `clerk_scope_resolver.zig`'s `DEFAULT_STALE_CEILING_MS`. Past this a claim
/// could contradict a revocation nobody can confirm, so the answer becomes an
/// outage.
pub const DEFAULT_STALE_CEILING_MS: i64 = 15 * 60 * 1_000;

/// Distinct subjects held at once.
///
/// `clerk_scope_resolver.zig`'s `MAX_CACHED_SUBJECTS`, far above any real
/// operator count. Where the Zig cache drops everything at this bound, this one
/// evicts its coldest entry.
pub const MAX_CACHED_SUBJECTS: u64 = 4096;

/// Reads a capability claim for a subject from the identity provider.
///
/// The network seam under [`ProviderCapabilities`], separate so the cache's
/// three windows are provable without a provider. Mirrors
/// `clerk_scope_fetch.zig`.
pub trait ClaimSource: Send + Sync + std::fmt::Debug + 'static {
    /// Reads the space-delimited claim the provider holds for `subject`.
    ///
    /// # Errors
    /// [`ClaimUnavailable::Unreachable`] when the provider could not be asked,
    /// and [`ClaimUnavailable::UnknownSubject`] when it answered that it does
    /// not know them — which is an ANSWER, and the caller turns it into the
    /// empty set rather than an outage.
    fn claim(
        &self,
        subject: &Subject,
    ) -> impl Future<Output = Result<String, ClaimUnavailable>> + Send;
}

/// One cached answer.
#[derive(Debug, Clone, Copy)]
struct Cached {
    scopes: ScopeSet,
    fetched_at: UnixMillis,
}

/// Live capabilities, cached against the provider.
#[derive(Debug)]
pub struct ProviderCapabilities<S> {
    source: Arc<S>,
    clock: Arc<dyn Clock>,
    cache: moka::future::Cache<Box<str>, Cached>,
    ttl_ms: i64,
    ceiling_ms: i64,
}

// Hand-written rather than derived, and the difference is load-bearing.
// `#[derive(Clone)]` would add an `S: Clone` bound that the fields do not need
// — the source is behind an `Arc` — so a perfectly shareable resolver over a
// non-cloneable claim source would fail to clone for a reason nothing in the
// struct explains. Every field here is a handle, and a clone shares the CACHE
// rather than duplicating it, which is the property that makes handing one to
// each credential plane correct.
impl<S> Clone for ProviderCapabilities<S> {
    fn clone(&self) -> Self {
        Self {
            source: Arc::clone(&self.source),
            clock: Arc::clone(&self.clock),
            cache: self.cache.clone(),
            ttl_ms: self.ttl_ms,
            ceiling_ms: self.ceiling_ms,
        }
    }
}

impl<S: ClaimSource> ProviderCapabilities<S> {
    /// Builds a resolver with the documented windows.
    #[must_use]
    pub fn new(source: S, clock: Arc<dyn Clock>) -> Self {
        Self::with_windows(source, clock, DEFAULT_TTL_MS, DEFAULT_STALE_CEILING_MS)
    }

    /// Builds a resolver with explicit windows, for tests and for an operator
    /// who has a reason.
    #[must_use]
    pub fn with_windows(source: S, clock: Arc<dyn Clock>, ttl_ms: i64, ceiling_ms: i64) -> Self {
        let ceiling = u64::try_from(ceiling_ms.max(0)).unwrap_or(u64::MAX);
        Self {
            source: Arc::new(source),
            clock,
            cache: moka::future::Cache::builder()
                .max_capacity(MAX_CACHED_SUBJECTS)
                // A reclaim backstop, never a decision: an entry past the
                // ceiling can no longer be served, so letting it linger until
                // capacity pressure would only waste a slot. Both windows are
                // still decided from `fetched_at` against the injected clock.
                .time_to_live(Duration::from_millis(ceiling))
                .build(),
            ttl_ms,
            ceiling_ms,
        }
    }

    /// The claim source, for a test to count provider calls against.
    #[must_use]
    pub fn source(&self) -> &S {
        &self.source
    }

    /// Resolves `subject`, asking the provider only when it must.
    async fn resolve(&self, subject: &Subject) -> Result<ScopeSet, Unavailable> {
        let key: Box<str> = subject.as_str().into();
        let held = self.cache.get(&key).await;

        if let Some(entry) = held
            && self.age(entry) <= self.ttl_ms
        {
            return Ok(entry.scopes);
        }

        // Past the freshness window, the entry must be REMOVED before the
        // flight. `try_get_with` returns a present value without running its
        // initialiser, and moka's own expiry is set to the ceiling rather than
        // the freshness window — so leaving it in place would serve a stale
        // answer for fifteen minutes and never re-ask, which is the opposite of
        // what both windows are for.
        //
        // Removing it does not lose the stale-serve: `held` is a copy taken
        // above, and the outage path below reads that rather than the cache.
        if held.is_some() {
            self.cache.invalidate(&key).await;
        }

        // One flight per subject. Concurrent misses for the same key await the
        // same fetch, so the `seq` ordering the Zig resolver needs cannot
        // arise: there is no second response to be out of order with.
        let fetched = {
            let source = Arc::clone(&self.source);
            let clock = Arc::clone(&self.clock);
            let subject = subject.clone();
            self.cache
                .try_get_with(key, async move {
                    let claim = source.claim(&subject).await?;
                    Ok::<_, ClaimUnavailable>(Cached {
                        // The same parser every credential shape feeds, so the
                        // three cannot drift in how a claim becomes a set.
                        scopes: parse_claim(&claim),
                        fetched_at: clock.now(),
                    })
                })
                .await
        };

        match fetched {
            Ok(entry) => Ok(entry.scopes),
            Err(err) if *err == ClaimUnavailable::UnknownSubject => {
                // Deliberately not cached: a deletion is permanent and needs no
                // cache, while a transient miss must not blank a live operator
                // for a whole freshness window.
                let subject = subject.as_str().to_owned();
                tracing::warn!(
                    subject,
                    event = "scopes_subject_unknown_to_provider",
                    "resolving to no capabilities"
                );
                Ok(ScopeSet::EMPTY)
            }
            Err(_unreachable) => self.serve_stale_or_refuse(subject, held),
        }
    }

    /// The outage path: a warm-enough entry, or a refusal.
    fn serve_stale_or_refuse(
        &self,
        subject: &Subject,
        held: Option<Cached>,
    ) -> Result<ScopeSet, Unavailable> {
        let Some(entry) = held.filter(|entry| self.age(*entry) <= self.ceiling_ms) else {
            let subject = subject.as_str().to_owned();
            tracing::error!(
                subject,
                event = "scopes_unavailable",
                "no warm entry and the provider is unreachable"
            );
            return Err(Unavailable);
        };
        let subject = subject.as_str().to_owned();
        let ceiling_ms = self.ceiling_ms;
        tracing::warn!(
            subject,
            ceiling_ms,
            event = "scopes_served_stale",
            "the provider is unreachable and the entry is within the ceiling"
        );
        Ok(entry.scopes)
    }

    /// How old `entry` is, by the injected clock.
    fn age(&self, entry: Cached) -> i64 {
        self.clock.now().saturating_millis_since(entry.fetched_at)
    }
}

impl<S: ClaimSource> CapabilitySource for ProviderCapabilities<S> {
    fn capabilities(
        &self,
        subject: &Subject,
    ) -> impl Future<Output = Result<ScopeSet, Unavailable>> + Send {
        let subject = subject.clone();
        async move { self.resolve(&subject).await }
    }
}
