//! Carrier changes, terminal ownership and explicit proxy routing use the production downlink.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

mod connect;
mod relay;
mod selection;
mod supersede;

use crate::e2e::Scenario;
use af_wire::runner::HeartbeatRequest;
use afr_supervisor::test_util::Session;

async fn session(run: &Scenario, origin: &str, proxy: Option<&str>) -> Session {
    use afr_supervisor::config::{ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STREAM_CA, ENV_STREAM_URL};
    let root = crate::support::runner_stream::root_file()
        .to_string_lossy()
        .into_owned();
    let pairs = [
        (ENV_API_URL, run.base.as_str()),
        (ENV_RUNNER_TOKEN, run.token.as_str()),
        (ENV_STREAM_URL, origin),
        (ENV_STREAM_CA, root.as_str()),
    ]
    .into_iter()
    .chain(
        proxy
            .into_iter()
            .flat_map(|url| [("HTTPS_PROXY", url), ("NO_PROXY", "")]),
    );
    let env = af_core::env::MapEnv::from_pairs(pairs);
    let config = afr_supervisor::Config::from_env(&env).expect("native lifecycle configuration");
    Session::connect(&config)
        .await
        .expect("production native downlink connects")
}

async fn heartbeat(session: &Session) -> afr_supervisor::Result<serde_json::Value> {
    let bytes = serde_json::to_vec(&crate::wire::capable_beat()).expect("heartbeat encodes");
    let beat: HeartbeatRequest<'_> =
        serde_json::from_slice(&bytes).expect("current heartbeat shape");
    session.heartbeat(&beat).await
}

async fn owner(run: &Scenario) -> af_core::id::Uuid7 {
    sqlx::query_scalar("SELECT connection_id FROM runner.runners WHERE id=$1")
        .bind(&run.runner_id)
        .fetch_one(&mut *run.booted.database.acquire().await.expect("owner observer"))
        .await
        .expect("the session has a durable owner")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn the_stream_connects_through_an_environment_proxy_without_a_direct_http3_attempt() {
    let mut supervisor = agentsfleetd::supervisor::Supervisor::new();
    let run = crate::e2e::scenario(&mut supervisor).await;
    let direct =
        crate::integration_runner_stream::raw::Native::connect(&run, af_stream::Transport::H3)
            .await;
    direct.close().await;
    let relay = relay::Relay::start(run.booted.runner_stream_address, true).await;
    let proxy = connect::Proxy::start(relay.address).await;
    // The proxy tunnels only to its backend's literal address, so the origin names it the same way.
    let origin = format!("https://[::1]:{}", relay.address.port());
    let mut runner = session(&run, &origin, Some(&proxy.url)).await;
    let reply = heartbeat(&runner)
        .await
        .expect("native heartbeat through CONNECT");
    assert!(reply.get("assigned_policy").is_some());
    assert_eq!(
        proxy.tunnels(),
        1,
        "framed native work used the required proxy even with direct UDP reachable"
    );
    assert_eq!(
        relay.connections(),
        (1, 0),
        "the origin saw one TCP path through the proxy and no direct HTTP/3 path"
    );
    runner.close().await.expect("downlink joins");
    proxy.close().await;
    relay.close().await;
    supervisor.shutdown().await;
    run.cleanup().await;
}
