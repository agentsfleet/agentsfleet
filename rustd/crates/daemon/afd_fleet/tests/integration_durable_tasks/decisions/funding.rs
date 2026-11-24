//! A refused funding leaves no call, no payload and no debit; once the payer is funded the
//! same request issues.

use af_core::{error_code, test_util::trace::Capture};
use af_wire::durable_task::DecisionCallState;
use afd_fleet::durable_task::DurableTaskLifecycle as _;
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
async fn a_refused_funding_leaves_no_call() {
    for funding in [Funding::Short, Funding::Debt] {
        exercise(async |case| refused(case, funding).await).await;
    }
}

#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
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
    refuse_twice(&fixture).await;
    funding_events(&capture, case, reason);
    recover(&fixture, balance).await;
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn refuse_twice(fixture: &Decisions<'_>) {
    let case = fixture.case;
    let financial = wallet_snapshot(fixture).await;
    for _ in 0..2 {
        let input = fixture.call_input().await;
        let failure = fixture
            .tasks
            .reserve_decision(case.run(), &input)
            .await
            .expect_err("native funding refusal cannot issue provider material");
        assert_eq!(
            failure.code(),
            error_code::DURABLE_TASK_DECISION_UNAVAILABLE
        );
        assert_eq!(fixture.counts().await, (0, 0, 0), "a refusal saves nothing");
        assert_eq!(wallet_snapshot(fixture).await, financial);
    }
}

fn funding_events(capture: &Capture, case: &Case, reason: &'static str) {
    let events = capture.events();
    let refusals = events
        .iter()
        .filter(|event| event.field("event") == Some("dt_decision_funding_refused"))
        .map(|event| (event.field("reason"), event.field("run_id")))
        .collect::<Vec<_>>();
    assert_eq!(
        refusals,
        [(Some(reason), Some(case.run().run_id.as_str())); 2]
    );
}

async fn recover(fixture: &Decisions<'_>, balance: i64) {
    let case = fixture.case;
    case.held
        .fixtures
        .seed_wallet(&case.held.tenant, balance)
        .await;
    let input = fixture.call_input().await;
    let (issued, body) = fixture.reserve(&input).await;
    assert!(issued);
    assert_eq!(fixture.counts().await, (1, 1, 0));
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
    // The known receipt's line debited the wallet as it was logged, once.
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
    sqlx::query_as("SELECT to_jsonb(w),COALESCE((SELECT jsonb_agg(to_jsonb(u) ORDER BY u.seq) FROM billing.usage_lines u WHERE u.work_id=$2),'[]'::jsonb) FROM billing.wallets w WHERE w.tenant_id=$1")
        .bind(&fixture.case.held.tenant).bind(&fixture.case.held.run_key)
        .fetch_one(&mut *fixture.case.held.fixtures.database.acquire().await.expect("native financial observer"))
        .await.expect("entire wallet and original usage ledger")
}
