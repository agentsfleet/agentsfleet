//! Served tenant reports over the migrated ledger and actual authorization layers.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "live-schema preconditions fail loudly")]

use crate::harness::{self, items, text};
use crate::integration_workspace_members::fixture::{Members, Person, owner_scopes};
use afd_billing::sql::{posture, work};
use afd_core::error_code;
use afd_db::test_util::mint_id;
use http::{Method, StatusCode, header};
use jiff::Timestamp;

#[path = "integration_billing_report_paging.rs"]
mod paging;

const REPORT: &str = "/v1/tenants/me/billing/reports/current";
const MODEL: &str = "billing-report-model";
const NOW: &str = "2026-10-25T12:01:00Z";

const INSERT_LINES: &str = "\
INSERT INTO billing.usage_lines (
    id, work_type, work_id, seq, tenant_id, workspace_id, authoring_attempt_id,
    provider, model, posture, requested_at, input_token_count, cached_input_token_count,
    cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count,
    output_token_count, runtime_ms, unpriced_token_count, charged_nanos,
    settled_at, created_at, updated_at
)
SELECT id::uuid, $5, id::uuid, 0, $2::uuid, $3::uuid, id::uuid, $4, $4, $6,
       '2026-10-25T12:00:00Z', 23, 0, 0, 0, 0, 7, 31, 0, $7,
       '2026-10-25T12:00:00Z', '2026-10-25T12:00:00Z', '2026-10-25T12:00:00Z'
  FROM unnest($1::text[]) AS source(id)
";

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn report_returns_exact_full_totals_and_a_bounded_private_page() {
    let members = Members::create().await;
    seed(&members, &members.john, 4, 17).await;
    seed(&members, &members.stranger, 3, 999).await;
    let response = get(&members, &members.john, &format!("{REPORT}?limit=2")).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .expect("cache policy"),
        "private, no-store"
    );
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("content type"),
        "application/json"
    );
    let report = harness::json_body(response).await;
    let totals = report.get("totals").expect("report totals");
    let activities = report.get("activities").expect("activity page");
    assert_eq!(text(totals, "charged_nanos"), Some("68"));
    assert_eq!(
        totals
            .get("activity_count")
            .and_then(serde_json::Value::as_u64),
        Some(4)
    );
    assert_eq!(
        activities.get("total").and_then(serde_json::Value::as_u64),
        Some(4)
    );
    assert_eq!(items(activities).len(), 2);
    assert!(text(activities, "next_cursor").is_some());
    for activity in items(activities) {
        assert_eq!(text(activity, "activity_type"), Some("setup"));
        assert!(
            activity
                .get("fleet_name")
                .expect("nullable fleet name")
                .is_null()
        );
        assert!(activity.get("draft_id").is_none());
        assert!(activity.get("messages").is_none());
    }
    assert_ledger(&members, &members.john, 4, 68).await;
    members.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn served_continuations_refuse_different_tenants_filters_and_limits() {
    let members = Members::create().await;
    seed(&members, &members.john, 3, 17).await;
    let first =
        harness::json_body(get(&members, &members.john, &format!("{REPORT}?limit=1")).await).await;
    let activities = first.get("activities").expect("first activity page");
    let cursor = text(activities, "next_cursor").expect("a continuation");
    for (person, extra) in [
        (&members.stranger, "limit=1"),
        (&members.john, "limit=2"),
        (&members.john, "limit=1&model=other"),
    ] {
        let path = format!("{REPORT}?{extra}&starting_after={cursor}");
        let response = get(&members, person, &path).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let problem = harness::json_body(response).await;
        assert_eq!(
            harness::error_code(&problem),
            Some(error_code::INVALID_REQUEST.as_str())
        );
    }
    let path = format!("{REPORT}?limit=1&starting_after={cursor}");
    let next = get(&members, &members.john, &path).await;
    assert_eq!(next.status(), StatusCode::OK);
    let next = harness::json_body(next).await;
    assert_eq!(
        next.get("through_at").expect("continued cutoff"),
        first.get("through_at").expect("first cutoff")
    );
    assert_eq!(
        next.get("totals").expect("continued totals"),
        first.get("totals").expect("first totals")
    );
    let next_activities = next.get("activities").expect("continued activity page");
    let next_activity = items(next_activities).first().expect("continued activity");
    let first_activity = items(activities).first().expect("first activity");
    assert_ne!(
        text(next_activity, "id").expect("continued activity identifier"),
        text(first_activity, "id").expect("first activity identifier")
    );
    assert_ledger(&members, &members.john, 3, 51).await;
    members.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn served_details_hide_foreign_work_and_keep_exact_amounts() {
    let members = Members::create().await;
    let ids = seed(&members, &members.john, 1, 1).await;
    let path = format!(
        "/v1/tenants/me/billing/activities/{}/charges?activity_type=setup",
        ids.first().expect("seeded activity identifier")
    );
    let own = get(&members, &members.john, &path).await;
    assert_eq!(own.status(), StatusCode::OK);
    let own = harness::json_body(own).await;
    assert_eq!(items(&own).len(), 1);
    let charge = items(&own).first().expect("an individual charge");
    assert_eq!(text(charge, "charged_nanos"), Some("1"));
    assert_eq!(text(charge, "model"), Some(MODEL));
    let foreign = get(&members, &members.stranger, &path).await;
    assert_eq!(foreign.status(), StatusCode::OK);
    assert_eq!(items(&harness::json_body(foreign).await).len(), 0);
    let filtered = get(&members, &members.john, &format!("{path}&model=other")).await;
    assert_eq!(filtered.status(), StatusCode::OK);
    assert_eq!(items(&harness::json_body(filtered).await).len(), 0);
    assert_ledger(&members, &members.john, 1, 1).await;
    members.cleanup().await;
}

async fn get(members: &Members, person: &Person, path: &str) -> axum::response::Response {
    let router = members.fleet(person, owner_scopes()).at(now()).router();
    harness::send(&router, Method::GET, path, Some(&person.token), "").await
}

fn now() -> Timestamp {
    NOW.parse().expect("a fixed instant")
}

async fn seed(members: &Members, person: &Person, count: usize, money: i64) -> Vec<String> {
    let ids: Vec<_> = (0..count).map(|_| mint_id()).collect();
    sqlx::query(INSERT_LINES)
        .bind(&ids)
        .bind(&person.tenant)
        .bind(person.workspace.as_str())
        .bind(MODEL)
        .bind(work::AUTHORING_ATTEMPT)
        .bind(posture::PLATFORM)
        .bind(money)
        .execute(&mut *members.database.acquire().await.expect("a connection"))
        .await
        .expect("report lines seed");
    ids
}

async fn assert_ledger(members: &Members, person: &Person, count: i64, money: i64) {
    let stored: (i64, i64) = sqlx::query_as("SELECT COUNT(*)::bigint, COALESCE(SUM(charged_nanos),0)::bigint FROM billing.usage_lines WHERE tenant_id=$1::uuid")
        .bind(&person.tenant).fetch_one(&mut *members.database.acquire().await.expect("a connection"))
        .await.expect("ledger remains readable");
    assert_eq!(stored, (count, money));
}
