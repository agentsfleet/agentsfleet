//! Per-replica caches on the lease path, each keyed by a version a statement
//! in the same pass read (M245 §3, Invariant 1).
//!
//! # Why a version key, and never a time to live
//!
//! A fleet's config is what the money gates price and the execution policy is
//! built from, and an edit must reach the NEXT run on every replica. A cache
//! that expired on a timer would make that "some later run", with a window
//! nobody can see. Keyed by a revision id instead, a cached entry can never
//! answer for a different document: `fleet.revisions` is append-only by
//! trigger, so one id names one document for good, and a new head revision
//! is a new key, which misses. No signal crosses between replicas, because
//! none is needed.
//!
//! # Keys come from statements
//!
//! [`RevisionKey::observed`] is the only constructor, and only the lease
//! path's row decoders call it with an id their statement just returned. A
//! lookup by a bare fleet id does not compile. A full cache evicts the least
//! recently used entry; an evicted entry is parsed again, never a fault.

use std::sync::Arc;

use af_core::id::Uuid7;
use af_fleet_runtime::Definition;
use af_observability::metrics::label::fleet::{CacheOutcome, LeaseCache};
use af_observability::producers::fleet::cache::looked_up;
use moka::future::Cache;
use moka::policy::EvictionPolicy;

use afd_billing::rates::Posture;

use crate::error::Result;
use crate::lease::usage::Pricing;

#[cfg(test)]
mod tests;

/// The bytes of setup documents one replica's definition cache weighs.
///
/// A performance parameter: at 64 MiB a replica holds every definition of a
/// few thousand busy fleets, and a fleet past it is parsed again.
pub const DEFINITION_CAPACITY_BYTES: u64 = 64 * 1024 * 1024;

/// A revision id exactly as a lease-path statement returned it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RevisionKey(Uuid7);

impl RevisionKey {
    /// The key for `id`, which the caller's statement returned in this pass.
    pub(in crate::lease) const fn observed(id: Uuid7) -> Self {
        Self(id)
    }
}

/// A parsed definition and the size of the document it came from.
#[derive(Debug)]
struct Parsed {
    definition: Arc<Definition>,
    bytes: u32,
}

/// Parsed setup definitions, by the revision each was read from.
#[derive(Debug, Clone)]
pub struct Definitions {
    entries: Cache<RevisionKey, Arc<Parsed>>,
}

impl Default for Definitions {
    fn default() -> Self {
        Self::with_capacity(DEFINITION_CAPACITY_BYTES)
    }
}

impl Definitions {
    /// A cache weighing at most `bytes` of documents.
    #[must_use]
    pub fn with_capacity(bytes: u64) -> Self {
        Self {
            entries: Cache::builder()
                .max_capacity(bytes)
                .weigher(|_key, parsed: &Arc<Parsed>| parsed.bytes)
                .eviction_policy(EvictionPolicy::lru())
                .build(),
        }
    }

    /// The definition revision `key` holds, parsed from `document` on a miss.
    ///
    /// `parse` runs only on a miss, and a document that fails it is not
    /// cached: the next lease reads and refuses it again, as it does today.
    ///
    /// # Errors
    /// Whatever `parse` returns for a document it refuses.
    pub async fn get_or_parse(
        &self,
        key: &RevisionKey,
        document: &str,
        parse: impl FnOnce(&str) -> Result<Definition>,
    ) -> Result<Arc<Definition>> {
        if let Some(parsed) = self.entries.get(key).await {
            looked_up(LeaseCache::Definition, CacheOutcome::Hit);
            return Ok(Arc::clone(&parsed.definition));
        }
        looked_up(LeaseCache::Definition, CacheOutcome::Miss);
        let definition = Arc::new(parse(document)?);
        let parsed = Arc::new(Parsed {
            definition: Arc::clone(&definition),
            bytes: u32::try_from(document.len()).unwrap_or(u32::MAX),
        });
        self.entries.insert(key.clone(), parsed).await;
        Ok(definition)
    }

    /// The definition `key` holds, if the cache has it. A hit is counted here;
    /// on a miss the caller reads the document and parses it through
    /// [`Self::get_or_parse`], which counts the miss.
    pub(crate) async fn get(&self, key: &RevisionKey) -> Option<Arc<Definition>> {
        let found = self.entries.get(key).await;
        if found.is_some() {
            looked_up(LeaseCache::Definition, CacheOutcome::Hit);
        }
        found.map(|parsed| Arc::clone(&parsed.definition))
    }

    /// Applies pending evictions, so a test reads the cache's settled state.
    #[cfg(test)]
    async fn settle(&self) {
        self.entries.run_pending_tasks().await;
    }
}

/// The price books one replica holds: a host's renewal prices every lease of
/// one model from one read, and the next tick reads none (M245 §3, §7).
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
        Self {
            entries: Cache::builder()
                .max_capacity(PRICE_BOOK_CAPACITY)
                .eviction_policy(EvictionPolicy::lru())
                .build(),
        }
    }
}

impl PriceBooks {
    /// The pricing for `posture`, `provider` and `model` at `catalogue_version`,
    /// read on a miss.
    ///
    /// A read that failed is not kept: it prices this pass with an empty book,
    /// logged as the per-lease path logs it, and the next pass reads again. A
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
    /// credit gate's, answers it (M245 §9).
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
    ) -> Result<Arc<Pricing>> {
        let key = PriceKey {
            posture: posture.as_str(),
            provider: provider.to_owned(),
            model: model.to_owned(),
            catalogue_version: catalogue_version.filter(|_| posture == Posture::Platform),
        };
        if let Some(pricing) = self.entries.get(&key).await {
            looked_up(LeaseCache::PriceBook, CacheOutcome::Hit);
            return Ok(pricing);
        }
        looked_up(LeaseCache::PriceBook, CacheOutcome::Miss);
        let pricing = Arc::new(Pricing::try_load(database, posture, provider, model).await?);
        self.entries.insert(key, Arc::clone(&pricing)).await;
        Ok(pricing)
    }
}

/// What a price book read for a host's renewal names in its log line, which
/// serves every lease of its model.
pub(crate) const LABEL_BATCH: &str = "renew_batch";
