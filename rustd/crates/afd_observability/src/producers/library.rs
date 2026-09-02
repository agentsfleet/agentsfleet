//! What one authenticated catalogue read records, stage by stage.
//!
//! Two of the seven families have no producer here and say so by name in
//! `crate::metrics::produced`: the response cache is a declared non-port, so no
//! read consults one, and the connection acquire happens inside the store where
//! a read path cannot see how it ended.
//!
//! Stage timing is recorded as two counters — elapsed and observations —
//! rather than one summary, because `rate(duration)/rate(observations)` is the
//! mean cost of a STAGE and `sql` fires twice per registry read. Dividing by
//! requests instead would halve its apparent cost, which is the one number
//! anybody reads this family for.

use std::time::Instant;

use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;

use crate::error::Result;
use crate::metrics::declared::library as declared;
use crate::metrics::instrument::Instruments;
use crate::metrics::label::library::{ReadOutcome, Stage, Surface};
use crate::producers::installed;
use crate::semconv;

/// The instruments a catalogue read records through.
#[derive(Debug)]
pub struct Handles {
    stage_seconds: Counter<f64>,
    stage_observations: Counter<u64>,
    read_outcomes: Counter<u64>,
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
                .counter_f64(&declared::LIBRARY_STAGE_DURATION_SECONDS_TOTAL)?,
            stage_observations: instruments
                .counter_u64(&declared::LIBRARY_STAGE_OBSERVATIONS_TOTAL)?,
            read_outcomes: instruments.counter_u64(&declared::LIBRARY_READ_OUTCOME_TOTAL)?,
            payload_bytes: instruments.counter_u64(&declared::LIBRARY_PAYLOAD_BYTES_TOTAL)?,
            results: instruments.counter_u64(&declared::LIBRARY_RESULTS_TOTAL)?,
        })
    }
}

/// Runs `work`, recording how long it took as one stage of one read.
///
/// A wrapper rather than a start/stop pair at the call site, because a stage
/// that returned early between the two would be timed as having taken the rest
/// of the request — and every early return in a read path is an error path,
/// which is exactly where the timing would mislead.
pub async fn timed<T, F>(surface: Surface, stage: Stage, work: F) -> T
where
    F: Future<Output = T>,
{
    let started = Instant::now();
    let done = work.await;
    stage_observed(surface, stage, started.elapsed());
    done
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

/// Records how many rows one read answered with.
pub fn read_served(surface: Surface, rows: u64) {
    if let Some(producers) = installed() {
        producers.library.results.add(
            rows,
            &[KeyValue::new(semconv::LABEL_SURFACE, surface.as_str())],
        );
    }
}

/// Records how many bytes one read wrote.
///
/// Separate from [`read_served`] rather than one call, because not every
/// surface can answer both: a read that hands its page to a JSON responder
/// knows its row count and never sees the bytes, and passing a zero there
/// would be a measurement nobody took reported as a small payload.
pub fn payload_served(surface: Surface, bytes: u64) {
    if let Some(producers) = installed() {
        producers.library.payload_bytes.add(
            bytes,
            &[KeyValue::new(semconv::LABEL_SURFACE, surface.as_str())],
        );
    }
}
