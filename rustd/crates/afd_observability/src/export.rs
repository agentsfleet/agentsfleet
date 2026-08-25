//! Export that cannot slow a request down, and says so when it loses spans.
//!
//! # The property, and why it is not obvious
//!
//! Telemetry is the one subsystem whose failure must never become the
//! application's failure. An exporter that blocks makes a slow collector look
//! like a slow API, and an exporter that retries forever makes an unreachable
//! collector look like an outage. The Zig daemon states the same rule as a
//! bounded buffer plus a drop counter, and reaching for the same shape here is
//! not a port — it is the only shape that has the property.
//!
//! The bounded buffer is the SDK's batch processor, which hands spans to a
//! background task and drops them when its queue is full. What the SDK does not
//! give is a number an operator can look at, and a system that discards data
//! silently is one that will be trusted while it is lying. This wrapper is that
//! number.
//!
//! # What counts as a drop
//!
//! Spans in a batch the exporter failed to send. That is the loss an operator
//! can act on — a collector that is down, an endpoint that is wrong, a
//! credential that expired — and it is the loss that grows without bound while
//! nobody is looking. A span the batch processor sheds because its queue filled
//! is a different signal, and the SDK counts it in its own internal metrics
//! rather than exposing a hook to count it here.

use std::fmt::Debug;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use opentelemetry_sdk::error::OTelSdkResult;
use opentelemetry_sdk::trace::{SpanData, SpanExporter};

/// How many spans this process has failed to export.
///
/// Cloneable, and every clone reads the same number — the exporter holds one
/// and whoever reports it holds another. `usize` rather than `u64` because a
/// batch's length is a `usize`, and a counter that needed a cast to accept its
/// own input would carry a conversion arm nothing could reach.
#[derive(Debug, Clone, Default)]
pub struct SpanDrops(Arc<AtomicUsize>);

impl SpanDrops {
    /// A counter at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Spans lost so far.
    ///
    /// `Relaxed`, deliberately: this is a monotonic counter read for reporting,
    /// and nothing branches on it. Ordering it against other memory would buy
    /// a guarantee no reader uses.
    #[must_use]
    pub fn count(&self) -> usize {
        self.0.load(Ordering::Relaxed)
    }

    /// Records `spans` lost.
    fn add(&self, spans: usize) {
        self.0.fetch_add(spans, Ordering::Relaxed);
    }
}

/// Wraps an exporter, counting the spans it fails to deliver.
///
/// Transparent otherwise: shutdown and flush reach the real exporter, because a
/// wrapper that swallowed them would turn a clean shutdown into a silent loss
/// of whatever was still buffered.
#[derive(Debug)]
pub struct CountingExporter<E> {
    inner: E,
    drops: SpanDrops,
}

impl<E: SpanExporter> CountingExporter<E> {
    /// Wraps `inner`, starting the count at zero.
    #[must_use]
    pub fn new(inner: E) -> Self {
        Self {
            inner,
            drops: SpanDrops::new(),
        }
    }

    /// A handle on this exporter's drop count.
    #[must_use]
    pub fn drops(&self) -> SpanDrops {
        self.drops.clone()
    }
}

impl<E: SpanExporter> SpanExporter for CountingExporter<E> {
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        // Counted before the move, and counted as SPANS rather than batches: an
        // operator needs to know how much telemetry is missing, and batch sizes
        // vary with load in exactly the way that would make a batch count
        // unreadable.
        let spans = batch.len();
        let outcome = self.inner.export(batch).await;

        if let Err(ref failure) = outcome {
            self.drops.add(spans);
            // Hoisted: the `log` bridge compiles a second copy of every field
            // expression, and llvm-cov scores the copy that never runs.
            let reason = failure.to_string();
            let lost = spans;
            let total = self.drops.count();
            tracing::warn!(
                reason,
                lost,
                total,
                "telemetry export failed — spans dropped, requests unaffected"
            );
        }
        outcome
    }

    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.inner.shutdown_with_timeout(timeout)
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.inner.force_flush()
    }

    fn set_resource(&mut self, resource: &opentelemetry_sdk::Resource) {
        self.inner.set_resource(resource);
    }
}
