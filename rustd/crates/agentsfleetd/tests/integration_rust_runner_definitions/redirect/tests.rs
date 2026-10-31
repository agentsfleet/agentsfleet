//! The hosted tool, TLS download and retained source records compose here.

use agentsfleetd::supervisor::Supervisor;
use hyper::Method;

use crate::definition_install::install_definition;
use crate::definition_responder::{
    JOB_LOG, JOB_LOG_LINE, LOG_READER_HOST, LOG_SIGNATURE, RUN, github_get, log_reader_routes,
};
use crate::definition_run::{assert_token_stayed_on_the_wire, run_event};
use crate::e2e::Scenario;
use crate::e2e_seed_keys::seed_tenant_key;
use crate::fake_model::{FakeModel, say};
use crate::https::{Reply, Route, Upstream};
use crate::integration_tenant_registry::{mint_tenant_token, provider_listener};
use crate::integration_tool_trace::{stored_trace, tenant_get};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn should_read_text_through_admitted_redirects() {
    let provider = provider_listener().await;
    for redirect in [301, 302, 303, 307, 308] {
        for status in [200, 403] {
            read_redirect_case(&provider, redirect, status).await;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn an_interrupted_job_log_returns_no_usable_source_text() {
    let mut supervisor = Supervisor::new();
    let run = install_definition(&mut supervisor, "github-log-reader", &[], None).await;
    let mut routes = vec![Route::new(
        LOG_READER_HOST,
        Method::GET,
        "/download/job",
        vec![Reply::cut("incomplete source bytes")],
    )];
    routes.extend(log_reader_routes(302, 200));
    let upstream = Upstream::serve(routes).await;
    let (model, transcript) = FakeModel::new(vec![
        vec![github_get("log", JOB_LOG)],
        vec![say("Required log unavailable.")],
    ]);
    let settled = run_event(&run, &run.event_id, &upstream, model).await;
    assert_eq!(settled.answer, "Required log unavailable.");
    let asked = transcript.asked();
    let result = asked
        .last()
        .expect("the scripted turn completed")
        .results
        .first()
        .expect("the log call returned its failure");
    assert!(result.starts_with("[upstream_unreachable]"), "{result}");
    assert!(!result.contains("incomplete source bytes"));
    assert!(!result.contains(LOG_SIGNATURE));
    let seen = upstream.seen();
    assert_download(&seen);
    assert_token_stayed_on_the_wire(&seen, &asked);
    let trace = stored_trace(&run)
        .await
        .expect("the failed call was reported");
    assert_eq!(trace["calls"][0]["status"], "failed");
    supervisor.shutdown().await;
    run.cleanup().await;
}

async fn read_redirect_case(provider: &str, redirect: u16, status: u16) {
    let mut supervisor = Supervisor::new();
    let run = install_definition(&mut supervisor, "github-log-reader", &[], Some(provider)).await;
    let upstream = Upstream::serve(log_reader_routes(redirect, status)).await;
    let (model, transcript) = FakeModel::new(vec![
        vec![
            github_get("run", RUN),
            github_get("job", "/repos/agentsfleet/linkwarden/actions/jobs/202"),
        ],
        vec![github_get("log", JOB_LOG)],
        vec![say("Source read inspected.")],
    ]);
    let settled = run_event(&run, &run.event_id, &upstream, model).await;
    assert_eq!(
        (settled.status.as_str(), settled.answer.as_str()),
        ("processed", "Source read inspected.")
    );
    let asked = transcript.asked();
    let results = &asked.last().expect("model read its tool results").results;
    assert!(results.iter().any(|output| output.contains(JOB_LOG_LINE)
        && output.contains(&format!("Status: {status}"))
        && output.contains("Redirects followed: 2")));
    assert!(results.iter().all(|output| !output.contains(LOG_SIGNATURE)));
    let seen = upstream.seen();
    assert_download(&seen);
    assert_token_stayed_on_the_wire(&seen, &asked);
    assert_retained_read(&run, status).await;
    supervisor.shutdown().await;
    run.cleanup().await;
}

fn assert_download(seen: &[crate::https::Seen]) {
    let downloads = seen
        .iter()
        .filter(|request| request.host == LOG_READER_HOST)
        .collect::<Vec<_>>();
    assert_eq!(downloads.len(), 1);
    assert!(downloads[0].headers.get("authorization").is_none());
    assert_eq!(downloads[0].query, LOG_SIGNATURE);
}

async fn assert_retained_read(run: &Scenario, status: u16) {
    let trace = stored_trace(run).await.expect("native trace");
    assert_eq!(trace["calls"].as_array().map(Vec::len), Some(3));
    let token = mint_tenant_token();
    seed_tenant_key(&run.booted, &run.tenant, &token, run.seeded_at).await;
    let http = afd_identity::direct(std::time::Duration::from_secs(10))
        .build()
        .expect("tenant client");
    let call = &trace["calls"][2];
    let id = call["call_id"].as_str().expect("fenced call id");
    let path = format!(
        "/v1/workspaces/{}/fleets/{}/events/{}/tool-calls/{id}",
        run.workspace, run.fleet, run.event_id
    );
    let detail = tenant_get(&http, run, &token, &path).await;
    assert_eq!(detail["truncated"], false);
    assert_eq!(detail["truncated_arguments"], false);
    let output = detail["output"].as_str().expect("retained text");
    assert!(output.contains(JOB_LOG_LINE) && output.contains(&format!("Status: {status}")));
    for text in [trace.to_string(), output.to_owned()] {
        assert!(!text.contains(LOG_SIGNATURE));
        assert!(text.contains("«secret:redirect»") && text.contains("job=202"));
    }
}
