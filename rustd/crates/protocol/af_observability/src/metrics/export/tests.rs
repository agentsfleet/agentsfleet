//! Dimension 3.3, unit half — what the counter counts, and what resets it.
//!
//! The property that a failing exporter never BLOCKS a recording call needs a
//! real provider and a real reader, so it is proven in `tests/otlp_outage.rs`
//! beside the span-side claim it twins. What is proven here is the arithmetic
//! that test reads: cycles counted once, a run that grows while the collector
//! is down and ends when it answers, and temporality passed through rather
//! than answered for.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use opentelemetry_sdk::error::{OTelSdkError, OTelSdkResult};
use opentelemetry_sdk::metrics::Temporality;
use opentelemetry_sdk::metrics::data::ResourceMetrics;
use opentelemetry_sdk::metrics::exporter::PushMetricExporter;

use super::{BatchDrops, CountingMetricExporter};

/// An exporter whose answer a test chooses, so the wrapper's arithmetic is
/// observed rather than inferred from a real collector's timing.
#[derive(Debug)]
struct Scripted {
    /// Whether the next export fails.
    failing: Arc<std::sync::atomic::AtomicBool>,
    /// Attempts made, proving the wrapper delegates rather than short-circuits.
    attempts: Arc<AtomicUsize>,
    /// What this exporter claims, so pass-through can be asserted.
    temporality: Temporality,
    /// Flushes and shutdowns that reached this exporter, proving the wrapper
    /// forwarded rather than answering for itself.
    flushes: Arc<AtomicUsize>,
    shutdowns: Arc<AtomicUsize>,
}

impl Scripted {
    fn new(temporality: Temporality) -> Self {
        Self {
            failing: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            attempts: Arc::new(AtomicUsize::new(0)),
            temporality,
            flushes: Arc::new(AtomicUsize::new(0)),
            shutdowns: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl PushMetricExporter for Scripted {
    // Ready rather than `async`: nothing here awaits, and the trait returns an
    // `impl Future` so an exporter that already has its answer hands one back.
    fn export(&self, _metrics: &ResourceMetrics) -> impl Future<Output = OTelSdkResult> + Send {
        self.attempts.fetch_add(1, Ordering::SeqCst);
        let answer = if self.failing.load(Ordering::SeqCst) {
            Err(OTelSdkError::InternalFailure(
                "the collector is not answering".to_owned(),
            ))
        } else {
            Ok(())
        };
        std::future::ready(answer)
    }

    fn force_flush(&self) -> OTelSdkResult {
        self.flushes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn shutdown_with_timeout(&self, _timeout: Duration) -> OTelSdkResult {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn temporality(&self) -> Temporality {
        self.temporality
    }
}

/// A fresh counter counts nothing, so a nonzero reading is always something
/// that happened rather than a starting value.
#[test]
fn test_a_fresh_counter_reports_no_loss() {
    let drops = BatchDrops::new();
    assert_eq!(drops.failed(), 0);
    assert_eq!(drops.consecutive(), 0);
}

/// A lost cycle counts once against both the total and the run.
#[test]
fn test_a_lost_cycle_counts_once() {
    let drops = BatchDrops::new();
    assert_eq!(drops.lost(), 1, "the first loss is a run of one");
    assert_eq!(drops.failed(), 1);
    assert_eq!(drops.consecutive(), 1);
}

/// The run grows while the collector stays down, and the total grows with it.
#[test]
fn test_consecutive_losses_grow_the_run() {
    let drops = BatchDrops::new();
    for expected in 1..=5 {
        assert_eq!(drops.lost(), expected);
    }
    assert_eq!(drops.failed(), 5);
    assert_eq!(drops.consecutive(), 5);
}

/// A delivered cycle ends the run but never the total.
///
/// The distinction this type exists for: the total is what was lost, the run is
/// whether it is still happening, and a recovery must clear exactly one of them.
#[test]
fn test_a_delivered_cycle_ends_the_run_but_keeps_the_total() {
    let drops = BatchDrops::new();
    drops.lost();
    drops.lost();
    drops.delivered();

    assert_eq!(
        drops.consecutive(),
        0,
        "a collector that answered is not still failing"
    );
    assert_eq!(
        drops.failed(),
        2,
        "the cycles already lost did not become un-lost"
    );
}

/// A second outage after a recovery starts a fresh run and continues the total.
#[test]
fn test_a_later_outage_starts_a_new_run() {
    let drops = BatchDrops::new();
    drops.lost();
    drops.delivered();
    assert_eq!(drops.lost(), 1, "a new outage is a run of one, not of two");
    assert_eq!(drops.failed(), 2);
}

/// Every clone reads the same numbers, which is what lets the exporter hold one
/// and a reporter hold another.
#[test]
fn test_clones_share_one_count() {
    let held = BatchDrops::new();
    let reported = held.clone();
    held.lost();
    assert_eq!(reported.failed(), 1);
    assert_eq!(reported.consecutive(), 1);
}

/// The wrapper answers the INNER exporter's temporality.
///
/// Load-bearing rather than cosmetic: the SDK asks the exporter which
/// temporality it wants and aggregates to match, so a wrapper answering for
/// itself would rewrite every family flowing through it — turning the cost
/// families' delta windows into running totals with nothing to notice.
#[test]
fn test_temporality_is_the_inner_exporters() {
    for declared in [Temporality::Delta, Temporality::Cumulative] {
        let wrapped = CountingMetricExporter::new(Scripted::new(declared));
        assert_eq!(wrapped.temporality(), declared);
    }
}

/// A wrapped exporter starts clean, so the first reading a test takes is the
/// first thing that happened.
#[test]
fn test_a_wrapped_exporter_starts_clean() {
    let wrapped = CountingMetricExporter::new(Scripted::new(Temporality::Delta));
    assert_eq!(wrapped.drops().failed(), 0);
    assert_eq!(wrapped.drops().consecutive(), 0);
}

/// Flush and shutdown reach the real exporter rather than stopping at the
/// wrapper.
///
/// The twin of the span wrapper's transparency test, and it matters for the
/// same reason: a wrapper answering `Ok(())` on its own behalf would look
/// correct everywhere — nothing errors, no counter moves — while whatever the
/// real exporter still held is discarded at shutdown without a word. This
/// wrapper sits on the path of every metric this process exports, so its
/// transparency is worth an assertion rather than a reading of the source.
#[test]
fn test_flush_and_shutdown_are_delegated_not_answered() {
    let collector = Scripted::new(Temporality::Delta);
    let flushes = Arc::clone(&collector.flushes);
    let shutdowns = Arc::clone(&collector.shutdowns);
    let wrapped = CountingMetricExporter::new(collector);

    wrapped
        .force_flush()
        .expect("a live collector accepts a flush");
    wrapped
        .shutdown_with_timeout(Duration::from_secs(1))
        .expect("a live collector accepts a shutdown");

    assert_eq!(
        flushes.load(Ordering::SeqCst),
        1,
        "the flush must reach the exporter underneath"
    );
    assert_eq!(
        shutdowns.load(Ordering::SeqCst),
        1,
        "the shutdown must reach the exporter underneath, or whatever it still \
         held is lost silently"
    );
}
