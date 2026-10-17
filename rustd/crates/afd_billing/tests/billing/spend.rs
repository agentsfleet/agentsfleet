//! The budget drain's apportioning, graded row shape by row shape.
//!
//! One charge row holds a whole run's spend over `[created_at, updated_at]`,
//! and each window counts only the share of that span it covers. Every case
//! below is one `CASE` arm of `select_budget_drain.sql`, and the fleet is fresh
//! per test, so each sum is exactly the rows that test wrote.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_billing::Spend;
use jiff::Timestamp;

use crate::lane::{Lane, Stage};

/// The instant the windows open from: 2026-07-15T12:04:00Z.
const NOW: Timestamp = Timestamp::constant(1_784_117_040, 0);

/// 24 hours before [`NOW`], where the rolling day window opens.
const DAY_FLOOR: Timestamp = Timestamp::constant(1_784_030_640, 0);

/// 2026-07-01T00:00:00Z, where the month window opens.
const MONTH_FLOOR: Timestamp = Timestamp::constant(1_782_864_000, 0);

/// One hour, for placing rows either side of a floor.
const HOUR_SECONDS: i64 = 3_600;

/// The amount every row below drains, so each share reads as a fraction of it.
const NANOS: i64 = 1_000_000;

/// The two charge types the drain counts, and one it must not.
const STAGE: &str = "stage";
const RECEIVE: &str = "receive";
const UNCOUNTED: &str = "refund";

/// `instant` moved by whole hours.
fn hours(instant: Timestamp, count: i64) -> Timestamp {
    Timestamp::from_second(instant.as_second() + count * HOUR_SECONDS).expect("inside jiff's range")
}

async fn spend(lane: &Lane) -> Spend {
    lane.accounts
        .spend(&lane.workspace, &lane.fleet, NOW)
        .await
        .expect("the drain answers")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_count_nothing_for_a_fleet_with_no_rows() {
    let lane = Lane::create().await;
    let spent = spend(&lane).await;
    assert_eq!((spent.day.as_i64(), spent.month.as_i64()), (0, 0));
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_count_a_run_that_began_inside_both_windows_in_full() {
    let lane = Lane::create().await;
    lane.charge(Stage {
        charge_type: STAGE,
        nanos: NANOS,
        created_at: hours(NOW, -2),
        updated_at: hours(NOW, -1),
    })
    .await;
    let spent = spend(&lane).await;
    assert_eq!((spent.day.as_i64(), spent.month.as_i64()), (NANOS, NANOS));
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_apportion_a_run_straddling_the_day_floor_by_its_covered_span() {
    let lane = Lane::create().await;
    // Four hours of run, one of them after the day floor: a quarter of the
    // spend falls in the day, all of it in the month.
    lane.charge(Stage {
        charge_type: STAGE,
        nanos: NANOS,
        created_at: hours(DAY_FLOOR, -3),
        updated_at: hours(DAY_FLOOR, 1),
    })
    .await;
    let spent = spend(&lane).await;
    assert_eq!(spent.day.as_i64(), NANOS / 4);
    assert_eq!(spent.month.as_i64(), NANOS);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_count_nothing_for_a_run_that_stopped_before_a_floor() {
    let lane = Lane::create().await;
    // Last charged before the day floor but inside the month.
    lane.charge(Stage {
        charge_type: STAGE,
        nanos: NANOS,
        created_at: hours(DAY_FLOOR, -5),
        updated_at: hours(DAY_FLOOR, -4),
    })
    .await;
    // Last charged before the month began: outside both windows.
    lane.charge(Stage {
        charge_type: STAGE,
        nanos: NANOS,
        created_at: hours(MONTH_FLOOR, -5),
        updated_at: hours(MONTH_FLOOR, -4),
    })
    .await;
    let spent = spend(&lane).await;
    assert_eq!(spent.day.as_i64(), 0);
    assert_eq!(spent.month.as_i64(), NANOS);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_count_a_point_charge_stamped_exactly_at_the_month_floor() {
    let lane = Lane::create().await;
    lane.charge(Stage {
        charge_type: RECEIVE,
        nanos: NANOS,
        created_at: MONTH_FLOOR,
        updated_at: MONTH_FLOOR,
    })
    .await;
    let spent = spend(&lane).await;
    assert_eq!(spent.month.as_i64(), NANOS, "the floor is inclusive");
    assert_eq!(spent.day.as_i64(), 0);
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_ignore_a_charge_type_the_drain_does_not_count() {
    let lane = Lane::create().await;
    lane.charge(Stage {
        charge_type: UNCOUNTED,
        nanos: NANOS,
        created_at: hours(NOW, -2),
        updated_at: hours(NOW, -1),
    })
    .await;
    let spent = spend(&lane).await;
    assert_eq!((spent.day.as_i64(), spent.month.as_i64()), (0, 0));
}
