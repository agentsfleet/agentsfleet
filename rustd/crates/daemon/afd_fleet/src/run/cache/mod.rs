//! Per-replica caches on the run path, each keyed by a version a statement
//! in the same pass read.
//!
//! # Why a version key, and never a time to live
//!
//! A price edit must reach the NEXT run on every replica. A cache that expired
//! on a timer would make that "some later run", with a window nobody can see.
//! Keyed by the catalogue version instead, a cached book can never answer for
//! a different catalogue: an edit moves the version, which misses. No signal
//! crosses between replicas, because none is needed.

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use af_observability::metrics::label::fleet::{CacheOutcome, RunCache};
use af_observability::producers::fleet::cache::looked_up;
use moka::future::Cache;
use moka::policy::EvictionPolicy;

use afd_billing::rates::Posture;

use crate::error::Result;
use crate::run::usage::Pricing;

/// The price books one replica holds: a host's renewal prices every run of
/// one model from one read, and the next tick reads none.
pub const PRICE_BOOK_CAPACITY: u64 = 1_024;

/// A model's price book, by the catalogue version it was read at: an edit
/// moves the version, so a cached book is never served past it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PriceKey {
    posture: &'static str,
    provider: String,
    model: String,
    catalogue_version: Option<i64>,
}

/// Price books by model and catalogue version.
#[derive(Debug, Clone)]
pub struct PriceBooks {
    entries: Cache<PriceKey, Arc<Pricing>>,
}

impl Default for PriceBooks {
    fn default() -> Self {
        Self::with_capacity(PRICE_BOOK_CAPACITY)
    }
}

impl PriceBooks {
    /// A cache holding at most `books` price books.
    #[must_use]
    pub fn with_capacity(books: u64) -> Self {
        Self {
            entries: Cache::builder()
                .max_capacity(books)
                .eviction_policy(EvictionPolicy::lru())
                .build(),
        }
    }

    /// The pricing for `posture`, `provider` and `model` at `catalogue_version`,
    /// read on a miss.
    ///
    /// A read that failed is not kept: it prices this pass with an empty book,
    /// logged as the per-run path logs it, and the next pass reads again. A
    /// book read is kept under the version asked for, even when it is empty:
    /// a model with no prices is an answer, and an edit moves the version.
    pub(crate) async fn get_or_load(
        &self,
        database: &afd_db::Db,
        posture: Posture,
        provider: &str,
        model: &str,
        catalogue_version: Option<i64>,
        label: &str,
    ) -> Arc<Pricing> {
        match self
            .try_get_or_load(database, posture, provider, model, catalogue_version)
            .await
        {
            Ok(pricing) => pricing,
            Err(_fault) => Arc::new(Pricing::load(database, posture, provider, model, label).await),
        }
    }

    /// The pricing for `posture`, `provider` and `model` at
    /// `catalogue_version`, or the fault that kept a missed book from being
    /// read, which is never kept: a caller with its own fault posture, the
    /// credit gate's, answers it.
    ///
    /// Concurrent misses on one key share one read: a replica's runners start
    /// together, and each would otherwise read the same book. The fault is
    /// shared the same way, so it comes back behind an `Arc`.
    ///
    /// # Errors
    /// A datastore that would not answer, or a catalogue row it cannot read.
    pub(crate) async fn try_get_or_load(
        &self,
        database: &afd_db::Db,
        posture: Posture,
        provider: &str,
        model: &str,
        catalogue_version: Option<i64>,
    ) -> Result<Arc<Pricing>, Arc<crate::Error>> {
        let key = PriceKey {
            posture: posture.as_str(),
            provider: provider.to_owned(),
            model: model.to_owned(),
            catalogue_version: catalogue_version.filter(|_| posture == Posture::Platform),
        };
        // Boxed: a cache future is large, and every run path awaits this.
        let looked_up_at = key.catalogue_version;
        let entry = Box::pin(self.entries.entry(key).or_try_insert_with(async {
            Pricing::try_load(database, posture, provider, model)
                .await
                .map(|pricing| Arc::new(pricing.looked_up_at(looked_up_at)))
        }))
        .await?;
        let outcome = if entry.is_fresh() {
            CacheOutcome::Miss
        } else {
            CacheOutcome::Hit
        };
        looked_up(RunCache::PriceBook, outcome);
        Ok(entry.into_value())
    }
}

/// What a price book read for a host's renewal names in its log line, which
/// serves every run of its model.
pub(crate) const LABEL_BATCH: &str = "renew_batch";

/// A catalogue version no statement has answered yet.
const UNKNOWN: i64 = i64::MIN;

/// The newest catalogue version this replica has read.
///
/// Every assignment reads it, so a host's renewal frame can name it without
/// its pre-read. The renewal statement guards on it, so a stale value writes
/// nothing; the run takes the per-run path, and the version is forgotten
/// until a read answers it again.
///
/// One pointer wide: every run store carries it, and the store rides inside
/// futures clippy bounds by size.
#[derive(Debug, Clone, Default)]
pub struct Latest(Arc<Catalogue>);

/// What [`Latest`] shares between its clones.
#[derive(Debug)]
struct Catalogue(AtomicI64);

impl Default for Catalogue {
    fn default() -> Self {
        Self(AtomicI64::new(UNKNOWN))
    }
}

impl Latest {
    /// A statement answered the catalogue as `catalogue`. Versions only move
    /// forward, so the larger is kept.
    pub(crate) fn observe(&self, catalogue: i64) {
        self.0.0.fetch_max(catalogue, Ordering::Relaxed);
    }

    /// The catalogue version, when one is known.
    pub(crate) fn known(&self) -> Option<i64> {
        Some(self.0.0.load(Ordering::Relaxed)).filter(|catalogue| *catalogue != UNKNOWN)
    }

    /// A guard found the version moved: it is not named again until a read
    /// answers it.
    pub(crate) fn forget(&self) {
        self.0.0.store(UNKNOWN, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::Latest;

    #[test]
    fn the_catalogue_version_only_moves_forward_until_forgotten() {
        let latest = Latest::default();
        assert_eq!(latest.known(), None);

        latest.observe(7);
        latest.observe(3);
        assert_eq!(latest.known(), Some(7));

        latest.forget();
        assert_eq!(latest.known(), None);
    }
}
