//! A park that no longer binds, on the paths beside the assignment: another
//! runner's candidate scan assigns a lapsed or silent park's fleet, and an
//! empty assignment on a lapsed park lets its hint go as an unparked fleet's would.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_runner::timing::{RUN_TTL, RUNNER_OFFLINE_AFTER, SANDBOX_PARK_IDLE};

use super::{live, park, thread};
use crate::rows::Row;
use crate::seed::{self, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

/// What ended a park's grip on its fleet.
#[derive(Clone, Copy)]
enum Lapse {
    /// The park passed its own deadline.
    Deadline,
    /// The keeper fell silent past the offline threshold.
    Silence,
}

/// Ages `fleet`'s park by `lapse`: its deadline, or its keeper's last beat.
async fn lapse_park(fixtures: &Fixtures, fleet: &Uuid7, keeper: &Uuid7, lapse: Lapse) {
    match lapse {
        Lapse::Deadline => {
            let past = SANDBOX_PARK_IDLE.saturating_mul(2);
            fixtures
                .age(Row::Thread(fleet), "park_expires_at", past)
                .await;
        }
        Lapse::Silence => {
            let past = RUNNER_OFFLINE_AFTER.saturating_mul(2);
            fixtures
                .age(Row::Runner(keeper), "last_seen_at", past)
                .await;
        }
    }
}

/// Whether another runner's poll runs a fleet whose park `lapse` ended. The
/// poll reaches the assignment only through the candidate scan, so a fleet the scan
/// filters is never held.
async fn assigned_to_another(lapse: Lapse) -> bool {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [keeper, other],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(
        &runs,
        &fixtures,
        &fleet,
        &keeper,
        fixtures.after(SANDBOX_PARK_IDLE),
    )
    .await;
    lapse_park(&fixtures, &fleet, &keeper, lapse).await;

    let found = seed::select_fleet(&runs, &other, &fleet).await;

    fixtures.cleanup().await;
    found.is_some()
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_poll_is_assigned_a_lapsed_park() {
    assert!(
        assigned_to_another(Lapse::Deadline).await,
        "a park past its deadline binds nobody, at the scan as at the assignment"
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_poll_is_assigned_a_silent_keepers_fleet() {
    assert!(
        assigned_to_another(Lapse::Silence).await,
        "nor does a keeper silent past the offline threshold"
    );
}

/// The keeper's empty assignment on a park that has lapsed drops the hint, as an
/// empty assignment on a fleet nobody parks does: only a live park keeps it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_empty_assignment_on_a_lapsed_park_drops_the_hint() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [keeper]) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(
        &runs,
        &fixtures,
        &fleet,
        &keeper,
        fixtures.after(SANDBOX_PARK_IDLE),
    )
    .await;
    lapse_park(&fixtures, &fleet, &keeper, Lapse::Deadline).await;

    let assigned = runs
        .assign(&fleet, &keeper, RUN_TTL)
        .await
        .expect("answers")
        .expect("the keeper wins");
    runs.release(&fleet, assigned.fence)
        .await
        .expect("an empty assignment lets go");

    assert_eq!(thread(&fixtures, &fleet).await.1, None, "the hint goes");
    fixtures.cleanup().await;
}
