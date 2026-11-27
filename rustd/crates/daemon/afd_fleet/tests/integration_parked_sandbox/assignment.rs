//! Assignments and polls against a parked fleet: only the keeper wins while the park
//! is live, its poll finds the fleet first, and a lapsed or silent park binds
//! nobody.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use super::{live, park, thread};
use crate::rows::Row;
use crate::seed::{self, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;
use afd_runner::timing::{RUN_TTL, RUNNER_OFFLINE_AFTER, SANDBOX_PARK_IDLE};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_keeper_is_assigned_its_parked_fleet() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [keeper, _other],
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

    let acquired = seed::select_fleet(&runs, &keeper, &fleet)
        .await
        .expect("the keeper runs its parked fleet");

    assert_eq!(acquired.fleet_id, fleet);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_other_runner_skips_parked_fleet() {
    let fixtures = Fixtures::create_isolated().await;
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

    let skipped = seed::select_fleet(&runs, &other, &fleet).await;
    let refused = runs
        .assign(&fleet, &other, RUN_TTL)
        .await
        .expect("the assignment answers");
    let parked = seed::select_fleet(&runs, &keeper, &fleet).await;

    assert!(
        skipped.is_none(),
        "another runner's poll passes a parked fleet by"
    );
    assert!(refused.is_none(), "and its assignment loses");
    assert!(parked.is_some(), "while the keeper still runs it");
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lapsed_park_is_assignable() {
    let fixtures = Fixtures::create().await;
    let (expired, _workspace, _tenant, [keeper, other]) = seeded_parts::<2>(&fixtures).await;
    let (silent, ..) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    let until = fixtures.after(SANDBOX_PARK_IDLE);
    live(&fixtures, &keeper).await;
    park(&runs, &fixtures, &expired, &keeper, until).await;
    park(&runs, &fixtures, &silent, &keeper, until).await;

    // The first park passes its deadline while its keeper is still live.
    let lapse = SANDBOX_PARK_IDLE.saturating_mul(2);
    fixtures
        .age(Row::Thread(&expired), "park_expires_at", lapse)
        .await;
    let past_the_park = runs
        .assign(&expired, &other, RUN_TTL)
        .await
        .expect("answers");
    // Then the keeper falls silent with the second park still in date.
    let silence = RUNNER_OFFLINE_AFTER.saturating_mul(2);
    fixtures
        .age(Row::Runner(&keeper), "last_seen_at", silence)
        .await;
    let past_the_silence = runs
        .assign(&silent, &other, RUN_TTL)
        .await
        .expect("answers");

    assert!(
        past_the_park.is_some(),
        "a park past its deadline binds nobody"
    );
    assert!(past_the_silence.is_some(), "nor does a silent keeper's");
    assert_eq!(
        thread(&fixtures, &silent).await,
        (None, Some(other.clone())),
        "another runner's assignment clears the park it overrode"
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_keeper_polls_its_parks_first() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [keeper],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
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

    let first_poll = runs
        .select(&keeper, std::slice::from_ref(&fleet))
        .await
        .expect("a poll answers")
        .expect("one poll finds the parked fleet, whichever partition it is in");

    assert_eq!(first_poll.fleet_id, fleet);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_drained_assignment_keeps_parked_hint() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [keeper]) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    let until = fixtures.after(SANDBOX_PARK_IDLE);
    live(&fixtures, &keeper).await;
    park(&runs, &fixtures, &fleet, &keeper, until).await;

    let assigned = runs
        .assign(&fleet, &keeper, RUN_TTL)
        .await
        .expect("answers")
        .expect("the keeper wins");
    runs.release(&fleet, assigned.fence)
        .await
        .expect("an empty assignment lets go");

    assert_eq!(
        thread(&fixtures, &fleet).await,
        (Some(until), Some(keeper.clone()))
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_unparked_fleet_is_assigned_as_before() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [first, second]) = seeded_parts::<2>(&fixtures).await;
    let runs = fixtures.runs();

    let assigned = runs
        .assign(&fleet, &first, RUN_TTL)
        .await
        .expect("answers")
        .expect("a free thread is won");
    let contended = runs
        .assign(&fleet, &second, RUN_TTL)
        .await
        .expect("answers");
    runs.release(&fleet, assigned.fence).await.expect("lets go");
    let after = thread(&fixtures, &fleet).await;
    let reassigned = runs
        .assign(&fleet, &second, RUN_TTL)
        .await
        .expect("answers");

    assert!(contended.is_none(), "one live holder per fleet, as before");
    assert_eq!(
        after,
        (None, None),
        "an empty assignment drops the hint, as before"
    );
    assert!(reassigned.is_some(), "and anyone may take it next");
    fixtures.cleanup().await;
}
