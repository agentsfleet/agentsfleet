//! §7 against live datastores — an empty assignment commits none of the four.
//!
//! Dimension 7.3's other half. The fence is what a transaction could silently
//! break: wrap four writes around an assignment and it is easy to end up committing
//! the three that carry no guard of their own. Both cases here drive an assignment
//! that matches no row and assert the same emptiness; what differs is WHY the
//! assignment missed, which is the distinction the disposition read exists to make
//! and the reason neither is answered the way a settled run is.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet::run::Committed;

use crate::report_commit::{RESPONSE_ACCEPTED, assert_nothing_landed, report};
use crate::report_seed::held;
use crate::rows::{LAPSE, Row};

/// Dimension 7.3 — a superseded report commits nothing, by either route.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_superseded_report_commits_nothing() {
    let held = held().await;
    let run_id = &held.issued.run_id;
    let runs = held.fixtures.runs_with_dead_bus();
    let run = runs
        .load_for_report(run_id, &held.runner)
        .await
        .expect("the run load must reach the datastore")
        .expect("the seeded run belongs to the seeded runner");

    // The holder dies and its work is taken back, which bumps the fleet's
    // fencing sequence past the token the first run carries.
    held.fixtures
        .age(Row::Thread(&held.fleet), "run_expires_at", LAPSE)
        .await;
    let reassigned = crate::seed::select_fleet(&held.runs, &held.spare, &held.fleet)
        .await
        .expect("a lapsed assignment is winnable");
    assert!(
        reassigned.fence > held.fence,
        "the reassignment must outrank the holder it displaced, or this test proves nothing"
    );

    let fenced = runs
        .commit_report(report(
            run_id,
            &held.runner,
            &run,
            (&held.run_key, &held.pricing),
            RESPONSE_ACCEPTED,
            held.now,
        ))
        .await
        .expect("a fenced report is an answer, not a fault");
    assert!(
        matches!(fenced, Committed::Fenced { .. }),
        "the displaced holder must not win the report; the current holder's result wins"
    );
    assert_nothing_landed(&held, "a superseded holder writes none of the four").await;

    // The same empty assignment for a different reason: the retention sweep removed
    // the row while this very late report was in flight. There is nothing left
    // to report against and nothing stored to hand back, so it is refused like
    // a lost fence rather than acknowledged like a settled run.
    held.fixtures.delete_run(run_id).await;
    let vanished = runs
        .commit_report(report(
            run_id,
            &held.runner,
            &run,
            (&held.run_key, &held.pricing),
            RESPONSE_ACCEPTED,
            held.now,
        ))
        .await
        .expect("a report against a row that is gone is an answer, not a fault");
    assert!(
        matches!(vanished, Committed::Fenced { .. }),
        "a run that no longer exists cannot have settled, so it is refused and not acknowledged"
    );
    assert_nothing_landed(&held, "a run row that is gone writes none of the four").await;

    held.fixtures.cleanup().await;
}
