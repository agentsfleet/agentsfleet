//! Each tool call's full record: posted by a runner under its lease's fence,
//! kept per call, read by a member one call at a time.
//!
//! The router suites prove the guard and the parse with no datastore
//! (`afd_api/tests/fleet_tool_calls.rs`, `runner_plane/satellites.rs`); these
//! prove the rows, over a real socket into a booted daemon.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_wire::tool_detail::{DETAIL_FIELD_MAX_BYTES, DETAIL_POST_MAX_BYTES};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Row as _};

use crate::e2e::{Scenario, scenario};
use crate::integration_tool_trace::{REPORTS, tenant_reader};
use crate::tail::lease;
use crate::wire::{json as body_of, post, report_body};

/// One record of `call_number` whose output is `output`, with no arguments.
pub(crate) fn record(call_number: u64, output: &str) -> Value {
    json!({"call_number": call_number, "arguments": {}, "truncated_arguments": false,
           "output": output, "output_line_count": output.lines().count(), "truncated": false})
}

/// Posts `calls` under `fence`, answering the status and the body.
pub(crate) async fn post_records(
    http: &reqwest::Client,
    run: &Scenario,
    (lease_id, fence): (&str, u64),
    calls: &[Value],
) -> (u16, Value) {
    let path = format!("/v1/runners/me/leases/{lease_id}/tool-calls");
    let body = json!({"fencing_token": fence, "calls": calls});
    let response = post(http, run, &path, &body).await;
    let status = response.status().as_u16();
    (status, body_of(response).await)
}

/// One statement over the scenario's pool, answering its single integer.
pub(crate) async fn scalar(run: &Scenario, statement: &str, fence: Option<u64>) -> i64 {
    let mut connection = run.booted.database.acquire().await.expect("a connection");
    let query = sqlx::query(AssertSqlSafe(statement.to_owned()))
        .bind(&run.fleet)
        .bind(&run.event_id);
    let query = match fence {
        Some(fence) => query.bind(i64::try_from(fence).expect("a fence fits")),
        None => query,
    };
    query
        .fetch_one(&mut *connection)
        .await
        .expect("the read runs")
        .try_get(0)
        .expect("an integer")
}

/// How many records the scenario's event keeps.
pub(crate) async fn rows(run: &Scenario) -> i64 {
    scalar(
        run,
        "SELECT count(*) FROM core.fleet_tool_call_details \
         WHERE fleet_id = $1::uuid AND event_id = $2",
        None,
    )
    .await
}

/// How many records the scenario's event keeps under `fence`.
async fn rows_at(run: &Scenario, fence: u64) -> i64 {
    scalar(
        run,
        "SELECT count(*) FROM core.fleet_tool_call_details \
         WHERE fleet_id = $1::uuid AND event_id = $2 AND fencing_token = $3",
        Some(fence),
    )
    .await
}

/// Runs one statement bound to the scenario's fleet and event.
pub(crate) async fn execute(run: &Scenario, statement: &str) {
    let mut connection = run.booted.database.acquire().await.expect("a connection");
    sqlx::query(AssertSqlSafe(statement.to_owned()))
        .bind(&run.fleet)
        .bind(&run.event_id)
        .execute(&mut *connection)
        .await
        .expect("the statement runs");
}

/// Boots a scenario and leases its event.
pub(crate) async fn leased(
    supervisor: &mut Supervisor,
) -> (Scenario, reqwest::Client, String, u64) {
    let run = scenario(supervisor).await;
    let http = reqwest::Client::new();
    let (lease_id, fence) = lease(&http, &run).await;
    (run, http, lease_id, fence)
}

/// Dimensions 1.1 and 1.3. A valid post stores one row per record, and the
/// same post again changes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_tool_call_details_stored_under_fence() {
    let mut supervisor = Supervisor::new();
    let (run, http, lease_id, fence) = leased(&mut supervisor).await;
    let calls = [record(1, "one"), record(2, "two")];
    let (status, body) = post_records(&http, &run, (&lease_id, fence), &calls).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({"stored_count": 2, "skipped_count": 0}));
    assert_eq!(rows_at(&run, fence).await, 2);
    supervisor.shutdown().await;
    run.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_tool_call_details_retry_is_idempotent() {
    let mut supervisor = Supervisor::new();
    let (run, http, lease_id, fence) = leased(&mut supervisor).await;
    let calls = [record(1, "one"), record(2, "two")];
    for _attempt in 0..2 {
        let (status, _) = post_records(&http, &run, (&lease_id, fence), &calls).await;
        assert_eq!(status, 200);
    }
    assert_eq!(rows(&run).await, 2, "a retried post upserts");
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Dimension 1.2. A holder the fleet has moved past keeps nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_stale_fence_tool_call_details_refused() {
    let mut supervisor = Supervisor::new();
    let (run, http, lease_id, fence) = leased(&mut supervisor).await;
    execute(
        &run,
        "UPDATE fleet.runner_affinity SET fencing_seq = fencing_seq + 1 \
         WHERE fleet_id = $1::uuid AND $2 <> ''",
    )
    .await;
    let (status, _) = post_records(&http, &run, (&lease_id, fence), &[record(1, "x")]).await;
    assert_eq!(status, 409, "the stale holder is refused");
    assert_eq!(rows(&run).await, 0);
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Dimension 1.4. One record over a bound is skipped; the rest are kept.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_over_bound_tool_call_detail_skipped() {
    let mut supervisor = Supervisor::new();
    let (run, http, lease_id, fence) = leased(&mut supervisor).await;
    // About 70 KiB: past the 64 KiB field bound.
    let over = "a".repeat(DETAIL_FIELD_MAX_BYTES + DETAIL_FIELD_MAX_BYTES / 16);
    let calls = [record(1, "one"), record(2, &over), record(3, "three")];
    let (status, body) = post_records(&http, &run, (&lease_id, fence), &calls).await;
    assert_eq!(status, 200);
    assert_eq!(body, json!({"stored_count": 2, "skipped_count": 1}));
    assert_eq!(rows(&run).await, 2);
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Dimension 1.5. An event keeps at most 1 MiB of records.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_event_detail_budget_caps_records() {
    let mut supervisor = Supervisor::new();
    let (run, http, lease_id, fence) = leased(&mut supervisor).await;
    // `{}` arguments make each record exactly 64 KiB; three fit a post.
    let output = "a".repeat(DETAIL_FIELD_MAX_BYTES - 2);
    let calls: Vec<Value> = (1..=20).map(|n| record(n, &output)).collect();
    let (mut stored, mut skipped) = (0, 0);
    for batch in calls.chunks(3) {
        let (status, body) = post_records(&http, &run, (&lease_id, fence), batch).await;
        assert_eq!(status, 200);
        stored += body["stored_count"].as_u64().expect("a count");
        skipped += body["skipped_count"].as_u64().expect("a count");
    }
    assert_eq!((stored, skipped), (16, 4));
    assert_eq!(rows(&run).await, 16);
    let one_post =
        serde_json::to_vec(&json!({"fencing_token": fence, "calls": calls[..4]})).expect("encodes");
    assert!(
        one_post.len() > DETAIL_POST_MAX_BYTES,
        "four would not fit a post"
    );
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Dimension 1.6. Settlement keeps only the settling fence's records.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_settle_drops_dead_lease_details() {
    let mut supervisor = Supervisor::new();
    let (run, http, lease_id, fence) = leased(&mut supervisor).await;
    // A dead lease's record, as a reclaim would have left it.
    let dead = afd_db::test_util::mint_id();
    execute(
        &run,
        &format!(
            "INSERT INTO core.fleet_tool_call_details \
             (id, workspace_id, fleet_id, event_id, fencing_token, call_number, arguments, \
              truncated_arguments, output, output_line_count, truncated, byte_count, \
              created_at, updated_at) \
             SELECT '{dead}'::uuid, workspace_id, fleet_id, event_id, {}, 1, '{{}}'::jsonb, \
                    false, 'dead', 1, false, 6, 0, 0 \
             FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2",
            fence - 1
        ),
    )
    .await;
    let (status, _) = post_records(&http, &run, (&lease_id, fence), &[record(1, "live")]).await;
    assert_eq!(status, 200);
    assert_eq!(
        (rows_at(&run, fence - 1).await, rows_at(&run, fence).await),
        (1, 1)
    );

    let settled = post(
        &http,
        &run,
        REPORTS,
        &report_body(&lease_id, &run.event_id, fence),
    )
    .await;
    assert_eq!(settled.status().as_u16(), 200);
    assert_eq!(
        rows_at(&run, fence - 1).await,
        0,
        "the dead lease's record goes"
    );
    assert_eq!(rows_at(&run, fence).await, 1, "the settling lease's stays");
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// GET a tenant path, answering the status and the body.
async fn tenant_status(
    http: &reqwest::Client,
    run: &Scenario,
    token: &str,
    path: &str,
) -> (u16, Value) {
    let response = http
        .get(format!("{}{path}", run.base))
        .bearer_auth(token)
        .send()
        .await
        .expect("the daemon answers");
    let status = response.status().as_u16();
    (status, body_of(response).await)
}

/// Dimensions 2.1 and 2.2. A member reads a kept call in full; an unknown
/// call, another fleet, and a call id that is not one each answer 404.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_member_reads_tool_call_detail() {
    let mut supervisor = Supervisor::new();
    let (run, token) = tenant_reader(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (lease_id, fence) = lease(&http, &run).await;
    let output: String = (1..=224).map(|n| format!("line {n}\n")).collect();
    let mut kept = record(3, &output);
    kept["arguments"] = json!({"url": "https://example.test/deploys/9312", "method": "GET"});
    let (status, _) = post_records(&http, &run, (&lease_id, fence), &[kept.clone()]).await;
    assert_eq!(status, 200);

    let events = format!(
        "/v1/workspaces/{}/fleets/{}/events",
        run.workspace, run.fleet
    );
    for call_id in [format!("{fence}:3"), format!("{fence}%3A3")] {
        let path = format!("{events}/{}/tool-calls/{call_id}", run.event_id);
        let (status, body) = tenant_status(&http, &run, &token, &path).await;
        assert_eq!(status, 200, "{path}: {body}");
        assert_eq!(body["call_id"], json!(format!("{fence}:3")));
        assert_eq!(body["output"], json!(output));
        assert_eq!(body["output_line_count"], json!(224));
        assert_eq!(body["arguments"], kept["arguments"]);
    }
    supervisor.shutdown().await;
    run.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_tool_call_detail_absent_is_not_found() {
    let mut supervisor = Supervisor::new();
    let (run, token) = tenant_reader(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (lease_id, fence) = lease(&http, &run).await;
    let (status, _) = post_records(&http, &run, (&lease_id, fence), &[record(3, "x")]).await;
    assert_eq!(status, 200);

    let other_fleet = afd_db::test_util::mint_id();
    let workspace = &run.workspace;
    for (fleet, call_id) in [
        (run.fleet.as_str(), format!("{fence}:99")),
        (other_fleet.as_str(), format!("{fence}:3")),
        (run.fleet.as_str(), "x:y:z".to_owned()),
    ] {
        let path = format!(
            "/v1/workspaces/{workspace}/fleets/{fleet}/events/{}/tool-calls/{call_id}",
            run.event_id
        );
        let (status, body) = tenant_status(&http, &run, &token, &path).await;
        assert_eq!(status, 404, "{path}: {body}");
        assert_eq!(
            body["error_code"],
            json!(afd_core::error_code::TOOL_CALL_NOT_FOUND.as_str())
        );
    }
    supervisor.shutdown().await;
    run.cleanup().await;
}
