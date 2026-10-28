//! History-only facts never create runnable or billable work or model turns.

use super::fixture::*;
use afd_core::id::Uuid7;
use afd_events::{Cursor, History};
use http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn recorded_receipts_never_admit_run_bill_deliver_or_enter_runtime_context() {
    let saved = Saved::create(json!({"credentials":[SERVICE]})).await;
    assert_eq!(
        saved.access(Method::POST, "fixture-access").await.status(),
        StatusCode::CREATED
    );
    assert_eq!(saved.activate().await.status(), StatusCode::OK);
    assert_eq!(saved.memory(true, true).await.status(), StatusCode::OK);
    assert_receipts(
        &saved.receipts().await,
        &[
            saved_receipt(),
            access_receipt(),
            ready_receipt(),
            memory_receipt(true, true),
        ],
    );
    assert_no_work(&saved).await;
    assert_runtime_context(&saved).await;
    let current = saved.get("").await;
    assert_eq!(current["budget_used_nanos"], "0");
    saved.case.cleanup().await;
}

async fn assert_no_work(saved: &Saved) {
    let counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
         (SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1::uuid),
         (SELECT count(*) FROM runner.leases WHERE fleet_id=$1::uuid),
         (SELECT count(*) FROM runner.fleet_slots WHERE fleet_id=$1::uuid),
         (SELECT count(*) FROM billing.usage_lines WHERE fleet_id=$1::uuid),
         (SELECT count(*) FROM billing.holds WHERE tenant_id=$2::uuid),
         (SELECT count(*) FROM fleet.obligations WHERE fleet_id=$1::uuid)",
    )
    .bind(saved.fleet.as_str())
    .bind(saved.case.base.tenant.as_str())
    .fetch_one(&mut *saved.case.base.db.acquire().await.expect("work connection"))
    .await
    .expect("runtime side-effect counts");
    assert_eq!(
        counts,
        (0, 0, 0, 0, 0, 0),
        "receipts create no queue, lease, run, charge, hold or delivery"
    );
}

async fn assert_runtime_context(saved: &Saved) {
    let history = History::new(saved.case.base.db.clone());
    let cursor = Cursor::after(jiff::Timestamp::constant(4_000_000_000, 0), "after-fixture");
    let read = || history.finished_before(&saved.case.base.workspace, &saved.fleet, &cursor, 100);
    assert_eq!(read().await.expect("context before chat"), []);
    sqlx::query("INSERT INTO fleet.events
        (fleet_id,workspace_id,event_id,actor,event_type,status,request_json,response_text,created_at,updated_at)
        VALUES($1::uuid,$2::uuid,'runtime-positive-control','steer:api','chat','processed',
               '{\"message\":\"Explain the failure\"}'::jsonb,'Runtime reply',now(),now())")
        .bind(saved.fleet.as_str()).bind(saved.case.base.workspace.as_str())
        .execute(&mut *saved.case.base.db.acquire().await.expect("positive-control connection"))
        .await.expect("completed runtime turn");
    let turns = read().await.expect("production lease history query");
    assert_eq!(
        turns.len(),
        1,
        "the positive runtime turn remains while all receipts are excluded"
    );
    assert_eq!(turns[0].row.event_id, "runtime-positive-control");
    assert_eq!(turns[0].response_text.as_deref(), Some("Runtime reply"));
    assert_eq!(turns[0].row.event_type, "chat");
    let foreign = Uuid7::parse(super::super::super::mint().as_str()).expect("foreign workspace");
    assert_eq!(
        history
            .finished_before(&foreign, &saved.fleet, &cursor, 100)
            .await
            .expect("foreign context"),
        []
    );
}
