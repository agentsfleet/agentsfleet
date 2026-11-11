//! One poll, both kinds accepted: the host's freed slot goes to the queued
//! authoring turn when authoring is the kind the host is owed next.
//!
//! The runtime lease the fixture issued made runtime the host's last kind, so
//! the alternation offers authoring first. The poll answers that turn as an
//! authoring lease, through the same response type a runtime lease rides in.

use super::{fixture::Running, replica::expire, seat::Seat};
use af_wire::lease::{AcceptedKinds, TaggedLease};
use afd_fleet::lease::OfferRequest;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_poll_accepting_both_kinds_places_the_owed_authoring_turn() {
    let running = Running::create_isolated().await;
    let seat = Seat::create(&running).await;
    let drafts = Seat::drafts(&running.fixtures.database);
    let draft = seat.queue(&drafts).await;
    expire(&running).await;

    let answer = running
        .fixtures
        .plane()
        .claim_with(
            &drafts,
            OfferRequest {
                runner: &running.runner,
                held: &[],
                degraded: false,
                now: running.now,
            },
            AcceptedKinds::ALL,
            &mut af_observability::verb_span::VerbClock::untimed(),
            |lease, _metadata| match lease {
                TaggedLease::Authoring(lease) => Some(lease),
                TaggedLease::Fleet(_) => None,
            },
        )
        .await
        .expect("the poll answers");

    let authoring = answer.flatten().expect("the owed authoring turn is placed");
    assert_eq!(authoring.draft_id, draft.id.as_str());
    seat.remove(&running.fixtures.database).await;
    running.cleanup().await;
}
