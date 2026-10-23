//! Claims and polls against a held fleet: only the holder wins while the hold
//! is live, its poll finds the fleet first, and a lapsed or silent hold binds
//! nobody.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::timing::{LEASE_TTL, RUNNER_OFFLINE_AFTER, SANDBOX_HOLD_IDLE};

use super::{hold, id, live, slot};
use crate::rows::Row;
use crate::seed::{self, Seeded, seeded, seeded_parts};
use crate::support::Fixtures;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_holder_claims_its_held_fleet() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [holder, _other],
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

    let acquired = seed::select_fleet(&leases, &holder, &fleet)
        .await
        .expect("the holder leases its held fleet");

    assert_eq!(acquired.fleet_id.as_str(), fleet);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_other_runner_skips_held_fleet() {
    let fixtures = Fixtures::create_isolated_with_queue().await;
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

    let skipped = seed::select_fleet(&leases, &other, &fleet).await;
    let refused = leases
        .claim(&id(&fleet), &other, LEASE_TTL)
        .await
        .expect("the claim answers");
    let held = seed::select_fleet(&leases, &holder, &fleet).await;

    assert!(
        skipped.is_none(),
        "another runner's poll passes a held fleet by"
    );
    assert!(refused.is_none(), "and its claim loses");
    assert!(held.is_some(), "while the holder still leases it");
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lapsed_hold_is_claimable() {
    let fixtures = Fixtures::create_with_queue().await;
    let (expired, _workspace, _tenant, [holder, other]) = seeded_parts::<2>(&fixtures).await;
    let (silent, ..) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    let until = fixtures.after(SANDBOX_HOLD_IDLE);
    live(&fixtures, &holder).await;
    hold(&leases, &fixtures, &expired, &holder, until).await;
    hold(&leases, &fixtures, &silent, &holder, until).await;

    // The first hold passes its deadline while its holder is still live.
    let lapse = SANDBOX_HOLD_IDLE.saturating_mul(2);
    fixtures
        .age(Row::Slot(&expired), "hold_expires_at", lapse)
        .await;
    let past_the_hold = leases
        .claim(&id(&expired), &other, LEASE_TTL)
        .await
        .expect("answers");
    // Then the holder falls silent with the second hold still in date.
    let silence = RUNNER_OFFLINE_AFTER.saturating_mul(2);
    fixtures
        .age(Row::Runner(holder.as_str()), "last_seen_at", silence)
        .await;
    let past_the_silence = leases
        .claim(&id(&silent), &other, LEASE_TTL)
        .await
        .expect("answers");

    assert!(
        past_the_hold.is_some(),
        "a hold past its deadline binds nobody"
    );
    assert!(past_the_silence.is_some(), "nor does a silent holder's");
    assert_eq!(
        slot(&fixtures, &silent).await,
        (None, Some(other.as_str().to_owned())),
        "another runner's claim clears the hold it overrode"
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_holder_polls_its_holds_first() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [holder],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
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

    let first_poll = leases
        .select(&holder, &[id(&fleet)])
        .await
        .expect("a poll answers")
        .expect("one poll finds the held fleet, whichever partition it is in");

    assert_eq!(first_poll.fleet_id.as_str(), fleet);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_drained_claim_keeps_held_hint() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    let until = fixtures.after(SANDBOX_HOLD_IDLE);
    live(&fixtures, &holder).await;
    hold(&leases, &fixtures, &fleet, &holder, until).await;

    let claimed = leases
        .claim(&id(&fleet), &holder, LEASE_TTL)
        .await
        .expect("answers")
        .expect("the holder wins");
    leases
        .release(&id(&fleet), claimed.fence)
        .await
        .expect("an empty claim lets go");

    assert_eq!(
        slot(&fixtures, &fleet).await,
        (Some(until), Some(holder.as_str().to_owned()))
    );
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_unheld_fleet_claims_as_before() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [first, second]) = seeded_parts::<2>(&fixtures).await;
    let leases = fixtures.leases();

    let claimed = leases
        .claim(&id(&fleet), &first, LEASE_TTL)
        .await
        .expect("answers")
        .expect("a free slot is won");
    let contended = leases
        .claim(&id(&fleet), &second, LEASE_TTL)
        .await
        .expect("answers");
    leases
        .release(&id(&fleet), claimed.fence)
        .await
        .expect("lets go");
    let after = slot(&fixtures, &fleet).await;
    let reclaimed = leases
        .claim(&id(&fleet), &second, LEASE_TTL)
        .await
        .expect("answers");

    assert!(contended.is_none(), "one live holder per fleet, as before");
    assert_eq!(
        after,
        (None, None),
        "an empty claim drops the hint, as before"
    );
    assert!(reclaimed.is_some(), "and anyone may claim it next");
    fixtures.cleanup().await;
}
