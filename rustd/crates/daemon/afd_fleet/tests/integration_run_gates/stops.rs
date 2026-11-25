//! A refusal frees the assignment.

use super::*;

use crate::rows::Row;
use crate::run_reads::COLUMN_LAST_RUNNER;

/// An event type this build does not know, refused before any money is read.
const EVENT_TYPE_UNKNOWN: &str = "fixture-unknown";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_stop_releases_the_assignment() {
    // A refusal used to return with the assignment still held, so the fleet's next
    // event waited out the assignment's lifetime behind an event that was over.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create().await;
    let (fleet, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let turn_id = send(&fixtures, &fleet, &workspace, EVENT_TYPE_UNKNOWN).await;
    let seeded = Ready {
        runner,
        fleet,
        turn_id,
        tenant,
    };

    let assigned = assign(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, assigned).await;
    assert!(answer.is_none(), "a refused event issued a run: {answer:?}");
    assert!(
        fixtures.thread_is_free(&seeded.fleet).await,
        "the refusal freed the assignment at once, not an assignment's lifetime later"
    );
    assert_eq!(
        fixtures
            .text(Row::Thread(&seeded.fleet), COLUMN_LAST_RUNNER)
            .await,
        None,
        "an assignment that ran nothing leaves no sticky hint, or a fleet that keeps \
         stopping would be tried first for this runner on every poll"
    );

    // One millisecond on is the next poll; a held assignment would refuse it.
    let next = send(&fixtures, &seeded.fleet, &workspace, EVENT_TYPE_CHAT).await;
    let held = crate::seed::select_fleet(&fixtures.runs(), &seeded.runner, &seeded.fleet)
        .await
        .expect("the fleet's next event runs on the next poll");
    assert_eq!(held.turn_id, next);

    fixtures.cleanup().await;
}

/// Ingress for one event of `event_type`, marking the fleet as ingress does.
async fn send(fixtures: &Fixtures, fleet: &Uuid7, workspace: &Uuid7, event_type: &str) -> Uuid7 {
    crate::queue::enqueue(
        &fixtures.database,
        fleet,
        workspace,
        ACTOR,
        event_type,
        REQUEST_JSON,
        fixtures.enrolled_at(),
    )
    .await
}
