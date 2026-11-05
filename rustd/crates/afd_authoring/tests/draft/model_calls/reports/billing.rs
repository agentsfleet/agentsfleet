//! A setup chat's report logs its usage lines and returns; the billing relay
//! settles them once, whether the report races the
//! expiry sweep or the relay's biller cannot answer on its first pass.

use super::{Attempt, Fixture, accounting, balance, fixture, report};
use afd_authoring::types::Source;
use afd_billing::ledger::LedgerBiller;
use afd_billing::sql::work::AUTHORING_ATTEMPT;
use afd_db::test_util::unreachable_db;
use std::sync::Arc;

/// One known dispatch line and one runtime line.
const LINES: i64 = 2;
/// One reported millisecond of runtime; the provider is billed by the owner's
/// key, so its tokens charge nothing.
const RUNTIME_DEBIT: i64 = 100;

/// The attempt's usage lines, and how many are settled.
pub(super) async fn ledger(f: &Fixture, a: &Attempt) -> (i64, i64) {
    sqlx::query_as(
        "SELECT count(*), count(settled_at) \
        FROM billing.usage_lines WHERE work_type=$2 AND work_id=$1::uuid",
    )
    .bind(a.id.as_str())
    .bind(AUTHORING_ATTEMPT)
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("the attempt's ledger")
}

async fn with_known_call(f: &Fixture) -> Attempt {
    let a = Attempt::start(f, f.access(), Source::Blank).await;
    let calls = fixture::calls(f, &a, Arc::new(fixture::Provider::known())).await;
    calls
        .call(&f.runner, &a.lease, fixture::request(&a))
        .await
        .expect("known physical receipt");
    a
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires live Postgres"]
async fn a_report_racing_the_expiry_sweep_settles_once() {
    let f = Fixture::create().await;
    let a = with_known_call(&f).await;
    sqlx::query(
        "UPDATE authoring.attempts SET lease_expires_at=clock_timestamp() WHERE id=$1::uuid",
    )
    .bind(a.id.as_str())
    .execute(&mut *f.connection().await)
    .await
    .expect("the lease is due for the expiry sweep");
    let before = balance(&f).await;
    let request = report();
    let (reported, swept) = tokio::join!(
        f.drafts.report(&f.runner, &a.lease, request.clone()),
        f.drafts.maintain()
    );
    swept.expect("the expiry sweep runs");
    // Whichever side won, the runner resends its accounting until it reads
    // settled, as its supervisor does.
    if !reported.is_ok_and(|ack| ack.settled) {
        let late = f
            .drafts
            .report(&f.runner, &a.lease, accounting(&request))
            .await
            .expect("the resent accounting settles");
        assert!(late.settled, "known usage settles on the resend");
    }
    f.relay(a.id.as_str()).await;
    assert_eq!(ledger(&f, &a).await, (LINES, LINES));
    assert_eq!(balance(&f).await, before - RUNTIME_DEBIT, "charged once");
    let again = f
        .drafts
        .report(&f.runner, &a.lease, accounting(&request))
        .await
        .expect("a further resend replays");
    assert!(again.settled && again.replayed);
    f.relay(a.id.as_str()).await;
    assert_eq!(ledger(&f, &a).await, (LINES, LINES));
    assert_eq!(balance(&f).await, before - RUNTIME_DEBIT, "still once");
    f.cleanup().await;
}

/// A setup chat report logs its lines, settles the attempt and
/// returns, and moves no money: the lines wait unsettled for the relay.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_report_logs_its_lines_and_leaves_them_to_the_relay() {
    let f = Fixture::create().await;
    let a = with_known_call(&f).await;
    let before = balance(&f).await;
    let ack = f
        .drafts
        .report(&f.runner, &a.lease, report())
        .await
        .expect("the report settles the attempt");
    assert!(ack.settled, "known usage settles the attempt");
    assert_eq!(
        ledger(&f, &a).await,
        (LINES, 0),
        "lines logged, none charged"
    );
    assert_eq!(balance(&f).await, before, "the report moved no money");
    f.relay(a.id.as_str()).await;
    f.cleanup().await;
}

/// The relay settles a reported attempt's lines on its next pass, with no
/// wait. A pass whose biller cannot answer charges nothing and the next one
/// does; a further pass charges nothing more.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_reported_attempt_is_settled_on_the_next_pass() {
    // Its own database: a pass settles every unsettled line it can see.
    let f = Fixture::isolated().await;
    let a = with_known_call(&f).await;
    let before = balance(&f).await;
    f.drafts
        .report(&f.runner, &a.lease, report())
        .await
        .expect("the report settles the attempt");

    let refused =
        afd_billing::ledger::resettle(&f.db, &LedgerBiller::on_system_clock(unreachable_db()))
            .await
            .expect("the line pass reads its batch");
    assert_eq!(
        refused.settled, 0,
        "a biller that cannot answer charges nothing"
    );
    assert!(refused.failed >= 1, "{refused:?}");
    assert_eq!(ledger(&f, &a).await, (LINES, 0), "the lines wait");

    let biller = crate::draft::fixture::ledger(&f.db);
    for pass in 0..2 {
        afd_billing::ledger::resettle(&f.db, biller.as_ref())
            .await
            .expect("the line pass runs");
        assert_eq!(ledger(&f, &a).await, (LINES, LINES), "pass {pass}");
        assert_eq!(
            balance(&f).await,
            before - RUNTIME_DEBIT,
            "charged once, pass {pass}"
        );
    }
    f.cleanup().await;
}
