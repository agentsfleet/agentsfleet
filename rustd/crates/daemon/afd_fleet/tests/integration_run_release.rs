//! A run whose frame never left the daemon goes back to its queue at once:
//! released under its own fence, its turn is assigned again at the next one,
//! and nothing is billed for it. A release that names another runner, another
//! fence or a run already settled changes nothing.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_state::sql::{RUN_STATUS_ACTIVE, RUN_STATUS_EXPIRED, RUN_STATUS_REPORTED};

use crate::report_seed::{Held, held};

/// The run's status, whether its lease is cut to its metering cursor, and
/// whether its thread is still held.
async fn state(held: &Held) -> (String, bool, bool) {
    sqlx::query_as(
        "SELECT l.status, l.run_expires_at = l.last_metered_at, \
                s.run_expires_at > clock_timestamp() \
         FROM thread.runs l JOIN thread.threads s ON s.id = l.fleet_id \
         WHERE l.id = $1",
    )
    .bind(&held.issued.run_id)
    .fetch_one(
        &mut *held
            .fixtures
            .database
            .acquire()
            .await
            .expect("a connection"),
    )
    .await
    .expect("the run and its thread read")
}

// Value: protects=a run whose frame never reached its runner is put back on the queue at
// once, not left to lapse and be billed its lease; fails_when=the release skips the run's
// expiry, the thread's, or the billing cut; why_new=the gateway released nothing before;
// seam=the release statement over live Postgres, then the production selection.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_unsent_run_is_released_and_its_turn_assigned_again() {
    let held = held().await;
    let released = held
        .runs
        .release_unsent(&held.issued.run_id, &held.runner, held.fence.as_u64())
        .await
        .expect("the release reaches the datastore");
    assert!(released, "the held run is released");
    assert_eq!(
        state(&held).await,
        (RUN_STATUS_EXPIRED.to_owned(), true, false),
        "the run expired with nothing to bill, and its thread is free"
    );

    let again = crate::seed::select_fleet(&held.runs, &held.spare, &held.fleet)
        .await
        .expect("the released turn is assignable at once");
    assert_eq!(again.turn_id, held.turn_id, "the same turn, assigned again");
    assert!(again.fence > held.fence, "at the next fence");
}

// Value: protects=a release fenced to its runner, its token and a live run; fails_when=the
// statement drops the runner, fence or status guard; why_new=the release is new;
// seam=the release statement over live Postgres.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_release_off_its_fence_or_runner_or_after_settlement_changes_nothing() {
    let held = held().await;
    let fence = held.fence.as_u64();
    for (runner, token) in [(&held.spare, fence), (&held.runner, fence + 1)] {
        let released = held
            .runs
            .release_unsent(&held.issued.run_id, runner, token)
            .await
            .expect("the release reaches the datastore");
        assert!(!released, "{runner} at {token} holds no such run");
        assert_eq!(
            state(&held).await,
            (RUN_STATUS_ACTIVE.to_owned(), false, true),
            "the run and its thread are untouched"
        );
    }

    sqlx::query("UPDATE thread.runs SET status = $2 WHERE id = $1")
        .bind(&held.issued.run_id)
        .bind(RUN_STATUS_REPORTED)
        .execute(
            &mut *held
                .fixtures
                .database
                .acquire()
                .await
                .expect("a connection"),
        )
        .await
        .expect("the run settles");
    let released = held
        .runs
        .release_unsent(&held.issued.run_id, &held.runner, fence)
        .await
        .expect("the release reaches the datastore");
    assert!(!released, "a settled run is not released");
    assert_eq!(
        state(&held).await,
        (RUN_STATUS_REPORTED.to_owned(), false, true),
        "a run settled meanwhile keeps its row and its thread"
    );
}
