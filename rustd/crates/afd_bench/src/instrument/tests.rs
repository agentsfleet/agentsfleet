//! The exporter's own contract, without installing the process-wide producers.
//!
//! Installing here would flip `producers::installed()` for every other unit
//! test in this binary (see `tests/lease_instrument.rs`), so these build a
//! provider over the exporter directly and record into it by name.

#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "a test asserts by panicking, and one poisons a lock by panicking on purpose"
)]

use std::sync::Arc;

use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::metrics::exporter::PushMetricExporter as _;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};

use super::{CANDIDATES, CapturedCounters, CapturingExporter, POLLS, PollCounters, ROUNDTRIPS};

/// A provider exporting into `sink`.
fn provider_into(sink: &Arc<CapturedCounters>) -> SdkMeterProvider {
    let exporter = CapturingExporter {
        sink: Arc::clone(sink),
    };
    SdkMeterProvider::builder()
        .with_reader(PeriodicReader::builder(exporter).build())
        .build()
}

fn latest(sink: &CapturedCounters) -> PollCounters {
    *sink
        .latest
        .lock()
        .expect("no test panics holding the capture")
}

#[test]
fn a_flush_and_a_shutdown_keep_the_last_capture_a_lane_reads() {
    let sink = Arc::new(CapturedCounters::default());
    let provider = provider_into(&sink);
    let meter = provider.meter("afd_bench_test");
    meter.u64_counter(POLLS).build().add(3, &[]);
    meter.u64_counter(CANDIDATES).build().add(30, &[]);
    meter.u64_counter(ROUNDTRIPS).build().add(6, &[]);
    meter
        .u64_counter("some_other_family_total")
        .build()
        .add(99, &[]);
    provider
        .force_flush()
        .expect("an in-memory export cannot fail");

    // The exporter's own flush and the provider's shutdown both run at the end
    // of a process; neither may fail, and neither may discard the sums the
    // lane reads after its window.
    CapturingExporter {
        sink: Arc::clone(&sink),
    }
    .force_flush()
    .expect("nothing is buffered, so a flush has nothing to fail on");
    provider
        .shutdown()
        .expect("shutting the exporter down releases nothing and cannot fail");

    assert_eq!(
        latest(&sink),
        PollCounters {
            polls: 3,
            candidates: 30,
            roundtrips: 6,
        },
        "only the three lease families are captured, and a flush or shutdown clears none"
    );
}

#[test]
fn a_capture_whose_lock_was_poisoned_fails_the_export_rather_than_reading_zero() {
    let sink = Arc::new(CapturedCounters::default());
    let poisoner = Arc::clone(&sink);
    // A thread that panics while holding the capture lock poisons it.
    let _panicked = std::thread::spawn(move || {
        let _held = poisoner.latest.lock();
        std::panic::panic_any("a holder panicked");
    })
    .join();
    let provider = provider_into(&sink);
    provider
        .meter("afd_bench_test")
        .u64_counter(POLLS)
        .build()
        .add(1, &[]);

    let flushed = provider.force_flush();

    assert!(
        flushed.is_err(),
        "a lane must see a failed export, never a capture it could not write"
    );
}
