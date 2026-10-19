//! One poll, both kinds accepted: the host's freed slot goes to the queued
//! authoring turn when authoring is the kind the host is owed next.
//!
//! The runtime lease the fixture issued made runtime the host's last kind, so
//! the alternation offers authoring first. The poll answers that turn as an
//! authoring lease, through the same response type a runtime lease rides in.

use super::{fixture::Running, replica::expire, seat::Seat};
use afd_wire::lease::{AcceptedKinds, LeaseResponse, TaggedLease};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_poll_accepting_both_kinds_places_the_owed_authoring_turn() {
    let running = Running::create().await;
    let seat = Seat::create(&running).await;
    let drafts = Seat::drafts(&running.fixtures.database);
    let draft = seat.queue(&drafts).await;
    expire(&running).await;

    let answer = running
        .fixtures
        .plane()
        .claim(
            &drafts,
            &running.runner,
            &[],
            AcceptedKinds::ALL,
            false,
            running.now,
        )
        .await
        .expect("the poll answers");

    let response: LeaseResponse<'_> =
        serde_json::from_str(&answer).expect("the poll answers a lease response");
    assert!(
        matches!(response.lease, Some(TaggedLease::Authoring(_))),
        "the owed authoring turn is placed: {answer}"
    );
    assert_eq!(response.retry_after_ms, None);
    assert!(answer.contains(draft.id.as_str()), "{answer}");
    seat.remove(&running.fixtures.database).await;
    running.cleanup().await;
}
