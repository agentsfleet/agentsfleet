//! A settled report's completion stages, each recorded once on the existing
//! delivery-stage histogram.

use std::time::Duration;

use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_observability::test_util::Capture as Metrics;
use jiff::Timestamp;

use super::completed;
use crate::lease::affinity::Fence;
use crate::lease::settle::Reported;

const DELIVERY_STAGE: &str = "agentsfleet_fleet_delivery_stage_seconds";
const STAGES: [&str; 3] = ["event_to_complete", "lease_to_report", "report_to_complete"];

/// When the fixture event was created.
fn created() -> Timestamp {
    Timestamp::constant(1_767_225_600, 0)
}

/// The fixture instant `ms` after the event was created.
fn after(ms: u64) -> Timestamp {
    created()
        .checked_add(Duration::from_millis(ms))
        .unwrap_or_else(|_overflow| created())
}

/// A lease issued 40 ms after its event.
fn lease() -> Result<Reported, &'static str> {
    let id = Uuid7::encode(created(), [7; ENTROPY_LEN])
        .map_err(|_unencodable| "a fixed timestamp and entropy encode to a Uuid7")?;
    Ok(Reported {
        fleet_id: id.clone(),
        workspace_id: id.clone(),
        tenant_id: id,
        event_id: "event-fixture".to_owned(),
        actor: "api".to_owned(),
        posture: "platform".to_owned(),
        provider: "anthropic".to_owned(),
        model: "claude-opus-5".to_owned(),
        fence: Fence::from_i64(1),
        lease_created_at: after(40),
        event_created_at: created(),
    })
}

/// Each stage's sample count and seconds summed so far.
fn read(metrics: &Metrics) -> Vec<(u64, f64)> {
    STAGES
        .iter()
        .map(|stage| {
            let labels = [("stage", *stage)];
            (
                metrics.histogram_count(DELIVERY_STAGE, &labels),
                metrics.histogram_sum(DELIVERY_STAGE, &labels),
            )
        })
        .collect()
}

/// A published completion records each stage once, on the daemon's instants;
/// a report that published nothing records nothing.
#[test]
fn test_completion_records_three_stages() -> Result<(), &'static str> {
    let metrics = Metrics::install();
    let lease = lease()?;
    let before = read(&metrics);

    completed(&lease, after(700), Some(after(900)));
    completed(&lease, after(700), None);

    let expected = [900, 660, 200].map(|ms| Duration::from_millis(ms).as_secs_f64());
    // One sample each, and its seconds within a nanosecond of the span, so a
    // float's last bit cannot fail the test.
    let recorded: Vec<(u64, bool)> = read(&metrics)
        .into_iter()
        .zip(before)
        .zip(expected)
        .map(|(((count, sum), (count_before, sum_before)), seconds)| {
            (
                count - count_before,
                (sum - sum_before - seconds).abs() < 1e-9,
            )
        })
        .collect();
    assert_eq!(recorded, vec![(1, true), (1, true), (1, true)]);
    Ok(())
}
