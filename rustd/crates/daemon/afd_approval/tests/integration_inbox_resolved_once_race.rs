//! Dimension 4.6 — two resolutions of one gate, on two tasks released at once.
//!
//! `integration_inbox_resolved_once.rs` answers a gate twice in sequence; this
//! file answers it twice in parallel. Each resolve runs on its own spawned task
//! on a multi-threaded runtime, both held at a barrier until the other is ready,
//! so the guarded `UPDATE ... WHERE resolved_at IS NULL` is what decides.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_api_wire::approval::status;
use afd_approval::{Decision, Resolution};
use tokio::sync::Barrier;

use crate::lane::{Lane, Window};

/// The operator who approves.
const APPROVER: &str = "human:approver";

/// The operator who denies.
const DENIER: &str = "human:denier";

/// The note both leave.
const NOTE: &str = "racing";

/// Rounds raced, so one lucky interleaving cannot pass the test alone.
const ROUNDS: usize = 8;

/// Two concurrent resolutions: exactly one wins, the other is told the
/// standing decision, and the row carries the winner's answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_resolution_happens_once_under_concurrent_resolves() {
    let lane = Lane::isolated().await;
    for _ in 0..ROUNDS {
        let action = lane.seed_gate(Window::Open).await;
        let barrier = Arc::new(Barrier::new(2));
        let spawn = |decision: Decision, who: &'static str| {
            let inbox = lane.inbox.clone();
            let barrier = Arc::clone(&barrier);
            let action = action.clone();
            tokio::spawn(async move {
                barrier.wait().await;
                inbox.resolve(&action, decision, who, NOTE, None).await
            })
        };
        let approve = spawn(Decision::Approved, APPROVER);
        let deny = spawn(Decision::Denied, DENIER);
        let approve = approve
            .await
            .expect("the task joins")
            .expect("the resolve must not fault");
        let deny = deny
            .await
            .expect("the task joins")
            .expect("the resolve must not fault");

        let (winner, standing) = match (approve, deny) {
            (Resolution::Resolved(won), Resolution::AlreadyResolved(standing))
            | (Resolution::AlreadyResolved(standing), Resolution::Resolved(won)) => (won, standing),
            other => unreachable!("exactly one resolve wins, got {other:?}"),
        };
        assert_eq!(standing.resolved_by, winner.resolved_by);
        assert_eq!(standing.resolved_at, winner.resolved_at);
        assert_eq!(standing.status, winner.status);
        let expected = if winner.resolved_by == APPROVER {
            status::APPROVED
        } else {
            assert_eq!(winner.resolved_by, DENIER);
            status::DENIED
        };
        assert_eq!(winner.status, expected);
        assert_eq!(lane.status_of(&action).await, expected);
    }
}
