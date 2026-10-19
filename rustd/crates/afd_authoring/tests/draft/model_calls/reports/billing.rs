//! A setup chat's money moves through the biller: its usage lines are logged
//! once, settled once, and its hold released, whether its report races the
//! expiry sweep or its biller cannot answer until the billing sweeper runs.

use super::{Attempt, Fixture, accounting, balance, fixture, report};
use afd_authoring::Drafts;
use afd_authoring::types::Source;
use afd_billing::ledger::LedgerBiller;
use afd_billing::sql::work::AUTHORING_ATTEMPT;
use afd_core::{error_code, test_util::trace::Capture};
use afd_crypto::entropy::Entropy;
use afd_db::test_util::unreachable_db;
use std::sync::Arc;

/// One known dispatch line and one runtime line.
const LINES: i64 = 2;
/// One reported millisecond of runtime; the provider is billed by the owner's
/// key, so its tokens charge nothing.
const RUNTIME_DEBIT: i64 = 100;
/// Moves a line and its attempt past the billing sweeper's grace.
const AGED: &str = "WITH lines AS (UPDATE billing.usage_lines SET created_at=created_at-interval '1 hour' \
WHERE work_type=$2 AND work_id=$1::uuid) \
UPDATE authoring.attempts SET updated_at=updated_at-interval '1 hour' WHERE id=$1::uuid";

/// The attempt's usage lines, how many are settled, and whether its hold is
/// still open.
pub(super) async fn ledger(f: &Fixture, a: &Attempt) -> (i64, i64, bool) {
    sqlx::query_as(
        "SELECT count(*), count(settled_at), \
        EXISTS(SELECT 1 FROM billing.holds WHERE work_type=$2 AND work_id=$1::uuid AND released_at IS NULL) \
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
    assert_eq!(ledger(&f, &a).await, (LINES, LINES, false));
    assert_eq!(balance(&f).await, before - RUNTIME_DEBIT, "charged once");
    let again = f
        .drafts
        .report(&f.runner, &a.lease, accounting(&request))
        .await
        .expect("a further resend replays");
    assert!(again.settled && again.replayed);
    assert_eq!(ledger(&f, &a).await, (LINES, LINES, false));
    assert_eq!(balance(&f).await, before - RUNTIME_DEBIT, "still once");
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn lines_a_biller_could_not_settle_are_settled_and_released_by_the_sweeper_once() {
    let f = Fixture::create().await;
    let a = with_known_call(&f).await;
    let offline = Drafts::new(
        f.db.clone(),
        Entropy::new(),
        Arc::new(LedgerBiller::on_system_clock(unreachable_db())),
    );
    let before = balance(&f).await;
    let capture = Capture::install();
    let ack = offline
        .report(&f.runner, &a.lease, report())
        .await
        .expect("the report commits though its biller cannot answer");
    let deferred = [
        capture.only("setup_chat_settle_deferred"),
        capture.only("setup_chat_release_deferred"),
    ];
    drop(capture);
    assert!(ack.settled, "the attempt settled; only its money waits");
    for event in deferred {
        assert_eq!(event.field("work_id"), Some(a.id.as_str()));
        assert_eq!(
            event.field("error_code"),
            Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str()),
            "the outage is named: {event:?}"
        );
    }
    assert_eq!(
        ledger(&f, &a).await,
        (LINES, 0, true),
        "lines logged, none charged, the hold still open"
    );
    assert_eq!(balance(&f).await, before, "no money moved yet");

    sqlx::query(AGED)
        .bind(a.id.as_str())
        .bind(AUTHORING_ATTEMPT)
        .execute(&mut *f.connection().await)
        .await
        .expect("past the sweeper's grace");
    let unreleased = afd_authoring::billing_sweep::release_finished(
        &f.db,
        &LedgerBiller::on_system_clock(unreachable_db()),
    )
    .await
    .expect("the hold sweep reads its batch");
    assert_eq!(
        unreleased.released, 0,
        "a biller that cannot answer frees nothing"
    );
    assert!(unreleased.release_failures >= 1, "{unreleased:?}");
    assert_eq!(ledger(&f, &a).await, (LINES, 0, true), "the hold waits");
    let biller = crate::draft::fixture::ledger(&f.db);
    for pass in 0..2 {
        afd_billing::ledger::resettle(&f.db, biller.as_ref())
            .await
            .expect("the line sweep runs");
        afd_authoring::billing_sweep::release_finished(&f.db, biller.as_ref())
            .await
            .expect("the hold sweep runs");
        assert_eq!(ledger(&f, &a).await, (LINES, LINES, false), "pass {pass}");
        assert_eq!(
            balance(&f).await,
            before - RUNTIME_DEBIT,
            "charged once, pass {pass}"
        );
    }
    f.cleanup().await;
}
