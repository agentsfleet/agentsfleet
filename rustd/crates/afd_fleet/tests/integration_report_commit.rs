//! §7 against live datastores — the report's four writes have one fate.
//!
//! Dimension 7.3. What §3's suite proves about the settle STATEMENT, this
//! proves about the transaction the statement now rides: the money, the run's
//! result, the session cursor and the freed slot commit together or none of
//! them does, and the queue is acknowledged only afterwards.
//!
//! The three phases run against ONE lease on purpose, because the second is
//! only reachable if the first rolled back. A settle that committed on its own
//! would leave the lease `reported`, and the runner's retry — the whole point
//! of retaining a terminal report — would be refused forever.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing datastores, and `make test-integration-rustd` — which runs
//! `--ignored` and nothing else — is the only lane that executes them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_fleet::lease::{Committed, Leases, Reported, sql};

use crate::queue;
use crate::report_commit::{
    LEDGER_ROWS_AFTER_SETTLE, RESPONSE_ACCEPTED, RESPONSE_POSTGRES_REFUSES, RESUME_EVENT_ID,
    assert_nothing_landed, report,
};
use crate::report_seed::{DEEP_POOL, Held, SLICE_MS, SLICE_NANOS, held};

/// Dimension 7.3 — the result commits with the money, or neither does.
///
/// Three phases on one lease, in the order a runner actually meets them.
///
/// The store is built over a Dragonfly that will not answer. Nothing in the
/// transaction may touch the queue — the acknowledgement is the one write that
/// loses an event outright if it runs early — so a dead queue that changes no
/// assertion below is the proof that the ordering holds by construction rather
/// than by comment.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_report_persists_result_with_settlement_atomically() {
    let held = held().await;
    let lease_id = held.issued.lease_id.as_str();
    let settled_at = held.now.saturating_add_millis(SLICE_MS);
    let leases = held.fixtures.leases_with_dead_queue();
    let lease = leases
        .load_for_report(lease_id, &held.runner)
        .await
        .expect("the lease load must reach the datastore")
        .expect("the seeded lease belongs to the seeded runner");

    assert_a_refused_result_charges_nothing(&held, &leases, &lease, settled_at).await;
    assert_the_retry_commits_all_four(&held, &leases, &lease, settled_at).await;
    assert_the_repeat_changes_nothing(&held, &leases, &lease, settled_at).await;

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// A result Postgres will not store takes the charge down with it.
///
/// The injected failure lands at the SECOND statement, so the settle has
/// already written when it fires. Before §7 that settle stood on its own and
/// this left a tenant charged for a run whose answer is nowhere — unrecoverable,
/// because the lease was `reported` and every retry was refused.
async fn assert_a_refused_result_charges_nothing(
    held: &Held,
    leases: &Leases,
    lease: &Reported,
    settled_at: UnixMillis,
) {
    let lease_id = held.issued.lease_id.as_str();
    let refused = leases
        .commit_report(report(
            lease_id,
            &held.runner,
            lease,
            RESPONSE_POSTGRES_REFUSES,
            settled_at,
        ))
        .await;
    assert!(
        refused.is_err(),
        "a result Postgres will not store must fail the report, not be logged past"
    );
    assert_nothing_landed(
        held,
        "the settle is the FIRST statement in that transaction and it rolled back with the rest",
    )
    .await;
    assert_eq!(
        held.fixtures.lease_column(lease_id, "status").await,
        Some(sql::LEASE_STATUS_ACTIVE.to_owned()),
        "the lease stays active, which is what makes the runner's retry possible at all"
    );
}

/// The runner retries with an answer that stores, and all four land together.
async fn assert_the_retry_commits_all_four(
    held: &Held,
    leases: &Leases,
    lease: &Reported,
    settled_at: UnixMillis,
) {
    let lease_id = held.issued.lease_id.as_str();
    let committed = leases
        .commit_report(report(
            lease_id,
            &held.runner,
            lease,
            RESPONSE_ACCEPTED,
            settled_at,
        ))
        .await
        .expect("the retry must reach the datastore");
    let Committed::Settled {
        charged,
        closed,
        owed,
    } = committed
    else {
        unreachable!("the only holder of this fleet cannot be fenced out of its own retry")
    };
    assert_eq!(
        charged.as_i64(),
        SLICE_NANOS,
        "the retry charges the slice ONCE — the refused attempt charged nothing"
    );
    assert!(
        owed.is_none(),
        "this lease's event reached the stream without an admission, so it names no \
         destination and nothing is owed; the report-owes-destination suite proves the \
         owed branch"
    );
    assert!(
        closed.is_some(),
        "the terminal write closed a row, and the frame announcing it rides that row rather \
         than a second read taken after the commit"
    );
    assert_all_four_landed(held, settled_at.as_millis()).await;
}

/// The response is lost, the runner re-sends, and nothing moves.
async fn assert_the_repeat_changes_nothing(
    held: &Held,
    leases: &Leases,
    lease: &Reported,
    settled_at: UnixMillis,
) {
    let repeated = leases
        .commit_report(report(
            held.issued.lease_id.as_str(),
            &held.runner,
            lease,
            RESPONSE_ACCEPTED,
            settled_at,
        ))
        .await
        .expect("a repeated report is an answer, not a fault");
    assert!(
        matches!(repeated, Committed::AlreadySettled),
        "this runner settled this lease, so the repeat is told so — a superseded holder is \
         the other empty claim, and collapsing the two refuses a finished run forever"
    );
    assert_all_four_landed(held, settled_at.as_millis()).await;
}

/// Dimension 7.3 — a run whose event is already terminal still settles.
///
/// One logical event can sit on two stream entries, so a lease can be `active`
/// over an event an earlier delivery already ended. The terminal write is
/// guarded on `received` for exactly that case: it matches nothing, the stored
/// result stands, and there is no new ending to announce.
///
/// What must NOT happen is the rest of the transaction failing with it. The
/// runner did the work and the tenant owes for it, so the settle, the cursor
/// and the freed slot all commit — an absent closing is a fact about the event
/// row, not a failure of the report.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_report_over_an_ended_event_keeps_the_stored_result() {
    let held = held().await;
    let lease_id = held.issued.lease_id.as_str();
    let settled_at = held.now.saturating_add_millis(SLICE_MS);
    let leases = held.fixtures.leases_with_dead_queue();
    let lease = leases
        .load_for_report(lease_id, &held.runner)
        .await
        .expect("the lease load must reach the datastore")
        .expect("the seeded lease belongs to the seeded runner");

    held.fixtures
        .end_event(
            &held.fleet,
            &held.event_id,
            afd_core::event::status::PROCESSED,
        )
        .await;

    let committed = leases
        .commit_report(report(
            lease_id,
            &held.runner,
            &lease,
            RESPONSE_ACCEPTED,
            settled_at,
        ))
        .await
        .expect("the report must reach the datastore");
    let Committed::Settled {
        charged,
        closed,
        owed,
    } = committed
    else {
        unreachable!("the lease is this runner's and still active, so the claim wins")
    };
    assert_eq!(
        charged.as_i64(),
        SLICE_NANOS,
        "the run happened and is charged for, whatever the event row already said"
    );
    assert!(
        closed.is_none(),
        "there is no NEW ending to announce, so no completion frame is published for one"
    );
    assert!(
        owed.is_none(),
        "charged does not imply owed: an answer is owed only where its question came \
         from, and this event names no destination"
    );
    assert_eq!(
        held.fixtures
            .event_column(&held.fleet, &held.event_id, "response_text")
            .await,
        None,
        "and the earlier delivery's row is left exactly as it was — a terminal row is \
         never reopened by a later report"
    );
    assert_eq!(
        held.fixtures.lease_column(lease_id, "status").await,
        Some(sql::LEASE_STATUS_REPORTED.to_owned()),
        "the lease is terminal, because the settle committed with the rest"
    );

    queue::clear_ready(held.fixtures.queue(), &held.fleet).await;
    held.fixtures.cleanup().await;
}

/// Every row the committed transaction wrote, asserted as a set.
///
/// Called after the settle and again after the repeat. The repeat charges
/// nothing and writes nothing, so "unchanged" and "correct" are the same
/// assertion — which is why this is one helper rather than two lists that
/// could drift apart.
async fn assert_all_four_landed(held: &Held, settled_at: i64) {
    let lease_id = held.issued.lease_id.as_str();
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL - SLICE_NANOS),
        "the wallet is drawn down by one slice, however many times the report is sent"
    );
    assert_eq!(
        held.fixtures.ledger_rows(&held.event_id).await,
        LEDGER_ROWS_AFTER_SETTLE,
        "one receive row and one stage row: the two-rows-per-event invariant"
    );
    assert_eq!(
        held.fixtures.lease_column(lease_id, "status").await,
        Some(sql::LEASE_STATUS_REPORTED.to_owned()),
        "the lease is terminal"
    );
    assert_eq!(
        held.fixtures
            .event_column(&held.fleet, &held.event_id, "status")
            .await,
        Some(afd_core::event::status::PROCESSED.to_owned()),
        "the run's RESULT is durable beside the charge for it — the pairing this dimension \
         exists for"
    );
    assert_eq!(
        held.fixtures
            .event_column(&held.fleet, &held.event_id, "response_text")
            .await,
        Some(RESPONSE_ACCEPTED.to_owned()),
        "and it is the answer the accepted report carried"
    );
    let cursor = held
        .fixtures
        .session_column(&held.fleet, "context_json")
        .await
        .expect("the checkpoint rides the same commit");
    assert!(
        cursor.contains(RESUME_EVENT_ID),
        "the session resumes after the event this run executed, not before it: {cursor}"
    );
    assert_eq!(
        held.fixtures
            .affinity_column(&held.fleet, "leased_until")
            .await,
        Some(settled_at.to_string()),
        "the slot is free, and freed in the same commit — a fresh lease can no longer race a \
         half-written finalize because there is no half-written state to race"
    );
}
