//! A viewer whose tail ends early keeps what it saw.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking on an unmet precondition"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use afd_bus::{Message, Subject};
use afd_sse::Frame;
use futures_util::StreamExt as _;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::{FRAMES, Rung, Shared, view};
use crate::lane::tail::publish::{PROBE, frame_of};
use crate::profile::Profile;
use crate::report::latency::Latency;
use crate::report::{Lane, Provenance, Report};

/// A frame as the tail would hand it over, carrying `payload`.
fn frame(payload: String) -> Frame {
    Frame::activity(
        1,
        Arc::new(Message {
            subject: Subject::Wake(0),
            payload: Arc::new(payload),
        }),
    )
}

fn shared() -> Arc<Shared> {
    Arc::new(Shared {
        epoch: Instant::now(),
        published_at: (0..FRAMES).map(|_| AtomicU64::new(0)).collect(),
        delivered: AtomicU64::new(0),
        ready: AtomicU64::new(0),
        progress: Notify::new(),
    })
}

#[tokio::test]
async fn a_tail_that_ends_early_reports_the_frames_and_lags_it_saw() {
    let shared = shared();
    // Two probes, two measured frames, a lag notice, a frame of no interest,
    // then the tail closes — well short of the rung's hundred.
    let tail = futures_util::stream::iter([
        frame(PROBE.to_owned()),
        frame(PROBE.to_owned()),
        frame(frame_of(200)),
        Frame::catching_up(3),
        frame(frame_of(200)),
        frame(r#"{"kind":"unrelated"}"#.to_owned()),
    ])
    .boxed();

    let seen = view(tail, Arc::clone(&shared), CancellationToken::new())
        .await
        .expect("an ended tail is a short count, not a failure");

    assert_eq!(seen.frames, 2, "only measured frames count as delivered");
    assert_eq!(seen.lagged, 1, "a lag notice is counted, not delivered");
    assert_eq!(seen.latency.count(), 2);
    assert_eq!(shared.delivered.load(Ordering::Relaxed), 2);
    assert_eq!(
        shared.ready.load(Ordering::Relaxed),
        1,
        "a viewer is ready once, however many probes it hears"
    );
}

/// A rung that timed `latencies`, one viewer, nothing allocated.
fn rung_of(latencies: impl IntoIterator<Item = core::time::Duration>) -> Rung {
    let mut latency = Latency::new().expect("an empty histogram");
    for elapsed in latencies {
        latency
            .record(elapsed)
            .expect("a latency the histogram holds");
    }
    Rung {
        viewers: 1,
        bytes: 200,
        delivered: latency.count(),
        lagged: 0,
        allocations: 0,
        busy: core::time::Duration::ZERO,
        latency,
    }
}

/// The one value a rung wrote under `key`.
fn only(report: &Report, key: &str) -> Option<f64> {
    report.series.get(key)?.first().copied()
}

/// M244 Dimension 1.1: the summary carries p99 beside p95.
#[test]
fn tail_summary_reports_p99() {
    let mut report = Report::new(Lane::Tail, Profile::Rig, Provenance::for_test());
    rung_of((1..=100).map(core::time::Duration::from_millis)).record(&mut report, false);

    let p95 = only(&report, super::super::RECEIVE_P95_MS).expect("a timed rung writes p95");
    let p99 = only(&report, super::super::RECEIVE_P99_MS).expect("a timed rung writes p99");
    assert!(
        (p95 - 95.0).abs() < 0.5,
        "p95 of 1..100 ms is 95, got {p95}"
    );
    assert!(
        (p99 - 99.0).abs() < 0.5,
        "p99 of 1..100 ms is 99, got {p99}"
    );
}

/// M244 Dimension 1.2: a rung with nothing timed reports no tail figure,
/// because a p99 of nothing is not a zero.
#[test]
fn rung_without_frames_reports_no_p99() {
    let mut report = Report::new(Lane::Tail, Profile::Rig, Provenance::for_test());
    rung_of([]).record(&mut report, false);

    assert_eq!(only(&report, super::super::RECEIVE_P99_MS), None);
    assert_eq!(only(&report, super::super::RECEIVE_P95_MS), None);
}
