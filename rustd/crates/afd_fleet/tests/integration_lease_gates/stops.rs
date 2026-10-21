//! A refusal frees the claim; a gate parks its admission until the answer.

use super::seed::{seed_gate, seed_provider_resolution};
use super::*;

use afd_admission::Admissions;
use afd_approval::{Decision, Inbox, Resolution};
use afd_crypto::entropy::Entropy;

use crate::lease_reads::COLUMN_LAST_RUNNER;
use crate::rows::Row;

/// An event type this build does not know, refused before any money is read.
const EVENT_TYPE_UNKNOWN: &str = "fixture-unknown";

/// Who answers the fixture's gate.
const REVIEWER: &str = "fixture:human";

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_stop_releases_the_claim() {
    // A refusal used to return with the claim still held, so the fleet's next
    // event waited out the claim's lifetime behind an event that was over.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    set_config(&fixtures, &fleet, BUDGETED_CONFIG).await;
    let event_id = send(&fixtures, &fleet, &workspace, EVENT_TYPE_UNKNOWN).await;
    let seeded = Ready {
        runner,
        fleet,
        event_id,
        tenant,
    };

    let claimed = claim(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, claimed).await;
    assert!(
        answer.contains(NO_LEASE),
        "a refused event issued a lease: {answer}"
    );
    assert!(
        fixtures.slot_is_free(&seeded.fleet).await,
        "the refusal freed the claim at once, not a claim's lifetime later"
    );
    assert_eq!(
        fixtures
            .text(Row::Slot(&seeded.fleet), COLUMN_LAST_RUNNER)
            .await,
        None,
        "a claim that ran nothing leaves no sticky hint, or a fleet that keeps \
         stopping would be tried first for this runner on every poll"
    );

    // One millisecond on is the next poll; a held claim would refuse it.
    let next = send(&fixtures, &seeded.fleet, &workspace, EVENT_TYPE_CHAT).await;
    let leased = crate::seed::select_fleet(&fixtures.leases(), &seeded.runner, &seeded.fleet)
        .await
        .expect("the fleet's next event leases on the next poll");
    assert_eq!(leased.event_id, next);

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_approval_resolution_leases_next_poll() {
    // A park frees the slot. Resolution un-parks the same admission before
    // landing its continuation, so the original delivery leases first.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, BUDGETED_CONFIG).await;
    let _default = seed_provider_resolution(&fixtures, &seeded.fleet).await;
    let action = seed_gate(&fixtures, &seeded, "pending").await;

    let claimed = claim(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, claimed).await;
    assert!(
        answer.contains(NO_LEASE),
        "an unanswered gate issued a lease: {answer}"
    );
    assert!(
        fixtures.slot_is_free(&seeded.fleet).await,
        "the park freed the claim"
    );
    assert!(
        parked(&fixtures, &seeded.event_id).await,
        "the admission is durably parked"
    );

    let resolved = Inbox::new(
        fixtures.database.clone(),
        fixtures.queue().clone(),
        Admissions::new(
            fixtures.database.clone(),
            fixtures.queue().clone(),
            Entropy::new(),
        ),
    )
    .resolve(
        &action,
        Decision::Approved,
        REVIEWER,
        "",
        Some(&seeded.fleet),
    )
    .await
    .expect("the resolve must not fault");
    assert!(matches!(resolved, Resolution::Resolved(_)), "{resolved:?}");
    assert!(
        !parked(&fixtures, &seeded.event_id).await,
        "the answer durably unparked the admission"
    );

    let leased = crate::seed::select_fleet(&fixtures.leases(), &seeded.runner, &seeded.fleet)
        .await
        .expect("the answered delivery leases on the next poll");
    assert_eq!(
        leased.event_id, seeded.event_id,
        "the parked delivery comes back first, ahead of its continuation"
    );

    fixtures.cleanup().await;
}

/// Ingress for one event of `event_type`, marking the fleet as ingress does.
async fn send(fixtures: &Fixtures, fleet: &str, workspace: &str, event_type: &str) -> String {
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

async fn parked(fixtures: &Fixtures, event: &str) -> bool {
    let mut connection = fixtures.database.acquire().await.expect("database");
    sqlx::query_scalar("SELECT parked_at IS NOT NULL FROM fleet.admissions WHERE id = $1::uuid")
        .bind(event)
        .fetch_one(&mut *connection)
        .await
        .expect("park state")
}
