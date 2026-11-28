//! A restricted fleet follows a moved guide only after the person grants its host.

mod boundaries;
mod decision;
mod script;

use af_core::id::Uuid7;
use agentsfleetd::supervisor::Supervisor;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::definition_run::{runner, settled};
use crate::e2e::Scenario;
use crate::e2e_seed_keys::seed_tenant_key;
use crate::integration_tenant_registry::mint_tenant_token;
use crate::integration_tool_trace::tenant_get;
use boundaries::{Pages, Vendors};
use script::{Outcome, SENTINEL, model};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn approving_a_redirected_host_finishes_the_original_read() {
    exercise(Outcome::Retry).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn approving_a_redirected_host_allows_the_agent_to_decline_the_retry() {
    exercise(Outcome::DeclineRetry).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn denying_a_redirected_host_contacts_neither_it_nor_a_vendor() {
    exercise(Outcome::Denied).await;
}

async fn exercise(outcome: Outcome) {
    let mut supervisor = Supervisor::new();
    let vendors = Vendors::start().await;
    let scene = vendors.scenario(&mut supervisor).await;
    let pages = Pages::serve().await;
    let (network, dns) = pages.network();
    let (model, transcript) = model(outcome);
    let home = tempfile::tempdir().expect("runner home");
    let sandbox = tempfile::tempdir_in("/tmp").expect("sandbox directory");
    let stop = CancellationToken::new();
    let running = tokio::spawn(runner(
        &scene,
        home.path(),
        sandbox.path(),
        network,
        (model, None),
        stop.clone(),
    ));
    let waiting = settled(&scene, &scene.turn_id).await;
    assert_eq!(
        waiting.as_deref(),
        Some("waiting"),
        "model requests: {:?}",
        transcript.asked()
    );
    pages.assert_phase(&dns, 1, 0);
    vendors.assert_idle(&scene).await;
    let resumed = decision::answer(&scene, outcome).await;
    let status = settled(&scene, &resumed).await;
    let asked = transcript.asked();
    assert_eq!(status.as_deref(), Some("processed"), "{asked:?}");
    script::assert_continuation(&asked, outcome);
    pages.assert_phase(
        &dns,
        usize::from(outcome == Outcome::Retry),
        usize::from(outcome == Outcome::Retry),
    );
    vendors.assert_idle(&scene).await;
    assert_thread(&scene, &resumed, outcome).await;
    stop.cancel();
    running.await.expect("runner joins").expect("runner stops");
    supervisor.shutdown().await;
    scene.cleanup().await;
}

async fn assert_thread(scene: &Scenario, resumed: &Uuid7, outcome: Outcome) {
    let token = mint_tenant_token();
    seed_tenant_key(&scene.booted, &scene.tenant, &token, scene.seeded_at).await;
    let client = afd_identity::direct(std::time::Duration::from_secs(10))
        .build()
        .expect("tenant client");
    let base = format!(
        "/v1/workspaces/{}/fleets/{}/turns",
        scene.workspace, scene.fleet
    );
    let original = tenant_get(&client, scene, &token, &format!("{base}/{}", scene.turn_id)).await;
    let continued = tenant_get(&client, scene, &token, &format!("{base}/{resumed}")).await;
    assert_eq!(continued["resumes_turn_id"], scene.turn_id.as_str());
    assert_eq!(continued["response_text"], outcome.answer());
    assert!(
        original["tool_calls"]["calls"]
            .as_array()
            .is_some_and(|calls| calls
                .iter()
                .any(|call| call["name"] == "web_fetch" && call["status"] == "failed")),
        "{original}"
    );
    assert_eq!(successful_reads(&original), 0, "{original}");
    assert_eq!(
        successful_reads(&continued),
        usize::from(outcome == Outcome::Retry),
        "{continued}"
    );
    assert_eq!(
        continued.to_string().contains(SENTINEL),
        outcome == Outcome::Retry
    );
}

fn successful_reads(turn: &Value) -> usize {
    turn["tool_calls"]["calls"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|call| call["name"] == "web_fetch" && call["status"] == "succeeded")
        .count()
}
