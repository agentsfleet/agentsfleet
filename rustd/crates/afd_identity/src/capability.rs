//! Live capability claims with bounded freshness and outage tolerance.
//!
//! Fresh answers avoid provider calls for sixty seconds. A separate stale
//! cache retains the last confirmed answer for at most fifteen minutes, so
//! failed or cancelled refreshes cannot consume the outage fallback.
//!
//! Moka coalesces provider calls per subject. Its atomic entry operation removes
//! only answers still stale when examined, so a queued caller cannot invalidate
//! a refresh that another request just published. An unknown subject clears the
//! stale claim inside that same provider flight before any caller is answered.
//!
//! Both windows use the injected clock. Moka's TTL is a reclamation backstop;
//! serving an answer always checks when the provider actually confirmed it.

use crate::error::{ClaimUnavailable, Result};
use moka::ops::compute::Op;
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
    stale: moka::future::Cache<Box<str>, Cached>,
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
            stale: self.stale.clone(),
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
        let cache = || {
            moka::future::Cache::builder()
                .max_capacity(MAX_CACHED_SUBJECTS)
                .time_to_live(Duration::from_millis(ceiling))
                .build()
        };
        Self {
            source: Arc::new(source),
            clock,
            cache: cache(),
            stale: cache(),
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

        self.cache
            .entry(key.clone())
            .and_compute_with(|entry| async move {
                if entry.is_some_and(|entry| self.age(*entry.value()) > self.ttl_ms) {
                    Op::Remove
                } else {
                    Op::Nop
                }
            })
            .await;

        let fetched = self
            .cache
            .try_get_with(key.clone(), self.fetch(subject, key.as_ref()))
            .await;

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
            Err(_unreachable) => self.serve_stale_or_refuse(subject, self.stale.get(&key).await),
        }
    }

    async fn fetch(&self, subject: &Subject, key: &str) -> Result<Cached, ClaimUnavailable> {
        match self.source.claim(subject).await {
            Ok(claim) => {
                let entry = Cached {
                    scopes: parse_claim(&claim),
                    fetched_at: self.clock.now(),
                };
                self.stale.insert(key.into(), entry).await;
                Ok(entry)
            }
            Err(ClaimUnavailable::UnknownSubject) => {
                self.stale.invalidate(key).await;
                Err(ClaimUnavailable::UnknownSubject)
            }
            Err(error) => Err(error),
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
