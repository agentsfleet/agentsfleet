use core::time::Duration;
use std::future::ready;

use af_crypto::entropy::Entropy;
use afd_bus::wake::Wake;
use tokio::time::Instant;

use super::{WINDOW, drive, summary_lines};
use crate::error::Result;
use crate::profile::Profile;
use crate::report::{Lane, Provenance, Report};

/// A delivery quick enough to time.
const PROMPT: Duration = Duration::from_millis(5);

/// A delivery past the missed deadline.
const LATE: Duration = Duration::from_millis(1_200);

#[tokio::test(start_paused = true)]
async fn wake_past_deadline_counts_missed() -> Result<()> {
    let hub = Wake::default();
    let mut delays = [LATE, PROMPT].into_iter();
    let ledger = drive(std::slice::from_ref(&hub), 2, &Entropy::new(), |fleet| {
        let (hub, delay) = (hub.clone(), delays.next().unwrap_or(PROMPT));
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            hub.ring(fleet);
        });
        ready(())
    })
    .await?;

    let mut report = Report::new(Lane::Tail, Profile::Rig, Provenance::for_test());
    ledger.record(&mut report, 1);
    assert_eq!(report.measurements.get("wake_missed_hubs_1"), Some(&1.0));
    let p99 = report.measurements.get("wake_p99_ms_hubs_1").copied();
    // Only the prompt ring is timed: the late one's 1.2 s is nowhere in the tail.
    assert!(
        p99.is_some_and(|ms| ms < 10.0),
        "p99 holds only the prompt ring, got {p99:?}"
    );
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn wake_ringer_waits_at_window() -> Result<()> {
    // No hub ever takes a ring, so the window fills and only an expiry frees it.
    let hub = Wake::default();
    let mut rung = Vec::new();
    let start = Instant::now();
    let rings = u64::try_from(WINDOW)? + 1;
    drive(
        std::slice::from_ref(&hub),
        rings,
        &Entropy::new(),
        |_fleet| {
            rung.push(Instant::now());
            ready(())
        },
    )
    .await?;

    let (filled, waited) = rung.split_at(WINDOW);
    assert!(
        filled.iter().all(|at| *at == start),
        "the first {WINDOW} rings go out at once"
    );
    assert_eq!(
        waited
            .iter()
            .map(|at| at.duration_since(start))
            .collect::<Vec<_>>(),
        [super::ledger::MISSED_AFTER],
        "ring {} waits until the oldest ring is missed",
        WINDOW + 1
    );
    Ok(())
}

#[test]
fn the_wake_summary_prints_one_line_per_hub_rung() {
    let mut report = Report::new(Lane::Tail, Profile::Rig, Provenance::for_test());
    report.measurement("wake_p50_ms_hubs_1", 0.25);
    report.measurement("wake_p99_ms_hubs_1", 0.75);
    report.measurement("wake_missed_hubs_1", 0.0);
    report.measurement("wake_missed_hubs_4", 3.0);

    assert_eq!(
        summary_lines(&report),
        [
            "wake_p50_ms_hubs_1=0.25 wake_p99_ms_hubs_1=0.75 wake_missed_hubs_1=0",
            "wake_missed_hubs_4=3",
        ]
    );
}
