//! The burst's accounting and pacing.

use core::time::Duration;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::time::Instant;

use super::pace::pace;
use super::{Counted, record};
use crate::error::Result;
use crate::profile::Profile;
use crate::report::{Lane, Latency, Provenance, Report};

fn report() -> Report {
    Report::new(Lane::LeaseBurst, Profile::Rig, Provenance::for_test())
}

fn latencies(waits: &[u64]) -> Result<Latency> {
    let mut latency = Latency::new()?;
    for wait in waits {
        latency.record(Duration::from_millis(*wait))?;
    }
    Ok(latency)
}

fn measured(report: &Report, key: &str) -> Option<f64> {
    report.measurements.get(key).copied()
}

#[test]
fn burst_counts_unleased_at_window_close() -> Result<()> {
    let mut report = report();
    let counted = Counted {
        accepted: 10,
        leased: 4,
        ..Counted::default()
    };

    record(&mut report, counted, &latencies(&[])?, &latencies(&[5; 4])?);

    assert_eq!(measured(&report, "burst_accepted"), Some(10.0));
    assert_eq!(
        measured(&report, "burst_unleased"),
        Some(6.0),
        "an accepted steer still waiting at the close is counted, not dropped"
    );
    Ok(())
}

#[test]
fn burst_excludes_refusals_from_pickup() -> Result<()> {
    let mut report = report();
    let counted = Counted {
        accepted: 7,
        refused: 3,
        leased: 7,
        ..Counted::default()
    };

    record(
        &mut report,
        counted,
        &latencies(&[])?,
        &latencies(&[100; 7])?,
    );

    assert_eq!(measured(&report, "burst_refused"), Some(3.0));
    let p95 = measured(&report, "burst_pickup_p95_ms");
    assert!(
        p95.is_some_and(|tail| (99.0..=101.0).contains(&tail)),
        "only the seven pickups shape the tail: {p95:?}"
    );
    Ok(())
}

#[test]
fn burst_with_no_pickup_reports_no_latency() -> Result<()> {
    let mut report = report();

    record(
        &mut report,
        Counted::default(),
        &latencies(&[])?,
        &latencies(&[])?,
    );

    assert_eq!(measured(&report, "burst_pickup_p99_ms"), None);
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn burst_pacer_spaces_appends() -> Result<()> {
    let started = Instant::now();
    let at = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&at);

    let ran = pace(10, Duration::from_secs(2), || {
        let seen = Arc::clone(&seen);
        async move {
            seen.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(started.elapsed());
        }
    })
    .await;

    let at = at.lock().unwrap_or_else(PoisonError::into_inner);
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
    Ok(())
}
