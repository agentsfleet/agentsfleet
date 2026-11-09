//! A coded ownership loss terminates the production downlink without a physical reconnect.

use super::{heartbeat, owner, relay::Relay, session};
use crate::integration_runner_stream::raw::Native;
use af_core::{
    error_code::RUN_STREAM_SUPERSEDED,
    timing::{RECONNECT_JITTER_MAX, STREAM_RECOVERY_BUDGET},
};
use af_stream::Transport;
use agentsfleetd::supervisor::Supervisor;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_superseded_runner_stops_without_opening_another_connection() {
    for transport in [Transport::H2, Transport::H3] {
        superseded(transport).await;
    }
}

async fn superseded(transport: Transport) {
    let mut supervisor = Supervisor::new();
    let run = crate::e2e::scenario(&mut supervisor).await;
    let relay = Relay::start(run.booted.runner_stream_address, transport == Transport::H3).await;
    let mut first = session(&run, &relay.origin, None).await;
    heartbeat(&first).await.expect("first owner heartbeat");
    let initial = relay.connections();
    assert_eq!(
        initial,
        if transport == Transport::H2 {
            (1, 0)
        } else {
            (0, 1)
        }
    );
    let mut replacement = Native::connect(&run, Transport::H3).await;
    replacement.heartbeat().await;
    let current = owner(&run).await;
    assert_eq!(current.as_str(), replacement.owner);
    tokio::time::sleep(RECONNECT_JITTER_MAX + STREAM_RECOVERY_BUDGET).await;
    assert_eq!(
        relay.connections(),
        initial,
        "supersession must not start even a refused replacement connection"
    );
    assert_eq!(owner(&run).await, current);
    let failure = first
        .close()
        .await
        .expect_err("coded supersession stops the downlink");
    assert_eq!(failure.refusal_code(), Some(RUN_STREAM_SUPERSEDED));
    replacement.close().await;
    relay.close().await;
    supervisor.shutdown().await;
    run.cleanup().await;
}
