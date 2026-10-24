//! Real routing, guards and extractors for settled usage reports.
#![cfg(feature = "test-util")]

use crate::harness::{self, Fleet, assert_reached_the_verb};
use crate::tenant_billing::{BILLING_READ, SUBJECT, TENANT_KEY, TERMINAL, read};
use afd_core::error_code;
use http::{Method, StatusCode};

const REPORT: &str = "/v1/tenants/me/billing/reports/current";
const DETAILS: &str =
    "/v1/tenants/me/billing/activities/01924f4e-0000-7000-8000-00000000c4a1/charges";

#[tokio::test]
async fn billing_read_reaches_each_reporting_verb() {
    for path in [REPORT.to_owned(), format!("{DETAILS}?activity_type=setup")] {
        let response = read(BILLING_READ, &path, Some(TENANT_KEY)).await;
        assert_reached_the_verb(response, &path).await;
    }
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
        "limit=0",
        "limit=101",
        "period=year",
        "through_at=unreadable",
        "model=%00",
        "starting_after=!!",
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

#[tokio::test]
async fn an_activity_requires_its_explicit_work_kind() {
    for query in [
        "",
        "?activity_type=other",
        "?activity_type=setup&limit=lots",
    ] {
        let path = format!("{DETAILS}{query}");
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
