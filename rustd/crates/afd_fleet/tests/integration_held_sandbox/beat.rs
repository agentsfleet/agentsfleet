//! What a beat's holds list does to the slots: it reconciles the beating
//! runner's holds and no one else's, an entry that is no identifier is dropped
//! alone, a list past its bounds holds nothing, and a hold reported inside the
//! last beat interval outlives the beat that raced it.

use afd_core::id::Uuid7;
use afd_core::timing::{HEARTBEAT_INTERVAL_MS, SANDBOX_HOLD_IDLE_MS};
use afd_db::test_util::mint_id;
use afd_runner::{Beat, NO_REPORT};
use afd_wire::runner::{FLEET_ID_TEXT_BYTES, HOLDS_MAX, HeartbeatRequest};

use super::{beat, beats_at, hold, live, slot};
use crate::requests::ENROLLED_AT;
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// When every hold made here lapses.
const UNTIL: i64 = ENROLLED_AT + SANDBOX_HOLD_IDLE_MS;

/// The first beat that may clear a hold reported at enrolment: one full
/// interval later.
const SETTLED: i64 = ENROLLED_AT + HEARTBEAT_INTERVAL_MS;

/// `runner` beats `request` once every hold made here has settled.
async fn beats(fixtures: &Fixtures, runner: &Uuid7, request: &HeartbeatRequest<'_>) -> Beat {
    beats_at(fixtures, runner, request, SETTLED).await
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

/// A list past its bounds, by count or by an entry's length, reads as
/// holding nothing, as an unreadable body does: each clears its runner's
/// hold. A standing hold blocks the fleet for every other runner, while a
/// hold cleared by mistake costs only a cold start.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_out_of_bounds_holds_list_holds_nothing() {
    let fixtures = Fixtures::create_with_queue().await;
    let leases = fixtures.leases();
    let stranger = mint_id();
    let too_many = vec![stranger.as_str(); HOLDS_MAX + 1];
    let wrong_length = [stranger.as_str(), "not-a-fleet"];
    let shapes = [
        ("too many", beat(&too_many)),
        ("an entry too short", beat(&wrong_length)),
        ("unreadable", NO_REPORT),
    ];

    for (shape, request) in &shapes {
        let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
        live(&fixtures, &holder).await;
        hold(&leases, &fixtures, &fleet, &holder, UNTIL).await;

        let answered = beats(&fixtures, &holder, request).await;

        assert!(answered.release_holds.is_empty(), "{answered:?}");
        assert_eq!(slot(&fixtures, &fleet).await.0, None, "{shape}");
    }
    fixtures.cleanup().await;
}

/// A runner lists its holds before its beat goes out, so a report that
/// commits a hold in between reaches a beat whose list leaves it out. The
/// slot was written inside the last interval, so that beat leaves it alone;
/// the next beat clears it if the list still leaves it out.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_hold_reported_inside_the_beat_interval_survives_that_beat() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _workspace, _tenant, [holder]) = seeded_parts::<1>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(&leases, &fixtures, &fleet, &holder, UNTIL).await;

    beats_at(&fixtures, &holder, &beat(&[]), ENROLLED_AT + 1).await;
    let raced = slot(&fixtures, &fleet).await.0;
    beats(&fixtures, &holder, &beat(&[])).await;

    assert_eq!(raced, Some(UNTIL), "the beat that raced the report");
    assert_eq!(
        slot(&fixtures, &fleet).await.0,
        None,
        "a beat a full interval later clears a hold still unlisted"
    );
    fixtures.cleanup().await;
}
