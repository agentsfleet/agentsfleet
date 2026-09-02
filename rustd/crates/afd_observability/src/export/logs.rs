//! The same drop count for the log signal, minus the one thing it cannot do.
//!
//! # Why logs get a counter and not a warning
//!
//! [`super::CountingExporter`] and the metric exporter beside it both count a
//! failed batch AND say so through `tracing`. This one only counts. A warning
//! raised here becomes a log record, the subscriber hands that record to the
//! batch processor, and the processor hands it back to this exporter — which
//! is failing, which would warn again. A collector that stays down would turn
//! that into a feedback loop that grows while nobody is looking, which is the
//! failure telemetry is forbidden to cause.
//!
//! So the count IS the report: `agentsfleet_otlp_entries_discarded_total` with
//! `signal="logs"`, on the same family and the same two labels the other two
//! signals use. Without it the log signal was the one third of the export that
//! could lose everything and show nothing, while `Signal::Logs` sat in the
//! label set with no producer.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::logs::{LogBatch, LogExporter};

use crate::metrics::label::http::Signal;

/// How many log records this process has failed to export.
///
/// Its own type rather than [`super::SpanDrops`], because this crate already
/// gives each signal the counter its failure shape needs — the span counter
/// totals records, the metric counter also tracks a consecutive run because a
/// lost cycle is only alarming in sequence. A log batch fails like a span
/// batch, so this is that shape and not the metric one.
#[derive(Debug, Clone, Default)]
pub struct LogDrops(Arc<AtomicUsize>);

impl LogDrops {
    /// A counter at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records lost so far.
    ///
    /// `Relaxed`, deliberately: a monotonic counter read for reporting, with
    /// nothing branching on it.
    #[must_use]
    pub fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }

    /// Records `records` lost.
    fn add(&self, records: usize) {
        self.0.fetch_add(records, Ordering::Relaxed);
    }
}

/// Wraps a log exporter, counting the records it fails to deliver.
///
/// Transparent otherwise: shutdown reaches the real exporter, because a
/// wrapper that swallowed it would turn a clean shutdown into a silent loss of
/// whatever was still buffered.
#[derive(Debug)]
pub struct CountingLogExporter<E> {
    inner: E,
    drops: LogDrops,
}

impl<E: LogExporter> CountingLogExporter<E> {
    /// Wraps `inner`, starting the count at zero.
    #[must_use]
    pub fn new(inner: E) -> Self {
        Self {
            inner,
            drops: LogDrops::new(),
        }
    }

    /// A handle on this exporter's drop count.
    #[must_use]
    pub fn drops(&self) -> LogDrops {
        self.drops.clone()
    }
}

impl<E: LogExporter> LogExporter for CountingLogExporter<E> {
    async fn export(&self, batch: LogBatch<'_>) -> OTelSdkResult {
        // Walked rather than read: the SDK's `LogBatch::len` is `cfg(test)`,
        // so a count is one pass over borrowed references. Taken BEFORE the
        // export, because the batch is moved into it.
        //
        // Counted as RECORDS and not batches, for the reason the span exporter
        // gives: batch sizes move with load, so a batch count is a number an
        // operator cannot compare against anything.
        let records = batch.iter().count();
        let outcome = self.inner.export(batch).await;

        if let Err(ref failure) = outcome {
            self.drops.add(records);
            crate::producers::http::export_discarded(
                Signal::Logs,
                super::discard_reason(failure),
                records as u64,
            );
        }
        outcome
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn set_resource(&mut self, resource: &Resource) {
        self.inner.set_resource(resource);
    }
}
