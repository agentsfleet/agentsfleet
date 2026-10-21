//! Report result, usage, cursor, slot, and admission settlement commit together.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet::lease::{Committed, Leases, Reported, sql};
use jiff::Timestamp;

use crate::report_commit::{
    LEDGER_ROWS_AFTER_SETTLE, RESPONSE_ACCEPTED, RESPONSE_POSTGRES_REFUSES, RESUME_EVENT_ID,
    assert_nothing_landed, report,
};
use crate::report_seed::{DEEP_POOL, Held, SLICE_NANOS, held, run_fee};
use crate::rows::Row;

const RUNTIME_LINE: &str = "2147483648";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_report_persists_result_with_settlement_atomically() {
    let held = held().await;
    let lease_id = held.issued.lease_id.as_str();
    let settled_at = held.now;
    let from = held.run_a_slice().await;
    let leases = held.fixtures.leases_with_dead_queue();
    let lease = leases
        .load_for_report(lease_id, &held.runner)
        .await
        .expect("the lease load must reach the datastore")
        .expect("the seeded lease belongs to the seeded runner");

    assert_a_refused_result_charges_nothing(&held, &leases, &lease, settled_at).await;
    let slice = assert_the_retry_commits_all_four(&held, &leases, &lease, settled_at, from).await;
    assert_the_repeat_changes_nothing(&held, &leases, &lease, settled_at, slice).await;

    held.fixtures.cleanup().await;
}

async fn assert_a_refused_result_charges_nothing(
    held: &Held,
    leases: &Leases,
    lease: &Reported,
    settled_at: Timestamp,
) {
    let lease_id = held.issued.lease_id.as_str();
    let refused = leases
        .commit_report(report(
            lease_id,
            &held.runner,
            lease,
            (&held.lease_key, &held.pricing),
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
        held.fixtures.text(Row::Lease(lease_id), "status").await,
        Some(sql::LEASE_STATUS_ACTIVE.to_owned()),
        "the lease stays active, which is what makes the runner's retry possible at all"
    );
}

async fn assert_the_retry_commits_all_four(
    held: &Held,
    leases: &Leases,
    lease: &Reported,
    settled_at: Timestamp,
    from: Timestamp,
) -> i64 {
    let lease_id = held.issued.lease_id.as_str();
    let committed = leases
        .commit_report(report(
            lease_id,
            &held.runner,
            lease,
            (&held.lease_key, &held.pricing),
            RESPONSE_ACCEPTED,
            settled_at,
        ))
        .await
        .expect("the retry must reach the datastore");
    let Committed::Settled {
        settlement,
        closed,
        owed,
    } = committed
    else {
        unreachable!("the only holder of this fleet cannot be fenced out of its own retry")
    };
    let slice = run_fee(held.metered_from(from).await);
    assert!(
        slice >= SLICE_NANOS,
        "the aged meter charges at least a slice: {slice}"
    );
    assert_eq!(
        line_charge(settlement.as_deref()),
        slice,
        "the retry logs the slice ONCE — the refused attempt logged nothing"
    );
    assert!(
        owed.is_none(),
        "this lease's event reached the stream without an admission, so it names no \
         destination and nothing is owed; the report-owes-destination suite proves the \
         owed branch"
    );
    let closed = closed.expect(
        "the terminal write closed a row, and the frame announcing it rides that row rather \
         than a second read taken after the commit",
    );
    assert_closing_carries_the_counters(held, &closed).await;
    assert_all_four_landed(held, slice).await;
    slice
}

async fn assert_closing_carries_the_counters(held: &Held, closed: &afd_events::Closed) {
    let stored = afd_events::fleet_counters(&held.fixtures.database, &held.fleet)
        .await
        .expect("the counters read back");
    assert_ne!(
        stored.events_processed, stored.budget_used_nanos,
        "the two counters must differ for a swap to be visible"
    );
    assert_eq!(
        closed.counters, stored,
        "the closing's counters are the database's"
    );
}

async fn assert_the_repeat_changes_nothing(
    held: &Held,
    leases: &Leases,
    lease: &Reported,
    settled_at: Timestamp,
    slice: i64,
) {
    let repeated = leases
        .commit_report(report(
            held.issued.lease_id.as_str(),
            &held.runner,
            lease,
            (&held.lease_key, &held.pricing),
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
    assert_all_four_landed(held, slice).await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_report_over_an_ended_event_keeps_the_stored_result() {
    let held = held().await;
    let lease_id = held.issued.lease_id.as_str();
    let settled_at = held.now;
    let from = held.run_a_slice().await;
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
            (&held.lease_key, &held.pricing),
            RESPONSE_ACCEPTED,
            settled_at,
        ))
        .await
        .expect("the report must reach the datastore");
    let Committed::Settled {
        settlement,
        closed,
        owed,
    } = committed
    else {
        unreachable!("the lease is this runner's and still active, so the claim wins")
    };
    assert_eq!(
        line_charge(settlement.as_deref()),
        run_fee(held.metered_from(from).await),
        "the run happened and its usage is logged, whatever the event row already said"
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
            .text(Row::Event(&held.fleet, &held.event_id), "response_text")
            .await,
        None,
        "and the earlier delivery's row is left exactly as it was — a terminal row is \
         never reopened by a later report"
    );
    assert_eq!(
        held.fixtures.text(Row::Lease(lease_id), "status").await,
        Some(sql::LEASE_STATUS_REPORTED.to_owned()),
        "the lease is terminal, because the settle committed with the rest"
    );

    held.fixtures.cleanup().await;
}

fn line_charge(settlement: Option<&afd_billing::biller::Settlement>) -> i64 {
    settlement.map_or(0, |settled| {
        settled.lines.iter().map(|line| line.charged.as_i64()).sum()
    })
}

async fn assert_all_four_landed(held: &Held, slice: i64) {
    let settled: bool = sqlx::query_scalar(
        "SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id = $1::uuid",
    )
    .bind(&held.event_id)
    .fetch_one(&mut *held.fixtures.database.acquire().await.expect("database"))
    .await
    .expect("admission settlement");
    assert!(settled, "the successful report settles its queue row");

    let lease_id = held.issued.lease_id.as_str();
    assert_eq!(
        held.fixtures.balance(&held.tenant).await,
        Some(DEEP_POOL),
        "the report's transaction logs usage and moves no money: the biller charges \
         after commit"
    );
    assert_eq!(
        held.fixtures.ledger_rows(&held.event_id).await,
        LEDGER_ROWS_AFTER_SETTLE,
        "one usage line for the final slice, however many times the report is sent"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Line(&held.event_id, RUNTIME_LINE), "charged_nanos")
            .await,
        Some(slice.to_string()),
        "the line is priced at the slice it measured"
    );
    assert_eq!(
        held.fixtures.text(Row::Lease(lease_id), "status").await,
        Some(sql::LEASE_STATUS_REPORTED.to_owned()),
        "the lease is terminal"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Event(&held.fleet, &held.event_id), "status")
            .await,
        Some(afd_core::event::status::PROCESSED.to_owned()),
        "the run's RESULT is durable beside the charge for it — the pairing this dimension \
         exists for"
    );
    assert_eq!(
        held.fixtures
            .text(Row::Event(&held.fleet, &held.event_id), "response_text")
            .await,
        Some(RESPONSE_ACCEPTED.to_owned()),
        "and it is the answer the accepted report carried"
    );
    let cursor = held
        .fixtures
        .text(Row::Session(&held.fleet), "context_json")
        .await
        .expect("the checkpoint rides the same commit");
    assert!(
        cursor.contains(RESUME_EVENT_ID),
        "the session resumes after the event this run executed, not before it: {cursor}"
    );
    assert!(
        held.fixtures.slot_is_free(&held.fleet).await,
        "the slot is free, and freed in the same commit — a fresh lease can no longer race a \
         half-written finalize because there is no half-written state to race"
    );
}
