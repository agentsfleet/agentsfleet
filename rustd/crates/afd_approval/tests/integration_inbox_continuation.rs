//! Dimension 6.2 — what an approval lands, and what it leaves untouched.
//!
//! The continuation row is a NEW event carrying `resumes_event_id`; the
//! blocked row is never reopened. Both halves are proven here over live
//! Postgres and Redis: the row that appears, the row that does not change, and
//! the idempotence that keeps a second answer from continuing the run twice.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;

use afd_approval::{Decision, Resolution};

use crate::lane::{Lane, NOW_MS, WINDOW_MS, sweeper_exclusive};

/// Who answers, when a test needs an operator.
const OPERATOR: &str = "human:fixture";
const OTHER_OPERATOR: &str = "human:somebody-else";

/// The note an operator leaves.
const NOTE: &str = "looks right";

/// How long after its deadline the late-answer case answers a gate.
///
/// Three hours, against a default window of one — comfortably past any window
/// a sweeper would have taken the row on, so the test cannot pass by accident
/// on a fast machine.
const LATE_ANSWER_MS: i64 = 3 * 60 * 60 * 1_000;

/// Dimension 6.2 — an approval lands a continuation that resumes the blocked run.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_approved_gate_lands_a_continuation_naming_what_it_resumes() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;

    let outcome = lane
        .inbox
        .resolve(
            &action,
            Decision::Approved,
            OPERATOR,
            NOTE,
            None,
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect("the resolve must not fault");

    let resolved = match outcome {
        Resolution::Resolved(resolved) => resolved,
        other => unreachable!("the first answer wins, got {other:?}"),
    };
    let continuation = resolved
        .continuation_event_id
        .expect("an approval continues the run it unblocked");

    let actor = lane
        .event_column(&continuation, "actor")
        .await
        .expect("a continuation row carries an actor");
    let kind = lane
        .event_column(&continuation, "event_type")
        .await
        .expect("a continuation row carries a type");
    let resumes = lane.event_column(&continuation, "resumes_event_id").await;

    // The chain reads forward: the blocked row says what stopped, and this one
    // says what it resumed from — so a reader never joins back through the gate
    // table to reconstruct the history.
    let held = resolved
        .event_id
        .as_deref()
        .expect("a gate raised on a run names the event it held");
    assert_eq!(actor, format!("continuation:{held}"));
    assert_eq!(kind, "continuation");
    assert_eq!(resumes.as_deref(), Some(held));
}

/// Dimension 6.2 — a denial continues nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_denied_gate_continues_nothing() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let before = lane.event_count().await;

    let outcome = lane
        .inbox
        .resolve(
            &action,
            Decision::Denied,
            OPERATOR,
            NOTE,
            None,
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect("the resolve must not fault");

    match outcome {
        Resolution::Resolved(resolved) => assert!(
            resolved.continuation_event_id.is_none(),
            "a refusal is the end of the run, not a pause in it"
        ),
        other => unreachable!("the first answer wins, got {other:?}"),
    }
    assert_eq!(before, lane.event_count().await, "a denial writes no event");
}

/// Dimension 6.2 — a re-answered gate does not continue the run twice.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_second_answer_does_not_continue_the_run_again() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let now = UnixMillis::from_millis(NOW_MS);

    lane.inbox
        .resolve(&action, Decision::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");
    let after_first = lane.event_count().await;

    let second = lane
        .inbox
        .resolve(&action, Decision::Approved, OTHER_OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");

    // The loser writes nothing: the `WHERE status = 'pending'` guard means the
    // second answer never reaches the continuation at all, so a retried resolve
    // restarts one run rather than two.
    assert!(matches!(second, Resolution::AlreadyResolved(_)));
    assert_eq!(
        after_first,
        lane.event_count().await,
        "answering twice continues the run once"
    );
}

/// Dimension 6.2's negative half — resolving a gate leaves the blocked row alone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn resolving_a_gate_does_not_reopen_the_event_it_blocked() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;

    let before = lane.event_count().await;
    lane.inbox
        .resolve(
            &action,
            Decision::Denied,
            OPERATOR,
            NOTE,
            None,
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect("the resolve must not fault");

    // A denial writes no event at all, so the count is the cleanest proof that
    // answering a gate never REOPENS the row it blocked. The approval's
    // continuation is a new row, asserted above; what neither does is bring the
    // blocked event back, which would erase that a person was ever asked.
    assert_eq!(
        before,
        lane.event_count().await,
        "answering a gate never rewrites the event it blocked"
    );
}

/// A gate nobody answered inside its window is still answerable HOURS later,
/// and answering it still resumes the run.
///
/// The worked case is the one an operator actually meets: a fleet asks to write
/// to a repository at 10:00 under the default one-hour window
/// (`afd_fleet_runtime::config::gates::DEFAULT_TIMEOUT_MS`), nobody is at their
/// desk, and a person approves it at 14:00 — three hours after the window
/// closed. That approval lands, and the run it blocked continues.
///
/// # This test exists to stop a well-meaning sweeper from breaking it
///
/// The Zig daemon spawns an approval-gate sweeper
/// (`cmd/serve_background.zig:49`) that flips `pending` → `timed_out` every
/// sixty seconds. This daemon does not: `Inbox::expire` has no production
/// caller. That gap reads like an omission, and the obvious "fix" is to wire the
/// sweeper — which would take the 10:00 gate at 11:00 and make the 14:00
/// approval answer `AlreadyResolved` instead of resuming anything.
///
/// So the absence is load-bearing for as long as a human is the approver, and
/// this is the test that says so. Wiring a sweeper is a PRODUCT decision about
/// whether an unanswered approval should lapse — not a parity chore — and it
/// fails here first.
///
/// # The one place a late answer is still refused, and it is not this one
///
/// `KIND_REPOSITORY_WRITE` alone carries a second predicate at the point of
/// USE: `sql::SELECT_APPROVED_WRITE_GATE` requires
/// `updated_at <= timeout_at`, so a late approval of a repository-write gate
/// flips the row and continues the run, and the branch write is then declined.
/// Every other gate kind honours the answer end to end. That inconsistency is
/// inherited from `fleet_runtime/sql.zig` and is recorded in the spec rather
/// than silently changed here.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_gate_answered_long_after_its_window_still_resumes_the_run() {
    // this gate is past its window, so a sibling's sweep would take it.
    let _sweeper = sweeper_exclusive().await;
    let lane = Lane::isolated().await;

    // Raised at 10:00 with the default one-hour window: the deadline passed
    // three hours before the answer arrives.
    let deadline = NOW_MS - LATE_ANSWER_MS;
    let action = lane.seed_gate(deadline).await;

    let outcome = lane
        .inbox
        .resolve(
            &action,
            Decision::Approved,
            OPERATOR,
            NOTE,
            None,
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect("the resolve must not fault");

    let resolved = match outcome {
        Resolution::Resolved(resolved) => resolved,
        other => unreachable!(
            "a gate past its window is still PENDING and must be answerable, got {other:?}"
        ),
    };

    assert_eq!(
        lane.status_of(&action).await,
        "approved",
        "a late answer is an answer: nothing swept this gate out from under it"
    );

    // The row flipping is not the claim — the run restarting is. An approval
    // that changed a status and continued nothing is a person told "done" over
    // work that never resumed.
    let continuation = resolved
        .continuation_event_id
        .expect("a late approval must still continue the run it unblocked");
    assert_eq!(
        lane.event_column(&continuation, "resumes_event_id").await,
        resolved.event_id,
        "the continuation names the blocked event it resumes"
    );
}
