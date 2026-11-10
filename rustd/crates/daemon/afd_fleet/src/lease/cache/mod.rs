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

use crate::error::Result;

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

    /// Applies pending evictions, so a test reads the cache's settled state.
    #[cfg(test)]
    async fn settle(&self) {
        self.entries.run_pending_tasks().await;
    }
}
