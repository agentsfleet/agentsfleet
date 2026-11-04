//! Funding outages and cancelled SQL cannot issue credentials or strand a slot.

use std::{sync::Arc, time::Duration};

use afd_billing::{
    biller::{OutagePolicy, WithOutagePolicy},
    ledger::LedgerBiller,
};
use afd_core::{error_code, test_util::trace::Capture};
use afd_db::{Db, config::DbRole, test_util::unreachable_db};
use afd_fleet::{
    lease::Leases,
    task::{TaskLifecycle as _, Tasks},
};
use afd_vault::{SecretBody, SecretName};
use afd_wire::task::{
    DecisionAssessment, DecisionCallState, DecisionRecordDisposition, DecisionReservationInput,
};
use serde_json::{json, value::RawValue};
use sqlx::Connection as _;
use tokio::time::timeout;

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, known, request},
};
use crate::seed::{MODEL, PROVIDER};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn unconfirmed_credit_keeps_the_intent_and_resolves_a_fresh_key_on_recovery() {
    let capture = Capture::install();
    exercise(async |case| funding(case, &capture).await).await;
}

async fn funding(case: &Case, capture: &Capture) {
    let mut fixture = Decisions::create(case).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    unpriced(&fixture, &input).await;
    unconfirmed(&fixture, &input).await;
    funding_events(capture, case);
    let raw = RawValue::from_string(json!({"api_key":"rotated-native-fixture"}).to_string())
        .expect("synthetic rotated key");
    fixture
        .vault
        .replace(
            &case.scope.workspace_id,
            &SecretName::parse("typesafe").expect("fixed provider bag"),
            &SecretBody::parse(&raw).expect("valid key object"),
        )
        .await
        .expect("admin rotates before first issuance");
    let (issued, body) = fixture.reserve(&input).await;
    assert!(issued);
    assert_eq!(
        body.pointer("/dispatch/api_key"),
        Some(&json!("rotated-native-fixture"))
    );
    disabled_after_issue(&mut fixture, &input, &body).await;
    fixture.model.finish().await;
}

async fn unpriced(fixture: &Decisions<'_>, input: &garde::Valid<DecisionReservationInput>) {
    let case = fixture.case;
    let removed = sqlx::query("DELETE FROM model.platform_model_prices WHERE platform_model_id IN (SELECT id FROM model.platform_models WHERE provider=$1 AND model_name=$2)")
        .bind(PROVIDER).bind(MODEL)
        .execute(&mut *case.held.fixtures.database.acquire().await.expect("owned rate injection"))
        .await.expect("remove this isolated fixture's coding price");
    assert_eq!(removed.rows_affected(), 1);
    let failure = fixture
        .tasks
        .reserve_decision(case.lease(), input)
        .await
        .expect_err("decision issuance requires known whole-work credit headroom");
    assert_eq!(failure.code(), error_code::TASK_DECISION_UNAVAILABLE);
    assert_eq!(fixture.counts().await, (1, 1, 0));
    case.held.fixtures.seed_model_rate(PROVIDER, MODEL).await;
}

fn funding_events(capture: &Capture, case: &Case) {
    let events = capture.events();
    let reasons = events
        .iter()
        .filter(|event| event.field("event") == Some("dt_decision_funding_refused"))
        .map(|event| (event.field("reason"), event.field("lease_id")))
        .collect::<Vec<_>>();
    assert_eq!(
        reasons,
        [
            (
                Some("coding_price_unavailable"),
                Some(case.lease().lease_id.as_str())
            ),
            (
                Some("credit_unconfirmed"),
                Some(case.lease().lease_id.as_str())
            ),
        ]
    );
}

async fn unconfirmed(fixture: &Decisions<'_>, input: &garde::Valid<DecisionReservationInput>) {
    let case = fixture.case;
    let mut dependencies = fixture.dependencies.clone();
    dependencies.biller = Arc::new(WithOutagePolicy::new(
        LedgerBiller::on_system_clock(unreachable_db()),
        OutagePolicy::Open,
    ));
    let unconfirmed = Tasks::new(
        case.held.leases.clone(),
        case.admissions.clone(),
        dependencies,
    );
    let failure = unconfirmed
        .reserve_decision(case.lease(), input)
        .await
        .expect_err("a physical decision needs confirmed funding");
    assert_eq!(failure.code(), error_code::TASK_DECISION_UNAVAILABLE);
    assert_eq!(fixture.counts().await, (1, 1, 0));
}

async fn disabled_after_issue(
    fixture: &mut Decisions<'_>,
    input: &garde::Valid<DecisionReservationInput>,
    body: &serde_json::Value,
) {
    fixture.model.disable().await;
    let call = body
        .get("call_id")
        .and_then(serde_json::Value::as_str)
        .expect("one original call");
    let result = fixture.record(call, known(&request(body))).await;
    assert_eq!(
        (
            result.state,
            result.disposition,
            result.charged_nanos.as_deref()
        ),
        (
            DecisionCallState::Known,
            DecisionRecordDisposition::Stale,
            Some("3")
        )
    );
    assert_eq!(fixture.counts().await, (1, 1, 1));
    let (issued, saved) = fixture.reserve(input).await;
    assert!(!issued);
    assert_eq!(
        saved.pointer("/recorded/disposition"),
        Some(&json!("stale"))
    );
    assert!(!saved.to_string().contains("rotated-native-fixture"));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn blocked_and_cancelled_decisions_release_connections_and_preserve_native_ownership() {
    let capture = Capture::install();
    exercise(async |case| blocked(case, &capture).await).await;
}

async fn blocked(case: &Case, capture: &Capture) {
    let fixture = Decisions::create(case).await;
    let (database, tasks) = single_connection(&fixture).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("source blocker");
    let mut blocker = connection
        .begin()
        .await
        .expect("source blocker transaction");
    sqlx::query("SELECT id FROM fleet.admissions WHERE id=$1::uuid FOR UPDATE")
        .bind(&case.held.event_id)
        .execute(&mut *blocker)
        .await
        .expect("hold source after slot can be acquired");
    statement_timeout(&tasks, case, &input, capture).await;
    cancellation(&tasks, case, &input, capture, &database).await;
    blocker.rollback().await.expect("release source blocker");
    assert_eq!(fixture.counts().await, (0, 0, 0));
    let result = timeout(
        Duration::from_secs(5),
        tasks.reserve_decision(case.lease(), &input),
    )
    .await
    .expect("original fleet slot is usable after cancellation")
    .expect("same accepted source recovers");
    assert!(result.into_parts().0);
    assert_eq!(fixture.counts().await, (1, 1, 0));
    database.close().await;
    fixture.model.finish().await;
}

async fn single_connection(fixture: &Decisions<'_>) -> (Db, Tasks) {
    let case = fixture.case;
    let database = case
        .held
        .fixtures
        .open_with(
            DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", "1"),
                ("DATABASE_MIN_POOL_SIZE_API", "0"),
            ],
        )
        .await;
    let tasks = Tasks::new(
        Leases::new(
            database.clone(),
            case.held.fixtures.queue().clone(),
            afd_crypto::entropy::Entropy::new(),
        ),
        case.admissions.clone(),
        fixture.dependencies.clone(),
    );
    (database, tasks)
}

async fn statement_timeout(
    tasks: &Tasks,
    case: &Case,
    input: &garde::Valid<DecisionReservationInput>,
    capture: &Capture,
) {
    let failure = timeout(
        Duration::from_secs(6),
        tasks.reserve_decision(case.lease(), input),
    )
    .await
    .expect("server deadline bounds source lock wait")
    .expect_err("the source is blocked");
    assert_eq!(failure.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    assert!(failure.is_datastore_unavailable());
    let event = capture.only("dt_operation_failed");
    assert_eq!(
        (
            event.field("operation"),
            event.field("outcome"),
            event.field("reason")
        ),
        (
            Some("decision_reserve"),
            Some("timeout"),
            Some("statement_deadline")
        )
    );
}

async fn cancellation(
    tasks: &Tasks,
    case: &Case,
    input: &garde::Valid<DecisionReservationInput>,
    capture: &Capture,
    database: &Db,
) {
    assert!(
        timeout(
            Duration::from_millis(200),
            tasks.reserve_decision(case.lease(), input)
        )
        .await
        .is_err(),
        "the caller cancels while the source remains blocked"
    );
    let cancelled = capture.only("dt_operation_cancelled");
    assert_eq!(
        (cancelled.field("operation"), cancelled.field("reason")),
        (Some("decision_reserve"), Some("operation_dropped"))
    );
    let mut fresh = timeout(Duration::from_secs(2), database.acquire())
        .await
        .expect("cancelled call releases the only permit")
        .expect("fresh usable connection");
    assert_eq!(
        sqlx::query_scalar::<_, i32>("SELECT 1")
            .fetch_one(&mut *fresh)
            .await
            .expect("socket protocol is healthy"),
        1
    );
    drop(fresh);
}
