//! CI evidence and known trace provenance through the native runner.

use agentsfleetd::supervisor::Supervisor;

use crate::definition_install::install_definition;
use crate::definition_repair::{FILE, GITHUB, HEAD_SHA};
use crate::definition_responder::{
    DIAGNOSIS, FAILED_STEP, FAILING_SHA, JOB_LOG_LINE, LOG_READER_HOST, LOG_SIGNATURE, MEMORY_KEY,
    RUN_URL, SOURCE_GAP, responder_stopping_turns, responder_turns, responder_upstream,
};
use crate::definition_run::{assert_token_stayed_on_the_wire, run_event};
use crate::fake_model::{FakeModel, say};
use crate::https::{Seen, Upstream};
use crate::integration_tool_trace::stored_trace;

const REMEMBERED: &str =
    "SELECT content FROM memory.entries WHERE fleet_id = $1::uuid AND key = $2";

/// GitHub-only evidence reaches the model and memory through native guarded reads.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_responder_reads_exact_ci_evidence() {
    let mut supervisor = Supervisor::new();
    let run = install_definition(&mut supervisor, "ci-responder", &[], None).await;
    let upstream = Upstream::serve(responder_upstream(200)).await;
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
    let seen = upstream.seen();
    assert_ci_sources(&seen, results);
    assert_returned_sources(results);
    assert_token_stayed_on_the_wire(&seen, &asked);
    assert_eq!(remembered(&run).await.as_deref(), Some(DIAGNOSIS));
    let trace = stored_trace(&run)
        .await
        .expect("the report carried a trace");
    assert_eq!(trace["calls"].as_array().map(Vec::len), Some(10));
    assert_eq!(trace["omitted_call_count"], serde_json::json!(0));
    supervisor.shutdown().await;
    run.cleanup().await;
}

fn assert_ci_sources(seen: &[Seen], results: &[String]) {
    for needle in [
        RUN_URL,
        FAILED_STEP,
        FAILING_SHA,
        JOB_LOG_LINE,
        "08:35:24Z",
        "08:35:26Z",
    ] {
        assert!(
            results.iter().any(|result| result.contains(needle)),
            "missing {needle}"
        );
    }
    assert!(
        results
            .iter()
            .any(|result| result.contains("Content-Type: text/plain")
                && result.contains(JOB_LOG_LINE)
                && result.contains("Status: 200"))
    );
    assert!(results.iter().all(|result| !result.contains(LOG_SIGNATURE)));
    assert!(
        seen.iter()
            .all(|request| request.method == hyper::Method::GET)
    );
    assert!(
        seen.iter()
            .all(|request| [GITHUB, LOG_READER_HOST].contains(&request.host.as_str()))
    );
    assert_reviewed_source_reads(seen);
}

fn assert_reviewed_source_reads(seen: &[Seen]) {
    let path = format!("/repos/agentsfleet/linkwarden/contents/{FILE}");
    let reads: Vec<_> = seen
        .iter()
        .filter(|request| request.path == path)
        .map(|request| request.query.as_str())
        .collect();
    assert_eq!(
        reads,
        [format!("ref={FAILING_SHA}"), format!("ref={HEAD_SHA}")]
    );
    assert!(
        seen.iter()
            .filter(|request| request.host == LOG_READER_HOST)
            .all(|request| !request.headers.contains_key("authorization"))
    );
}

fn assert_returned_sources(results: &[String]) {
    for index in [5, 7] {
        let result = results
            .get(index)
            .expect("both source reads reached the model");
        assert!(result.starts_with("Status: 200\n"), "{result}");
        let (_, body) = result
            .split_once("Response Body:\n")
            .expect("HTTP body framing");
        let source: serde_json::Value = serde_json::from_str(body).expect("GitHub source JSON");
        assert_eq!(source["path"], "src/lib/fetch.ts");
        assert_eq!(source["encoding"], "base64");
        assert_eq!(source["content"], "ZXhwb3J0IGNvbnN0IHJldHJpZXMgPSAwOwo=");
    }
    let head = results
        .get(6)
        .expect("the current base read reached the model");
    assert!(head.starts_with("Status: 200\n"), "{head}");
    let (_, body) = head
        .split_once("Response Body:\n")
        .expect("HTTP body framing");
    let branch: serde_json::Value = serde_json::from_str(body).expect("GitHub branch JSON");
    assert_eq!(branch["name"], "dev");
    assert_eq!(branch["commit"]["sha"], HEAD_SHA);
}

/// A failed required log read stops before source, memory or repository writes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_responder_stops_on_required_source_gap() {
    let mut supervisor = Supervisor::new();
    let run = install_definition(&mut supervisor, "ci-responder", &[], None).await;
    let upstream = Upstream::serve(responder_upstream(403)).await;
    let (model, transcript) = FakeModel::new(responder_stopping_turns());
    let settled = run_event(&run, &run.event_id, &upstream, model).await;
    assert_eq!(settled.answer, SOURCE_GAP);
    let seen = upstream.seen();
    assert!(
        seen.iter()
            .all(|request| request.method == hyper::Method::GET
                && !request.path.contains("/contents/")
                && !request.path.contains("/branches/"))
    );
    let asked = transcript.asked();
    let result = asked
        .last()
        .expect("model completed")
        .results
        .last()
        .expect("log result");
    assert!(result.starts_with("[upstream_status]"), "{result}");
    assert!(result.contains("Status: 403"));
    assert_eq!(remembered(&run).await, None);
    let trace = stored_trace(&run).await.expect("known trace");
    assert_eq!(trace["calls"].as_array().map(Vec::len), Some(5));
    assert_eq!(trace["calls"][4]["status"], serde_json::json!("failed"));
    assert_token_stayed_on_the_wire(&seen, &asked);
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// The runner reports a known empty trace when the scripted model uses no tool.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_completed_definition_turn_reports_known_zero_tool_calls() {
    let mut supervisor = Supervisor::new();
    let run = install_definition(&mut supervisor, "github-log-reader", &[], None).await;
    let upstream = Upstream::serve(Vec::new()).await;
    let (model, _transcript) = FakeModel::new(vec![vec![say("No operation was requested.")]]);
    let settled = run_event(&run, &run.event_id, &upstream, model).await;
    assert_eq!(settled.answer, "No operation was requested.");
    let trace = stored_trace(&run)
        .await
        .expect("the completed turn has known trace data");
    assert_eq!(trace["calls"], serde_json::json!([]));
    assert_eq!(trace["omitted_call_count"], serde_json::json!(0));
    assert_eq!(upstream.seen().len(), 0);
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
