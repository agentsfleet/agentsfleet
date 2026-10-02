//! A run's tool trace, from the runner's report to the operator's read.
//!
//! The report carries the trace as raw JSON. The daemon narrows it after the
//! report's own fields, fences each call id, and writes it in the statement
//! that settles the event. These suites speak as a runner over a real socket,
//! then read the row back and, for the tenant reads, ask the daemon for it.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_wire::tool_trace::TRACE_MAX_CALLS;
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Row as _};

use crate::e2e::{Scenario, scenario, scenario_with_provider};
use crate::e2e_seed_keys::seed_tenant_key;
use crate::integration_tenant_registry::{mint_tenant_token, provider_listener};
use crate::reads::event_column;
use crate::tail::lease;
use crate::wire::{field, json as body_of, post, report_body};

/// The report verb every case here settles through.
const REPORTS: &str = "/v1/runners/me/reports";

/// A trace of `count` calls, numbered from 1 as the runner numbers them.
fn trace(count: usize) -> Value {
    let calls: Vec<Value> = (1..=count)
        .map(|n| {
            json!({"call_id": n.to_string(), "name": "file_read",
                   "arguments": {"path": "README.md"}, "status": "succeeded",
                   "output_head": "# agentsfleet", "output_tail": "MIT",
                   "output_line_count": 214, "duration_ms": 12})
        })
        .collect();
    json!({"calls": calls, "omitted_call_count": 0})
}

/// The report a run sends, carrying `tool_calls`.
fn report_with(lease_id: &str, run: &Scenario, fence: u64, tool_calls: Value) -> Value {
    let mut body = report_body(lease_id, &run.event_id, fence);
    body["tool_calls"] = tool_calls;
    body
}

/// One nullable text column of the scenario's event row.
///
/// `event_column` reads a column that holds a value; the columns these cases
/// read are NULL exactly when the case passes.
async fn nullable_column(run: &Scenario, column: &str) -> Option<String> {
    let statement = AssertSqlSafe(format!(
        "SELECT {column}::text FROM core.fleet_events \
         WHERE fleet_id = $1::uuid AND event_id = $2"
    ));
    let mut connection = run.booted.database.acquire().await.expect("a connection");
    let row = sqlx::query(statement)
        .bind(&run.fleet)
        .bind(&run.event_id)
        .fetch_one(&mut *connection)
        .await
        .expect("the event row exists once leased");
    row.try_get(0).expect("the column reads as nullable text")
}

/// The stored trace of the scenario's event, parsed, or `None`.
async fn stored_trace(run: &Scenario) -> Option<Value> {
    nullable_column(run, "tool_calls")
        .await
        .map(|text| serde_json::from_str(&text).expect("the column holds JSON"))
}

/// `sent` with each call id fenced the way the daemon stores it.
fn fenced(sent: &Value, fence: u64) -> Value {
    let mut expected = sent.clone();
    for call in expected["calls"].as_array_mut().expect("calls") {
        let id = call["call_id"].as_str().expect("a call id").to_owned();
        call["call_id"] = json!(format!("{fence}:{id}"));
    }
    expected
}

/// Dimension 3.3. A settled report stores the trace beside its answer, with
/// every call id fenced as the live frames carry it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_report_writes_tool_calls_with_result() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (lease_id, fence) = lease(&http, &run).await;

    let sent = trace(2);
    let settled = post(
        &http,
        &run,
        REPORTS,
        &report_with(&lease_id, &run, fence, sent.clone()),
    )
    .await;
    assert_eq!(settled.status().as_u16(), 200, "the report settles");

    assert_eq!(stored_trace(&run).await, Some(fenced(&sent, fence)));
    assert_eq!(
        event_column(&run, &run.event_id, "response_text")
            .await
            .as_deref(),
        Some("the fixture run produced this"),
        "the trace rides the statement that writes the answer"
    );

    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Dimension 3.2. A trace past a bound is dropped; the run still settles.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_oversize_tool_trace_dropped_report_settles() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (lease_id, fence) = lease(&http, &run).await;

    let over = trace(TRACE_MAX_CALLS + 1);
    let settled = post(
        &http,
        &run,
        REPORTS,
        &report_with(&lease_id, &run, fence, over),
    )
    .await;
    assert_eq!(
        settled.status().as_u16(),
        200,
        "a bad trace never refuses the report"
    );
    assert_eq!(stored_trace(&run).await, None, "the trace is not stored");
    assert_eq!(
        event_column(&run, &run.event_id, "status").await.as_deref(),
        Some("processed"),
        "and the run's answer is"
    );

    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Dimension 3.4. A report the fence refuses writes neither answer nor trace.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_fenced_report_writes_no_tool_calls() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (lease_id, fence) = lease(&http, &run).await;

    // A reclaim bumps the fleet's sequence past the lease's token; this is
    // that bump without the reclaim, so the report is the stale holder's.
    let mut connection = run.booted.database.acquire().await.expect("a connection");
    sqlx::query(AssertSqlSafe(
        "UPDATE fleet.runner_affinity SET fencing_seq = fencing_seq + 1 \
         WHERE fleet_id = $1::uuid"
            .to_owned(),
    ))
    .bind(&run.fleet)
    .execute(&mut *connection)
    .await
    .expect("the sequence moves past the lease");
    drop(connection);

    let refused = post(
        &http,
        &run,
        REPORTS,
        &report_with(&lease_id, &run, fence, trace(2)),
    )
    .await;
    assert_eq!(
        refused.status().as_u16(),
        409,
        "the stale holder is refused"
    );
    assert_eq!(stored_trace(&run).await, None, "no trace was written");
    assert_eq!(
        nullable_column(&run, "response_text").await,
        None,
        "and no answer"
    );

    supervisor.shutdown().await;
    run.cleanup().await;
}

/// A tenant credential for the scenario's workspace, read with `fleet:read`.
async fn tenant_reader(supervisor: &mut Supervisor) -> (Scenario, String) {
    let provider_base = provider_listener().await;
    let run = scenario_with_provider(supervisor, Some(&provider_base)).await;
    let token = mint_tenant_token();
    seed_tenant_key(&run.booted, &run.tenant, &token, run.seeded_at).await;
    (run, token)
}

/// One authenticated tenant read, answered as its JSON body.
async fn tenant_get(http: &reqwest::Client, run: &Scenario, token: &str, path: &str) -> Value {
    let response = http
        .get(format!("{}{path}", run.base))
        .bearer_auth(token)
        .send()
        .await
        .expect("the daemon answers");
    assert_eq!(response.status().as_u16(), 200, "GET {path}");
    body_of(response).await
}

/// Settles a lease the caller holds with `tool_calls`.
async fn settle_with(
    http: &reqwest::Client,
    run: &Scenario,
    (lease_id, fence): (String, u64),
    tool_calls: Value,
) {
    let body = report_with(&lease_id, run, fence, tool_calls);
    assert_eq!(post(http, run, REPORTS, &body).await.status().as_u16(), 200);
}

/// Dimension 4.1. The single-event read serves the stored trace, and `null`
/// for a row that recorded none.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_event_detail_serves_tool_calls() {
    let mut supervisor = Supervisor::new();
    let (run, token) = tenant_reader(&mut supervisor).await;
    let http = reqwest::Client::new();
    let detail = format!(
        "/v1/workspaces/{}/fleets/{}/events/{}",
        run.workspace, run.fleet, run.event_id
    );

    // The row is written when a runner takes the event, before any report.
    let claimed = lease(&http, &run).await;
    let fence = claimed.1;
    let before = tenant_get(&http, &run, &token, &detail).await;
    assert_eq!(
        field(&before, "tool_calls"),
        &Value::Null,
        "a run still in flight has recorded nothing"
    );

    let sent = trace(3);
    settle_with(&http, &run, claimed, sent.clone()).await;
    let after = tenant_get(&http, &run, &token, &detail).await;
    assert_eq!(field(&after, "tool_calls"), &fenced(&sent, fence));

    let thread = format!(
        "/v1/workspaces/{}/fleets/{}/messages",
        run.workspace, run.fleet
    );
    let page = tenant_get(&http, &run, &token, &thread).await;
    let turn = page["items"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["event_id"] == json!(run.event_id))
        })
        .expect("the settled turn is on the thread");
    assert_eq!(field(turn, "tool_calls"), &fenced(&sent, fence));

    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Dimension 4.3. The events list never carries the trace.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_event_list_omits_tool_calls() {
    let mut supervisor = Supervisor::new();
    let (run, token) = tenant_reader(&mut supervisor).await;
    let http = reqwest::Client::new();
    let claimed = lease(&http, &run).await;
    settle_with(&http, &run, claimed, trace(2)).await;

    let list = format!(
        "/v1/workspaces/{}/fleets/{}/events",
        run.workspace, run.fleet
    );
    let page = tenant_get(&http, &run, &token, &list).await;
    let items = page["items"].as_array().expect("a page of items");
    let ours = items
        .iter()
        .find(|item| item["event_id"] == json!(run.event_id))
        .expect("the settled event is listed");
    assert!(ours.get("tool_calls").is_none(), "{ours}");

    supervisor.shutdown().await;
    run.cleanup().await;
}
