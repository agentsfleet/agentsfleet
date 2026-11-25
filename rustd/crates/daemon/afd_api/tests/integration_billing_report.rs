//! Served tenant reports over the daily rollups and actual authorization layers.
//! Every line reaches the report the way production's does: the insert that
//! logs it charges it and adds it to the rollups in the same statement.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "live-schema preconditions fail loudly")]

use crate::harness::{self, text};
use crate::integration_workspace_members::fixture::{Members, Person, owner_scopes};
use af_core::id::Uuid7;
use afd_billing::sql::{posture, work};
use afd_db::test_util::mint_id;
use http::{Method, StatusCode, header};
use jiff::Timestamp;
use serde_json::{Value, json};

const REPORT: &str = "/v1/tenants/me/billing/reports/current";
const MODEL: &str = "billing-report-model";
const NOW: &str = "2026-10-25T12:01:00Z";

/// Run lines as the run's own transaction logs them: the run as their work
/// and run, the message it answered as their turn, and the fleet and model
/// they are attributed to. A line's references are opaque, so none needs a
/// row. A model line carries no run time, as production's never does.
const INSERT_LINES: &str = "\
INSERT INTO billing.usage_lines (
    id, work_type, work_id, seq, kind, tenant_id, workspace_id, fleet_id, fleet_name, turn_id,
    run_id, provider, model, posture, requested_at, input_token_count, cached_input_token_count,
    cache_write_token_count, cache_write_5m_token_count, cache_write_1h_token_count,
    output_token_count, runtime_ms, unpriced_token_count, charged_nanos,
    created_at
)
SELECT id, $5, id, 0, 'model', $2, $3, $9, $10, message, id, $4, $11, $6,
       '2026-10-25T12:00:00Z', 23, 0, 0, 0, 0, 7, 0, 0, $7,
       '2026-10-25T12:00:00Z'
  FROM unnest($1, $8) AS source(id, message)
";

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn report_returns_exact_full_totals_without_private_content() {
    let members = Members::create().await;
    seed(&members, &members.john, 4, 17).await;
    seed(&members, &members.stranger, 3, 999).await;
    let response = get(&members, &members.john, REPORT).await;
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
    assert_eq!(text(totals, "charged_nanos"), Some("68"));
    assert_eq!(totals.get("line_count"), Some(&json!(4)));
    // No line here is a runtime slice, so no run is counted.
    assert_eq!(totals.get("activity_count"), Some(&json!(0)));
    assert!(report.get("activities").is_none(), "{report}");
    let unattributed = report
        .pointer("/spending/by_fleet/0")
        .expect("the group of lines no fleet is named on");
    assert_eq!(unattributed.get("key"), Some(&Value::Null));
    assert_eq!(unattributed.get("models"), Some(&json!([MODEL])));
    assert_eq!(
        unattributed.pointer("/totals/activity_count"),
        Some(&json!(0))
    );
    let model = report
        .pointer("/spending/by_model/0")
        .expect("the model group");
    assert_eq!(model.get("key"), Some(&json!(MODEL)));
    assert_eq!(model.pointer("/totals/charged_nanos"), Some(&json!("68")));
    // One run spans several models, so a model group counts none.
    assert!(model.pointer("/totals/activity_count").is_none(), "{model}");
    let body = report.to_string();
    assert!(
        !body.contains("draft_id") && !body.contains("messages"),
        "{body}"
    );
    assert_ledger(&members, &members.john, (4, 68)).await;
    members.cleanup().await;
}

// Value: an empty account returns exact zero totals.
#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn an_empty_report_has_exact_zero_totals() {
    let members = Members::create().await;
    let response = get(&members, &members.john, REPORT).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = harness::json_body(response).await;
    assert_eq!(body.pointer("/totals/charged_nanos"), Some(&json!("0")));
    assert_eq!(body.pointer("/totals/activity_count"), Some(&json!(0)));
    assert_eq!(body.pointer("/spending/by_fleet"), Some(&json!([])));
    assert_eq!(
        body.pointer("/filters/models"),
        None,
        "the report offers no model filter"
    );
    assert_ledger(&members, &members.john, (0, 0)).await;
    members.cleanup().await;
}

// Value: equal spending has stable keys and choices across more than one fleet/model.
#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn equal_spending_uses_stable_fleet_and_model_ordering() {
    let members = Members::create().await;
    seed(&members, &members.john, 2, 17).await;
    // The second attribution bucket is written as such: a line joins its
    // rollup row as it is logged, so its attribution is fixed then.
    seed_as(
        &members,
        &members.john,
        Lines {
            count: 2,
            money: 17,
            fleet: Some((&members.fleet, "Fleet")),
            model: "second-report-model",
        },
    )
    .await;
    let response = get(&members, &members.john, REPORT).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = harness::json_body(response).await;
    for dimension in ["by_fleet", "by_model"] {
        let buckets = body
            .pointer(&format!("/spending/{dimension}"))
            .and_then(Value::as_array)
            .expect("spending buckets");
        assert_eq!(buckets.len(), 2);
        for bucket in buckets {
            assert_eq!(bucket.pointer("/totals/charged_nanos"), Some(&json!("34")));
        }
    }
    assert_eq!(body.pointer("/spending/by_fleet/0/key"), Some(&Value::Null));
    assert_eq!(
        body.pointer("/spending/by_fleet/1/key"),
        Some(&json!(members.fleet.as_str()))
    );
    assert_eq!(
        body.pointer("/spending/by_fleet/1/label"),
        Some(&json!("Fleet"))
    );
    assert_eq!(
        body.pointer("/spending/by_model/0/key"),
        Some(&json!(MODEL))
    );
    assert_eq!(
        body.pointer("/spending/by_model/1/key"),
        Some(&json!("second-report-model"))
    );
    assert_eq!(body.pointer("/filters/models"), None);
    assert_eq!(body.pointer("/filters/fleets/0/id"), Some(&Value::Null));
    assert_ledger(&members, &members.john, (4, 68)).await;
    members.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn served_daily_report_includes_zero_days_and_complete_models() {
    let members = Members::create().await;
    seed(&members, &members.john, 3, 17).await;
    let path = format!("{REPORT}?period=week");
    let body = harness::json_body(get(&members, &members.john, &path).await).await;
    assert_week_projection(&body);
    // Every seeded line is a fleet run's, so the runtime filter keeps them all.
    let runtime = harness::json_body(
        get(
            &members,
            &members.john,
            &format!("{path}&activity_type=runtime"),
        )
        .await,
    )
    .await;
    assert_week_projection(&runtime);
    // None of them names John's fleet, so a filter on it selects nothing.
    let other = harness::json_body(
        get(
            &members,
            &members.john,
            &format!("{path}&fleet_id={}", members.fleet),
        )
        .await,
    )
    .await;
    assert_eq!(
        other.pointer("/daily/6/totals/charged_nanos"),
        Some(&json!("0"))
    );
    assert_eq!(other.pointer("/spending/by_fleet"), Some(&json!([])));
    assert!(
        other
            .pointer("/filters/fleets")
            .and_then(Value::as_array)
            .is_some_and(|fleets| !fleets.is_empty()),
        "a filter keeps the unfiltered choices"
    );
    assert_ledger(&members, &members.john, (3, 51)).await;
    members.cleanup().await;
}

fn assert_week_projection(body: &Value) {
    let daily = body
        .get("daily")
        .and_then(Value::as_array)
        .expect("daily totals");
    assert_eq!(daily.len(), 7);
    assert_eq!(
        daily.first().expect("first date").get("day"),
        Some(&json!("2026-10-19"))
    );
    assert_eq!(
        daily.last().expect("last date").get("day"),
        Some(&json!("2026-10-25"))
    );
    for day in daily.iter().take(6) {
        assert_eq!(day.pointer("/totals/charged_nanos"), Some(&json!("0")));
        assert!(day.pointer("/totals/activity_count").is_none(), "{day}");
    }
    let last = daily.last().expect("last date");
    assert_eq!(last.pointer("/totals/charged_nanos"), Some(&json!("51")));
    assert_eq!(last.pointer("/totals/line_count"), Some(&json!(3)));
    assert_eq!(body.pointer("/totals/charged_nanos"), Some(&json!("51")));
    assert_eq!(
        body.pointer("/spending/by_fleet/0/models"),
        Some(&json!([MODEL]))
    );
}

async fn get(members: &Members, person: &Person, path: &str) -> axum::response::Response {
    let router = members.fleet(person, owner_scopes()).at(now()).router();
    harness::send(&router, Method::GET, path, Some(&person.token), "").await
}

fn now() -> Timestamp {
    NOW.parse().expect("a fixed instant")
}

/// What [`seed_as`] logs: `count` one-line runs, each charged `money`,
/// attributed to `fleet` (its id and name) and `model`.
#[derive(Clone, Copy)]
struct Lines<'a> {
    count: usize,
    money: i64,
    fleet: Option<(&'a Uuid7, &'a str)>,
    model: &'a str,
}

/// Logs `count` one-line runs for `person`, each answering a
/// message of its own and naming no fleet; logging charges them.
async fn seed(members: &Members, person: &Person, count: usize, money: i64) {
    let lines = Lines {
        count,
        money,
        fleet: None,
        model: MODEL,
    };
    seed_as(members, person, lines).await;
}

/// Logs `lines` for `person`, as [`seed`] does with their attribution.
async fn seed_as(members: &Members, person: &Person, lines: Lines<'_>) {
    let ids: Vec<_> = (0..lines.count).map(|_| mint_id()).collect();
    let messages: Vec<_> = (0..lines.count).map(|_| mint_id()).collect();
    sqlx::query(INSERT_LINES)
        .bind(&ids)
        .bind(&person.tenant)
        .bind(&person.workspace)
        .bind(MODEL)
        .bind(work::FLEET_RUN)
        .bind(posture::PLATFORM)
        .bind(lines.money)
        .bind(&messages)
        .bind(lines.fleet.map(|(fleet, _)| fleet))
        .bind(lines.fleet.map(|(_, name)| name))
        .bind(lines.model)
        .execute(&mut *members.database.acquire().await.expect("a connection"))
        .await
        .expect("report lines seed");
}

/// The person's logged lines and their money.
async fn assert_ledger(members: &Members, person: &Person, expected: (i64, i64)) {
    let stored: (i64, i64) = sqlx::query_as("SELECT COUNT(*)::bigint, COALESCE(SUM(charged_nanos),0)::bigint FROM billing.usage_lines WHERE tenant_id=$1")
        .bind(&person.tenant).fetch_one(&mut *members.database.acquire().await.expect("a connection"))
        .await.expect("ledger remains readable");
    assert_eq!(stored, expected);
}
