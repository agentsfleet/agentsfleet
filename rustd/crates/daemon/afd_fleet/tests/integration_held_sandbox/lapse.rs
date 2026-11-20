//! A hold that no longer binds, on the paths beside the assignment: another
//! runner's candidate scan assigns a lapsed or silent hold's fleet, and an
//! empty assignment on a lapsed hold lets its hint go as an unheld fleet's would.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use af_core::timing::{RUN_TTL, RUNNER_OFFLINE_AFTER, SANDBOX_HOLD_IDLE};

use super::{hold, live, thread};
use crate::rows::Row;
use crate::seed::{self, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

/// What ended a hold's grip on its fleet.
#[derive(Clone, Copy)]
enum Lapse {
    /// The hold passed its own deadline.
    Deadline,
    /// The holder fell silent past the offline threshold.
    Silence,
}

/// Ages `fleet`'s hold by `lapse`: its deadline, or its holder's last beat.
async fn lapse_hold(fixtures: &Fixtures, fleet: &Uuid7, holder: &Uuid7, lapse: Lapse) {
    match lapse {
        Lapse::Deadline => {
            let past = SANDBOX_HOLD_IDLE.saturating_mul(2);
            fixtures
                .age(Row::Thread(fleet), "hold_expires_at", past)
                .await;
        }
        Lapse::Silence => {
            let past = RUNNER_OFFLINE_AFTER.saturating_mul(2);
            fixtures
                .age(Row::Runner(holder), "last_seen_at", past)
                .await;
        }
    }
}

/// Whether another runner's poll runs a fleet whose hold `lapse` ended. The
/// poll reaches the assignment only through the candidate scan, so a fleet the scan
/// filters is never held.
async fn assigned_to_another(lapse: Lapse) -> bool {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [holder, other],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &holder).await;
    hold(
        &runs,
        &fixtures,
        &fleet,
        &holder,
        fixtures.after(SANDBOX_HOLD_IDLE),
    )
    .await;
    lapse_hold(&fixtures, &fleet, &holder, lapse).await;

    let found = seed::select_fleet(&runs, &other, &fleet).await;

    fixtures.cleanup().await;
    found.is_some()
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_poll_is_assigned_a_lapsed_hold() {
    assert!(
        assigned_to_another(Lapse::Deadline).await,
        "a hold past its deadline binds nobody, at the scan as at the assignment"
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_poll_is_assigned_a_silent_holders_fleet() {
    assert!(
        assigned_to_another(Lapse::Silence).await,
        "nor does a holder silent past the offline threshold"
    );
}

/// The holder's empty assignment on a hold that has lapsed drops the hint, as an
/// empty assignment on a fleet nobody holds does: only a live hold keeps it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_empty_assignment_on_a_lapsed_hold_drops_the_hint() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &holder).await;
    hold(
        &runs,
        &fixtures,
        &fleet,
        &holder,
        fixtures.after(SANDBOX_HOLD_IDLE),
    )
    .await;
    lapse_hold(&fixtures, &fleet, &holder, Lapse::Deadline).await;

    let assigned = runs
        .assign(&fleet, &holder, RUN_TTL)
        .await
        .expect("answers")
        .expect("the holder wins");
    runs.release(&fleet, assigned.fence)
        .await
        .expect("an empty assignment lets go");

    assert_eq!(thread(&fixtures, &fleet).await.1, None, "the hint goes");
    fixtures.cleanup().await;
}
