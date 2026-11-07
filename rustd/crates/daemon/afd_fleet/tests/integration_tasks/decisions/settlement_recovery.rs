//! Receipt replay, lease completion and sweeping preserve one original charge.

use std::time::Duration;

use af_core::{clock, error_code, test_util::trace::Capture};
use af_crypto::entropy::Entropy;
use af_wire::{
    task::{
        DecisionCallState, DecisionReceipt, DecisionReceiptInput, DecisionReceiptResult,
        DecisionRecordDisposition, DecisionTaskVersion, DecisionUnavailableCause,
    },
    usage::DECISION_SEQ_BASE,
};
use afd_billing::ledger::LedgerBiller;
use afd_fleet::{
    lease::billing_sweep::{BillingSwept, GRACE, sweep},
    task::{TaskLifecycle as _, Tasks},
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
    settled_cost: i64,
    decision_rows: i64,
    unsettled: i64,
    decision_cost: i64,
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn receipt_settlement_recovers_without_replay_charges_or_losing_unknown_calls() {
    exercise(recovery).await;
}

async fn recovery(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let balance = fixture.wallet().await;
    let first = fixture.reserve(&fixture.goal_reservation(&task)).await.1;
    let second = fixture.reserve(&fixture.goal_reservation(&task)).await.1;
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
    assert_eq!(financials(&fixture).await, expected(balance, 0, 1, 1));
    assert_eq!(
        swept(&fixture).await,
        BillingSwept {
            resettled: 2,
            ..BillingSwept::default()
        }
    );
    assert_eq!(financials(&fixture).await, expected(balance, fee + 3, 1, 0));
    late_usage(&fixture, &second, &task, balance, fee).await;
    original_attribution(&fixture).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn deferred_receipt(fixture: &Decisions<'_>, body: &Value, balance: i64) {
    let mut dependencies = fixture.dependencies.clone();
    dependencies.biller = Faulty::over(
        &fixture.case.held.fixtures.database,
        Faults { settle: true },
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
    assert!(
        !capture
            .events()
            .iter()
            .any(|event| event.field("event") == Some("lease_settle_deferred"))
    );
    drop(capture);
    let replay = fixture
        .record(&request.call_id, input.into_inner().receipt)
        .await;
    let mut expected_receipt = recorded;
    expected_receipt.replayed = true;
    assert_eq!(replay, expected_receipt);
    assert_eq!(financials(fixture).await, expected(balance, 0, 1, 1));
    assert_eq!(fixture.counts().await, (2, 2, 1));
    failed_relay(fixture).await;
    assert_eq!(financials(fixture).await, expected(balance, 0, 1, 1));
}

async fn failed_relay(fixture: &Decisions<'_>) {
    let database = &fixture.case.held.fixtures.database;
    let biller = Faulty::over(database, Faults { settle: true });
    let failed = sweep(database, &*biller, &Entropy::new(), clock::now(), None)
        .await
        .expect("failed relay retains measured usage");
    assert_eq!(
        failed,
        BillingSwept {
            settle_failures: 1,
            refused: Some(error_code::INTERNAL_DB_UNAVAILABLE),
            ..BillingSwept::default()
        }
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
    assert!(
        !capture
            .events()
            .iter()
            .any(|event| event.field("event") == Some("lease_release_deferred"))
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
    assert_eq!(financials(fixture).await, expected(balance, fee + 3, 2, 1));
    assert_eq!(
        swept(fixture).await,
        BillingSwept {
            resettled: 1,
            ..BillingSwept::default()
        }
    );
    let completed = expected(balance, fee + 6, 2, 0);
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

const fn expected(balance: i64, charged: i64, rows: i64, unsettled: i64) -> Financials {
    Financials {
        balance: balance - charged,
        settled_cost: charged,
        decision_rows: rows,
        unsettled,
        decision_cost: rows * 3,
    }
}

async fn financials(fixture: &Decisions<'_>) -> Financials {
    sqlx::query_as("SELECT w.balance_nanos AS balance,(SELECT COALESCE(sum(u.charged_nanos),0)::bigint FROM billing.usage_lines u WHERE u.work_id=l.id AND u.settled_at IS NOT NULL) AS settled_cost,(SELECT count(*) FROM billing.usage_lines u WHERE u.work_id=l.id AND u.seq >= $2) AS decision_rows,(SELECT count(*) FROM billing.usage_lines u WHERE u.work_id=l.id AND u.seq >= $2 AND u.settled_at IS NULL) AS unsettled,(SELECT COALESCE(sum(u.charged_nanos),0)::bigint FROM billing.usage_lines u WHERE u.work_id=l.id AND u.seq >= $2) AS decision_cost FROM runner.leases l JOIN billing.wallets w ON w.tenant_id=l.tenant_id WHERE l.id=$1::uuid")
        .bind(fixture.case.held.lease_key.as_str()).bind(i64::from(DECISION_SEQ_BASE))
        .fetch_one(&mut *fixture.case.held.fixtures.database.acquire().await.expect("original financial state"))
        .await.expect("complete original wallet and immutable usage")
}

async fn swept(fixture: &Decisions<'_>) -> BillingSwept {
    let database = &fixture.case.held.fixtures.database;
    let biller = LedgerBiller::on_system_clock(database.clone());
    sweep(database, &biller, &Entropy::new(), clock::now(), None)
        .await
        .expect("normal native financial recovery")
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
