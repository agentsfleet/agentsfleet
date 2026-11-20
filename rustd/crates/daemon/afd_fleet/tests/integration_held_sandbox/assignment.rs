//! Assignments and polls against a held fleet: only the holder wins while the hold
//! is live, its poll finds the fleet first, and a lapsed or silent hold binds
//! nobody.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::timing::{RUN_TTL, RUNNER_OFFLINE_AFTER, SANDBOX_HOLD_IDLE};

use super::{hold, live, thread};
use crate::rows::Row;
use crate::seed::{self, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_holder_is_assigned_its_held_fleet() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [holder, _other],
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

    let acquired = seed::select_fleet(&runs, &holder, &fleet)
        .await
        .expect("the holder runs its held fleet");

    assert_eq!(acquired.fleet_id, fleet);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_other_runner_skips_held_fleet() {
    let fixtures = Fixtures::create_isolated().await;
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

    let skipped = seed::select_fleet(&runs, &other, &fleet).await;
    let refused = runs
        .assign(&fleet, &other, RUN_TTL)
        .await
        .expect("the assignment answers");
    let held = seed::select_fleet(&runs, &holder, &fleet).await;

    assert!(
        skipped.is_none(),
        "another runner's poll passes a held fleet by"
    );
    assert!(refused.is_none(), "and its assignment loses");
    assert!(held.is_some(), "while the holder still runs it");
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lapsed_hold_is_assignable() {
    let fixtures = Fixtures::create().await;
    let (expired, _workspace, _tenant, [holder, other]) = seeded_parts::<2>(&fixtures).await;
    let (silent, ..) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    let until = fixtures.after(SANDBOX_HOLD_IDLE);
    live(&fixtures, &holder).await;
    hold(&runs, &fixtures, &expired, &holder, until).await;
    hold(&runs, &fixtures, &silent, &holder, until).await;

    // The first hold passes its deadline while its holder is still live.
    let lapse = SANDBOX_HOLD_IDLE.saturating_mul(2);
    fixtures
        .age(Row::Thread(&expired), "hold_expires_at", lapse)
        .await;
    let past_the_hold = runs
        .assign(&expired, &other, RUN_TTL)
        .await
        .expect("answers");
    // Then the holder falls silent with the second hold still in date.
    let silence = RUNNER_OFFLINE_AFTER.saturating_mul(2);
    fixtures
        .age(Row::Runner(&holder), "last_seen_at", silence)
        .await;
    let past_the_silence = runs
        .assign(&silent, &other, RUN_TTL)
        .await
        .expect("answers");

    assert!(
        past_the_hold.is_some(),
        "a hold past its deadline binds nobody"
    );
    assert!(past_the_silence.is_some(), "nor does a silent holder's");
    assert_eq!(
        thread(&fixtures, &silent).await,
        (None, Some(other.clone())),
        "another runner's assignment clears the hold it overrode"
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_holder_polls_its_holds_first() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [holder],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
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

    let first_poll = runs
        .select(&holder, std::slice::from_ref(&fleet))
        .await
        .expect("a poll answers")
        .expect("one poll finds the held fleet, whichever partition it is in");

    assert_eq!(first_poll.fleet_id, fleet);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_drained_assignment_keeps_held_hint() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    let until = fixtures.after(SANDBOX_HOLD_IDLE);
    live(&fixtures, &holder).await;
    hold(&runs, &fixtures, &fleet, &holder, until).await;

    let assigned = runs
        .assign(&fleet, &holder, RUN_TTL)
        .await
        .expect("answers")
        .expect("the holder wins");
    runs.release(&fleet, assigned.fence)
        .await
        .expect("an empty assignment lets go");

    assert_eq!(
        thread(&fixtures, &fleet).await,
        (Some(until), Some(holder.clone()))
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_unheld_fleet_is_assigned_as_before() {
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
