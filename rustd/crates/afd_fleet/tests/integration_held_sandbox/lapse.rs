//! A hold that no longer binds, on the paths beside the claim: another
//! runner's candidate scan offers a lapsed or silent hold's fleet, and an
//! empty claim on a lapsed hold lets its hint go as an unheld fleet's would.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::timing::{LEASE_TTL, RUNNER_OFFLINE_AFTER, SANDBOX_HOLD_IDLE};

use super::{hold, id, live, slot};
use crate::queue;
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
async fn lapse_hold(fixtures: &Fixtures, fleet: &str, holder: &str, lapse: Lapse) {
    match lapse {
        Lapse::Deadline => {
            let past = SANDBOX_HOLD_IDLE.saturating_mul(2);
            fixtures
                .age(Row::Slot(fleet), "hold_expires_at", past)
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

/// Whether another runner's poll leases a fleet whose hold `lapse` ended. The
/// poll reaches the claim only through the candidate scan, so a fleet the scan
/// filters is never leased.
async fn offered_to_another(lapse: Lapse) -> bool {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [holder, other],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(
        &leases,
        &fixtures,
        &fleet,
        &holder,
        fixtures.after(SANDBOX_HOLD_IDLE),
    )
    .await;
    lapse_hold(&fixtures, &fleet, holder.as_str(), lapse).await;

    let found = seed::select_fleet_within_rotations(&leases, &other, &fleet).await;

    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
    found.is_some()
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_poll_is_offered_a_lapsed_hold() {
    assert!(
        offered_to_another(Lapse::Deadline).await,
        "a hold past its deadline binds nobody, at the scan as at the claim"
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_poll_is_offered_a_silent_holders_fleet() {
    assert!(
        offered_to_another(Lapse::Silence).await,
        "nor does a holder silent past the offline threshold"
    );
}

/// The holder's empty claim on a hold that has lapsed drops the hint, as an
/// empty claim on a fleet nobody holds does: only a live hold keeps it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_empty_claim_on_a_lapsed_hold_drops_the_hint() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(
        &leases,
        &fixtures,
        &fleet,
        &holder,
        fixtures.after(SANDBOX_HOLD_IDLE),
    )
    .await;
    lapse_hold(&fixtures, &fleet, holder.as_str(), Lapse::Deadline).await;

    let claimed = leases
        .claim(&id(&fleet), &holder, LEASE_TTL)
        .await
        .expect("answers")
        .expect("the holder wins");
    leases
        .release(&id(&fleet), claimed.fence)
        .await
        .expect("an empty claim lets go");

    assert_eq!(slot(&fixtures, &fleet).await.1, None, "the hint goes");
    fixtures.cleanup().await;
}
