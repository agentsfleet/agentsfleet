//! Dimension 6.2 — a collector that is down costs spans, never requests.
//!
//! No network here, and that is deliberate rather than a shortcut. From this
//! process's point of view "the collector is down" IS "export returns an
//! error", and an exporter that fails on demand makes the assertion
//! deterministic where a real unreachable endpoint would make it a timing
//! question.
//!
//! What IS real is everything between the caller and that exporter: a genuine
//! `SdkTracerProvider` and a genuine batch processor, because those are the
//! pieces doing the work of keeping one off the other. Hand-built `SpanData`
//! would have tested the counter and nothing else.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use afd_observability::{CountingExporter, SpanDrops};
use opentelemetry::Key;
use opentelemetry::trace::{Tracer as _, TracerProvider as _};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::error::{OTelSdkError, OTelSdkResult};
use opentelemetry_sdk::trace::{SdkTracerProvider, SpanData, SpanExporter};

/// An exporter standing in for a collector that is not answering.
#[derive(Debug, Default)]
struct DeadCollector {
    /// How long each attempt takes before failing.
    latency: Duration,
    /// Attempts made, so a test can prove the export was tried at all.
    attempts: Arc<AtomicUsize>,
}

impl SpanExporter for DeadCollector {
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        self.attempts.fetch_add(batch.len(), Ordering::SeqCst);
        tokio::time::sleep(self.latency).await;
        Err(OTelSdkError::InternalFailure(
            "the collector is not answering".to_owned(),
        ))
    }
}

/// An exporter that accepts everything, and remembers the resource it was told.
#[derive(Debug, Default)]
struct LiveCollector {
    /// The `service.name` the provider handed down, if it handed one.
    service_name: Arc<Mutex<Option<String>>>,
    /// How many times a flush reached this exporter.
    flushes: Arc<AtomicUsize>,
}

impl SpanExporter for LiveCollector {
    // Not `async fn`: there is nothing to await, and the trait returns an
    // `impl Future` so an exporter that already has its answer can hand back a
    // ready one.
    fn export(&self, batch: Vec<SpanData>) -> impl Future<Output = OTelSdkResult> + Send {
        drop(batch);
        std::future::ready(Ok(()))
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.flushes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn set_resource(&mut self, resource: &Resource) {
        let name = resource
            .get(&Key::from_static_str("service.name"))
            .map(|value| value.to_string());
        *self
            .service_name
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = name;
    }
}

/// Opens and closes `count` spans through `provider`.
fn emit_spans(provider: &SdkTracerProvider, count: usize) {
    let tracer = provider.tracer("afd_observability_test");
    for index in 0..count {
        let span = tracer.start(format!("unit-of-work-{index}"));
        drop(span);
    }
}

/// Every span lost to a failed export is counted, as spans and not batches.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_otlp_outage_nonblocking() {
    let exporter = CountingExporter::new(DeadCollector::default());
    let drops: SpanDrops = exporter.drops();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter)
        .build();

    assert_eq!(drops.count(), 0, "a fresh counter has lost nothing");

    emit_spans(&provider, 3);
    let _flushed = provider.force_flush();

    assert_eq!(
        drops.count(),
        3,
        "three spans were emitted and none reached the collector — a batch \
         count would have moved by one and told an operator nothing about how \
         much telemetry is missing"
    );
}

/// A collector that answers costs nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_a_delivered_span_is_not_counted_as_lost() {
    let collector = LiveCollector::default();
    let seen_name = Arc::clone(&collector.service_name);
    let exporter = CountingExporter::new(collector);
    let drops = exporter.drops();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_service_name("agentsfleetd")
                .build(),
        )
        .build();

    emit_spans(&provider, 4);
    let _flushed = provider.force_flush();

    assert_eq!(
        drops.count(),
        0,
        "counting a delivered span would make the number useless as an alarm"
    );
    assert_eq!(
        seen_name
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_deref(),
        Some("agentsfleetd"),
        "the wrapper must pass the resource through — swallowing it would ship \
         every span without the service identity that makes it findable"
    );
}

/// A stalled, failing export does not slow the caller down.
///
/// The half of the dimension about REQUESTS. Measured against the exporter's
/// own stall rather than a wall-clock budget, so the assertion is a comparison
/// a loaded machine cannot fail spuriously.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_export_latency_does_not_reach_the_caller() {
    let stall = Duration::from_millis(500);
    let attempts = Arc::new(AtomicUsize::new(0));
    let exporter = CountingExporter::new(DeadCollector {
        latency: stall,
        attempts: Arc::clone(&attempts),
    });
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build();

    let started = Instant::now();
    emit_spans(&provider, 1);
    let elapsed = started.elapsed();

    assert!(
        elapsed < stall,
        "emitting a span took {elapsed:?}, at least the {stall:?} the export \
         stalls for — the export is on the caller's path"
    );

    // And the export really was attempted, so the speed above is the batch
    // processor doing its job rather than the span being discarded outright.
    let _flushed = provider.force_flush();
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "the span reached the exporter, just not on the caller's thread"
    );
}

/// Flush reaches the real exporter rather than stopping at the wrapper.
///
/// A wrapper that answered `Ok(())` on its own behalf would look correct
/// everywhere: nothing errors, no counter moves, and whatever the real exporter
/// still held is lost at shutdown without a word. The counting wrapper is on
/// the path of every span this process emits, so its transparency is a property
/// worth an assertion rather than a reading of the source.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_flush_is_delegated_not_answered_by_the_wrapper() {
    let collector = LiveCollector::default();
    let flushes = Arc::clone(&collector.flushes);
    let exporter = CountingExporter::new(collector);

    assert_eq!(flushes.load(Ordering::SeqCst), 0);

    exporter
        .force_flush()
        .expect("a live collector accepts a flush");

    assert_eq!(
        flushes.load(Ordering::SeqCst),
        1,
        "the flush must reach the exporter underneath"
    );
}
