//! Real routing, guards and extractors for settled usage reports.
#![cfg(feature = "test-util")]

use crate::harness::{self, Fleet, assert_reached_the_verb};
use crate::tenant_billing::{BILLING_READ, SUBJECT, TENANT_KEY, TERMINAL, read};
use af_core::error_code;
use http::{Method, StatusCode};

const REPORT: &str = "/v1/tenants/me/billing/reports/current";
/// The removed per-activity charges route.
const ACTIVITY_CHARGES: &str = "/v1/tenants/me/billing/activities/01924f4e-0000-7000-8000-00000000c4a1/charges?activity_type=setup";
/// The sentence a request still sending the removed model filter is told.

// Value: a person with no resolved account cannot reach the billing report.
#[tokio::test]
async fn unresolved_tenant_authority_refuses_the_report() {
    let fleet = Fleet::new().with_person(TENANT_KEY, SUBJECT, BILLING_READ);
    fleet.ownership().revoke();
    let router = fleet.router();
    let response = harness::send(&router, Method::GET, REPORT, Some(TENANT_KEY), "").await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let problem = harness::json_body(response).await;
    assert_eq!(
        harness::error_code(&problem),
        Some(error_code::AUTH_FORBIDDEN.as_str())
    );
    assert_eq!(
        harness::text(&problem, "detail"),
        Some(afd_api::handler::tenant::DETAIL_TENANT_REQUIRED)
    );
}

#[tokio::test]
async fn billing_read_reaches_the_report_verb() {
    let response = read(BILLING_READ, REPORT, Some(TENANT_KEY)).await;
    assert_reached_the_verb(response, REPORT).await;
}

#[tokio::test]
async fn a_terminal_credential_can_read_settled_reports() {
    let router = Fleet::new()
        .with_terminal(TERMINAL, SUBJECT, BILLING_READ)
        .router();
    let response = harness::send(&router, Method::GET, REPORT, Some(TERMINAL), "").await;
    assert_reached_the_verb(response, REPORT).await;
}

#[tokio::test]
async fn malformed_report_filters_refuse_before_any_datastore_read() {
    for query in [
        "period=year",
        "period=months",
        "through_at=unreadable",
        "fleet_id=not-an-identifier",
        "activity_type=other",
        "activity_type=runtimes",
    ] {
        let path = format!("{REPORT}?{query}");
        let response = read(BILLING_READ, &path, Some(TENANT_KEY)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        let problem = harness::json_body(response).await;
        assert_eq!(
            harness::error_code(&problem),
            Some(error_code::INVALID_REQUEST.as_str()),
            "{problem}"
        );
    }
}

// Value: a client still filtering by model is refused rather than handed a
// wider, unfiltered report it would read as the model's spend.
#[tokio::test]
async fn a_model_filter_is_refused_before_any_datastore_read() {
    for query in [
        "model=x",
        "model=report-model-a",
        "model=%00",
        "model=%E2%9C%93",
    ] {
        let path = format!("{REPORT}?{query}");
        let response = read(BILLING_READ, &path, Some(TENANT_KEY)).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{path}");
        let problem = harness::json_body(response).await;
        assert_eq!(
            harness::error_code(&problem),
            Some(error_code::INVALID_REQUEST.as_str()),
            "{problem}"
        );
        assert_eq!(
            harness::text(&problem, "detail"),
            Some(afd_api::handler::tenant::DETAIL_MODEL),
            "{path}"
        );
    }
    // An empty value names no model, so the report is read unfiltered.
    let path = format!("{REPORT}?model=");
    let response = read(BILLING_READ, &path, Some(TENANT_KEY)).await;
    assert_reached_the_verb(response, &path).await;
}

#[tokio::test]
async fn the_activity_charges_route_answers_gone() {
    let response = read(BILLING_READ, ACTIVITY_CHARGES, Some(TENANT_KEY)).await;
    assert_eq!(response.status(), StatusCode::GONE);
    let problem = harness::json_body(response).await;
    assert_eq!(
        harness::error_code(&problem),
        Some(error_code::ENDPOINT_REMOVED.as_str()),
        "{problem}"
    );
    assert_eq!(
        harness::text(&problem, "detail"),
        Some(afd_api::handler::tenant::DETAIL_CHARGES_REMOVED)
    );
}
