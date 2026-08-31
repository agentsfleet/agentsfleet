//! Metric export that cannot slow a recording call down, and says so when it
//! loses a batch.
//!
//! The twin of [`crate::export`], which does this for spans. Same property,
//! same reason: telemetry is the one subsystem whose failure must never become
//! the application's failure.
//!
//! # Why this counts batches where the span wrapper counts spans
//!
//! Not an inconsistency — the two answer different operator questions. A span
//! is a thing an operator lost, so "how many spans" is the actionable number. A
//! metric batch is a COLLECTION CYCLE: every family's current value, gathered
//! at once. Losing one does not lose a quantity, it loses a moment, and the
//! next cycle carries the running total again for every cumulative family.
//! Counting data points would report a number that grows with how many families
//! are declared, which says more about the census than about the outage.
//!
//! So the unit here is cycles missed, and consecutive cycles missed is what a
//! reader should watch: one is a blip the next cycle repairs, and a hundred is
//! a collector that has been unreachable for a hundred intervals.
//!
//! # There is no retry, and that is by omission on purpose
//!
//! `PushMetricExporter` documents that "all retry logic must be contained in
//! this function. The SDK does not implement any retry logic." So writing none
//! IS the no-retry decision — nothing here has to opt out of anything. A retry
//! would double-count delta families, whose whole payload is the window's
//! increment: resending one adds it twice.
//!
//! The reader also never overlaps cycles, so a slow exporter delays the next
//! collection rather than stacking work. There is no queue to bound, which is
//! why this module has none.

use std::fmt::Debug;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::metrics::Temporality;
use opentelemetry_sdk::metrics::data::ResourceMetrics;
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;

#[cfg(test)]
mod tests;

/// How many metric collection cycles this process has failed to export.
///
/// Cloneable, and every clone reads the same numbers — the exporter holds one
/// and whoever reports it holds another.
#[derive(Debug, Clone, Default)]
pub struct BatchDrops {
    failed: Arc<AtomicU64>,
    consecutive: Arc<AtomicU64>,
}

impl BatchDrops {
    /// Counters at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Cycles lost since this process started.
    ///
    /// `Relaxed`, deliberately: a monotonic counter read for reporting, with
    /// nothing branching on it. Ordering it against other memory would buy a
    /// guarantee no reader uses.
    #[must_use]
    pub fn failed(&self) -> u64 {
        self.failed.load(Ordering::Relaxed)
    }

    /// Cycles lost in an unbroken run, reset by the next success.
    ///
    /// The number that separates a blip from an outage. A total alone cannot:
    /// a thousand failures spread over a month and a thousand in the last hour
    /// read identically, and only one of them is happening now.
    #[must_use]
    pub fn consecutive(&self) -> u64 {
        self.consecutive.load(Ordering::Relaxed)
    }

    /// Records a lost cycle, answering the length of the run it extends.
    fn lost(&self) -> u64 {
        self.failed.fetch_add(1, Ordering::Relaxed);
        self.consecutive.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Records a delivered cycle, ending any run.
    fn delivered(&self) {
        self.consecutive.store(0, Ordering::Relaxed);
    }
}

/// Wraps a metric exporter, counting the collection cycles it fails to deliver.
///
/// Transparent otherwise: flush, shutdown and temporality reach the real
/// exporter. Temporality especially — it is how the SDK decides whether a
/// family reports a running total or a window, so a wrapper that answered for
/// itself would silently rewrite every family flowing through it.
#[derive(Debug)]
pub struct CountingMetricExporter<E> {
    inner: E,
    drops: BatchDrops,
}

impl<E: PushMetricExporter> CountingMetricExporter<E> {
    /// Wraps `inner`, starting the count at zero.
    #[must_use]
    pub fn new(inner: E) -> Self {
        Self {
            inner,
            drops: BatchDrops::new(),
        }
    }

    /// A handle on this exporter's drop counts.
    #[must_use]
    pub fn drops(&self) -> BatchDrops {
        self.drops.clone()
    }
}

impl<E: PushMetricExporter> PushMetricExporter for CountingMetricExporter<E> {
    async fn export(&self, metrics: &ResourceMetrics) -> OTelSdkResult {
        let outcome = self.inner.export(metrics).await;

        let Err(ref failure) = outcome else {
            self.drops.delivered();
            return outcome;
        };

        // Hoisted before the macro: the `log` bridge compiles a second copy of
        // every field expression, and llvm-cov scores the copy that never runs.
        let run = self.drops.lost();
        let reason = failure.to_string();
        let total = self.drops.failed();
        tracing::warn!(
            reason,
            consecutive = run,
            total,
            event = "metric_export_failed",
            "metric export failed — a collection cycle was lost, recording unaffected"
        );
        outcome
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    /// The inner exporter's, never this wrapper's own.
    ///
    /// The SDK asks the EXPORTER which temporality it wants and aggregates to
    /// match, which is exactly why the pipeline needs one exporter per
    /// temporality rather than one exporter and a per-family setting.
    fn temporality(&self) -> Temporality {
        self.inner.temporality()
    }
}
