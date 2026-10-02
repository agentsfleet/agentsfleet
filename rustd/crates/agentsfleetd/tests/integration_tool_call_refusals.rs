//! Who may not write a call's record, who may not read one, and what a post
//! or a report carrying a NUL does.
//!
//! Split from `integration_tool_call_details.rs` at the length cap. The router
//! suites prove the guard with no datastore; these prove the statements' own
//! scopes — the runner, the lease's liveness, the workspace — against rows.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_crypto::entropy::Entropy;
use afd_runner::Runners;
use afd_wire::tool_detail::{DETAIL_EVENT_MAX_BYTES, DETAIL_FIELD_MAX_BYTES};
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::e2e::Scenario;
use crate::e2e::scenario;
use crate::e2e_seed::enrolment;
use crate::e2e_seed_keys::seed_tenant_key;
use crate::integration_tenant_registry::mint_tenant_token;
use crate::integration_tool_call_details::{execute, leased, post_records, record, rows};
use crate::integration_tool_trace::{REPORTS, report_with, stored_trace, tenant_reader, trace};
use crate::reads::event_column;
use crate::tail::lease;
use crate::wire::json as body_of;
use crate::wire::post;

/// Posts `calls` to `lease_id` under `fence`, presenting `token`.
async fn post_as(
    http: &reqwest::Client,
    run: &Scenario,
    token: &str,
    (lease_id, fence): (&str, u64),
    calls: &[Value],
) -> u16 {
    http.post(format!(
        "{}/v1/runners/me/leases/{lease_id}/tool-calls",
        run.base
    ))
    .bearer_auth(token)
    .header(reqwest::header::CONTENT_TYPE, "application/json")
    .body(
        serde_json::to_vec(&json!({"fencing_token": fence, "calls": calls}))
            .expect("the body serializes"),
    )
    .send()
    .await
    .expect("the daemon answers")
    .status()
    .as_u16()
}

/// A runner other than the scenario's, enrolled through the production verb.
async fn another_runner(run: &Scenario) -> String {
    Runners::new(run.booted.database.clone(), Entropy::new())
        .register(&enrolment(), run.seeded_at)
        .await
        .expect("enrolment must succeed")
        .token
        .expose()
        .to_owned()
}

/// A lease that is not the caller's, one that has expired, and one that does
/// not exist each answer 404 and keep nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_tool_call_details_refused_for_a_lease_not_held() {
    let mut supervisor = Supervisor::new();
    let (run, http, lease_id, fence) = leased(&mut supervisor).await;
    let calls = [record(1, "x")];

    let stranger = another_runner(&run).await;
    let status = post_as(&http, &run, &stranger, (&lease_id, fence), &calls).await;
    assert_eq!(status, 404, "another runner's lease is not found");

    let unknown = afd_db::test_util::mint_id();
    let (status, _) = post_records(&http, &run, (&unknown, fence), &calls).await;
    assert_eq!(status, 404, "an unknown lease is not found");

    execute(
        &run,
        &format!(
            "UPDATE fleet.runner_leases SET lease_expires_at = 0 \
             WHERE id = '{lease_id}'::uuid AND fleet_id = $1::uuid AND event_id = $2"
        ),
    )
    .await;
    let (status, _) = post_records(&http, &run, (&lease_id, fence), &calls).await;
    assert_eq!(status, 404, "an expired lease is not found");
    assert_eq!(rows(&run).await, 0, "nothing was kept");
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// A record holding a NUL is skipped and counted; the rest of the post stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_nul_tool_call_detail_skipped() {
    let mut supervisor = Supervisor::new();
    let (run, http, lease_id, fence) = leased(&mut supervisor).await;
    let calls = [
        record(1, "one"),
        record(2, "bin\u{0}ary"),
        record(3, "three"),
    ];
    let (status, body) = post_records(&http, &run, (&lease_id, fence), &calls).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({"stored_count": 2, "skipped_count": 1}));
    assert_eq!(rows(&run).await, 2);
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// Posting a call again replaces its record, and at a full budget a
/// replacement is charged only what it adds.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_tool_call_detail_repost_replaces_its_record() {
    let mut supervisor = Supervisor::new();
    let (run, token) = tenant_reader(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (lease_id, fence) = lease(&http, &run).await;
    let claimed = (lease_id.as_str(), fence);

    // `{}` arguments make each record exactly 64 KiB; sixteen fill the event.
    let output = "a".repeat(DETAIL_FIELD_MAX_BYTES - 2);
    let full: Vec<Value> = (1..=16).map(|n| record(n, &output)).collect();
    for batch in full.chunks(3) {
        assert_eq!(post_records(&http, &run, claimed, batch).await.0, 200);
    }
    let (_, body) = post_records(&http, &run, claimed, &[record(17, "x")]).await;
    assert_eq!(body["skipped_count"], 1, "the event is full: {body}");

    let replacement = [record(16, "replaced")];
    let (status, body) = post_records(&http, &run, claimed, &replacement).await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"stored_count": 1, "skipped_count": 0}),
        "a replacement at the cap swaps its old size for its new one"
    );
    let path = format!(
        "/v1/workspaces/{}/fleets/{}/events/{}/tool-calls/{fence}:16",
        run.workspace, run.fleet, run.event_id
    );
    let read = http
        .get(format!("{}{path}", run.base))
        .bearer_auth(&token)
        .send()
        .await
        .expect("the daemon answers");
    assert_eq!(body_of(read).await["output"], "replaced");
    assert!(DETAIL_EVENT_MAX_BYTES / DETAIL_FIELD_MAX_BYTES == 16);
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// A member of another workspace cannot read this workspace's records by
/// naming its fleet under their own workspace.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_tool_call_detail_of_another_workspace_is_not_found() {
    let mut supervisor = Supervisor::new();
    let (run, _owner) = tenant_reader(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (lease_id, fence) = lease(&http, &run).await;
    let (status, _) = post_records(&http, &run, (&lease_id, fence), &[record(3, "secret")]).await;
    assert_eq!(status, 200);

    let (tenant, workspace) = (afd_db::test_util::mint_id(), afd_db::test_util::mint_id());
    execute(
        &run,
        &format!(
            "WITH t AS (INSERT INTO core.tenants (id, name, created_at, updated_at) \
               VALUES ('{tenant}'::uuid, 'other', 0, 0) RETURNING id) \
             INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
             SELECT '{workspace}'::uuid, t.id, 'other', 'e2e', 0 FROM t \
             WHERE $1 <> '' AND $2 <> ''"
        ),
    )
    .await;
    let token = mint_tenant_token();
    seed_tenant_key(&run.booted, &tenant, &token, run.seeded_at).await;

    let path = format!(
        "/v1/workspaces/{workspace}/fleets/{}/events/{}/tool-calls/{fence}:3",
        run.fleet, run.event_id
    );
    let answer = http
        .get(format!("{}{path}", run.base))
        .bearer_auth(&token)
        .send()
        .await
        .expect("the daemon answers");
    assert_eq!(answer.status().as_u16(), 404, "{path}");
    assert_eq!(
        body_of(answer).await["error_code"],
        json!(afd_core::error_code::TOOL_CALL_NOT_FOUND.as_str())
    );
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// A trace holding a NUL — tool output from a binary file — is dropped, and
/// the run's answer still settles: Postgres `jsonb` cannot hold `\u0000`, and
/// the trace is written in the statement that settles the run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_nul_tool_trace_dropped_report_settles() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (lease_id, fence) = lease(&http, &run).await;

    let mut sent = trace(1);
    sent["calls"][0]["output_head"] = json!("bin\u{0}ary");
    let settled = post(
        &http,
        &run,
        REPORTS,
        &report_with(&lease_id, &run, fence, sent),
    )
    .await;
    assert_eq!(
        settled.status().as_u16(),
        200,
        "the answer is never lost to its trace"
    );
    assert_eq!(stored_trace(&run).await, None);
    assert_eq!(
        event_column(&run, &run.event_id, "status").await.as_deref(),
        Some("processed")
    );

    supervisor.shutdown().await;
    run.cleanup().await;
}
