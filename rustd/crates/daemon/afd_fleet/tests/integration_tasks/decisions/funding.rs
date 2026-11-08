//! Native credit refusals retain one unsent assessment and never debit its payer.

use af_core::{error_code, test_util::trace::Capture};
use af_wire::task::{DecisionAssessment, DecisionCallState};
use afd_fleet::task::TaskLifecycle as _;
use serde_json::{Value, json};

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, known, request},
};

#[derive(Clone, Copy)]
enum Funding {
    Short,
    Debt,
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn native_credit_shortage_and_payer_debt_preserve_unsent_calls_without_charges() {
    for funding in [Funding::Short, Funding::Debt] {
        exercise(async |case| refused(case, funding).await).await;
    }
}

async fn refused(case: &Case, funding: Funding) {
    let capture = Capture::install();
    let fixture = Decisions::create(case).await;
    let balance = case
        .held
        .fixtures
        .balance(&case.held.tenant)
        .await
        .expect("funded owner");
    let remaining = match funding {
        Funding::Short => 0,
        Funding::Debt => -1,
    };
    case.held
        .fixtures
        .seed_wallet(&case.held.tenant, remaining)
        .await;
    let reason = "insufficient_credit";
    let call = retain_refusals(&fixture).await;
    funding_events(&capture, case, reason);
    recover(&fixture, &call, balance).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn retain_refusals(fixture: &Decisions<'_>) -> Value {
    let case = fixture.case;
    let financial = wallet_snapshot(fixture).await;
    let mut saved = None;
    for _ in 0..2 {
        let input = fixture.reservation(DecisionAssessment::Message, None);
        let failure = fixture
            .tasks
            .reserve_decision(case.lease(), &input)
            .await
            .expect_err("native funding refusal cannot issue provider material");
        assert_eq!(failure.code(), error_code::TASK_DECISION_UNAVAILABLE);
        assert_eq!(fixture.counts().await, (1, 1, 0));
        assert_eq!(wallet_snapshot(fixture).await, financial);
        let current = fixture.call_snapshot().await;
        if let Some(original) = &saved {
            assert_eq!(&current, original);
        } else {
            saved = Some(current);
        }
    }
    saved.expect("one retained assessment").0
}

fn funding_events(capture: &Capture, case: &Case, reason: &'static str) {
    let events = capture.events();
    let refusals = events
        .iter()
        .filter(|event| event.field("event") == Some("dt_decision_funding_refused"))
        .map(|event| (event.field("reason"), event.field("lease_id")))
        .collect::<Vec<_>>();
    assert_eq!(
        refusals,
        [(Some(reason), Some(case.lease().lease_id.as_str())); 2]
    );
}

async fn recover(fixture: &Decisions<'_>, call: &Value, balance: i64) {
    let case = fixture.case;
    case.held
        .fixtures
        .seed_wallet(&case.held.tenant, balance)
        .await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let (issued, body) = fixture.reserve(&input).await;
    assert!(issued);
    assert_eq!(body.get("call_id"), call.get("id"));
    let request = request(&body);
    let receipt = known(&request);
    let recorded = fixture.record(&request.call_id, receipt.clone()).await;
    assert_eq!(
        (
            recorded.state,
            recorded.charged_nanos.as_deref(),
            recorded.reserved_nanos.as_str()
        ),
        (DecisionCallState::Known, Some("3"), "0")
    );
    let mut replay = recorded;
    replay.replayed = true;
    assert_eq!(fixture.record(&request.call_id, receipt).await, replay);
    assert_eq!(fixture.counts().await, (1, 1, 1));
    assert_eq!(fixture.wallet().await, balance);
    case.held.relay().await;
    assert_eq!(
        case.held.fixtures.balance(&case.held.tenant).await,
        Some(balance - 3)
    );
    let (issued, response) = fixture.reserve(&input).await;
    assert!(!issued);
    assert_eq!(
        response.pointer("/recorded/disposition"),
        Some(&json!("current"))
    );
    assert_eq!(fixture.counts().await, (1, 1, 1));
}

async fn wallet_snapshot(fixture: &Decisions<'_>) -> (Value, Value) {
    sqlx::query_as("SELECT to_jsonb(w),COALESCE((SELECT jsonb_agg(to_jsonb(u) ORDER BY u.seq) FROM billing.usage_lines u WHERE u.work_id=$2::uuid),'[]'::jsonb) FROM billing.wallets w WHERE w.tenant_id=$1::uuid")
        .bind(&fixture.case.held.tenant).bind(fixture.case.held.lease_key.as_str())
        .fetch_one(&mut *fixture.case.held.fixtures.database.acquire().await.expect("native financial observer"))
        .await.expect("entire wallet and original usage ledger")
}
