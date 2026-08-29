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
//! # The terminal-row rule lives here too
//!
//! Nothing in the resolve writes back to `core.fleet_events`. Dimension 6.2's
//! continuation row is a NEW event, so what this suite proves is the negative
//! half: a resolved gate leaves the blocked row exactly as it found it.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;

use afd_approval::{Decision, Filter, Resolution};

use crate::lane::{Lane, NOW_MS, WINDOW_MS};

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

/// The resolver a swept gate records, mirrored from the store.
const SWEEPER: &str = "system:approval_gate_sweeper";

/// Dimension 6.1 — one gate, two answers, exactly one decision.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn two_operators_answering_one_gate_resolve_to_one_decision() {
    let lane = Lane::create().await;
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
    let lane = Lane::create().await;
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
    let lane = Lane::create().await;
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
    let lane = Lane::create().await;
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
    let lane = Lane::create().await;
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
    let lane = Lane::create().await;
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

/// Dimension 6.2 — an approval lands a continuation that resumes the blocked run.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_approved_gate_lands_a_continuation_naming_what_it_resumes() {
    let lane = Lane::create().await;
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
    assert_eq!(actor, format!("continuation:{}", resolved.event_id));
    assert_eq!(kind, "continuation");
    assert_eq!(resumes.as_deref(), Some(resolved.event_id.as_str()));
}

/// Dimension 6.2 — a denial continues nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_denied_gate_continues_nothing() {
    let lane = Lane::create().await;
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
    let lane = Lane::create().await;
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
    let lane = Lane::create().await;
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
    let lane = Lane::create().await;

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
        Some(resolved.event_id.clone()),
        "the continuation names the blocked event it resumes"
    );
}
