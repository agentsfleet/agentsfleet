//! What the lease path's in-process caches label their lookups with.

use crate::metrics::label::closed_set;

closed_set! {
    /// Which of the lease path's in-process caches a lookup asked. Each is
    /// keyed by a version a statement in the same pass read.
    LeaseCache {
        /// A parsed setup definition, keyed by its revision id.
        Definition => "definition",
        /// A model's price tiers, keyed by the catalogue version.
        PriceBook => "price_book",
        /// The platform default's resolution and key, keyed by the
        /// catalogue version, the secret's id and the digest of what it
        /// holds.
        PlatformKey => "platform_key",
    }
}

closed_set! {
    /// Whether a cache lookup answered from memory.
    CacheOutcome {
        /// The entry for this exact version was held.
        Hit => "hit",
        /// No entry for this version; the caller read or parsed it.
        Miss => "miss",
    }
}
