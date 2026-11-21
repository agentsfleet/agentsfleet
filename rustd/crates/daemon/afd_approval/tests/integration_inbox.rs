//! Answering a gate under a race, and reading one past its deadline.
//!
//! # Why the race is proven against a live datastore and not a stub
//!
//! The whole decision is `WHERE resolved_at IS NULL` inside one UPDATE: two
//! callers run the same statement and Postgres picks the winner. A stub would
//! be asserting that the code CALLS a statement, which is the one thing that
//! was never in doubt — what is under test is that the statement decides, and
//! only a real one does.
//!
//! # The continuation is the sibling suite's
//!
//! What an approval lands, and what it leaves untouched, is
//! `integration_inbox_continuation.rs`; this suite is the race, the scope and
//! a late gate's read.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_approval::{Decision, Filter, Resolution};

use crate::lane::{Lane, Window};

/// Who answers, when a test needs an operator.
const OPERATOR: &str = "human:fixture";
const OTHER_OPERATOR: &str = "human:somebody-else";

/// The note an operator leaves.
const NOTE: &str = "looks right";

/// One gate, two answers, exactly one decision.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn two_operators_answering_one_gate_resolve_to_one_decision() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(Window::Open).await;

    // Both run the same statement at the same instant. Postgres decides.
    let (left, right) = tokio::join!(
        lane.inbox
            .resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None),
        lane.inbox.resolve(
            action.as_str(),
            Decision::Denied,
            OTHER_OPERATOR,
            NOTE,
            None
        ),
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

/// A second decision on a settled gate never rewrites it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_settled_gate_keeps_its_first_answer() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(Window::Open).await;

    let first = lane
        .inbox
        .resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the resolve must not fault");
    assert!(matches!(first, Resolution::Resolved(_)));

    let second = lane
        .inbox
        .resolve(
            action.as_str(),
            Decision::Denied,
            OTHER_OPERATOR,
            NOTE,
            None,
        )
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

/// The fleet filter is an authorization, not a convenience.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_decision_scoped_to_another_fleet_resolves_nothing() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(Window::Open).await;
    let stranger = afd_db::test_util::mint_id();

    let outcome = lane
        .inbox
        .resolve(
            action.as_str(),
            Decision::Approved,
            OPERATOR,
            NOTE,
            Some(stranger.as_str()),
        )
        .await
        .expect("the resolve must not fault");

    // Not found rather than resolved: an actor holding a signature for one
    // fleet must not be able to answer another's gate by guessing an action id.
    assert_eq!(outcome, Resolution::NotFound);
    // An action or a fleet that is not a `UUIDv7` names no gate either.
    for (action, fleet) in [("not-an-id", None), (action.as_str(), Some("not-an-id"))] {
        let outcome = lane
            .inbox
            .resolve(action, Decision::Approved, OPERATOR, NOTE, fleet)
            .await
            .expect("a malformed id must not fault");
        assert_eq!(outcome, Resolution::NotFound);
    }
    assert_eq!(
        lane.status_of(&action).await,
        "pending",
        "a foreign-scoped decision leaves the gate waiting"
    );
}

/// The queue read is scoped to its own workspace.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn one_workspaces_queue_never_shows_anothers_gate() {
    let mine = Lane::isolated().await;
    let theirs = Lane::isolated().await;
    mine.seed_gate(Window::Open).await;
    theirs.seed_gate(Window::Open).await;

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
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_gate_id_from_another_workspace_is_indistinguishable_from_absent() {
    let mine = Lane::isolated().await;
    let theirs = Lane::isolated().await;
    theirs.seed_gate(Window::Open).await;

    let their_gate = theirs
        .inbox
        .page(&theirs.workspace, Filter::default(), None, 1)
        .await
        .expect("the queue read must not fault");
    let gate = Uuid7::parse(
        their_gate
            .first()
            .expect("one seeded gate")
            .gate_id
            .as_str(),
    )
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

/// Nothing sweeps a gate: past its deadline it reads `timed_out`, its row
/// stays `pending`, and the `pending` queue still lists it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_gate_past_its_deadline_reads_timed_out_and_stays_answerable() {
    let lane = Lane::isolated().await;
    let lapsed = lane.seed_gate(Window::Lapsed).await;
    let waiting = lane.seed_gate(Window::Open).await;

    let page = lane
        .inbox
        .page(&lane.workspace, Filter::default(), None, 10)
        .await
        .expect("the read must not fault");
    let status = |action: &Uuid7| {
        page.iter()
            .find(|row| &row.action_id == action)
            .map(|row| row.status.clone())
    };
    assert_eq!(status(&lapsed).as_deref(), Some("timed_out"));
    assert_eq!(status(&waiting).as_deref(), Some("pending"));
    assert_eq!(lane.status_of(&lapsed).await, "pending", "no sweep moved the row");

    let queued = lane
        .inbox
        .page(
            &lane.workspace,
            Filter {
                status: Some(afd_api_wire::approval::GateStatus::Pending),
                ..Filter::default()
            },
            None,
            10,
        )
        .await
        .expect("the read must not fault");
    assert_eq!(queued.len(), 2, "a late gate still waits on a person");
}
