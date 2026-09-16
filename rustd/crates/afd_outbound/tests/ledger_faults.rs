//! What the requeue pass does when the ledger it scans will not answer, and
//! what a producer does with a token that is already cancelled.
//!
//! Neither needs a datastore, which is the point: the live-ledger suite proves
//! what a SUCCESSFUL scan requeues, and by construction never reaches the arm
//! that exists for a ledger which is down. A pool that resolves and never
//! answers reaches it on the first statement.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::{Dragonfly, OutboundQueue};
use afd_outbound::obligation::Owed;
use afd_outbound::producer::Producer;
use tokio_util::sync::CancellationToken;

#[path = "support/no_ledger.rs"]
mod no_ledger;

/// Long enough for a pass to reach its first statement and give up on it,
/// short enough that a producer which never stops fails the test rather than
/// hanging it.
const SHUTDOWN_BUDGET: Duration = Duration::from_secs(10);

/// A queue handle that never opens a socket. The pass under test fails at the
/// LEDGER, before anything is appended, so the queue is only here to satisfy
/// the constructor.
fn unreachable_queue() -> OutboundQueue {
    let config =
        DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:1/".to_owned());
    OutboundQueue::new(Dragonfly::unreachable(&config).expect("a well-formed URL builds a handle"))
}

/// The one place the owned and borrowed halves of an obligation meet. A scan
/// hands its rows to the same append the report path uses, so a field that
/// went missing here would send an answer to the wrong destination rather
/// than fail to compile.
#[test]
fn an_owed_row_addresses_the_delivery_it_came_from() {
    let owed = Owed {
        id: "01998000-0000-7000-8000-00000000000a".to_owned(),
        fleet_id: "0199a0b0-0000-7000-8000-0000000000f1".to_owned(),
        workspace_id: "workspace-7".to_owned(),
        provider: "slack".to_owned(),
        event_id: "1700000000123-0".to_owned(),
        answer: "the run finished".to_owned(),
    };

    let addressed = owed.addressed();
    assert_eq!(addressed.fleet_id, owed.fleet_id);
    assert_eq!(addressed.workspace_id, owed.workspace_id);
    assert_eq!(addressed.provider, owed.provider);
    assert_eq!(addressed.event_id, owed.event_id);
    assert_eq!(addressed.answer, owed.answer);
}

/// A ledger that will not answer stops the PASS, not the producer: the failure
/// is reported and the loop waits for its next turn, because one unavailable
/// store must not take the delivery path down for the life of the process.
#[tokio::test(flavor = "multi_thread")]
async fn a_scan_that_cannot_reach_the_ledger_reports_and_keeps_running() {
    let token = CancellationToken::new();
    let producer = Producer::new(unreachable_queue(), no_ledger::no_ledger());

    let running = tokio::spawn(producer.run(token.clone()));
    // Long enough for the first pass to meet the refusal on both scans. The
    // producer must still be parked afterwards rather than returned.
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(!running.is_finished(), "a failed scan ended the producer");

    token.cancel();
    tokio::time::timeout(SHUTDOWN_BUDGET, running)
        .await
        .expect("a cancelled producer stops inside the budget")
        .expect("the producer task finished cleanly");
}

/// A producer handed a token that is ALREADY cancelled does no work at all —
/// the check is at the top of the loop, before the first scan, so a shutdown
/// that raced the spawn cannot cost a pass against a store on its way down.
#[tokio::test(flavor = "multi_thread")]
async fn a_producer_started_after_the_shutdown_never_scans() {
    let token = CancellationToken::new();
    token.cancel();
    let producer = Producer::new(unreachable_queue(), no_ledger::no_ledger());

    tokio::time::timeout(SHUTDOWN_BUDGET, producer.run(token))
        .await
        .expect("an already-cancelled producer returns without a pass");
}
