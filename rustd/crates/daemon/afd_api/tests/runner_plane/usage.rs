//! What the renew and report verbs say about the usage lines a body carries.

use afd_auth::directory::Liveness;
use http::{Method, StatusCode};

use super::{LEASE_ID, RUNNER_TOKEN};
use crate::harness::{Fleet, json_body, problem, runner_id, send};

/// An empty body, one that will not parse, and one that breaks a bound all
/// still renew: none of them records a line, so none names one. A body that
/// reads names the highest line it carried.
#[tokio::test]
async fn a_renewal_that_records_nothing_still_extends() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();
    let renew_path = format!("/v1/runners/me/leases/{LEASE_ID}/renew");
    let daemon_range = format!(r#"{{"requests":[{}]}}"#, usage_line(1 << 31));
    for body in ["", "not-json", daemon_range.as_str()] {
        let renewed = send(&router, Method::POST, &renew_path, Some(RUNNER_TOKEN), body).await;
        assert_eq!(renewed.status(), StatusCode::OK, "{body}");
        let body = json_body(renewed).await;
        assert_eq!(
            body.get("lease_expires_at"),
            Some(&serde_json::json!("2025-10-09T08:53:20Z"))
        );
        assert_eq!(body.get("recorded_through"), None);
    }
    let lines = format!(r#"{{"requests":[{},{}]}}"#, usage_line(0), usage_line(4));
    let renewed = send(
        &router,
        Method::POST,
        &renew_path,
        Some(RUNNER_TOKEN),
        &lines,
    )
    .await;
    assert_eq!(renewed.status(), StatusCode::OK);
    assert_eq!(
        json_body(renewed).await.get("recorded_through"),
        Some(&serde_json::json!(4))
    );
}

/// One provider request's line, numbered `seq`.
fn usage_line(seq: u64) -> String {
    serde_json::json!({
        "seq": seq, "at": "2025-10-09T08:00:00Z", "input_tokens": 10,
        "cached_input_tokens": 0, "cache_write_tokens": 0,
        "cache_write_5m_tokens": 0, "cache_write_1h_tokens": 0, "output_tokens": 5,
    })
    .to_string()
}

/// The cumulative totals runners sent before usage moved to request lines.
const CUMULATIVE: &str = r#""input_tokens":1,"cached_input_tokens":2,"output_tokens":3"#;

/// A renewal carrying the retired cumulative totals is refused, field named,
/// rather than renewed with its usage silently dropped; the lease is not
/// extended, so the stub plane, which would have answered 200, is never asked.
#[tokio::test]
async fn a_renewal_with_cumulative_totals_is_refused_naming_the_field() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();
    let path = format!("/v1/runners/me/leases/{LEASE_ID}/renew");

    let refused = send(
        &router,
        Method::POST,
        &path,
        Some(RUNNER_TOKEN),
        &format!("{{{CUMULATIVE}}}"),
    )
    .await;

    let (status, code, detail) = problem(refused).await;
    assert_eq!(
        (status, code.as_str()),
        (StatusCode::BAD_REQUEST, "UZ-RUN-013")
    );
    assert_eq!(
        detail,
        "input_tokens is a retired cumulative total; send one line per provider request in requests"
    );
}

/// A report says which field it could not take: a retired cumulative total, or
/// the bound its request lines broke.
#[tokio::test]
async fn a_refused_report_names_the_field() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();
    let report = |usage: &str| {
        format!(
            r#"{{"lease_id":"{LEASE_ID}","event_id":"e","fencing_token":1,"outcome":"processed","failure_reason":null,"failure_detail":"","response_text":"","tokens":6,{usage},"telemetry":{{"time_to_first_token_ms":0,"wall_ms":1}},"checkpoint":{{"last_event_id":"e","last_response":""}}}}"#
        )
    };
    let over: Vec<String> = (0..=512).map(usage_line).collect();
    for (usage, expected) in [
        (
            CUMULATIVE.to_owned(),
            "input_tokens is a retired cumulative total; send one line per provider request in requests",
        ),
        (
            format!(r#""requests":[{}]"#, over.join(",")),
            "requests must carry at most 512 lines",
        ),
        (
            format!(r#""requests":[{}]"#, usage_line(1 << 31)),
            "requests[].seq must be at most 2147483647",
        ),
        (r#""requests":"x""#.to_owned(), "Malformed report body"),
    ] {
        let refused = send(
            &router,
            Method::POST,
            "/v1/runners/me/reports",
            Some(RUNNER_TOKEN),
            &report(&usage),
        )
        .await;
        let (status, code, detail) = problem(refused).await;
        assert_eq!(
            (status, code.as_str(), detail.as_str()),
            (StatusCode::BAD_REQUEST, "UZ-REQ-001", expected)
        );
    }

    let settled = send(
        &router,
        Method::POST,
        "/v1/runners/me/reports",
        Some(RUNNER_TOKEN),
        &report(&format!(r#""requests":[{}]"#, usage_line(0))),
    )
    .await;
    assert_eq!(
        settled.status(),
        StatusCode::OK,
        "the fixture is a report the daemon takes"
    );
}

/// A failed run settles like a clean one and is recorded as the failure it
/// was, whether or not the runner classified it: an unclassified fleet error
/// is still a fleet error, never counted as a processed run.
#[tokio::test]
async fn a_failed_report_settles_and_is_recorded_as_a_fleet_error() {
    let (fleet, recorded) = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .with_recorded_analytics();
    let router = fleet.router();
    for reason in [r#""timeout_kill""#, "null"] {
        let body = format!(
            r#"{{"lease_id":"{LEASE_ID}","event_id":"e","fencing_token":1,"outcome":"fleet_error","failure_reason":{reason},"failure_detail":"","response_text":"","tokens":6,"requests":[{}],"telemetry":{{"time_to_first_token_ms":0,"wall_ms":1}},"checkpoint":{{"last_event_id":"e","last_response":""}}}}"#,
            usage_line(0)
        );
        let settled = send(
            &router,
            Method::POST,
            "/v1/runners/me/reports",
            Some(RUNNER_TOKEN),
            &body,
        )
        .await;
        assert_eq!(settled.status(), StatusCode::OK, "{reason}");
        assert_eq!(json_body(settled).await, serde_json::json!({"ok": true}));
    }

    let exits: Vec<String> = recorded
        .events()
        .into_iter()
        .filter_map(|event| match event {
            afd_observability::Telemetry::FleetCompleted { exit_status, .. } => Some(exit_status),
            _ => None,
        })
        .collect();
    assert_eq!(exits, ["fleet_error", "fleet_error"]);
}
