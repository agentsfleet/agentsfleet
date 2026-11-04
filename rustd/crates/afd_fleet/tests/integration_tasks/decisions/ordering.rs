//! Intent charges share the slot that protects the next physical issuance.

use std::time::Duration;

use afd_core::error_code;
use afd_fleet::task::TaskLifecycle as _;
use afd_wire::task::{DecisionAssessment, DecisionCallState};
use sqlx::Connection as _;
use tokio::time::{interval, timeout};

use super::{
    super::fixture::{Case, GOALS, exercise},
    fixture::{Decisions, known, lock_slot, request},
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn intent_receipts_serialize_with_issuance_before_actual_overages_block_more_paid_work() {
    exercise(ordered).await;
}

async fn ordered(case: &Case) {
    let fixture = Decisions::create(case).await;
    fixture.reprice().await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let (_, body) = fixture.reserve(&input).await;
    let call = body
        .get("call_id")
        .and_then(serde_json::Value::as_str)
        .expect("issued intent");
    let receipt = known(&request(&body));
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("issuance slot owner");
    let mut blocker = connection.begin().await.expect("issuance slot transaction");
    let pid = lock_slot(&mut blocker, case).await;
    let unblock = async {
        waiting_on_slot(&fixture, pid).await;
        assert_eq!(fixture.counts().await, (1, 1, 0));
        blocker.rollback().await.expect("release issuance slot");
    };
    let (recorded, ()) = tokio::join!(fixture.record(call, receipt), unblock);
    assert_eq!(
        (
            recorded.state,
            recorded.charged_nanos.as_deref(),
            recorded.overage_nanos.as_deref(),
            recorded.reserved_nanos.as_str()
        ),
        (
            DecisionCallState::Known,
            Some("40000002"),
            Some("20000002"),
            "0"
        )
    );
    assert_eq!(fixture.counts().await, (1, 1, 1));
    next_physical_call_is_blocked(&fixture).await;
    fixture.model.finish().await;
}

async fn waiting_on_slot(fixture: &Decisions<'_>, pid: i32) {
    let observed = async {
        let mut ticks = interval(Duration::from_millis(10));
        loop {
            ticks.tick().await;
            let blocked: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(pid)
            .fetch_one(
                &mut *fixture
                    .case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("lock observer"),
            )
            .await
            .expect("real database lock wait");
            if blocked {
                break;
            }
        }
    };
    timeout(Duration::from_secs(2), observed)
        .await
        .expect("intent receipt must wait for issuance's slot");
}

async fn next_physical_call_is_blocked(fixture: &Decisions<'_>) {
    let input = fixture.reservation(
        DecisionAssessment::Item {
            ordinal: 0,
            interpretation_digest: "a".repeat(64),
            source_start: 0,
            source_end: i32::try_from(GOALS.first().expect("complete source action").len())
                .expect("bounded source"),
        },
        None,
    );
    let refused = fixture
        .tasks
        .reserve_decision(fixture.case.lease(), &input)
        .await
        .expect_err("a fresh physical intent cannot bypass its durable actual overage");
    assert_eq!(refused.code(), error_code::TASK_LIMIT_EXHAUSTED);
    assert_eq!(fixture.counts().await, (1, 1, 1));
}
