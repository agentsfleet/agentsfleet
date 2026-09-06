//! Dimensions 6.1 and 6.3 — answering a gate under a race, and expiring the
//! ones nobody answered.
//!
//! # Why the race is proven against a live datastore and not a stub
//!
//! The whole decision is `WHERE status = 'pending'` inside one UPDATE: two
//! callers run the same statement and Postgres picks the winner. A stub would
//! be asserting that the code CALLS a statement, which is the one thing that
//! was never in doubt — what is under test is that the statement decides, and
//! only a real one does.
//!
//! # The continuation is the sibling suite's
//!
//! What an approval lands, and what it leaves untouched, is
//! `integration_inbox_continuation.rs`; this suite is the race, the scope and
//! the sweep.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;

use afd_approval::{Decision, Filter, Resolution};

use crate::lane::{Lane, NOW_MS, WINDOW_MS, sweeper_exclusive};

/// Who answers, when a test needs an operator.
const OPERATOR: &str = "human:fixture";
const OTHER_OPERATOR: &str = "human:somebody-else";

/// The note an operator leaves.
const NOTE: &str = "looks right";

/// The resolver a swept gate records, mirrored from the store.
const SWEEPER: &str = "system:approval_gate_sweeper";

/// Dimension 6.1 — one gate, two answers, exactly one decision.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn two_operators_answering_one_gate_resolve_to_one_decision() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let now = UnixMillis::from_millis(NOW_MS);

    // Both run the same statement at the same instant. Postgres decides.
    let (left, right) = tokio::join!(
        lane.inbox
            .resolve(&action, Decision::Approved, OPERATOR, NOTE, None, now),
        lane.inbox
            .resolve(&action, Decision::Denied, OTHER_OPERATOR, NOTE, None, now),
    );
    let left = left.expect("the resolve must not fault");
    let right = right.expect("the resolve must not fault");

    let winners = [&left, &right]
        .iter()
        .filter(|outcome| matches!(outcome, Resolution::Resolved(_)))
        .count();
    assert_eq!(
        winners, 1,
        "exactly one caller decides; the other is told somebody already had"
    );

    // And the loser is told the truth rather than an error: the gate IS
    // resolved, which is what they wanted, just not by them.
    let losers = [&left, &right]
        .iter()
        .filter(|outcome| matches!(outcome, Resolution::AlreadyResolved(_)))
        .count();
    assert_eq!(losers, 1, "the losing answer reports the standing decision");
}

/// Dimension 6.1 — a second decision on a settled gate never rewrites it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_settled_gate_keeps_its_first_answer() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let now = UnixMillis::from_millis(NOW_MS);

    let first = lane
        .inbox
        .resolve(&action, Decision::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");
    assert!(matches!(first, Resolution::Resolved(_)));

    let second = lane
        .inbox
        .resolve(&action, Decision::Denied, OTHER_OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");

    let standing = match second {
        Resolution::AlreadyResolved(standing) => standing,
        other => unreachable!("a settled gate is already resolved, got {other:?}"),
    };
    // The attribution is the FIRST operator's. A second answer that overwrote
    // it would rewrite who authorised a live action.
    assert_eq!(standing.resolved_by, OPERATOR);
    assert_eq!(standing.status, Decision::Approved.as_str());

    assert_eq!(lane.status_of(&action).await, "approved");
}

/// Dimension 6.1 — the fleet filter is an authorization, not a convenience.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_decision_scoped_to_another_fleet_resolves_nothing() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let stranger = afd_db::test_util::mint_id();
    let now = UnixMillis::from_millis(NOW_MS);

    let outcome = lane
        .inbox
        .resolve(
            &action,
            Decision::Approved,
            OPERATOR,
            NOTE,
            Some(&stranger),
            now,
        )
        .await
        .expect("the resolve must not fault");

    // Not found rather than resolved: an actor holding a signature for one
    // fleet must not be able to answer another's gate by guessing an action id.
    assert_eq!(outcome, Resolution::NotFound);
    assert_eq!(
        lane.status_of(&action).await,
        "pending",
        "a foreign-scoped decision leaves the gate waiting"
    );
}

/// Dimension 6.1 — the queue read is scoped to its own workspace.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn one_workspaces_queue_never_shows_anothers_gate() {
    let mine = Lane::isolated().await;
    let theirs = Lane::isolated().await;
    mine.seed_gate(NOW_MS + WINDOW_MS).await;
    theirs.seed_gate(NOW_MS + WINDOW_MS).await;

    let page = mine
        .inbox
        .page(&mine.workspace, Filter::default(), None, 50)
        .await
        .expect("the queue read must not fault");

    assert_eq!(page.len(), 1, "one workspace, one gate");
    let only = page
        .first()
        .expect("the one row the assertion above counted");
    assert_eq!(only.fleet_id, mine.fleet.as_str());

    // And the fleet NAME is joined rather than stored, so the card has a
    // heading rather than a blank.
    assert!(
        !only.fleet_name.is_empty(),
        "an inbox row names the fleet a person is being asked about"
    );
}

/// A gate id from another workspace reads as absent, not as forbidden.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_gate_id_from_another_workspace_is_indistinguishable_from_absent() {
    let mine = Lane::isolated().await;
    let theirs = Lane::isolated().await;
    theirs.seed_gate(NOW_MS + WINDOW_MS).await;

    let their_gate = theirs
        .inbox
        .page(&theirs.workspace, Filter::default(), None, 1)
        .await
        .expect("the queue read must not fault");
    let gate = afd_core::id::Uuid7::parse(&their_gate.first().expect("one seeded gate").gate_id)
        .expect("a gate id parses");

    let found = mine
        .inbox
        .one(&mine.workspace, &gate)
        .await
        .expect("the read must not fault");

    // `None`, the same answer a made-up id gets. Telling the two apart would
    // confirm that a gate exists somewhere, which is the leak the scope closes.
    assert!(found.is_none(), "another workspace's gate is not visible");
}

/// Dimension 6.3 — the sweeper takes gates whose window closed, and only those.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_sweeper_expires_only_the_gates_whose_window_closed() {
    // the sweep is global; a sibling's lapsed gate is in its statement.
    let _sweeper = sweeper_exclusive().await;
    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);

    let lapsed = lane.seed_gate(NOW_MS - 1).await;
    let waiting = lane.seed_gate(NOW_MS + WINDOW_MS).await;

    let swept = lane
        .inbox
        .expire(now)
        .await
        .expect("the sweep must not fault");
    assert!(swept >= 1, "the lapsed gate is taken");

    assert_eq!(
        lane.status_of(&lapsed).await,
        "timed_out",
        "a window that closed with no answer times the gate out"
    );
    assert_eq!(
        lane.status_of(&waiting).await,
        "pending",
        "a gate still inside its window is left alone"
    );
}

/// Dimension 6.3 — an answer that landed first outranks the clock.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_gate_answered_before_the_deadline_is_not_swept() {
    // the sweep is global; a sibling's lapsed gate is in its statement.
    let _sweeper = sweeper_exclusive().await;
    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);
    // Deadline already past, and a person answered anyway — the ordering a
    // sweep must not undo.
    let action = lane.seed_gate(NOW_MS - 1).await;

    lane.inbox
        .resolve(&action, Decision::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");
    lane.inbox
        .expire(now)
        .await
        .expect("the sweep must not fault");

    assert_eq!(
        lane.status_of(&action).await,
        "approved",
        "the operator's decision outranks the deadline"
    );
}

/// Dimension 6.3 — a swept gate records who took it and why.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_swept_gate_says_the_system_took_it() {
    // the sweep is global; a sibling's lapsed gate is in its statement.
    let _sweeper = sweeper_exclusive().await;
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(NOW_MS - 1).await;
    lane.inbox
        .expire(UnixMillis::from_millis(NOW_MS))
        .await
        .expect("the sweep must not fault");

    // Attribution matters: an audit reading this row has to be able to tell a
    // gate a human denied from one that simply ran out of time.
    let resolved_by = lane.gate_column(&action, "resolved_by").await;
    let detail = lane.gate_column(&action, "detail").await;
    assert_eq!(resolved_by, SWEEPER);
    assert!(!detail.is_empty(), "a swept gate says why it closed");
}
