//! A secret is available to the runnable fleets a person approved it for.
//!
//! The secrets page shows this list beside each secret, and a run resolves the
//! same approved grants, so a pending request, a revoked grant or an archived
//! fleet never reads as access.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_api_wire::grant::{kind, status};
use afd_db::test_util::GrantSeed;

use crate::support::Lane;

/// The secret every grant here names.
const SECRET: &str = "sentry";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn only_an_approved_grant_on_a_runnable_fleet_lists_access() {
    let lane = Lane::create().await;
    let approved = lane.fleet().await;
    let pending = lane.fleet().await;
    let revoked = lane.fleet().await;
    let archived = lane.fleet().await;
    for (fleet, grant_status) in [
        (&approved, status::APPROVED),
        (&pending, status::PENDING),
        (&revoked, status::REVOKED),
        (&archived, status::APPROVED),
    ] {
        GrantSeed::new(fleet, SECRET, kind::SECRET, grant_status)
            .insert(&mut *lane.connection().await)
            .await
            .expect("seeding a grant");
    }
    lane.archive(&archived).await;

    let access = lane
        .fleets
        .secret_access(&lane.workspace)
        .await
        .expect("the workspace's access reads");

    let holders: Vec<_> = access
        .get(SECRET)
        .expect("the approved grant lists its secret")
        .iter()
        .map(|fleet| fleet.id.clone())
        .collect();
    assert_eq!(holders, [approved]);
    assert_eq!(access.len(), 1, "no other name is listed");

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the lane's Postgres"]
async fn another_workspaces_grant_lists_no_access_here() {
    let lane = Lane::create().await;
    let elsewhere = lane.another_workspace().await;
    let foreign = lane.fleet_at(&elsewhere, crate::support::NOW).await;
    GrantSeed::new(&foreign, SECRET, kind::SECRET, status::APPROVED)
        .insert(&mut *lane.connection().await)
        .await
        .expect("seeding a grant");

    let access = lane
        .fleets
        .secret_access(&lane.workspace)
        .await
        .expect("the workspace's access reads");

    assert!(
        access.is_empty(),
        "a grant in another workspace leaks nothing"
    );
    lane.cleanup().await;
}
