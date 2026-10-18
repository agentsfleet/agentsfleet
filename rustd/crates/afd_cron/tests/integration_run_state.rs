//! A stored schedule renders its run state and next run from its own row.
//!
//! The store hands back the row a create or a finalize wrote; the view reads
//! that row at one instant. A create still held by its creator is saved and
//! not yet registered, so it names no run; once finalized it is registered and
//! names its next match in its zone.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

#[path = "support/cron_lane.rs"]
mod support;

use afd_api_wire::schedule::RunState;
use jiff::Timestamp;

use self::support::{CronLane, SEED_AT};

/// 03:00 UTC every day.
const NIGHTLY: &str = "0 3 * * *";

/// 2025-10-10T03:00:00Z: the first 03:00 after [`SEED_AT`] (2025-10-09T08:53:20Z).
const NEXT_NIGHT: Timestamp = Timestamp::constant(1_760_065_200, 0);

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn a_schedule_names_a_next_run_only_once_it_is_registered() {
    let lane = CronLane::open().await;

    let created = lane.create("key-pending", NIGHTLY).await;
    let pending = created.view(SEED_AT);
    assert_eq!(pending.run_state, RunState::ActivationPending);
    assert_eq!(
        pending.next_run_at, None,
        "a schedule not yet registered fires nowhere, so it names no time"
    );

    let settled = lane.settled("key-registered", NIGHTLY).await;
    let registered = settled.view(SEED_AT);
    assert_eq!(registered.run_state, RunState::Scheduled);
    assert_eq!(registered.next_run_at, Some(NEXT_NIGHT));
    assert!(
        registered.description.ends_with(" (UTC)"),
        "the description names its zone: {}",
        registered.description
    );
}
