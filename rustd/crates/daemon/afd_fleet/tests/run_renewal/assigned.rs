//! One poll, both kinds accepted: the host's freed slot goes to the queued
//! authoring turn when authoring is the kind the host is owed next.
//!
//! The runtime run the fixture issued made runtime the host's last kind, so
//! the alternation assigns authoring first. The poll answers that turn as an
//! authoring run, through the same response type a runtime run rides in.

use super::{fixture::Running, replica::expire, seat::Seat};
use af_wire::run::{AcceptedKinds, TaggedRun};
use afd_fleet::run::AssignRequest;

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
            AssignRequest {
                runner: &running.runner,
                held: &[],
                degraded: false,
                now: running.now,
                recovery: None,
            },
            AcceptedKinds::ALL,
            false,
            &mut af_observability::verb_span::VerbClock::untimed(),
            |run, _metadata| match run {
                TaggedRun::Authoring(run) => Some(run),
                TaggedRun::Fleet(_) => None,
            },
        )
        .await
        .expect("the poll answers");

    let authoring = answer.flatten().expect("the owed authoring turn is placed");
    assert_eq!(authoring.draft_id, draft.id.as_str());
    seat.remove(&running.fixtures.database).await;
    running.cleanup().await;
}
