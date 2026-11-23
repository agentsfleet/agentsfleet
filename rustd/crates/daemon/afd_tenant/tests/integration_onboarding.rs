//! Onboarding round-trips against live Postgres.
//!
//! The checklist is five `EXISTS` subqueries plus a platform-default read. What
//! is pinned here is the checklist turning over as the workspace fills up, and
//! each signal staying inside its own workspace and tenant.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::onboarding_lane::Lane;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_checklist_turns_over_as_the_workspace_fills_up() {
    let lane = Lane::create().await;

    let empty = lane
        .onboarding
        .signals(&lane.workspace, &lane.tenant)
        .await
        .expect("the signal read must succeed");
    assert!(!empty.has_fleet, "a fresh workspace holds no fleet");
    assert!(!empty.has_secret, "nor a secret");
    assert!(!empty.has_processed_event, "nor an event");
    assert!(!empty.has_steer_event, "nor a steer");

    lane.seed_fleet().await;
    lane.seed_tenant_model("claude-fixture").await;

    let filled = lane
        .onboarding
        .signals(&lane.workspace, &lane.tenant)
        .await
        .expect("the signal read must succeed");
    assert!(filled.has_fleet, "the seeded fleet is seen");
    assert!(
        filled.model_configured,
        "the tenant's own selection configures a model"
    );
    // Untouched by the two writes above: each signal reads its own table, so a
    // fleet appearing must not make a secret appear with it.
    assert!(!filled.has_secret, "seeding a fleet seeds no secret");
    assert!(!filled.has_steer_event, "and no steer event");

    lane.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_signals_are_scoped_to_their_own_workspace_and_tenant() {
    let mine = Lane::create().await;
    let theirs = Lane::create().await;

    mine.seed_fleet().await;
    mine.seed_tenant_model("claude-fixture").await;

    let other = theirs
        .onboarding
        .signals(&theirs.workspace, &theirs.tenant)
        .await
        .expect("the signal read must succeed");

    // The failure this catches is a signal query that dropped its scope: with
    // one shared database, an unscoped `EXISTS` answers true for every
    // workspace the moment ANY workspace holds a fleet.
    assert!(
        !other.has_fleet,
        "another workspace's fleet is not this one's"
    );

    mine.cleanup().await;
    theirs.cleanup().await;
}
