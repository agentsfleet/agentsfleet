//! The read-bound fleets, end to end: each installed from the corpus into a
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

use agentsfleetd::supervisor::Supervisor;

use crate::definition_code::{
    ANSWER, FAILED, FIX, FLEET, PASSED, REPOSITORY, SUBJECT, fixer_turns, greeter_origin,
};
use crate::definition_install::install_definition;
use crate::definition_responder::{RUN, RUN_URL, github_get, responder_upstream};
use crate::definition_run::{run_event, run_event_from};
use crate::e2e_seed_keys::seed_tenant_key;
use crate::fake_model::{Asked, FakeModel, say};
use crate::https::Upstream;
use crate::integration_tenant_registry::{mint_tenant_token, provider_listener};
use crate::integration_tool_trace::{stored_trace, tenant_get};

#[path = "integration_rust_runner_definitions/ci/tests.rs"]
mod ci;

#[path = "integration_rust_runner_definitions/redirect/tests.rs"]
mod redirect;

/// A settled run's trace and one call's full record are read
/// back through the tenant routes an operator's thread uses.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_definition_run_trace_is_readable() {
    let mut supervisor = Supervisor::new();
    let provider = provider_listener().await;
    let run = install_definition(&mut supervisor, "ci-responder", &[], Some(&provider)).await;
    let token = mint_tenant_token();
    seed_tenant_key(&run.booted, &run.tenant, &token, run.seeded_at).await;
    let upstream = Upstream::serve(responder_upstream(200)).await;
    let (model, _transcript) = FakeModel::new(vec![
        vec![github_get("run", RUN)],
        vec![say("Run 101 failed.")],
    ]);

    let settled = run_event(&run, &run.event_id, &upstream, model).await;

    assert_eq!(settled.status, "processed");
    assert_readable_trace(&run, &token).await;
    supervisor.shutdown().await;
    run.cleanup().await;
}

async fn assert_readable_trace(run: &crate::e2e::Scenario, token: &str) {
    let http = afd_identity::direct(std::time::Duration::from_secs(10))
        .build()
        .expect("tenant client");
    let event = format!(
        "/v1/workspaces/{}/fleets/{}/events/{}",
        run.workspace, run.fleet, run.event_id
    );
    let detail = tenant_get(&http, run, token, &event).await;
    let calls = detail["tool_calls"]["calls"]
        .as_array()
        .expect("the event contains its known calls");
    assert_eq!(calls.len(), 1, "{detail}");
    assert_eq!(
        (calls[0]["name"].as_str(), calls[0]["status"].as_str()),
        (Some("http_request"), Some("succeeded"))
    );
    let call_id = calls[0]["call_id"].as_str().expect("a call id");
    assert_readable_record(&http, run, token, &format!("{event}/tool-calls/{call_id}")).await;
}

async fn assert_readable_record(
    http: &reqwest::Client,
    run: &crate::e2e::Scenario,
    token: &str,
    path: &str,
) {
    let record = tenant_get(http, run, token, path).await;
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
}

/// The code-running fleet's repository, bound for reading, is
/// checked out at its default branch before the turn; the fleet runs the
/// suite, fixes the script with a patch, runs the suite again and commits,
/// and the trace holds what the thread shows: each call's status and exit
/// code, the patch, and its `+N −M`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_code_running_definition_roundtrip() {
    let mut supervisor = Supervisor::new();
    let run = install_definition(&mut supervisor, FLEET, &[], None).await;
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
    assert_code_results(&asked);
    assert_code_trace(&run).await;
    supervisor.shutdown().await;
    run.cleanup().await;
}

fn assert_code_results(asked: &[Asked]) {
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
}

async fn assert_code_trace(run: &crate::e2e::Scenario) {
    let trace = stored_trace(run).await.expect("the report carried a trace");
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
}
