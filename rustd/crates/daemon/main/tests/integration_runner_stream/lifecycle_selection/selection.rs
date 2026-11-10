//! Restored UDP is used only after a healthy HTTP/2 session ends normally.

use super::{heartbeat, owner, relay::Relay, session};
use af_core::timing::{
    HEARTBEAT_INTERVAL, RECONNECT_JITTER_MAX, STREAM_RECOVERY_BUDGET, STREAM_STABLE_RESET_AFTER,
};
use afr_supervisor::test_util::Session;
use agentsfleetd::supervisor::Supervisor;
use tokio::time::Instant;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn http3_is_reselected_only_when_the_http2_session_ends() {
    let mut supervisor = Supervisor::new();
    let run = crate::e2e::scenario(&mut supervisor).await;
    let relay = Relay::start(run.booted.runner_stream_address, false).await;
    let mut runner = session(&run, &relay.origin, None).await;
    heartbeat(&runner).await.expect("initial HTTP/2 heartbeat");
    let first = owner(&run).await;
    assert_eq!(relay.connections(), (1, 0));
    // Let the expired QUIC attempt finish cleanup before opening the UDP path.
    tokio::time::sleep(HEARTBEAT_INTERVAL / 2).await;
    relay.enable_udp();
    hold_healthy_http2(&run, &runner, &relay, &first).await;
    relay.end_tcp();
    reconnect_heartbeat(&run, &runner, &first).await;
    assert_ne!(owner(&run).await, first);
    assert_eq!(
        relay.connections(),
        (1, 1),
        "the next session selected HTTP/3 without another TCP connection"
    );
    runner.close().await.expect("downlink joins");
    relay.close().await;
    supervisor.shutdown().await;
    run.cleanup().await;
}

async fn hold_healthy_http2(
    run: &crate::e2e::Scenario,
    runner: &Session,
    relay: &Relay,
    first: &af_core::id::Uuid7,
) {
    let began = Instant::now();
    while began.elapsed() < STREAM_STABLE_RESET_AFTER {
        tokio::time::sleep(HEARTBEAT_INTERVAL / 2).await;
        heartbeat(runner)
            .await
            .expect("healthy HTTP/2 session continues after UDP returns");
        assert_eq!(&owner(run).await, first);
        assert_eq!(
            relay.connections(),
            (1, 0),
            "a healthy session cannot hot-switch or launch an upgrade probe"
        );
    }
}

#[expect(
    clippy::panic,
    reason = "a terminal refusal must fail this recovery test"
)]
async fn reconnect_heartbeat(
    run: &crate::e2e::Scenario,
    runner: &Session,
    first: &af_core::id::Uuid7,
) {
    tokio::time::timeout(RECONNECT_JITTER_MAX + STREAM_RECOVERY_BUDGET, async {
        loop {
            match heartbeat(runner).await {
                Ok(_) if owner(run).await != *first => return,
                Ok(_) => tokio::task::yield_now().await,
                Err(error) if error.is_retryable() => tokio::task::yield_now().await,
                Err(error) => panic!("normal connection loss became terminal: {error}"),
            }
        }
    })
    .await
    .expect("fresh HTTP/3 session recovers within its complete budget");
}
