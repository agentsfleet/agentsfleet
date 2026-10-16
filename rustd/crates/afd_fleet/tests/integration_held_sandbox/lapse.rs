//! A hold that no longer binds, on the paths beside the claim: another
//! runner's candidate scan offers a lapsed or silent hold's fleet, and an
//! empty claim on a lapsed hold lets its hint go as an unheld fleet's would.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::timing::{LEASE_TTL_MS, RUNNER_OFFLINE_AFTER_MS};

use super::{at, hold, id, live, slot};
use crate::queue;
use crate::seed::{self, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

/// How long past enrolment a short hold lasts.
const SHORT_HOLD_MS: i64 = 10;

/// When a short hold lapses.
fn short(fixtures: &Fixtures) -> i64 {
    fixtures.enrolled_at() + SHORT_HOLD_MS
}

/// Whether another runner's poll `polled_after` ms past enrolment leases a
/// fleet whose live holder held it until `until_after` ms past enrolment. The
/// poll reaches the claim only through the candidate scan, so a fleet the scan
/// filters is never leased.
async fn offered_to_another(until_after: i64, polled_after: i64) -> bool {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [holder, other],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    let enrolled = fixtures.enrolled_at();
    hold(&leases, &fixtures, &fleet, &holder, enrolled + until_after).await;

    let polled_at = at(enrolled + polled_after);
    let found = seed::select_fleet_within_rotations(&leases, &other, polled_at, &fleet).await;

    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
    found.is_some()
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_poll_is_offered_a_lapsed_hold() {
    assert!(
        offered_to_another(SHORT_HOLD_MS, SHORT_HOLD_MS + 1).await,
        "a hold past its deadline binds nobody, at the scan as at the claim"
    );
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_another_runners_poll_is_offered_a_silent_holders_fleet() {
    let far = 10 * RUNNER_OFFLINE_AFTER_MS;
    assert!(
        offered_to_another(far, RUNNER_OFFLINE_AFTER_MS + 1).await,
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
    hold(&leases, &fixtures, &fleet, &holder, short(&fixtures)).await;
    let now = at(short(&fixtures) + 1);

    let claimed = leases
        .claim(&id(&fleet), &holder, now, LEASE_TTL_MS)
        .await
        .expect("answers")
        .expect("the holder wins");
    leases
        .release(&id(&fleet), claimed.fence, now)
        .await
        .expect("an empty claim lets go");

    assert_eq!(slot(&fixtures, &fleet).await.1, None, "the hint goes");
    fixtures.cleanup().await;
}
