//! The read-bound bundles, end to end: each installed from the corpus into a
//! real daemon, leased by the Rust runner's real loop, its calls sent through
//! the production egress to HTTPS fakes, and the model a script.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it just read"
)]

use afr_providers::Chunk;
use afr_tools::ToolErrorCode;
use agentsfleetd::supervisor::Supervisor;
use hyper::Method;
use serde_json::json;

use crate::bundle_install::{Secret, install_bundle};
use crate::bundle_repair::{GITHUB, github_auth};
use crate::bundle_run::{assert_token_stayed_on_the_wire, run_event};
use crate::e2e_seed_keys::seed_tenant_key;
use crate::fake_model::{FakeModel, call, http, say};
use crate::https::{Reply, Route, Upstream};
use crate::integration_tenant_registry::{mint_tenant_token, provider_listener};
use crate::integration_tool_trace::{stored_trace, tenant_get};

/// The run, jobs and commits the responder reads, under its one repository.
const RUN: &str = "/repos/agentsfleet/linkwarden/actions/runs/101";
const JOBS: &str = "/repos/agentsfleet/linkwarden/actions/runs/101/jobs";
const JOB_LOG: &str = "/repos/agentsfleet/linkwarden/actions/jobs/202/logs";
const COMMITS: &str = "/repos/agentsfleet/linkwarden/commits";
const RUN_URL: &str = "https://github.com/agentsfleet/linkwarden/actions/runs/101";
const FAILED_STEP: &str = "Run unit tests";
/// Where GitHub's 302 sends a job log, signature and all; the model reads the
/// origin alone.
const LOG_STORE: &str = "https://pipelines.actions.githubusercontent.com";
const LOG_SIGNATURE: &str = "sig=fixture-signature";
/// The bundle's Grafana stack, as its install binding names it, and the
/// Viewer token sealed for it.
const GRAFANA: &str = "grafana.example.net";
const GRAFANA_TOKEN: &str = "glsa_fixture_viewer";
const LOKI: &str = "/api/datasources/proxy/uid/loki-uid/loki/api/v1/query_range";
const LOKI_LINE: &str = "linkwarden-api TypeError: cannot read properties of undefined";
const ANNOTATIONS: &str = "/api/annotations";
/// What the responder answers and remembers.
const DIAGNOSIS: &str = "Run 101 failed at Run unit tests; job log unavailable (302); \
                         Loki shows the TypeError; annotations unreadable.";
const MEMORY_KEY: &str = "ci:linkwarden:101";
/// The memory row a run's push writes.
const REMEMBERED: &str =
    "SELECT content FROM memory.memory_entries WHERE fleet_id = $1::uuid AND key = $2";

/// The Grafana credential the responder declares.
fn grafana() -> Secret {
    Secret {
        name: "grafana",
        body: json!({"host": GRAFANA, "token": GRAFANA_TOKEN}),
    }
}

/// A GET to `path` on GitHub with the minted credential.
fn github_get(id: &str, path: &str) -> Chunk {
    http(
        id,
        json!({"url": format!("https://{GITHUB}{path}"), "headers": github_auth()}),
    )
}

/// A GET to `path_and_query` on the sealed Grafana host, its token in place.
fn grafana_get(id: &str, path_and_query: &str) -> Chunk {
    http(
        id,
        json!({"url": format!("https://${{secrets.grafana.host}}{path_and_query}"),
                    "headers": {"Authorization": "Bearer ${secrets.grafana.token}"}}),
    )
}

/// GitHub and Grafana as a failed linkwarden run left them.
fn responder_upstream() -> Vec<Route> {
    let ok = |body: serde_json::Value| vec![Reply::json(200, &body)];
    vec![
        Route::new(
            GITHUB,
            Method::GET,
            RUN,
            ok(json!({"id": 101, "html_url": RUN_URL,
            "conclusion": "failure", "head_sha": "c0ffee1"})),
        ),
        Route::new(
            GITHUB,
            Method::GET,
            JOBS,
            ok(json!({"jobs": [{"id": 202, "name": "test",
            "conclusion": "failure", "steps": [{"name": FAILED_STEP, "conclusion": "failure"}]}]})),
        ),
        Route::new(
            GITHUB,
            Method::GET,
            JOB_LOG,
            vec![Reply::found(&format!(
                "{LOG_STORE}/logs/202?{LOG_SIGNATURE}"
            ))],
        ),
        Route::new(
            GITHUB,
            Method::GET,
            COMMITS,
            ok(json!([{"sha": "c0ffee1"}])),
        ),
        Route::new(
            GRAFANA,
            Method::GET,
            "/api/datasources",
            ok(json!([{"uid": "loki-uid", "type": "loki"}])),
        ),
        Route::new(
            GRAFANA,
            Method::GET,
            LOKI,
            ok(json!({"data": {"result": [{"values": [["1", LOKI_LINE]]}]}})),
        ),
        Route::new(
            GRAFANA,
            Method::GET,
            ANNOTATIONS,
            vec![Reply::cut(r#"[{"id": 1, "text""#)],
        ),
    ]
}

/// The responder's investigation, as `ci-responder/SKILL.md` orders it.
fn responder_turns() -> Vec<Vec<Chunk>> {
    vec![
        vec![
            call("recall", "memory_recall", json!({"query": "linkwarden"})),
            github_get("run", RUN),
            github_get("jobs", JOBS),
        ],
        vec![github_get("log", JOB_LOG), github_get("commits", COMMITS)],
        vec![
            grafana_get("datasources", "/api/datasources"),
            grafana_get(
                "loki",
                &format!("{LOKI}?query=%7Bapp%3D%22linkwarden%22%7D&direction=backward"),
            ),
            grafana_get("annotations", ANNOTATIONS),
        ],
        vec![call(
            "store",
            "memory_store",
            json!({"key": MEMORY_KEY, "content": DIAGNOSIS, "category": "core"}),
        )],
        vec![say(DIAGNOSIS)],
    ]
}

/// Dimension 6.1. The responder is offered exactly its policy's tools, reads
/// GitHub with a minted token and Grafana with its sealed one, reads the 302's
/// origin and not its signed target, names an unreadable source, stores a
/// memory, and settles with its diagnosis and a trace of every call.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_ci_responder_triages_a_failed_run() {
    let mut supervisor = Supervisor::new();
    let run = install_bundle(&mut supervisor, "ci-responder", &[grafana()], None).await;
    let upstream = Upstream::serve(responder_upstream()).await;
    let (model, transcript) = FakeModel::new(responder_turns());

    let settled = run_event(&run, &run.event_id, &upstream, model).await;

    assert_eq!(
        (settled.status.as_str(), settled.answer.as_str()),
        ("processed", DIAGNOSIS)
    );
    let asked = transcript.asked();
    let mut offered = asked[0].tools.clone();
    offered.sort_unstable();
    assert_eq!(offered, ["http_request", "memory_recall", "memory_store"]);
    let results = &asked.last().expect("the model was asked").results;
    let read = |needle: &str| results.iter().any(|result| result.contains(needle));
    assert!(
        read(RUN_URL) && read(FAILED_STEP) && read(LOKI_LINE),
        "{results:#?}"
    );
    assert!(
        read("Status: 302") && read(&format!("Location: {LOG_STORE}")),
        "{results:#?}"
    );
    assert!(
        !read(LOG_SIGNATURE),
        "a redirect's signed target never reaches the model"
    );
    let unreadable = format!("[{}]", ToolErrorCode::UpstreamUnreachable.as_str());
    assert!(
        results.iter().any(|result| result.starts_with(&unreadable)
            && result.contains("the response could not be read")),
        "a body cut mid-read reaches the model as a named gap: {results:#?}"
    );

    let seen = upstream.seen();
    let grafana_auth = format!("Bearer {GRAFANA_TOKEN}");
    let to_grafana: Vec<_> = seen
        .iter()
        .filter(|request| request.host == GRAFANA)
        .collect();
    assert_eq!(
        to_grafana.len(),
        3,
        "the sealed host was substituted for every Grafana call"
    );
    for request in to_grafana {
        let sent = request
            .headers
            .get("authorization")
            .and_then(|value| value.to_str().ok());
        assert_eq!(sent, Some(grafana_auth.as_str()));
    }
    assert_token_stayed_on_the_wire(&seen, &asked);
    assert_eq!(remembered(&run).await.as_deref(), Some(DIAGNOSIS));
    let trace = stored_trace(&run)
        .await
        .expect("the report carried a trace");
    assert_eq!(trace["calls"].as_array().map(Vec::len), Some(9), "{trace}");

    supervisor.shutdown().await;
    run.cleanup().await;
}

/// What the fleet's durable memory holds under [`MEMORY_KEY`].
async fn remembered(run: &crate::e2e::Scenario) -> Option<String> {
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    sqlx::query_scalar(REMEMBERED)
        .bind(&run.fleet)
        .bind(MEMORY_KEY)
        .fetch_optional(&mut *connection)
        .await
        .expect("the memory table reads")
}

/// Dimension 6.5. A settled run's trace and one call's full record are read
/// back through the tenant routes an operator's thread uses.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_bundle_run_trace_is_readable() {
    let mut supervisor = Supervisor::new();
    let provider = provider_listener().await;
    let run = install_bundle(
        &mut supervisor,
        "ci-responder",
        &[grafana()],
        Some(&provider),
    )
    .await;
    let token = mint_tenant_token();
    seed_tenant_key(&run.booted, &run.tenant, &token, run.seeded_at).await;
    let upstream = Upstream::serve(responder_upstream()).await;
    let (model, _transcript) = FakeModel::new(vec![
        vec![github_get("run", RUN)],
        vec![say("Run 101 failed.")],
    ]);

    let settled = run_event(&run, &run.event_id, &upstream, model).await;

    assert_eq!(settled.status, "processed");
    let http = reqwest::Client::new();
    let event = format!(
        "/v1/workspaces/{}/fleets/{}/events/{}",
        run.workspace, run.fleet, run.event_id
    );
    let detail = tenant_get(&http, &run, &token, &event).await;
    let calls = detail["tool_calls"]["calls"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(calls.len(), 1, "{detail}");
    assert_eq!(
        (calls[0]["name"].as_str(), calls[0]["status"].as_str()),
        (Some("http_request"), Some("succeeded"))
    );
    let call_id = calls[0]["call_id"].as_str().expect("a call id");
    let record = tenant_get(
        &http,
        &run,
        &token,
        &format!("{event}/tool-calls/{call_id}"),
    )
    .await;
    assert!(
        record["output"]
            .as_str()
            .is_some_and(|output| output.contains(RUN_URL)),
        "{record}"
    );
    assert!(
        record["arguments"]["url"]
            .as_str()
            .is_some_and(|url| url.ends_with(RUN)),
        "{record}"
    );

    supervisor.shutdown().await;
    run.cleanup().await;
}
