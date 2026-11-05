//! Receipt replay, lease completion and sweeping preserve one original charge.

use std::time::Duration;

use afd_billing::ledger::LedgerBiller;
use afd_core::{clock, error_code, test_util::trace::Capture};
use afd_crypto::entropy::Entropy;
use afd_fleet::{
    lease::billing_sweep::{BillingSwept, GRACE, sweep},
    task::{TaskLifecycle as _, Tasks},
};
use afd_wire::{
    task::{
        DecisionCallState, DecisionReceipt, DecisionReceiptInput, DecisionReceiptResult,
        DecisionRecordDisposition, DecisionTaskVersion, DecisionUnavailableCause,
    },
    usage::DECISION_SEQ_BASE,
};
use serde_json::Value;

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, known, request},
};
use crate::{
    faulty_biller::{Faults, Faulty},
    integration_report_memory::report,
    rows::Row,
};

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct Financials {
    balance: i64,
    captured: i64,
    released: bool,
    decision_rows: i64,
    unsettled: i64,
    decision_cost: i64,
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn receipt_settlement_recovers_without_replay_charges_or_releasing_unknown_calls() {
    exercise(recovery).await;
}

async fn recovery(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let balance = fixture.wallet().await;
    let first = fixture.reserve(&fixture.goal_reservation(&task)).await.1;
    let second = fixture.reserve(&fixture.goal_reservation(&task)).await.1;
    let retained = hold_identity(&fixture).await;
    deferred_receipt(&fixture, &first, balance).await;
    fixture
        .record(
            &request(&second).call_id,
            DecisionReceipt::Unknown {
                cause: DecisionUnavailableCause::Timeout,
            },
        )
        .await;
    let fee = completed_lease(&fixture).await;
    age_recovery(&fixture, &request(&first).call_id).await;
    assert_eq!(
        financials(&fixture).await,
        expected(balance, fee, 1, 1, false)
    );
    assert_eq!(
        swept(&fixture).await,
        BillingSwept {
            resettled: 1,
            ..BillingSwept::default()
        }
    );
    assert_eq!(
        financials(&fixture).await,
        expected(balance, fee + 3, 1, 0, false)
    );
    late_usage(&fixture, &second, &task, balance, fee).await;
    assert_eq!(hold_identity(&fixture).await, retained);
    original_attribution(&fixture).await;
    fixture.model.finish().await;
}

async fn deferred_receipt(fixture: &Decisions<'_>, body: &Value, balance: i64) {
    let mut dependencies = fixture.dependencies.clone();
    dependencies.biller = Faulty::over(
        &fixture.case.held.fixtures.database,
        Faults {
            settle: true,
            ..Faults::NONE
        },
    );
    let down = Tasks::new(
        fixture.case.held.leases.clone(),
        fixture.case.admissions.clone(),
        dependencies,
    );
    let request = request(body);
    let input = garde::Unvalidated::new(DecisionReceiptInput {
        fencing_token: fixture.case.lease().fencing_token,
        receipt: known(&request),
    })
    .validate()
    .expect("complete provider receipt");
    let capture = Capture::install();
    let recorded = down
        .record_decision(fixture.case.lease(), &id(&request.call_id), &input)
        .await
        .expect("receipt commits before the failed financial transport");
    assert_receipt(&recorded, DecisionRecordDisposition::Current);
    deferred_diagnostic(&capture, fixture);
    drop(capture);
    let replay = fixture
        .record(&request.call_id, input.into_inner().receipt)
        .await;
    let mut expected_receipt = recorded;
    expected_receipt.replayed = true;
    assert_eq!(replay, expected_receipt);
    assert_eq!(financials(fixture).await, expected(balance, 0, 1, 1, false));
    assert_eq!(fixture.counts().await, (2, 2, 1));
}

fn deferred_diagnostic(capture: &Capture, fixture: &Decisions<'_>) {
    let event = capture.only("lease_settle_deferred");
    assert_eq!(event.level, tracing::Level::WARN);
    assert_eq!(
        (
            event.field("error_code"),
            event.field("reason"),
            event.field("work_id")
        ),
        (
            Some(error_code::INTERNAL_DB_UNAVAILABLE.as_str()),
            Some("decision_settlement_pending"),
            Some(fixture.case.held.lease_key.as_str())
        )
    );
}

fn assert_receipt(receipt: &DecisionReceiptResult, disposition: DecisionRecordDisposition) {
    assert_eq!(
        (
            receipt.state,
            receipt.disposition,
            receipt.replayed,
            receipt.charged_nanos.as_deref(),
            receipt.reserved_nanos.as_str()
        ),
        (DecisionCallState::Known, disposition, false, Some("3"), "0")
    );
}

async fn completed_lease(fixture: &Decisions<'_>) -> i64 {
    let held = &fixture.case.held;
    let capture = Capture::install();
    let saved = held
        .fixtures
        .plane()
        .report(
            &held.runner,
            &report(held, held.fence.as_u64(), Vec::new()),
            clock::now(),
        )
        .await
        .expect("normal terminal report");
    assert!(!saved.repeated);
    let event = capture.only("lease_release_deferred");
    assert_eq!(event.level, tracing::Level::INFO);
    assert_eq!(
        (event.field("reason"), event.field("work_id")),
        (Some("decision_pending"), Some(held.lease_key.as_str()))
    );
    assert!(held.fixtures.admission_settled(&held.event_id).await);
    saved.charged.as_i64()
}

async fn age_recovery(fixture: &Decisions<'_>, first: &str) {
    let held = &fixture.case.held;
    let sequence: i64 =
        sqlx::query_scalar("SELECT usage_sequence FROM runner.decision_calls WHERE id=$1::uuid")
            .bind(first)
            .fetch_one(
                &mut *held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("original sequence"),
            )
            .await
            .expect("retained usage identity");
    let elapsed = GRACE.saturating_add(Duration::from_secs(1));
    held.fixtures
        .age(
            Row::Line(&held.event_id, &sequence.to_string()),
            "created_at",
            elapsed,
        )
        .await;
    held.fixtures
        .age(Row::Lease(held.lease_key.as_str()), "updated_at", elapsed)
        .await;
}

async fn late_usage(
    fixture: &Decisions<'_>,
    body: &Value,
    task: &DecisionTaskVersion,
    balance: i64,
    fee: i64,
) {
    let request = request(body);
    let receipt = known(&request);
    let recorded = fixture.record(&request.call_id, receipt.clone()).await;
    assert_receipt(&recorded, DecisionRecordDisposition::Stale);
    assert_eq!(fixture.counts().await, (2, 2, 2));
    assert_eq!(
        financials(fixture).await,
        expected(balance, fee + 6, 2, 0, false)
    );
    assert_eq!(
        swept(fixture).await,
        BillingSwept {
            released: 1,
            ..BillingSwept::default()
        }
    );
    let completed = expected(balance, fee + 6, 2, 0, true);
    assert_eq!(financials(fixture).await, completed);
    assert!(fixture.record(&request.call_id, receipt).await.replayed);
    assert_eq!(swept(fixture).await, BillingSwept::default());
    assert_eq!(financials(fixture).await, completed);
    let usage = fixture.goal_snapshot(task).await.context.usage;
    assert_eq!(
        (
            usage.model_nanos,
            usage.reserved_model_nanos,
            usage.decision_calls
        ),
        (6, 0, 2)
    );
}

fn expected(balance: i64, charged: i64, rows: i64, unsettled: i64, released: bool) -> Financials {
    Financials {
        balance: balance - charged,
        captured: charged,
        released,
        decision_rows: rows,
        unsettled,
        decision_cost: rows * 3,
    }
}

async fn financials(fixture: &Decisions<'_>) -> Financials {
    sqlx::query_as("SELECT w.balance_nanos AS balance,h.captured_nanos AS captured,h.released_at IS NOT NULL AS released,(SELECT count(*) FROM billing.usage_lines u WHERE u.work_id=h.work_id AND u.seq >= $2) AS decision_rows,(SELECT count(*) FROM billing.usage_lines u WHERE u.work_id=h.work_id AND u.seq >= $2 AND u.settled_at IS NULL) AS unsettled,(SELECT COALESCE(sum(u.charged_nanos),0)::bigint FROM billing.usage_lines u WHERE u.work_id=h.work_id AND u.seq >= $2) AS decision_cost FROM billing.holds h JOIN billing.wallets w ON w.tenant_id=h.tenant_id WHERE h.work_id=$1::uuid")
        .bind(fixture.case.held.lease_key.as_str()).bind(i64::from(DECISION_SEQ_BASE))
        .fetch_one(&mut *fixture.case.held.fixtures.database.acquire().await.expect("original financial state"))
        .await.expect("complete original wallet, usage and hold")
}

async fn swept(fixture: &Decisions<'_>) -> BillingSwept {
    let database = &fixture.case.held.fixtures.database;
    let biller = LedgerBiller::on_system_clock(database.clone());
    sweep(database, &biller, &Entropy::new(), clock::now())
        .await
        .expect("normal native financial recovery")
}

async fn hold_identity(fixture: &Decisions<'_>) -> Value {
    sqlx::query_scalar("SELECT to_jsonb(h)-'updated_at'-'captured_nanos'-'released_at' FROM billing.holds h WHERE h.work_id=$1::uuid")
        .bind(fixture.case.held.lease_key.as_str())
        .fetch_one(&mut *fixture.case.held.fixtures.database.acquire().await.expect("original hold identity"))
        .await.expect("hold ceiling and original ownership remain unchanged")
}

async fn original_attribution(fixture: &Decisions<'_>) {
    let held = &fixture.case.held;
    let actual: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('seq',seq,'work_id',work_id,'tenant_id',tenant_id,'workspace_id',workspace_id,'fleet_id',fleet_id,'event_id',event_id,'provider',provider,'model',model,'charged_nanos',charged_nanos,'settled',settled_at IS NOT NULL) FROM billing.usage_lines WHERE work_id=$1::uuid AND seq >= $2 ORDER BY seq")
        .bind(held.lease_key.as_str()).bind(i64::from(DECISION_SEQ_BASE))
        .fetch_all(&mut *held.fixtures.database.acquire().await.expect("original decision lines"))
        .await.expect("both original attributions remain available");
    let expected = [DECISION_SEQ_BASE, DECISION_SEQ_BASE + 1].map(|seq| {
        serde_json::json!({
            "seq":seq,"work_id":held.lease_key.as_str(),"tenant_id":held.tenant,
            "workspace_id":fixture.case.scope.workspace_id.as_str(),"fleet_id":held.fleet,
        "event_id":held.event_id,"provider":"typesafe","model":fixture.model.model.model_name,
            "charged_nanos":3,"settled":true
        })
    });
    assert_eq!(actual, expected);
}
