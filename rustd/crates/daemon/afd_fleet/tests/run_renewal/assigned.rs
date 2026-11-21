//! One poll, both kinds accepted: the host's freed thread goes to the queued
//! setup turn when setup is the kind the host is owed next.
//!
//! The runtime run the fixture issued made runtime the host's last kind, so
//! the alternation assigns setup first. The poll answers that turn as a run
//! the draft owns, on the same common run path a runtime run rides.

use super::{fixture::Running, replica::expire, seat::Seat};
use af_wire::run::{AcceptedKinds, Owner, TaggedRun};
use afd_fleet::run::AssignRequest;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_poll_accepting_both_kinds_places_the_owed_setup_turn() {
    let running = Running::create_isolated().await;
    let seat = Seat::create(&running).await;
    let drafts = Seat::drafts(&running.fixtures.database);
    let draft = seat.queue(&drafts).await;
    expire(&running).await;

    let answer = running
        .fixtures
        .plane()
        .assign_with(
            &drafts,
            AssignRequest {
                runner: &running.runner,
                held: &[],
                degraded: false,
                now: running.now,
                recovery: None,
            },
            AcceptedKinds::ALL,
            &mut af_observability::verb_span::VerbClock::untimed(),
            |run, _metadata| {
                let TaggedRun::Fleet(run) = run;
                match run.owner {
                    Owner::Draft { draft_id, .. } => Some(draft_id.into_owned()),
                    Owner::Fleet { .. } => None,
                }
            },
        )
        .await
        .expect("the poll answers");

    let setup = answer.flatten().expect("the owed setup turn is placed");
    assert_eq!(setup, draft.id.as_str());
    seat.remove(&running.fixtures.database).await;
    running.cleanup().await;
}
