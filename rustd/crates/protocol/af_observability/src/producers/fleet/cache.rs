//! Whether a run-path cache answered a lookup from memory.

use opentelemetry::KeyValue;

use crate::metrics::label::fleet::{CacheOutcome, RunCache};
use crate::producers::installed;
use crate::semconv;

/// Records one lookup in `cache`, by whether it hit.
pub fn looked_up(cache: RunCache, outcome: CacheOutcome) {
    if let Some(producers) = installed() {
        producers.fleet.cache_lookups.add(
            1,
            &[
                KeyValue::new(semconv::LABEL_CACHE, cache.as_str()),
                KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str()),
            ],
        );
    }
}
