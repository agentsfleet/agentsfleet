//! What a pass that issues no lease leaves behind: a freed claim, and — when
//! it parks on a person — a cleared mark that the answer puts back.

use super::seed::{seed_gate, seed_provider_resolution};
use super::*;

use afd_admission::Admissions;
use afd_approval::{Decision, Inbox, Resolution};
use afd_crypto::entropy::Entropy;
use afd_dragonfly::ReadyIndex;

use crate::lease_reads::COLUMN_LEASED_UNTIL;

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
    assert_eq!(
        fixtures
            .affinity_column(&seeded.fleet, COLUMN_LEASED_UNTIL)
            .await,
        Some(ENROLLED_AT.to_string()),
        "the refusal freed the claim at its own instant, not a claim's lifetime later"
    );

    // One millisecond on is the next poll; a held claim would refuse it.
    let next = send(&fixtures, &seeded.fleet, &workspace, EVENT_TYPE_CHAT).await;
    let leased = crate::seed::select_fleet_within_rotations(
        &fixtures.leases(),
        &seeded.runner,
        UnixMillis::from_millis(ENROLLED_AT + 1),
        &seeded.fleet,
    )
    .await
    .expect("the fleet's next event leases on the next poll");
    assert_eq!(leased.event_id, next);

    crate::queue::clear_ready(fixtures.queue(), &seeded.fleet).await;
    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_approval_resolution_leases_next_poll() {
    // A park frees the claim and clears the fleet's mark, because a person owes
    // the answer and polling meanwhile would re-ask the same question. The
    // answer is what puts the mark back, so the next poll leases the delivery.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, BUDGETED_CONFIG).await;
    seed_provider_resolution(&fixtures, &seeded.fleet).await;
    let action = seed_gate(&fixtures, &seeded, "pending").await;

    let claimed = claim(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, claimed).await;
    assert!(
        answer.contains(NO_LEASE),
        "an unanswered gate issued a lease: {answer}"
    );
    assert_eq!(
        fixtures
            .affinity_column(&seeded.fleet, COLUMN_LEASED_UNTIL)
            .await,
        Some(ENROLLED_AT.to_string()),
        "the park freed the claim"
    );
    assert!(
        !marked(&fixtures, &seeded.fleet).await,
        "the park cleared the mark"
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
        UnixMillis::from_millis(ENROLLED_AT + 1),
    )
    .await
    .expect("the resolve must not fault");
    assert!(matches!(resolved, Resolution::Resolved(_)), "{resolved:?}");
    assert!(
        marked(&fixtures, &seeded.fleet).await,
        "the answer re-marked the fleet"
    );

    let leased = crate::seed::select_fleet_within_rotations(
        &fixtures.leases(),
        &seeded.runner,
        UnixMillis::from_millis(ENROLLED_AT + 2),
        &seeded.fleet,
    )
    .await
    .expect("the answered delivery leases on the next poll");
    assert_eq!(
        leased.event_id, seeded.event_id,
        "the parked delivery comes back first, ahead of its continuation"
    );

    crate::queue::clear_ready(fixtures.queue(), &seeded.fleet).await;
    fixtures.cleanup().await;
}

/// Ingress for one event of `event_type`, marking the fleet as ingress does.
async fn send(fixtures: &Fixtures, fleet: &str, workspace: &str, event_type: &str) -> String {
    crate::queue::enqueue(
        fixtures.queue(),
        fleet,
        workspace,
        ACTOR,
        event_type,
        REQUEST_JSON,
        ENROLLED_AT,
    )
    .await
}

/// Whether `fleet` holds a readiness mark.
async fn marked(fixtures: &Fixtures, fleet: &str) -> bool {
    ReadyIndex::new(fixtures.queue().clone())
        .token_for(fleet)
        .await
        .expect("the ready index is readable")
        .is_some()
}
