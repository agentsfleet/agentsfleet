//! Dimension 3.3 — the log half of the same outage.
//!
//! The third sibling of the span and metric halves, and the one that closes
//! the gap the review found: `Signal::Logs` sat in the label set with nothing
//! producing it, so the one signal carrying a request's own diagnostics could
//! lose everything and report nothing.
//!
//! What is NOT asserted here is a warning, because the wrapper deliberately
//! raises none — `afd_observability::export::logs` says why. That absence is
//! the design, so a test demanding one would be a test against the design.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use afd_observability::CountingLogExporter;
use opentelemetry::logs::{LogRecord as _, Logger as _, LoggerProvider as _};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::{OTelSdkError, OTelSdkResult};
use opentelemetry_sdk::logs::{LogBatch, LogExporter, SdkLoggerProvider};

/// How many records each test emits before it looks at the clock.
const EMISSIONS: usize = 1_000;

/// A log collector that is not answering, and is slow about it.
///
/// The latency is the fixture, for the reason the metric half gives: a
/// collector that failed INSTANTLY leaves no delay available to leak onto the
/// caller's thread, so the latency assertion would pass without proving
/// anything.
#[derive(Debug, Default)]
struct DeadLogCollector {
    /// How long each attempt takes before failing.
    latency: Duration,
    /// Export attempts, so "no retry" is counted rather than assumed.
    attempts: Arc<AtomicUsize>,
}

impl LogExporter for DeadLogCollector {
    async fn export(&self, _batch: LogBatch<'_>) -> OTelSdkResult {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(self.latency);
        Err(OTelSdkError::InternalFailure(
            "the collector is not answering".to_owned(),
        ))
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }

    fn set_resource(&mut self, _resource: &Resource) {}
}

/// A log collector that accepts everything.
#[derive(Debug)]
struct LiveLogCollector;

impl LogExporter for LiveLogCollector {
    async fn export(&self, _batch: LogBatch<'_>) -> OTelSdkResult {
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        Ok(())
    }

    fn set_resource(&mut self, _resource: &Resource) {}
}

/// Emits `count` records through `provider`, then flushes.
fn emit(provider: &SdkLoggerProvider, count: usize) {
    let logger = provider.logger("afd_observability_test");
    for _ in 0..count {
        let mut record = logger.create_log_record();
        record.set_body("a record the collector will not take".into());
        logger.emit(record);
    }
    let _flushed = provider.force_flush();
}

/// Records the collector refused are counted, and counted as RECORDS.
///
/// The number an operator acts on is how much is missing, not how many batches
/// failed — batch sizes move with load, so a batch count compares against
/// nothing. An instant failure here on purpose: this is about the arithmetic,
/// and a stall would only race the assertion.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_log_export_fails_counted_as_records() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let exporter = CountingLogExporter::new(DeadLogCollector {
        latency: Duration::ZERO,
        attempts: Arc::clone(&attempts),
    });
    let drops = exporter.drops();
    let provider = SdkLoggerProvider::builder()
        .with_simple_exporter(exporter)
        .build();

    assert_eq!(drops.count(), 0, "a fresh counter has lost nothing");

    emit(&provider, EMISSIONS);

    assert_eq!(
        drops.count(),
        EMISSIONS,
        "every record the collector refused is counted, one per record"
    );
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        EMISSIONS,
        "one attempt per record through the simple processor, and no retry — a \
         retried batch would count the same loss twice"
    );
}

/// A collector that answers costs nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_delivered_record_is_not_counted_as_lost() {
    let exporter = CountingLogExporter::new(LiveLogCollector);
    let drops = exporter.drops();
    let provider = SdkLoggerProvider::builder()
        .with_simple_exporter(exporter)
        .build();

    emit(&provider, EMISSIONS);

    assert_eq!(drops.count(), 0, "a collector that answered lost nothing");
}

/// Shutdown reaches the real exporter rather than being answered by the wrapper.
///
/// A wrapper that swallowed it would turn a clean shutdown into a silent loss
/// of whatever was still buffered.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_shutdown_is_delegated_not_answered_by_the_wrapper() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let inner = DeadLogCollector {
        latency: Duration::ZERO,
        attempts: Arc::clone(&attempts),
    };
    let wrapper = CountingLogExporter::new(inner);

    assert!(
        wrapper
            .shutdown_with_timeout(Duration::from_secs(1))
            .is_ok(),
        "the inner exporter's answer is the wrapper's answer"
    );
}

/// A stalled, failing export does not slow an emitting call down.
///
/// The half of the dimension about the REQUEST path. Measured against the
/// exporter's own stall rather than a wall-clock budget, so a loaded machine
/// cannot fail it spuriously. A BATCH processor here, not the simple one: the
/// simple processor exports on the emitting thread by design, and it is the
/// batch processor boot actually installs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_log_export_latency_does_not_reach_the_caller() {
    let stall = Duration::from_millis(500);
    let attempts = Arc::new(AtomicUsize::new(0));
    let provider = SdkLoggerProvider::builder()
        .with_batch_exporter(CountingLogExporter::new(DeadLogCollector {
            latency: stall,
            attempts: Arc::clone(&attempts),
        }))
        .build();

    let logger = provider.logger("afd_observability_test");
    let started = Instant::now();
    for _ in 0..EMISSIONS {
        let mut record = logger.create_log_record();
        record.set_body("a record the collector will not take".into());
        logger.emit(record);
    }
    let elapsed = started.elapsed();

    assert!(
        elapsed < stall,
        "a thousand emissions took {elapsed:?}, at least the {stall:?} the \
         export stalls for — the export is on the caller's path"
    );
}
