//! What a beat's parks list does to the threads: it reconciles the beating
//! runner's parks and no one else's, an entry that is no identifier is dropped
//! alone, a list past its bounds parks nothing, and a park reported inside the
//! last beat interval outlives the beat that raced it, unless that beat is a
//! closing runner's final list.

use af_core::id::Uuid7;
use afd_runner::timing::SANDBOX_PARK_IDLE;

use af_wire::runner::{FLEET_ID_TEXT_BYTES, HeartbeatRequest, PARKED_MAX};
use afd_db::test_util::mint_id;
use afd_runner::{Beat, NO_REPORT};
use jiff::Timestamp;

use super::{beat, beats_at, live, park, settle, thread};
use crate::seed::seeded_parts;
use crate::support::Fixtures;

/// When every park made here lapses.
fn until(fixtures: &Fixtures) -> Timestamp {
    fixtures.after(SANDBOX_PARK_IDLE)
}

/// `runner` beats `request`.
async fn beats(fixtures: &Fixtures, runner: &Uuid7, request: &HeartbeatRequest<'_>) -> Beat {
    beats_at(fixtures, runner, request, fixtures.enrolled_at()).await
}

/// Another runner's beat listing nothing clears that runner's own park and
/// leaves this one's standing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_beat_clears_only_its_own_runners_parks() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [keeper, other]) = seeded_parts::<2>(&fixtures).await;
    let (others_fleet, ..) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(&runs, &fixtures, &fleet, &keeper, until(&fixtures)).await;
    park(&runs, &fixtures, &others_fleet, &other, until(&fixtures)).await;
    settle(&fixtures, &[&fleet, &others_fleet]).await;

    beats(&fixtures, &other, &beat(&[])).await;

    assert_eq!(thread(&fixtures, &others_fleet).await.0, None, "its own");
    assert_eq!(
        thread(&fixtures, &fleet).await,
        (Some(until(&fixtures)), Some(keeper.clone())),
        "the keeper's park is not the other runner's to clear"
    );
    fixtures.cleanup().await;
}

/// An entry the right length that is no identifier is dropped, and the rest
/// of the list still reconciles: a parked fleet it leaves out is cleared, and
/// the dropped entry is not answered back.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_park_that_is_no_identifier_is_dropped_alone() {
    let fixtures = Fixtures::create().await;
    let (kept, _workspace, _tenant, [keeper]) = seeded_parts::<1>(&fixtures).await;
    let (dropped, ..) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(&runs, &fixtures, &kept, &keeper, until(&fixtures)).await;
    park(&runs, &fixtures, &dropped, &keeper, until(&fixtures)).await;
    let not_an_id = "x".repeat(FLEET_ID_TEXT_BYTES);
    settle(&fixtures, &[&kept, &dropped]).await;

    let answered = beats(&fixtures, &keeper, &beat(&[kept.as_str(), &not_an_id])).await;

    assert!(answered.release_parked.is_empty(), "{answered:?}");
    assert_eq!(thread(&fixtures, &kept).await.0, Some(until(&fixtures)));
    assert_eq!(thread(&fixtures, &dropped).await.0, None);
    fixtures.cleanup().await;
}

/// A list past its bounds, by count or by an entry's length, reads as
/// parking nothing, as an unreadable body does: each clears its runner's
/// park. A standing park blocks the fleet for every other runner, while a
/// park cleared by mistake costs only a cold start.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_out_of_bounds_parks_list_parks_nothing() {
    let fixtures = Fixtures::create().await;
    let runs = fixtures.runs();
    let stranger = mint_id();
    let too_many = vec![stranger.as_str(); PARKED_MAX + 1];
    let wrong_length = [stranger.as_str(), "not-a-fleet"];
    let shapes = [
        ("too many", beat(&too_many)),
        ("an entry too short", beat(&wrong_length)),
        ("unreadable", NO_REPORT),
    ];

    for (shape, request) in &shapes {
        let (fleet, _workspace, _tenant, [keeper]) = seeded_parts::<1>(&fixtures).await;
        live(&fixtures, &keeper).await;
        park(&runs, &fixtures, &fleet, &keeper, until(&fixtures)).await;
        settle(&fixtures, &[&fleet]).await;

        let answered = beats(&fixtures, &keeper, request).await;

        assert!(answered.release_parked.is_empty(), "{answered:?}");
        assert_eq!(thread(&fixtures, &fleet).await.0, None, "{shape}");
    }
    fixtures.cleanup().await;
}

/// A runner lists its parks before its beat goes out, so a report that
/// commits a park in between reaches a beat whose list leaves it out. The
/// thread was written inside the last interval, so that beat leaves it alone;
/// the next beat clears it if the list still leaves it out.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_park_reported_inside_the_beat_interval_survives_that_beat() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [keeper]) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(&runs, &fixtures, &fleet, &keeper, until(&fixtures)).await;

    beats(&fixtures, &keeper, &beat(&[])).await;
    let raced = thread(&fixtures, &fleet).await.0;
    settle(&fixtures, &[&fleet]).await;
    beats(&fixtures, &keeper, &beat(&[])).await;

    assert_eq!(
        raced,
        Some(until(&fixtures)),
        "the beat that raced the report"
    );
    assert_eq!(
        thread(&fixtures, &fleet).await.0,
        None,
        "a beat a full interval later clears a park still unlisted"
    );
    fixtures.cleanup().await;
}

/// A closing runner has ended every park and parks no more, so its list is
/// final: the first beat that leaves a park out clears it, even one reported
/// inside the interval, rather than leave the fleet bound to a stopping
/// runner until it reads offline.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_closing_beat_clears_a_park_reported_inside_the_interval() {
    let fixtures = Fixtures::create().await;
    let (fleet, _workspace, _tenant, [keeper]) = seeded_parts::<1>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(&runs, &fixtures, &fleet, &keeper, until(&fixtures)).await;
    let closing = HeartbeatRequest {
        closing: true,
        host: af_wire::runner::HostFacts::default(),
        ..beat(&[])
    };

    beats(&fixtures, &keeper, &closing).await;

    assert_eq!(thread(&fixtures, &fleet).await.0, None);
    fixtures.cleanup().await;
}
