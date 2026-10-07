//! What a beat's holds list does to the slots: it reconciles the beating
//! runner's holds and no one else's, an entry that is no identifier is dropped
//! alone, and a list past its bounds reconciles nothing.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;
use afd_core::timing::SANDBOX_HOLD_IDLE_MS;
use afd_db::test_util::mint_id;
use afd_runner::{Beat, NO_REPORT};
use afd_wire::runner::{FLEET_ID_TEXT_BYTES, HOLDS_MAX, HeartbeatRequest};

use super::{at, beat, hold, live, slot};
use crate::requests::ENROLLED_AT;
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// When every hold made here lapses.
const UNTIL: i64 = ENROLLED_AT + SANDBOX_HOLD_IDLE_MS;

/// `runner` beats `request` one millisecond after enrolment.
async fn beats(fixtures: &Fixtures, runner: &Uuid7, request: &HeartbeatRequest<'_>) -> Beat {
    fixtures
        .runners()
        .heartbeat(runner, request, at(ENROLLED_AT + 1))
        .await
        .expect("a beat lands")
}

/// Another runner's beat listing nothing clears that runner's own hold and
/// leaves this one's standing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_beat_clears_only_its_own_runners_holds() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [holder, other]) = seeded_parts::<2>(&fixtures).await;
    let (others_fleet, ..) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(&leases, &fixtures, &fleet, &holder, UNTIL).await;
    hold(&leases, &fixtures, &others_fleet, &other, UNTIL).await;

    beats(&fixtures, &other, &beat(&[])).await;

    assert_eq!(slot(&fixtures, &others_fleet).await.0, None, "its own");
    assert_eq!(
        slot(&fixtures, &fleet).await,
        (Some(UNTIL), Some(holder.as_str().to_owned())),
        "the holder's hold is not the other runner's to clear"
    );
    fixtures.cleanup().await;
}

/// An entry the right length that is no identifier is dropped, and the rest
/// of the list still reconciles: a held fleet it leaves out is cleared, and
/// the dropped entry is not answered back.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_hold_that_is_no_identifier_is_dropped_alone() {
    let fixtures = Fixtures::create_with_queue().await;
    let (kept, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let (dropped, ..) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(&leases, &fixtures, &kept, &holder, UNTIL).await;
    hold(&leases, &fixtures, &dropped, &holder, UNTIL).await;
    let not_an_id = "x".repeat(FLEET_ID_TEXT_BYTES);

    let answered = beats(&fixtures, &holder, &beat(&[&kept, &not_an_id])).await;

    assert!(answered.release_holds.is_empty(), "{answered:?}");
    assert_eq!(slot(&fixtures, &kept).await.0, Some(UNTIL));
    assert_eq!(slot(&fixtures, &dropped).await.0, None);
    fixtures.cleanup().await;
}

/// A list past its bounds reads as nothing reported and reconciles nothing:
/// the hold it leaves out stands. A beat whose body could not be read at all
/// reads as an empty list instead, which clears every hold the runner has.
/// The asymmetry is pinned here as the code has it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_out_of_bounds_holds_list_reconciles_nothing() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(&leases, &fixtures, &fleet, &holder, UNTIL).await;
    let stranger = mint_id();
    let too_many = vec![stranger.as_str(); HOLDS_MAX + 1];
    let wrong_length = [stranger.as_str(), "not-a-fleet"];

    let over = beats(&fixtures, &holder, &beat(&too_many)).await;
    let short = beats(&fixtures, &holder, &beat(&wrong_length)).await;
    let after_bounds = slot(&fixtures, &fleet).await.0;
    let unreadable = beats(&fixtures, &holder, &NO_REPORT).await;

    assert!(over.release_holds.is_empty() && short.release_holds.is_empty());
    assert_eq!(after_bounds, Some(UNTIL), "out of bounds clears nothing");
    assert!(
        unreadable.release_holds.is_empty(),
        "an empty list names nothing to release: {unreadable:?}"
    );
    assert_eq!(
        slot(&fixtures, &fleet).await.0,
        None,
        "an unreadable body reads as holding nothing, and clears the hold"
    );
    fixtures.cleanup().await;
}
