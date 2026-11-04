//! Verified callbacks are counted once per answer; inventory arrives from a bounded sampler.
use crate::error::Result;
use crate::metrics::declared::scheduler as declared;
use crate::metrics::instrument::{Instruments, Reading};
use crate::metrics::label::scheduler::{FireOutcome, ScheduleState, SyncOutcome};
use crate::metrics::observed::Observed;
use crate::producers::installed;
use crate::semconv;
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Histogram};
use std::time::{Duration, Instant};

static COUNTS: [Observed; 7] = [const { Observed::new() }; 7];

/// The scheduler's instruments.
#[derive(Debug)]
pub struct Handles {
    fires: Counter<u64>,
    callbacks: Counter<u64>,
    duration: Histogram<f64>,
    lag: Histogram<f64>,
    syncs: Counter<u64>,
}
impl Handles {
    pub(super) fn claim(instruments: &Instruments) -> Result<Self> {
        instruments.gauge_u64(&declared::SCHEDULES, || {
            ScheduleState::ALL
                .iter()
                .zip(&COUNTS)
                .filter_map(|(state, count)| {
                    count.load().map(|value| Reading {
                        attributes: vec![KeyValue::new(
                            crate::semconv::LABEL_STATE,
                            state.as_str(),
                        )],
                        value,
                    })
                })
                .collect()
        })?;
        Ok(Self {
            fires: instruments.counter_u64(&declared::FIRES)?,
            callbacks: instruments.counter_u64(&declared::CALLBACKS)?,
            duration: instruments.histogram_f64(&declared::CALLBACK_DURATION)?,
            lag: instruments.histogram_f64(&declared::DISPATCH_LAG)?,
            syncs: instruments.counter_u64(&declared::SYNCS)?,
        })
    }
}
/// A pending verified callback; dropping before an answer records cancellation.
#[derive(Debug)]
pub struct Callback {
    started: Instant,
    outcome: FireOutcome,
}
impl Callback {
    /// Starts after signature validation, so forged requests cannot count as fires.
    #[must_use]
    pub fn begin() -> Self {
        Self {
            started: Instant::now(),
            outcome: FireOutcome::Cancelled,
        }
    }
    /// Classifies a completed answer without attaching callback identities.
    pub fn answered(&mut self, outcome: FireOutcome) {
        self.outcome = outcome;
    }
}
impl Drop for Callback {
    fn drop(&mut self) {
        callback(self.outcome, self.started.elapsed());
    }
}
/// Records one completed or cancelled verified callback.
pub fn callback(outcome: FireOutcome, elapsed: Duration) {
    if let Some(producers) = installed() {
        let labels = [KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())];
        producers.scheduler.callbacks.add(1, &labels);
        producers
            .scheduler
            .duration
            .record(elapsed.as_secs_f64(), &labels);
    }
}
/// Observes a known scheduled instant only for newly accepted work.
pub fn dispatched(scheduled: Option<jiff::Timestamp>, now: jiff::Timestamp) {
    if let Some(producers) = installed()
        && let Some(millis) =
            scheduled.and_then(|at| now.as_millisecond().checked_sub(at.as_millisecond()))
        && let Ok(millis) = u64::try_from(millis)
    {
        producers
            .scheduler
            .lag
            .record(Duration::from_millis(millis).as_secs_f64(), &[]);
    }
}
/// Records a completed reconciliation decision.
pub fn synced(outcome: SyncOutcome) {
    if let Some(producers) = installed() {
        producers.scheduler.syncs.add(
            1,
            &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())],
        );
    }
}
/// Publishes a real database snapshot, or withdraws every count on read failure.
pub fn counts(values: Option<[u64; 7]>) {
    match values {
        Some(values) => {
            for (cell, value) in COUNTS.iter().zip(values) {
                cell.publish(value);
            }
        }
        None => {
            for cell in &COUNTS {
                cell.withdraw();
            }
        }
    }
}

/// Counts the durable admission even if a later one-shot retirement fails.
pub fn fired(replayed: bool) {
    if let Some(producers) = installed() {
        let outcome = if replayed {
            FireOutcome::Replayed
        } else {
            FireOutcome::Accepted
        };
        producers.scheduler.fires.add(
            1,
            &[KeyValue::new(semconv::LABEL_OUTCOME, outcome.as_str())],
        );
    }
}
