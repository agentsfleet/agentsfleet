//! A workspace's live set is read fresh: a fleet created or deleted shows in the
//! very next read, since a wall reads it on the notice that change publishes, and
//! an outage answers as an outage. `#[ignore]`d where it needs the lane's
//! Postgres; `make test-integration-rustd` runs it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;

use af_core::error_code;

use crate::support::Lane;

#[tokio::test]
#[ignore = "needs the lane's Postgres"]
async fn each_read_sees_the_fleets_committed_before_it() {
    let lane = Lane::create().await;
    let first = lane.fleet().await;
    let set = |fleets: &[&af_core::id::Uuid7]| -> BTreeSet<String> {
        fleets.iter().map(ToString::to_string).collect()
    };
    let read = lane.fleets.live_set(&lane.workspace).await;
    assert_eq!(read.expect("readable"), set(&[&first]));

    let second = lane.fleet().await;
    let read = lane.fleets.live_set(&lane.workspace).await;
    assert_eq!(
        read.expect("readable"),
        set(&[&first, &second]),
        "a fleet created a moment ago is in the next read"
    );

    lane.delete(&lane.workspace, &first)
        .await
        .expect("the fleet deletes");
    let read = lane.fleets.live_set(&lane.workspace).await;
    assert_eq!(
        read.expect("readable"),
        set(&[&second]),
        "a deleted fleet is gone from the next read"
    );
    lane.cleanup().await;
}

#[tokio::test]
async fn a_read_on_a_dead_database_is_an_outage() {
    let fleets = Lane::with_dead_database();
    let workspace = crate::support::mint();
    let error = fleets
        .live_set(&workspace)
        .await
        .expect_err("nobody answers on the dead database");
    assert_eq!(error.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    assert!(
        error.is_datastore_unavailable(),
        "an outage stays an outage"
    );
}
