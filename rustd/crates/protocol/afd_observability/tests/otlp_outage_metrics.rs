//! Dimension 3.3 — the metric half of the same outage.
//!
//! A sibling module of the span half rather than a second `tests/*.rs`, because
//! it is one claim about one failure and a second file would be a second test
//! binary for the same subject. Files under `tests/<name>/` are not discovered
//! as targets, so both halves compile into the one binary `otlp_outage.rs`
//! declares.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use afd_observability::metrics::export::CountingMetricExporter;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::error::{OTelSdkError, OTelSdkResult};
use opentelemetry_sdk::metrics::data::ResourceMetrics;
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};

/// How many measurements each test records before it looks at the clock.
///
/// Enough that a per-call cost would be visible against the exporter's stall,
/// and small enough to be instant when the recording path is doing its job.
const RECORDINGS: usize = 1_000;

/// A metric collector that is not answering, and is slow about it.
///
/// The latency is the whole fixture. A collector that failed INSTANTLY would
/// let `test_metric_export_fails_counted_never_blocks` pass whether or not the
/// exporter could reach the caller's thread — there would be no delay available
/// to leak. Injecting one is what gives that assertion teeth, exactly as
/// `DeadCollector` does for the span side.
#[derive(Debug, Default)]
struct DeadMetricCollector {
    /// How long each attempt takes before failing.
    latency: Duration,
    /// Export attempts, so "no retry" is counted rather than assumed.
    attempts: Arc<AtomicUsize>,
}

impl PushMetricExporter for DeadMetricCollector {
    // `std::thread::sleep`, not `tokio::time::sleep`, and the difference is not
    // stylistic: `PeriodicReader` collects on a plain thread of its own with no
    // tokio reactor on it, so an awaited timer there panics. Blocking that
    // thread is also the more faithful stall — it is what a synchronous
    // collector call actually does to the reader.
    fn export(&self, _metrics: &ResourceMetrics) -> impl Future<Output = OTelSdkResult> + Send {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(self.latency);
        std::future::ready(Err(OTelSdkError::InternalFailure(
            "the collector is not answering".to_owned(),
        )))
    }

    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }

    fn temporality(&self) -> Temporality {
        Temporality::Delta
    }
}

/// A failed collection cycle is counted once, and attempted exactly once.
///
/// An INSTANT failure here on purpose: this test is about the arithmetic
/// through a real provider and reader, and a stall would only make the counter
/// race the assertion. The latency half is the test below.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_metric_export_fails_counted_never_blocks() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let exporter = CountingMetricExporter::new(DeadMetricCollector {
        latency: Duration::ZERO,
        attempts: Arc::clone(&attempts),
    });
    let drops = exporter.drops();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter).build())
        .build();

    assert_eq!(drops.failed(), 0, "a fresh counter has lost nothing");

    let recorded = provider
        .meter("afd_observability_test")
        .u64_counter("units_of_work")
        .build();
    for _ in 0..RECORDINGS {
        recorded.add(1, &[]);
    }

    let _flushed = provider.force_flush();

    assert_eq!(
        drops.failed(),
        1,
        "one collection cycle was attempted and lost, so one is counted"
    );
    assert_eq!(
        drops.consecutive(),
        1,
        "a single failure is a run of one, which is a blip and not an outage"
    );
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "exactly one attempt per cycle — a retry would double-count every delta \
         family, whose payload is the window's increment"
    );
}

/// A stalled, failing export does not slow a recording call down.
///
/// The half of the dimension about the REQUEST path, and the twin of
/// `test_export_latency_does_not_reach_the_caller` above. Measured against the
/// exporter's own stall rather than a wall-clock budget, so a loaded machine
/// cannot fail it spuriously.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_metric_export_latency_does_not_reach_the_caller() {
    let stall = Duration::from_millis(500);
    let attempts = Arc::new(AtomicUsize::new(0));
    let exporter = CountingMetricExporter::new(DeadMetricCollector {
        latency: stall,
        attempts: Arc::clone(&attempts),
    });
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter).build())
        .build();
    let recorded = provider
        .meter("afd_observability_test")
        .u64_counter("units_of_work")
        .build();

    let started = Instant::now();
    for _ in 0..RECORDINGS {
        recorded.add(1, &[]);
    }
    let elapsed = started.elapsed();

    assert!(
        elapsed < stall,
        "a thousand recordings took {elapsed:?}, at least the {stall:?} the \
         export stalls for — the export is on the caller's path"
    );

    // And the export really was attempted, so the speed above is the reader
    // doing its job rather than the measurements being discarded outright.
    // `attempts` increments BEFORE the stall, which is what makes it readable
    // without waiting the export out.
    let _flushed = provider.force_flush();
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "the measurements reached the exporter, just not on the caller's thread"
    );
}

/// A collector that answers costs nothing, and clears a run left by an outage.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_delivered_cycle_is_not_counted_as_lost() {
    let exporter = CountingMetricExporter::new(LiveMetricCollector);
    let drops = exporter.drops();
    let provider = SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter).build())
        .build();

    provider
        .meter("afd_observability_test")
        .u64_counter("t")
        .build()
        .add(1, &[]);
    let _flushed = provider.force_flush();

    assert_eq!(drops.failed(), 0, "a collector that answered lost nothing");
    assert_eq!(drops.consecutive(), 0);
}

/// A metric collector that accepts everything.
#[derive(Debug)]
struct LiveMetricCollector;

impl PushMetricExporter for LiveMetricCollector {
    fn export(&self, _metrics: &ResourceMetrics) -> impl Future<Output = OTelSdkResult> + Send {
        std::future::ready(Ok(()))
    }

    fn force_flush(&self) -> OTelSdkResult {
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }

    fn temporality(&self) -> Temporality {
        Temporality::Cumulative
    }
}
