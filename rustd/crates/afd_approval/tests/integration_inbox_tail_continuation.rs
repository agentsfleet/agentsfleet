//! What the tail says when an approval continues a run.
//!
//! Split from `integration_inbox_tail.rs` at the 350-line cap. The
//! continuation is the half of the resolve with an ORDER to prove — the run
//! opens before the answer is announced — and the half where two writers can
//! reach one row, so its proofs sit together.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_admission::{Admission, Admissions, Key, Producer};
use afd_approval::{Decision, Inbox, Resolution};
use afd_core::clock::UnixMillis;
use afd_dragonfly::SubscriptionHub;
use afd_wire::event::EventType;
use serde_json::json;

use crate::lane::{Lane, NOW_MS, WINDOW_MS, dead_queue, dragonfly_config};
use crate::tail_watch::{
    CONTINUATION_ACTOR_PREFIX, CONTINUATION_BODY, NOTE, OPERATOR, SUBSCRIBE_SETTLE, next_frame,
};

/// An approval opens the continued run on the tail before it announces the
/// answer.
///
/// The common operator path, and the one with an ordering to prove: the
/// continuation row is inserted by the resolve, so the runner's pull finds it
/// already there and announces nothing — the resolve is the one writer that
/// can open it on the tail. A watcher hears `event_received` for the
/// continuation, then `gate_resolved` for the answer, and the count on the
/// answer says the sibling still waits.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_approval_opens_the_continued_run_before_it_announces_the_answer() {
    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);

    let hub = SubscriptionHub::start(dragonfly_config())
        .await
        .expect("the lane's Dragonfly accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", lane.fleet));
    tokio::time::sleep(SUBSCRIBE_SETTLE).await;

    let approved = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let _still_waiting = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let before = afd_events::fleet_counters(&lane.pool, lane.fleet.as_str())
        .await
        .expect("the counters read before the resolve");
    let outcome = lane
        .inbox
        .resolve(&approved, Decision::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");
    let continuation = match outcome {
        Resolution::Resolved(resolved) => resolved.continuation_event_id,
        Resolution::AlreadyResolved(_) | Resolution::NotFound => None,
    }
    .expect("a pending gate is this caller's to answer, and an approval continues its run");

    let opened = next_frame(&mut tail)
        .await
        .expect("the continued run opens on the fleet's tail");
    assert_eq!(opened.get("kind"), Some(&json!("event_received")));
    assert_eq!(opened.get("event_id"), Some(&json!(continuation)));
    assert_eq!(opened.get("event_type"), Some(&json!("continuation")));
    assert_eq!(
        opened.get("actor"),
        Some(&json!(format!(
            "continuation:{}",
            lane.gate_column(&approved, "event_id").await
        )))
    );
    // The continuation's own insert moved the count, and its frame carries
    // the moved figure — read after the row landed, on the same connection.
    assert_eq!(
        opened.get("events_processed"),
        Some(&json!(before.events_processed + 1)),
        "the continued run's frame counts the row it wrote"
    );

    let answered = next_frame(&mut tail)
        .await
        .expect("then the answer reaches the tail");
    assert_eq!(answered.get("kind"), Some(&json!("gate_resolved")));
    assert_eq!(answered.get("status"), Some(&json!("approved")));
    assert_eq!(
        answered.get("pending_approvals"),
        Some(&json!(1)),
        "the sibling gate still waits, and the frame says so"
    );
    // Read AFTER the continuation, so the answer carries the continued run's
    // row too — a read before it would be one short.
    assert_eq!(
        answered.get("events_processed"),
        Some(&json!(before.events_processed + 1)),
        "the answer's snapshot includes the continuation it started"
    );
    assert_eq!(
        answered.get("budget_used_nanos"),
        Some(&json!(before.budget_used_nanos))
    );
}

/// An approval whose continuation the queue refuses is still answered.
///
/// The row moved before the continuation was attempted, so the decision is the
/// operator's whatever the queue does: the answer is announced (into the same
/// queue, which drops it) and the row reads `approved`.
///
/// The continuation SUCCEEDS, which is the guarantee the admission ledger was
/// built for. Its row commits to Postgres before the append is attempted, so a
/// queue that will not take the entry leaves an admitted row with a NULL
/// receipt and the replay sweeper owes it one. Reporting a failure here would
/// now be a lie: the run restarts when the queue comes back. Before the ledger
/// the entry WAS the acceptance, so a refused append lost the continuation and
/// an error was the only honest answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_approval_whose_continuation_the_queue_refuses_is_still_answered() {
    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);
    let inbox = Inbox::new(
        lane.pool.clone(),
        dead_queue(),
        afd_admission::Admissions::for_tests(lane.pool.clone(), dead_queue()),
    );
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;

    let outcome = inbox
        .resolve(&action, Decision::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("a queue that refuses the entry does not lose the continuation");
    assert_eq!(lane.status_of(&action).await, "approved");
    let continuation = match outcome {
        Resolution::Resolved(resolved) => resolved.continuation_event_id,
        Resolution::AlreadyResolved(_) | Resolution::NotFound => None,
    };
    assert!(
        continuation.is_some(),
        "the continuation has a logical id even though no entry carries it yet"
    );
    assert!(
        lane.awaits_replay(&action).await,
        "the continuation is admitted with no receipt, so the sweeper owes it an entry"
    );
}

/// A continuation the lease path already opened is not announced twice.
///
/// Both writers run `INSERT_FLEET_EVENT` against `(fleet_id, event_id)`, and
/// the admission ledger hands them the SAME id, because the gate's action is
/// the repeated key. Whichever lands second converges on the first's row,
/// `RETURNING (xmax = 0)` reports `inserted = false`, and the frame must stay
/// home: a console that heard `event_received` twice would count one run as
/// two, and the second frame would carry a counter that never moved.
///
/// Absence is proved by ORDER, not by waiting. The positive case publishes
/// `event_received` BEFORE `gate_resolved`, so a tail whose FIRST frame is
/// `gate_resolved` is a tail the continuation never reached. Sleeping and
/// finding nothing would prove only that the publisher was slow.
///
/// `a_second_answer_does_not_continue_the_run_again` does not cover this: its
/// second resolve is stopped by the gate's `WHERE status = 'pending'` guard
/// before `continue_from` is ever reached.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_converged_continuation_announces_nothing() {
    use sqlx::Row as _;

    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);

    let approved = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let predecessor = lane.gate_column(&approved, "event_id").await;
    let actor = format!("{CONTINUATION_ACTOR_PREFIX}{predecessor}");

    // The lease path, played by hand: the same admission the resolve is about
    // to ask for, then the row it would have written. Keyed on the gate's
    // action, so the resolve's own `admit` returns this very id.
    let admitted = Admissions::for_tests(lane.pool.clone(), lane.queue.clone())
        .admit(Admission {
            producer: Producer::GateContinuation,
            key: Key::Repeated(&approved),
            fleet: lane.fleet.as_str(),
            workspace: lane.workspace.as_str(),
            actor: actor.as_str(),
            event_type: EventType::Continuation,
            request_json: CONTINUATION_BODY,
            reply: afd_admission::Reply::None,
        })
        .await
        .expect("the admission ledger accepts the continuation");
    let landed: bool = sqlx::query(afd_events::sql::INSERT_FLEET_EVENT)
        .bind(lane.fleet.as_str())
        .bind(admitted.id.as_str())
        .bind(lane.workspace.as_str())
        .bind(&actor)
        .bind(EventType::Continuation.as_str())
        .bind(CONTINUATION_BODY)
        .bind(&predecessor)
        .bind(now.as_millis())
        .bind(afd_core::event::status::RECEIVED)
        .fetch_one(&mut *lane.pool.acquire().await.expect("the lane's pool answers"))
        .await
        .expect("the lease path's insert must not fault")
        .try_get(0)
        .expect("the insert returns its `inserted` flag");
    assert!(landed, "the fixture must be the writer that opened the row");

    // Read AFTER the pre-insert: that row moved the counter legitimately, and
    // what this test asserts is that the RESOLVE moves it no further.
    let before = afd_events::fleet_counters(&lane.pool, lane.fleet.as_str())
        .await
        .expect("the counters read before the resolve");

    // Subscribe only now, so the only frames on this tail are the resolve's.
    let hub = SubscriptionHub::start(dragonfly_config())
        .await
        .expect("the lane's Dragonfly accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", lane.fleet));
    tokio::time::sleep(SUBSCRIBE_SETTLE).await;

    let outcome = lane
        .inbox
        .resolve(&approved, Decision::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");
    let continuation = match outcome {
        Resolution::Resolved(resolved) => resolved.continuation_event_id,
        Resolution::AlreadyResolved(_) | Resolution::NotFound => None,
    }
    .expect("an approved gate still reports the run it continues");
    assert_eq!(
        continuation, admitted.id,
        "the resolve converged on the row the lease path opened, it did not mint a second"
    );

    let frame = next_frame(&mut tail)
        .await
        .expect("the answer reaches the fleet's tail");
    assert_eq!(
        frame.get("kind"),
        Some(&json!("gate_resolved")),
        "the first frame is the answer: no `event_received` was published for a row that already stood"
    );
    assert_eq!(frame.get("status"), Some(&json!("approved")));
    assert_eq!(
        frame.get("events_processed"),
        Some(&json!(before.events_processed)),
        "a converged continuation writes no row, so it moves no counter"
    );
}
