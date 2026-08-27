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

#[path = "support/fleet_fixtures.rs"]
mod support;

#[path = "support/fleet_queue.rs"]
mod queue;

#[path = "support/fleet_lease_reads.rs"]
mod lease_reads;

#[path = "support/fleet_lease_seed.rs"]
mod seed;

#[path = "support/fleet_requests.rs"]
mod requests;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::gate::{Filter, Inbox, Resolution, Status};
use sqlx::Row as _;

use self::requests::ENROLLED_AT;
use self::seed::seeded as seed_fleet_with_runner;
use self::support::Fixtures;

/// The instant every fixture row is stamped with.
const NOW_MS: i64 = ENROLLED_AT;

/// How long a fixture gate waits before the sweeper may take it.
const WINDOW_MS: i64 = 60_000;

/// Who answers, when a test needs an operator.
const OPERATOR: &str = "human:fixture";
const OTHER_OPERATOR: &str = "human:somebody-else";

/// The note an operator leaves.
const NOTE: &str = "looks right";

/// The kind a fixture gate carries.
///
/// Deliberately NOT `integration_grant`: that kind makes the resolve write a
/// second table, and these tests are about the gate row's own transition. The
/// grant arm has its own suite.
const KIND: &str = "repository_write";

/// The resolver a swept gate records, mirrored from the store.
const SWEEPER: &str = "system:approval_gate_sweeper";

/// Seeds one pending gate, returning its action id.
///
/// `timeout_at` is a parameter because the sweeper tests need a deadline in the
/// past and the race tests need one that has not arrived — a fixture that fixed
/// it would make one of the two impossible to write.
async fn seed_gate(fixtures: &Fixtures, fleet: &str, timeout_at: i64) -> String {
    let action = afd_db::test_util::mint_id();
    let workspace = workspace_of(fixtures, fleet).await;
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query(
        "INSERT INTO core.fleet_approval_gates
           (id, fleet_id, workspace_id, action_id, tool_name, action_name,
            gate_kind, proposed_action, evidence, blast_radius, timeout_at,
            resolved_by, status, detail, created_at, updated_at, event_id,
            spend_count, spend_ceiling)
         VALUES ($1::uuid, $2::uuid, $3::uuid, $4, 'git', 'push',
                 $5, 'open a pull request', '{}'::jsonb, 'one repository',
                 $6, '', 'pending', '', $7, NULL, $8, 0, 32)",
    )
    .bind(afd_db::test_util::mint_id())
    .bind(fleet)
    .bind(&workspace)
    .bind(&action)
    .bind(KIND)
    .bind(timeout_at)
    .bind(NOW_MS)
    .bind(afd_db::test_util::mint_id())
    .execute(&mut *connection)
    .await
    .expect("the gate row must insert");
    action
}

/// The workspace a seeded fleet lives in.
async fn workspace_of(fixtures: &Fixtures, fleet: &str) -> String {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("SELECT workspace_id::text FROM core.fleets WHERE id = $1::uuid")
        .bind(fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("the seeded fleet has a workspace")
        .try_get(0)
        .expect("a workspace id is text")
}

/// The status column of one gate, by action.
async fn status_of(fixtures: &Fixtures, action: &str) -> String {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("SELECT status FROM core.fleet_approval_gates WHERE action_id = $1")
        .bind(action)
        .fetch_one(&mut *connection)
        .await
        .expect("the gate row is readable")
        .try_get(0)
        .expect("a status is text")
}

/// A seeded fleet and the inbox over it.
async fn seeded() -> (Fixtures, String, Inbox) {
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = seed_fleet_with_runner::<1>(&fixtures).await;
    let inbox = fixtures.inbox();
    (fixtures, seeded.fleet, inbox)
}

/// Dimension 6.1 — one gate, two answers, exactly one decision.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn two_operators_answering_one_gate_resolve_to_one_decision() {
    let (fixtures, fleet, inbox) = seeded().await;
    let action = seed_gate(&fixtures, &fleet, NOW_MS + WINDOW_MS).await;
    let now = UnixMillis::from_millis(NOW_MS);

    // Both run the same statement at the same instant. Postgres decides.
    let (left, right) = tokio::join!(
        inbox.resolve(&action, Status::Approved, OPERATOR, NOTE, None, now),
        inbox.resolve(&action, Status::Denied, OTHER_OPERATOR, NOTE, None, now),
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

    fixtures.cleanup().await;
}

/// Dimension 6.1 — a second decision on a settled gate never rewrites it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_settled_gate_keeps_its_first_answer() {
    let (fixtures, fleet, inbox) = seeded().await;
    let action = seed_gate(&fixtures, &fleet, NOW_MS + WINDOW_MS).await;
    let now = UnixMillis::from_millis(NOW_MS);

    let first = inbox
        .resolve(&action, Status::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");
    assert!(matches!(first, Resolution::Resolved(_)));

    let second = inbox
        .resolve(&action, Status::Denied, OTHER_OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");

    let standing = match second {
        Resolution::AlreadyResolved(standing) => standing,
        other => unreachable!("a settled gate is already resolved, got {other:?}"),
    };
    // The attribution is the FIRST operator's. A second answer that overwrote
    // it would rewrite who authorised a live action.
    assert_eq!(standing.resolved_by, OPERATOR);
    assert_eq!(standing.status, Status::Approved.as_str());

    assert_eq!(status_of(&fixtures, &action).await, "approved");

    fixtures.cleanup().await;
}

/// Dimension 6.1 — the fleet filter is an authorization, not a convenience.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_decision_scoped_to_another_fleet_resolves_nothing() {
    let (fixtures, fleet, inbox) = seeded().await;
    let action = seed_gate(&fixtures, &fleet, NOW_MS + WINDOW_MS).await;
    let stranger = afd_db::test_util::mint_id();
    let now = UnixMillis::from_millis(NOW_MS);

    let outcome = inbox
        .resolve(
            &action,
            Status::Approved,
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
        status_of(&fixtures, &action).await,
        "pending",
        "a foreign-scoped decision leaves the gate waiting"
    );

    fixtures.cleanup().await;
}

/// A resolve to `pending` is refused before any statement runs.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_resolution_that_resolves_to_nothing_is_refused() {
    let (fixtures, fleet, inbox) = seeded().await;
    let action = seed_gate(&fixtures, &fleet, NOW_MS + WINDOW_MS).await;

    let refused = inbox
        .resolve(
            &action,
            Status::Pending,
            OPERATOR,
            NOTE,
            None,
            UnixMillis::from_millis(NOW_MS),
        )
        .await;
    assert!(refused.is_err(), "pending is not a transition");
    assert_eq!(status_of(&fixtures, &action).await, "pending");

    fixtures.cleanup().await;
}

/// Dimension 6.1 — the queue read is scoped to its own workspace.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn one_workspaces_queue_never_shows_anothers_gate() {
    let (mine, my_fleet, my_inbox) = seeded().await;
    let (theirs, their_fleet, _) = seeded().await;
    seed_gate(&mine, &my_fleet, NOW_MS + WINDOW_MS).await;
    seed_gate(&theirs, &their_fleet, NOW_MS + WINDOW_MS).await;

    let my_workspace =
        Uuid7::parse(&workspace_of(&mine, &my_fleet).await).expect("a seeded workspace parses");
    let page = my_inbox
        .page(&my_workspace, Filter::default(), None, 50)
        .await
        .expect("the queue read must not fault");

    assert_eq!(page.len(), 1, "one workspace, one gate");
    let only = page
        .first()
        .expect("the one row the assertion above counted");
    assert_eq!(only.fleet_id, my_fleet);

    // And the fleet NAME is joined rather than stored, so the card has a
    // heading rather than a blank.
    assert!(
        !only.fleet_name.is_empty(),
        "an inbox row names the fleet a person is being asked about"
    );

    mine.cleanup().await;
    theirs.cleanup().await;
}

/// A gate id from another workspace reads as absent, not as forbidden.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_gate_id_from_another_workspace_is_indistinguishable_from_absent() {
    let (mine, my_fleet, my_inbox) = seeded().await;
    let (theirs, their_fleet, their_inbox) = seeded().await;
    seed_gate(&theirs, &their_fleet, NOW_MS + WINDOW_MS).await;

    let their_workspace =
        Uuid7::parse(&workspace_of(&theirs, &their_fleet).await).expect("a workspace parses");
    let their_gate = their_inbox
        .page(&their_workspace, Filter::default(), None, 1)
        .await
        .expect("the queue read must not fault");
    let gate = Uuid7::parse(&their_gate.first().expect("one seeded gate").gate_id)
        .expect("a gate id parses");

    let my_workspace =
        Uuid7::parse(&workspace_of(&mine, &my_fleet).await).expect("a workspace parses");
    let found = my_inbox
        .one(&my_workspace, &gate)
        .await
        .expect("the read must not fault");

    // `None`, the same answer a made-up id gets. Telling the two apart would
    // confirm that a gate exists somewhere, which is the leak the scope closes.
    assert!(found.is_none(), "another workspace's gate is not visible");

    mine.cleanup().await;
    theirs.cleanup().await;
}

/// Dimension 6.3 — the sweeper takes gates whose window closed, and only those.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_sweeper_expires_only_the_gates_whose_window_closed() {
    let (fixtures, fleet, inbox) = seeded().await;
    let now = UnixMillis::from_millis(NOW_MS);

    let lapsed = seed_gate(&fixtures, &fleet, NOW_MS - 1).await;
    let waiting = seed_gate(&fixtures, &fleet, NOW_MS + WINDOW_MS).await;

    let swept = inbox.expire(now).await.expect("the sweep must not fault");
    assert!(swept >= 1, "the lapsed gate is taken");

    assert_eq!(
        status_of(&fixtures, &lapsed).await,
        "timed_out",
        "a window that closed with no answer times the gate out"
    );
    assert_eq!(
        status_of(&fixtures, &waiting).await,
        "pending",
        "a gate still inside its window is left alone"
    );

    fixtures.cleanup().await;
}

/// Dimension 6.3 — an answer that landed first outranks the clock.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_gate_answered_before_the_deadline_is_not_swept() {
    let (fixtures, fleet, inbox) = seeded().await;
    let now = UnixMillis::from_millis(NOW_MS);
    // Deadline already past, and a person answered anyway — the ordering a
    // sweep must not undo.
    let action = seed_gate(&fixtures, &fleet, NOW_MS - 1).await;

    inbox
        .resolve(&action, Status::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");
    inbox.expire(now).await.expect("the sweep must not fault");

    assert_eq!(
        status_of(&fixtures, &action).await,
        "approved",
        "the operator's decision outranks the deadline"
    );

    fixtures.cleanup().await;
}

/// Dimension 6.3 — a swept gate records who took it and why.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_swept_gate_says_the_system_took_it() {
    let (fixtures, fleet, inbox) = seeded().await;
    let action = seed_gate(&fixtures, &fleet, NOW_MS - 1).await;
    inbox
        .expire(UnixMillis::from_millis(NOW_MS))
        .await
        .expect("the sweep must not fault");

    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let row = sqlx::query(
        "SELECT resolved_by, detail FROM core.fleet_approval_gates WHERE action_id = $1",
    )
    .bind(&action)
    .fetch_one(&mut *connection)
    .await
    .expect("the gate row is readable");

    // Attribution matters: an audit reading this row has to be able to tell a
    // gate a human denied from one that simply ran out of time.
    let resolved_by: String = row.try_get(0).expect("a resolver is text");
    let detail: String = row.try_get(1).expect("a detail is text");
    assert_eq!(resolved_by, SWEEPER);
    assert!(!detail.is_empty(), "a swept gate says why it closed");

    drop(connection);
    fixtures.cleanup().await;
}

/// Dimension 6.2's negative half — resolving a gate leaves the blocked row alone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn resolving_a_gate_does_not_reopen_the_event_it_blocked() {
    let (fixtures, fleet, inbox) = seeded().await;
    let action = seed_gate(&fixtures, &fleet, NOW_MS + WINDOW_MS).await;

    let before = event_count(&fixtures, &fleet).await;
    inbox
        .resolve(
            &action,
            Status::Approved,
            OPERATOR,
            NOTE,
            None,
            UnixMillis::from_millis(NOW_MS),
        )
        .await
        .expect("the resolve must not fault");
    let after = event_count(&fixtures, &fleet).await;

    // The resolve writes gates, not events. A continuation is a NEW row the
    // run path lands, so the blocked one stays terminal and the history keeps
    // both the run that was stopped and the one that followed.
    assert_eq!(
        before, after,
        "answering a gate is not itself an event write"
    );

    fixtures.cleanup().await;
}

/// How many events one fleet holds.
async fn event_count(fixtures: &Fixtures, fleet: &str) -> i64 {
    let mut connection = fixtures
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query("SELECT count(*) FROM core.fleet_events WHERE fleet_id = $1::uuid")
        .bind(fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("the count must run")
        .try_get(0)
        .expect("a count is a bigint")
}
