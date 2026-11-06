//! The burst's accounting and pacing (`M241_001` §5).

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use core::time::Duration;
use std::sync::{Arc, Mutex};

use tokio::time::Instant;

use super::pace::pace;
use super::{Counted, record};
use crate::profile::Profile;
use crate::report::{Lane, Latency, Provenance, Report};

fn report() -> Report {
    Report::new(Lane::LeaseBurst, Profile::Rig, Provenance::for_test())
}

fn latencies(waits: &[u64]) -> Latency {
    let mut latency = Latency::new().expect("a histogram builds");
    for wait in waits {
        latency
            .record(Duration::from_millis(*wait))
            .expect("a short wait fits");
    }
    latency
}

fn measured(report: &Report, key: &str) -> Option<f64> {
    report.measurements.get(key).copied()
}

#[test]
fn burst_counts_unleased_at_window_close() {
    let mut report = report();
    let counted = Counted {
        accepted: 10,
        leased: 4,
        ..Counted::default()
    };

    record(&mut report, counted, &latencies(&[]), &latencies(&[5; 4]));

    assert_eq!(measured(&report, "burst_accepted"), Some(10.0));
    assert_eq!(
        measured(&report, "burst_unleased"),
        Some(6.0),
        "an accepted steer still waiting at the close is counted, not dropped"
    );
}

#[test]
fn burst_excludes_refusals_from_pickup() {
    let mut report = report();
    let counted = Counted {
        accepted: 7,
        refused: 3,
        leased: 7,
        ..Counted::default()
    };

    record(&mut report, counted, &latencies(&[]), &latencies(&[100; 7]));

    assert_eq!(measured(&report, "burst_refused"), Some(3.0));
    let p95 = measured(&report, "burst_pickup_p95_ms").expect("seven pickups report a tail");
    assert!(
        (99.0..=101.0).contains(&p95),
        "only the seven pickups shape the tail: {p95}"
    );
}

#[test]
fn burst_with_no_pickup_reports_no_latency() {
    let mut report = report();

    record(
        &mut report,
        Counted::default(),
        &latencies(&[]),
        &latencies(&[]),
    );

    assert_eq!(measured(&report, "burst_pickup_p99_ms"), None);
}

#[tokio::test(start_paused = true)]
async fn burst_pacer_spaces_appends() {
    let started = Instant::now();
    let at = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&at);

    let ran = pace(10, Duration::from_secs(2), || {
        let seen = Arc::clone(&seen);
        async move {
            seen.lock()
                .expect("no test panics holding the list")
                .push(started.elapsed());
        }
    })
    .await;

    let at = at.lock().expect("no test panics holding the list");
    assert_eq!(ran, 20, "ten a second for two seconds");
    let spacing: Vec<Duration> = at
        .iter()
        .zip(at.iter().skip(1))
        .map(|(earlier, later)| later.saturating_sub(*earlier))
        .collect();
    assert!(
        spacing.iter().all(|gap| *gap == Duration::from_millis(100)),
        "every append is 100 ms after the last: {spacing:?}"
    );
}
