//! §7 against live datastores — an empty claim commits none of the four.
//!
//! Dimension 7.3's other half. The fence is what a transaction could silently
//! break: wrap four writes around a claim and it is easy to end up committing
//! the three that carry no guard of their own. Both cases here drive a claim
//! that matches no row and assert the same emptiness; what differs is WHY the
//! claim missed, which is the distinction the disposition read exists to make
//! and the reason neither is answered the way a settled lease is.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet::lease::Committed;

use crate::queue;
use crate::report_commit::{RESPONSE_ACCEPTED, assert_nothing_landed, report};
use crate::report_seed::held;

/// Dimension 7.3 — a superseded report commits nothing, by either route.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_superseded_report_commits_nothing() {
    let held = held().await;
    let lease_id = held.issued.lease_id.as_str();
    let leases = held.fixtures.leases_with_dead_queue();
    let lease = leases
        .load_for_report(lease_id, &held.runner)
        .await
        .expect("the lease load must reach the datastore")
        .expect("the seeded lease belongs to the seeded runner");

    // The holder dies and its work is taken back, which bumps the fleet's
    // fencing sequence past the token the first lease carries.
    let lapsed = held
        .now
        .saturating_add_millis(afd_core::timing::LEASE_TTL_MS + 1);
    let reclaimed =
        crate::seed::select_fleet_within_rotations(&held.leases, &held.spare, lapsed, &held.fleet)
            .await
            .expect("a lapsed claim is winnable");
    assert!(
        reclaimed.fence > held.fence,
        "the reclaim must outrank the holder it displaced, or this test proves nothing"
    );

    let fenced = leases
        .commit_report(report(
            lease_id,
            &held.runner,
            &lease,
            RESPONSE_ACCEPTED,
            lapsed,
        ))
        .await
        .expect("a fenced report is an answer, not a fault");
    assert!(
        matches!(fenced, Committed::Fenced),
        "the displaced holder must not win the report; the current holder's result wins"
    );
    assert_nothing_landed(&held, "a superseded holder writes none of the four").await;

    // The same empty claim for a different reason: the retention sweep removed
    // the row while this very late report was in flight. There is nothing left
    // to report against and nothing stored to hand back, so it is refused like
    // a lost fence rather than acknowledged like a settled lease.
    held.fixtures.delete_lease(lease_id).await;
    let vanished = leases
        .commit_report(report(
            lease_id,
            &held.runner,
            &lease,
            RESPONSE_ACCEPTED,
            lapsed,
        ))
        .await
        .expect("a report against a row that is gone is an answer, not a fault");
    assert!(
        matches!(vanished, Committed::Fenced),
        "a lease that no longer exists cannot have settled, so it is refused and not acknowledged"
    );
    assert_nothing_landed(&held, "a lease row that is gone writes none of the four").await;

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}
