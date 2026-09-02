//! What one authenticated catalogue read records, stage by stage.
//!
//! Stage timing is recorded as two counters — elapsed and observations —
//! rather than one summary, because `rate(duration)/rate(observations)` is the
//! mean cost of a STAGE and `sql` fires twice per registry read. Dividing by
//! requests instead would halve its apparent cost, which is the one number
//! anybody reads this family for.

use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;

use crate::error::Result;
use crate::metrics::declared::library as declared;
use crate::metrics::instrument::Instruments;
use crate::metrics::label::library::{CacheOutcome, PoolResult, ReadOutcome, Stage, Surface};
use crate::producers::installed;
use crate::semconv;

/// The instruments a catalogue read records through.
#[derive(Debug)]
pub struct Handles {
    stage_seconds: Counter<f64>,
    stage_observations: Counter<u64>,
    read_outcomes: Counter<u64>,
    pool_results: Counter<u64>,
    cache_outcomes: Counter<u64>,
    payload_bytes: Counter<u64>,
    results: Counter<u64>,
}

impl Handles {
    /// Claims every instrument this domain records through.
    ///
    /// # Errors
    ///
    /// Whatever [`Instruments`] refuses — see [`super::install`].
    pub(super) fn claim(instruments: &Instruments) -> Result<Self> {
        Ok(Self {
            stage_seconds: instruments
                .counter_f64(declared::LIBRARY_STAGE_DURATION_SECONDS_TOTAL)?,
            stage_observations: instruments
                .counter_u64(declared::LIBRARY_STAGE_OBSERVATIONS_TOTAL)?,
            read_outcomes: instruments.counter_u64(declared::LIBRARY_READ_OUTCOME_TOTAL)?,
            pool_results: instruments.counter_u64(declared::LIBRARY_POOL_RESULT_TOTAL)?,
            cache_outcomes: instruments.counter_u64(declared::LIBRARY_CACHE_OUTCOME_TOTAL)?,
            payload_bytes: instruments.counter_u64(declared::LIBRARY_PAYLOAD_BYTES_TOTAL)?,
            results: instruments.counter_u64(declared::LIBRARY_RESULTS_TOTAL)?,
        })
    }
}

/// Records one completed stage of one read.
///
/// The two families move together by construction: a stage that recorded its
/// elapsed time and not its observation would inflate every mean taken over it.
pub fn stage_observed(surface: Surface, stage: Stage, elapsed: core::time::Duration) {
    if let Some(producers) = installed() {
        let attributes = [
            KeyValue::new(semconv::LABEL_SURFACE, surface.as_str()),
            KeyValue::new(semconv::LABEL_STAGE, stage.as_str()),
        ];
        producers
            .library
            .stage_seconds
            .add(elapsed.as_secs_f64(), &attributes);
        producers.library.stage_observations.add(1, &attributes);
    }
}

/// Records how one read ended.
///
/// Exactly once per request, on every exit path. The default at the call site
/// is [`ReadOutcome::InternalError`], so a path that ends without classifying
/// itself surfaces as something to investigate rather than as a success.
pub fn read_finished(surface: Surface, outcome: ReadOutcome) {
    if let Some(producers) = installed() {
        producers.library.read_outcomes.add(
            1,
            &[
                KeyValue::new(semconv::LABEL_SURFACE, surface.as_str()),
                KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str()),
            ],
        );
    }
}

/// Records how one pool acquisition ended.
///
/// Carries no surface: a starving pool is a process-wide fact, and attributing
/// it per surface would invite a reader to blame one catalogue for it.
pub fn pool_acquired(result: PoolResult) {
    if let Some(producers) = installed() {
        producers.library.pool_results.add(
            1,
            &[KeyValue::new(semconv::LABEL_POOL_RESULT, result.as_str())],
        );
    }
}

/// Records what the catalogue cache did for a read that consulted one.
///
/// A read that consulted no cache calls this not at all — absence rather than
/// a member, so the ratio this family exists for stays a ratio of decisions.
pub fn cache_consulted(outcome: CacheOutcome) {
    if let Some(producers) = installed() {
        producers
            .library
            .cache_outcomes
            .add(1, &[KeyValue::new(semconv::LABEL_CACHE, outcome.as_str())]);
    }
}

/// Records what one read answered with.
pub fn read_served(surface: Surface, rows: u64, bytes: u64) {
    if let Some(producers) = installed() {
        let attributes = [KeyValue::new(semconv::LABEL_SURFACE, surface.as_str())];
        producers.library.results.add(rows, &attributes);
        producers.library.payload_bytes.add(bytes, &attributes);
    }
}
