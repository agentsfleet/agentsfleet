//! The read-bound bundles, end to end: each installed from the corpus into a
//! real daemon, leased by the Rust runner's real loop, the model a script, and
//! its calls sent through the production egress to HTTPS fakes or run in the
//! unsandboxed engine's workspace, on a repository the runner checked out.
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

use afr_tools::ToolErrorCode;
use agentsfleetd::supervisor::Supervisor;

use crate::bundle_code::{
    ANSWER, BUNDLE, FAILED, FIX, PASSED, REPOSITORY, SUBJECT, fixer_turns, greeter_origin,
};
use crate::bundle_install::install_bundle;
use crate::bundle_responder::{
    DIAGNOSIS, FAILED_STEP, GRAFANA, GRAFANA_TOKEN, LOG_SIGNATURE, LOG_STORE, LOKI_LINE,
    MEMORY_KEY, RUN, RUN_URL, github_get, grafana, responder_turns, responder_upstream,
};
use crate::bundle_run::{assert_token_stayed_on_the_wire, run_event, run_event_from};
use crate::e2e_seed_keys::seed_tenant_key;
use crate::fake_model::{FakeModel, say};
use crate::https::Upstream;
use crate::integration_tenant_registry::{mint_tenant_token, provider_listener};
use crate::integration_tool_trace::{stored_trace, tenant_get};

/// The memory row a run's push writes.
const REMEMBERED: &str =
    "SELECT content FROM memory.memory_entries WHERE fleet_id = $1::uuid AND key = $2";

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

/// Dimension 7.1. The code-running bundle's repository, bound for reading, is
/// checked out at its default branch before the turn; the fleet runs the
/// suite, fixes the script with a patch, runs the suite again and commits,
/// and the trace holds what the thread shows: each call's status and exit
/// code, the patch, and its `+N −M`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_code_running_bundle_roundtrip() {
    let mut supervisor = Supervisor::new();
    let run = install_bundle(&mut supervisor, BUNDLE, &[], None).await;
    let origins = tempfile::tempdir().expect("an origin directory");
    let origin = greeter_origin(origins.path());
    let upstream = Upstream::serve(Vec::new()).await;
    let (model, transcript) = FakeModel::new(fixer_turns());

    let settled = run_event_from(&run, &run.event_id, &upstream, model, Some(origin)).await;

    assert_eq!(
        (settled.status.as_str(), settled.answer.as_str()),
        ("processed", ANSWER)
    );
    let asked = transcript.asked();
    let mut offered = asked[0].tools.clone();
    offered.sort_unstable();
    assert_eq!(offered, ["apply_patch", "git", "shell"]);
    let checked_out = format!("{REPOSITORY} is checked out at ./greeter on its default branch");
    assert!(
        asked[0].instructions.contains(&checked_out),
        "{}",
        asked[0].instructions
    );
    let results = &asked.last().expect("the model was asked").results;
    assert_eq!(results.len(), 4, "{results:#?}");
    assert!(
        results[0].starts_with(FAILED) && results[0].ends_with("Process exited with code 1"),
        "{results:#?}"
    );
    assert!(results[1].ends_with("\n+1 \u{2212}1"), "{results:#?}");
    assert_eq!(
        results[2].trim_end(),
        PASSED,
        "a command that succeeds answers its output alone"
    );
    assert!(results[3].contains(SUBJECT), "{results:#?}");

    let trace = stored_trace(&run)
        .await
        .expect("the report carried a trace");
    let calls = trace["calls"]
        .as_array()
        .expect("the trace lists its calls");
    let ended: Vec<_> = calls
        .iter()
        .map(|call| {
            (
                call["name"].as_str(),
                call["status"].as_str(),
                call["exit_code"].as_i64(),
            )
        })
        .collect();
    assert_eq!(
        ended,
        [
            (Some("shell"), Some("failed"), Some(1)),
            (Some("apply_patch"), Some("succeeded"), None),
            (Some("shell"), Some("succeeded"), Some(0)),
            (Some("git"), Some("succeeded"), Some(0)),
        ],
        "{trace}"
    );
    assert_eq!(
        calls[1]["arguments"]["patch"].as_str(),
        Some(FIX),
        "{trace}"
    );

    supervisor.shutdown().await;
    run.cleanup().await;
}
