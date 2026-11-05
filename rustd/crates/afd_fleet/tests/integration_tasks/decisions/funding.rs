//! Native credit refusals retain one unsent assessment and never debit its payer.

use afd_billing::{
    Accounts, Nanos, Posture,
    biller::{Account, Hold, Release, Reservation, WorkKey},
    rates::Estimate,
};
use afd_core::{clock, error_code, test_util::trace::Capture};
use afd_fleet::task::TaskLifecycle as _;
use afd_wire::task::{DecisionAssessment, DecisionCallState};
use serde_json::{Value, json};

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, known, request},
};
use crate::seed::{MODEL, PROVIDER};

#[derive(Clone, Copy)]
enum Funding {
    Short,
    Closed,
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn native_credit_shortage_and_closed_work_preserve_unsent_calls_without_charges() {
    for funding in [Funding::Short, Funding::Closed] {
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
    let work = original_hold(&fixture).await;
    let reason = match funding {
        Funding::Short => {
            case.held.fixtures.seed_wallet(&case.held.tenant, 0).await;
            "insufficient_credit"
        }
        Funding::Closed => {
            fixture
                .dependencies
                .biller
                .release(&work, Release::Completed)
                .await
                .expect("native hold closes");
            "work_closed"
        }
    };
    let call = retain_refusals(&fixture).await;
    funding_events(&capture, case, reason);
    if matches!(funding, Funding::Short) {
        recover(&fixture, &call, balance).await;
    }
    fixture
        .model
        .finish()
        .await
        .expect("decision model fixture lifecycle");
}

async fn original_hold(fixture: &Decisions<'_>) -> WorkKey {
    let case = fixture.case;
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("native coding price");
    let estimate = Accounts::estimate_on(
        &mut connection,
        Posture::Platform,
        PROVIDER,
        MODEL,
        clock::now(),
    )
    .await
    .expect("current coding estimate");
    assert!(matches!(estimate, Estimate::Priced(_)));
    let ceiling = estimate.floor();
    drop(connection);
    let ask = Reservation {
        work: WorkKey::FleetLease(case.lease().lease_id.clone()),
        payer: Account {
            tenant: id(&case.held.tenant),
            workspace: case.scope.workspace_id.clone(),
        },
        ceiling,
    };
    assert_eq!(
        fixture
            .dependencies
            .biller
            .reserve(&ask)
            .await
            .expect("native initial lease hold"),
        Hold::Held {
            ceiling,
            captured: Nanos::ZERO
        }
    );
    ask.work
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
    let work = WorkKey::FleetLease(fixture.case.lease().lease_id.clone());
    sqlx::query_as("SELECT to_jsonb(w),to_jsonb(h) FROM billing.wallets w JOIN billing.holds h ON h.tenant_id=w.tenant_id AND h.work_id=$2::uuid AND h.work_type=$3 WHERE w.tenant_id=$1::uuid")
        .bind(&fixture.case.held.tenant).bind(work.id().as_str()).bind(work.work_type())
        .fetch_one(&mut *fixture.case.held.fixtures.database.acquire().await.expect("native financial observer"))
        .await.expect("entire wallet and owned hold")
}
