//! Who may not write a call's record, who may not read one, and what a post
//! or a report carrying a NUL does.
//!
//! Split from `integration_tool_call_details.rs` at the length cap. The router
//! suites prove the guard with no datastore; these prove the statements' own
//! scopes — the runner, the run's liveness, the workspace — against rows.
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

use af_crypto::entropy::Entropy;
use af_wire::tool_detail::DETAIL_FIELD_MAX_BYTES;
use afd_runner::Runners;
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::e2e::Scenario;
use crate::e2e::scenario;
use crate::e2e_seed::enrolment;
use crate::e2e_seed_keys::seed_tenant_key;
use crate::integration_tenant_registry::mint_tenant_token;
use crate::integration_tool_call_details::{execute, held, post_records, record, rows};
use crate::integration_tool_trace::{report_with, stored_trace, tenant_reader, trace};
use crate::reads::event_column;
use crate::tail::run;
use crate::wire::json as body_of;

/// Posts `calls` to `run_id` under `fence`, presenting `token`.
async fn post_as(
    http: &reqwest::Client,
    scene: &Scenario,
    token: &str,
    (run_id, fence): (&str, u64),
    calls: &[Value],
) -> u16 {
    http.post(format!(
        "{}/v1/runners/me/runs/{run_id}/tool-calls",
        scene.base
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
async fn another_runner(scene: &Scenario) -> String {
    Runners::new(scene.booted.database.clone(), Entropy::new())
        .register(&enrolment(), "fixture:platform-operator", scene.seeded_at)
        .await
        .expect("enrolment must succeed")
        .token
        .expose()
        .to_owned()
}

/// A run that is not the caller's, one that has expired, and one that does
/// not exist each answer 404 and keep nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_tool_call_details_refused_for_a_run_not_held() {
    let mut supervisor = Supervisor::new();
    let (scene, http, run_id, fence) = held(&mut supervisor).await;
    let calls = [record(1, "x")];

    let stranger = another_runner(&scene).await;
    let status = post_as(&http, &scene, &stranger, (&run_id, fence), &calls).await;
    assert_eq!(status, 404, "another runner's run is not found");

    let unknown = afd_db::test_util::mint_id();
    let (status, _) = post_records(&http, &scene, (unknown.as_str(), fence), &calls).await;
    assert_eq!(status, 404, "an unknown run is not found");
    let report = report_with(unknown.as_str(), &scene, fence, trace(1));
    let refused = crate::wire::report(&scene, &report).await;
    assert_eq!(
        refused
            .expect_err("an unknown run is refused")
            .refusal_code(),
        Some(af_core::error_code::RUN_NOT_FOUND),
        "and its report, trace and all, is refused before anything is read"
    );

    execute(
        &scene,
        &format!(
            "UPDATE runner.runs SET run_expires_at = to_timestamp(0) \
             WHERE id = '{run_id}'::uuid AND fleet_id = $1 AND event_id = $2"
        ),
    )
    .await;
    let (status, _) = post_records(&http, &scene, (&run_id, fence), &calls).await;
    assert_eq!(status, 404, "an expired run is not found");
    assert_eq!(rows(&scene).await, 0, "nothing was kept");
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// A record holding a NUL is skipped and counted; the rest of the post stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_nul_tool_call_detail_skipped() {
    let mut supervisor = Supervisor::new();
    let (scene, http, run_id, fence) = held(&mut supervisor).await;
    let calls = [
        record(1, "one"),
        record(2, "bin\u{0}ary"),
        record(3, "three"),
    ];
    let (status, body) = post_records(&http, &scene, (&run_id, fence), &calls).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body, json!({"stored_count": 2, "skipped_count": 1}));
    assert_eq!(rows(&scene).await, 2);
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// Posting a call again replaces its record, and at a full budget a
/// replacement is charged only what it adds.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_tool_call_detail_repost_replaces_its_record() {
    let mut supervisor = Supervisor::new();
    let (scene, token) = tenant_reader(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (run_id, fence) = run(&scene).await;
    let claimed = (run_id.as_str(), fence);

    // `{}` arguments make each record exactly 64 KiB; sixteen fill the event.
    let output = "a".repeat(DETAIL_FIELD_MAX_BYTES - 2);
    let full: Vec<Value> = (1..=16).map(|n| record(n, &output)).collect();
    for batch in full.chunks(3) {
        assert_eq!(post_records(&http, &scene, claimed, batch).await.0, 200);
    }
    let (_, body) = post_records(&http, &scene, claimed, &[record(17, "x")]).await;
    assert_eq!(body["skipped_count"], 1, "the event is full: {body}");

    let replacement = [record(16, "replaced")];
    let (status, body) = post_records(&http, &scene, claimed, &replacement).await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"stored_count": 1, "skipped_count": 0}),
        "a replacement at the cap swaps its old size for its new one"
    );
    let path = format!(
        "/v1/workspaces/{}/fleets/{}/events/{}/tool-calls/{fence}:16",
        scene.workspace, scene.fleet, scene.event_id
    );
    let read = http
        .get(format!("{}{path}", scene.base))
        .bearer_auth(&token)
        .send()
        .await
        .expect("the daemon answers");
    assert_eq!(body_of(read).await["output"], "replaced");
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// A member of another workspace cannot read this workspace's records by
/// naming its fleet under their own workspace.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_tool_call_detail_of_another_workspace_is_not_found() {
    let mut supervisor = Supervisor::new();
    let (scene, _owner) = tenant_reader(&mut supervisor).await;
    let http = reqwest::Client::new();
    let (run_id, fence) = run(&scene).await;
    let (status, _) = post_records(&http, &scene, (&run_id, fence), &[record(3, "secret")]).await;
    assert_eq!(status, 200);

    let (tenant, workspace) = (afd_db::test_util::mint_id(), afd_db::test_util::mint_id());
    execute(
        &scene,
        &format!(
            "WITH t AS (INSERT INTO identity.tenants (id, name, created_at, updated_at) \
               VALUES ('{tenant}'::uuid, 'other', now(), now()) RETURNING id) \
             INSERT INTO identity.workspaces (id, tenant_id, name, created_at) \
             SELECT '{workspace}'::uuid, t.id, 'other', now() FROM t \
             WHERE $1 IS NOT NULL AND $2 IS NOT NULL"
        ),
    )
    .await;
    let token = mint_tenant_token();
    seed_tenant_key(&scene.booted, &tenant, &token, scene.seeded_at).await;

    let path = format!(
        "/v1/workspaces/{workspace}/fleets/{}/events/{}/tool-calls/{fence}:3",
        scene.fleet, scene.event_id
    );
    let answer = http
        .get(format!("{}{path}", scene.base))
        .bearer_auth(&token)
        .send()
        .await
        .expect("the daemon answers");
    assert_eq!(answer.status().as_u16(), 404, "{path}");
    assert_eq!(
        body_of(answer).await["error_code"],
        json!(af_core::error_code::TOOL_CALL_NOT_FOUND.as_str())
    );
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// A trace holding a NUL — tool output from a binary file — is dropped, and
/// the run's answer still settles: Postgres `jsonb` cannot hold `\u0000`, and
/// the trace is written in the statement that settles the run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_nul_tool_trace_dropped_report_settles() {
    let mut supervisor = Supervisor::new();
    let scene = scenario(&mut supervisor).await;
    let (run_id, fence) = run(&scene).await;

    let mut sent = trace(1);
    sent["calls"][0]["output_head"] = json!("bin\u{0}ary");
    let settled = crate::wire::report(&scene, &report_with(&run_id, &scene, fence, sent)).await;
    assert!(settled.is_ok(), "native report refused: {settled:?}");
    assert_eq!(stored_trace(&scene).await, None);
    assert_eq!(
        event_column(&scene, &scene.event_id, "status")
            .await
            .as_deref(),
        Some("processed")
    );

    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// Posts racing for one event serialise on the run row, so together they
/// keep no more than the budget: 24 posts of one 64 KiB record each, released
/// together, store exactly 16 and skip exactly 8.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_concurrent_posts_never_pass_the_event_budget() {
    let mut supervisor = Supervisor::new();
    let (scene, http, run_id, fence) = held(&mut supervisor).await;
    let output = "a".repeat(DETAIL_FIELD_MAX_BYTES - 2);
    let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(24));
    let url = format!("{}/v1/runners/me/runs/{run_id}/tool-calls", scene.base);
    let mut posts = tokio::task::JoinSet::new();
    for n in 1..=24_u64 {
        let (http, url, token, barrier) = (
            http.clone(),
            url.clone(),
            scene.token.clone(),
            std::sync::Arc::clone(&barrier),
        );
        let body =
            serde_json::to_vec(&json!({"fencing_token": fence, "calls": [record(n, &output)]}))
                .expect("the body serializes");
        posts.spawn(async move {
            barrier.wait().await;
            let answer = http
                .post(url)
                .bearer_auth(token)
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body)
                .send()
                .await
                .expect("the daemon answers");
            (answer.status().as_u16(), body_of(answer).await)
        });
    }
    let (mut stored, mut skipped) = (0, 0);
    while let Some(answer) = posts.join_next().await {
        let (status, body) = answer.expect("the post task completes");
        assert_eq!(status, 200, "{body}");
        stored += body["stored_count"].as_u64().expect("a count");
        skipped += body["skipped_count"].as_u64().expect("a count");
    }
    assert_eq!((stored, skipped), (16, 8), "exactly the budget, once");
    assert_eq!(rows(&scene).await, 16);
    supervisor.shutdown().await;
    scene.cleanup().await;
}
