//! Each tool call's full record: posted by a runner under its run's fence,
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
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it just built"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "plain clients call the daemon these tests booted; a proxy rule and a deadline are not what the tests prove"
)]

use af_wire::tool_detail::{DETAIL_FIELD_MAX_BYTES, DETAIL_POST_MAX_BYTES};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};
use sqlx::AssertSqlSafe;

use crate::e2e::{Scenario, scenario};
use crate::integration_tool_trace::tenant_reader;
use crate::tail::run;
use crate::wire::{json as body_of, post};

/// One `shell` call's record of `call_number` whose output is `output`, with
/// no arguments. [`post_records`] names the turn it belongs to.
pub(crate) fn record(call_number: u64, output: &str) -> Value {
    json!({"call_number": call_number, "arguments": {}, "truncated_arguments": false,
           "output": output, "output_line_count": output.lines().count(), "truncated": false,
           "kind": "tool_call", "tool": "shell", "status": "succeeded"})
}

/// Posts `calls` under `fence`, answering the status and the body. A record
/// naming no turn is posted naming the scenario's, the run's own.
pub(crate) async fn post_records(
    http: &reqwest::Client,
    scene: &Scenario,
    (run_id, fence): (&str, u64),
    calls: &[Value],
) -> (u16, Value) {
    let path = format!("/v1/runners/me/runs/{run_id}/items");
    let calls: Vec<Value> = calls
        .iter()
        .cloned()
        .map(|mut call| {
            if let Some(fields) = call.as_object_mut() {
                fields
                    .entry("turn_id")
                    .or_insert_with(|| json!(scene.turn_id.as_str()));
            }
            call
        })
        .collect();
    let body = json!({"fencing_token": fence, "calls": calls});
    let response = post(http, scene, &path, &body).await;
    let status = response.status().as_u16();
    (status, body_of(response).await)
}

/// One statement over the scenario's pool, answering its single integer.
pub(crate) async fn scalar(scene: &Scenario, statement: &str, fence: Option<u64>) -> i64 {
    let mut connection = scene.booted.database.acquire().await.expect("a connection");
    let query = sqlx::query_scalar(AssertSqlSafe(statement.to_owned()))
        .bind(&scene.fleet)
        .bind(&scene.turn_id);
    let query = match fence {
        Some(fence) => query.bind(i64::try_from(fence).expect("a fence fits")),
        None => query,
    };
    query
        .fetch_one(&mut *connection)
        .await
        .expect("the read runs")
}

/// How many records the scenario's event keeps.
pub(crate) async fn rows(scene: &Scenario) -> i64 {
    scalar(
        scene,
        "SELECT count(*) FROM thread.items t \
         JOIN thread.turns e ON e.id = t.turn_id \
         WHERE t.fleet_id = $1 AND e.id = $2",
        None,
    )
    .await
}

/// How many records the scenario's event keeps under `fence`.
pub(crate) async fn rows_at(scene: &Scenario, fence: u64) -> i64 {
    scalar(
        scene,
        "SELECT count(*) FROM thread.items t \
         JOIN thread.turns e ON e.id = t.turn_id \
         WHERE t.fleet_id = $1 AND e.id = $2 AND t.fencing_token = $3",
        Some(fence),
    )
    .await
}

/// Runs one statement bound to the scenario's fleet and event.
pub(crate) async fn execute(scene: &Scenario, statement: &str) {
    let mut connection = scene.booted.database.acquire().await.expect("a connection");
    sqlx::query(AssertSqlSafe(statement.to_owned()))
        .bind(&scene.fleet)
        .bind(&scene.turn_id)
        .execute(&mut *connection)
        .await
        .expect("the statement runs");
}

/// Boots a scenario and runs its event.
pub(crate) async fn held(supervisor: &mut Supervisor) -> (Scenario, reqwest::Client, String, u64) {
    let scene = scenario(supervisor).await;
    let http = reqwest::Client::new();
    let (run_id, fence) = run(&scene).await;
    (scene, http, run_id, fence)
}

/// Dimensions 1.1 and 1.3. A valid post stores one row per record, and the
/// same post again changes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_tool_call_details_stored_under_fence() {
    let mut supervisor = Supervisor::new();
    let (scene, http, run_id, fence) = held(&mut supervisor).await;
    let calls = [record(1, "one"), record(2, "two")];
    let (status, body) = post_records(&http, &scene, (&run_id, fence), &calls).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({"stored_count": 2, "skipped_count": 0}));
    assert_eq!(rows_at(&scene, fence).await, 2);
    supervisor.shutdown().await;
    scene.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_tool_call_details_retry_is_idempotent() {
    let mut supervisor = Supervisor::new();
    let (scene, http, run_id, fence) = held(&mut supervisor).await;
    let calls = [record(1, "one"), record(2, "two")];
    for _attempt in 0..2 {
        let (status, _) = post_records(&http, &scene, (&run_id, fence), &calls).await;
        assert_eq!(status, 200);
    }
    assert_eq!(rows(&scene).await, 2, "a retried post upserts");
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// Dimension 1.2. A holder the fleet has moved past keeps nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_stale_fence_tool_call_details_refused() {
    let mut supervisor = Supervisor::new();
    let (scene, http, run_id, fence) = held(&mut supervisor).await;
    execute(
        &scene,
        "UPDATE thread.threads SET fencing_token = fencing_token + 1 \
         WHERE fleet_id = $1 AND $2 IS NOT NULL",
    )
    .await;
    let (status, _) = post_records(&http, &scene, (&run_id, fence), &[record(1, "x")]).await;
    assert_eq!(status, 409, "the stale holder is refused");
    assert_eq!(rows(&scene).await, 0);
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// Dimension 1.4. One record over a bound is skipped; the rest are kept.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_over_bound_tool_call_detail_skipped() {
    let mut supervisor = Supervisor::new();
    let (scene, http, run_id, fence) = held(&mut supervisor).await;
    // About 70 KiB: past the 64 KiB field bound.
    let over = "a".repeat(DETAIL_FIELD_MAX_BYTES + DETAIL_FIELD_MAX_BYTES / 16);
    let calls = [record(1, "one"), record(2, &over), record(3, "three")];
    let (status, body) = post_records(&http, &scene, (&run_id, fence), &calls).await;
    assert_eq!(status, 200);
    assert_eq!(body, json!({"stored_count": 2, "skipped_count": 1}));
    assert_eq!(rows(&scene).await, 2);
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// Dimension 1.5. An event keeps at most 1 MiB of records.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_event_detail_budget_caps_records() {
    let mut supervisor = Supervisor::new();
    let (scene, http, run_id, fence) = held(&mut supervisor).await;
    // `{}` arguments make each record exactly 64 KiB; three fit a post.
    let output = "a".repeat(DETAIL_FIELD_MAX_BYTES - 2);
    let calls: Vec<Value> = (1..=20).map(|n| record(n, &output)).collect();
    let (mut stored, mut skipped) = (0, 0);
    for batch in calls.chunks(3) {
        let (status, body) = post_records(&http, &scene, (&run_id, fence), batch).await;
        assert_eq!(status, 200);
        stored += body["stored_count"].as_u64().expect("a count");
        skipped += body["skipped_count"].as_u64().expect("a count");
    }
    assert_eq!((stored, skipped), (16, 4));
    assert_eq!(rows(&scene).await, 16);
    let one_post =
        serde_json::to_vec(&json!({"fencing_token": fence, "calls": calls[..4]})).expect("encodes");
    assert!(
        one_post.len() > DETAIL_POST_MAX_BYTES,
        "four would not fit a post"
    );
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// GET a tenant path, answering the status and the body.
async fn tenant_status(
    http: &reqwest::Client,
    scene: &Scenario,
    token: &str,
    path: &str,
) -> (u16, Value) {
    let response = http
        .get(format!("{}{path}", scene.base))
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
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_member_reads_tool_call_detail() {
    let mut supervisor = Supervisor::new();
    let (scene, token) = tenant_reader(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (run_id, fence) = run(&scene).await;
    let lines: Vec<String> = (1..=224).map(|n| format!("line {n}")).collect();
    let output = format!("{}\n", lines.join("\n"));
    let mut kept = record(3, &output);
    kept["arguments"] = json!({"url": "https://example.test/deploys/9312", "method": "GET"});
    let (status, _) = post_records(&http, &scene, (&run_id, fence), &[kept.clone()]).await;
    assert_eq!(status, 200);

    let events = format!(
        "/v1/workspaces/{}/fleets/{}/turns",
        scene.workspace, scene.fleet
    );
    for call_id in [format!("{fence}:3"), format!("{fence}%3A3")] {
        let path = format!("{events}/{}/items/{call_id}", scene.turn_id);
        let (status, body) = tenant_status(&http, &scene, &token, &path).await;
        assert_eq!(status, 200, "{path}: {body}");
        assert_eq!(body["call_id"], json!(format!("{fence}:3")));
        assert_eq!(body["output"], json!(output));
        assert_eq!(body["output_line_count"], json!(224));
        assert_eq!(body["arguments"], kept["arguments"]);
    }
    supervisor.shutdown().await;
    scene.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_tool_call_detail_absent_is_not_found() {
    let mut supervisor = Supervisor::new();
    let (scene, token) = tenant_reader(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (run_id, fence) = run(&scene).await;
    let (status, _) = post_records(&http, &scene, (&run_id, fence), &[record(3, "x")]).await;
    assert_eq!(status, 200);

    let other_fleet = afd_db::test_util::mint_id();
    let workspace = &scene.workspace;
    for (fleet, call_id) in [
        (scene.fleet.as_str(), format!("{fence}:99")),
        (other_fleet.as_str(), format!("{fence}:3")),
        (scene.fleet.as_str(), "x:y:z".to_owned()),
    ] {
        let path = format!(
            "/v1/workspaces/{workspace}/fleets/{fleet}/turns/{}/items/{call_id}",
            scene.turn_id
        );
        let (status, body) = tenant_status(&http, &scene, &token, &path).await;
        assert_eq!(status, 404, "{path}: {body}");
        assert_eq!(
            body["error_code"],
            json!(af_core::error_code::TOOL_CALL_NOT_FOUND.as_str())
        );
    }
    supervisor.shutdown().await;
    scene.cleanup().await;
}
