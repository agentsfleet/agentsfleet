//! The fleet list reads each fleet's saved purpose from its head revision.
//!
//! A fleet with a saved setup lists that setup's `description`; a fleet with
//! none lists `None`, not an empty string, so a client can tell the two apart.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::id::Uuid7;

use crate::support::Lane;

/// The purpose the shared fixture setup declares (`FIXTURE_SETUP_DEFINITION`).
const FIXTURE_PURPOSE: &str = "Runtime fixture.";

/// A page larger than the lane seeds.
const WHOLE_PAGE: u32 = 50;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres and Dragonfly"]
async fn the_fleet_page_lists_the_head_revisions_purpose() {
    let lane = Lane::create().await;
    let saved = lane.fleet().await;
    let unsaved = lane.fleet_without_setup().await;

    let rows = lane
        .fleets
        .page(&lane.workspace, None, WHOLE_PAGE)
        .await
        .expect("the workspace pages")
        .rows;
    let purpose_of = |fleet: &Uuid7| {
        rows.iter()
            .find(|row| row.id == fleet.as_str())
            .expect("every seeded fleet lists")
            .description
            .as_deref()
    };

    assert_eq!(purpose_of(&saved), Some(FIXTURE_PURPOSE));
    assert_eq!(
        purpose_of(&unsaved),
        None,
        "a fleet with no saved setup has no purpose to list"
    );

    lane.cleanup().await;
}
