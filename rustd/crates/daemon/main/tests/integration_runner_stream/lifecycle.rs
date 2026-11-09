//! A native carrier's application traffic cannot replace liveness or durable authority.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use af_core::timing::{HEARTBEAT_INTERVAL, STREAM_ANSWER_TIMEOUT};
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use tokio::time::Instant;

use super::raw::Native;
use crate::e2e::scenario;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn application_pings_do_not_keep_a_heartbeat_silent_native_session_alive() {
    for transport in [Transport::H2, Transport::H3] {
        let mut supervisor = Supervisor::new();
        let run = scenario(&mut supervisor).await;
        let mut native = Native::connect(&run, transport).await;
        let last_heartbeat = Instant::now();
        native.heartbeat().await;
        for request_id in 4..=6 {
            tokio::time::sleep(HEARTBEAT_INTERVAL / 2).await;
            native
                .send(
                    StreamKind::Control,
                    &json!({"type":"ping", "request_id":request_id}),
                )
                .await;
            assert_eq!(
                (*native
                    .expect(StreamKind::Control, "pong")
                    .await
                    .pointer("/request_id")
                    .expect("native response field")),
                request_id
            );
        }
        assert!(last_heartbeat.elapsed() < HEARTBEAT_INTERVAL * 2);
        native.ended(HEARTBEAT_INTERVAL).await;
        assert!(last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL * 2);
        native.close().await;
        supervisor.shutdown().await;
        run.cleanup().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_second_native_connection_supersedes_the_first_on_each_transport() {
    for transport in [Transport::H2, Transport::H3] {
        let mut supervisor = Supervisor::new();
        let run = scenario(&mut supervisor).await;
        let mut first = Native::connect(&run, transport).await;
        let mut second = Native::connect(&run, transport).await;
        let previous = first.owner.clone();
        assert_ne!(previous, second.owner);
        let away = first.expect(StreamKind::Control, "go_away").await;
        assert_eq!(
            (*crate::wire::pointer(&away, "/error_code")),
            af_core::error_code::RUN_STREAM_SUPERSEDED.as_str()
        );
        let owner: af_core::id::Uuid7 =
            sqlx::query_scalar("SELECT connection_id FROM runner.runners WHERE id=$1")
                .bind(&run.runner_id)
                .fetch_one(
                    &mut *run
                        .booted
                        .database
                        .acquire()
                        .await
                        .expect("owner read connection"),
                )
                .await
                .expect("durable owner");
        assert_eq!(owner.as_str(), second.owner);
        second.heartbeat().await;
        second.close().await;
        first.close().await;
        supervisor.shutdown().await;
        run.cleanup().await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_durable_revocation_closes_at_the_next_native_heartbeat_without_a_push() {
    for transport in [Transport::H2, Transport::H3] {
        let mut supervisor = Supervisor::new();
        let run = scenario(&mut supervisor).await;
        let mut native = Native::connect(&run, transport).await;
        native.heartbeat().await;
        sqlx::query("UPDATE runner.runners SET status=$2 WHERE id=$1")
            .bind(&run.runner_id)
            .bind("revoked")
            .execute(
                &mut *run
                    .booted
                    .database
                    .acquire()
                    .await
                    .expect("revocation connection"),
            )
            .await
            .expect("commit revocation without a bus publish");
        native
            .send(
                StreamKind::Control,
                &json!({
                    "type":"heartbeat", "request_id":4, "request":crate::wire::capable_beat()
                }),
            )
            .await;
        let reply = native.expect(StreamKind::Control, "heartbeat").await;
        assert_eq!((*crate::wire::pointer(&reply, "/answer/status")), "refused");
        assert_eq!(
            (*crate::wire::pointer(&reply, "/answer/error_code")),
            af_core::error_code::RUN_ADMIN_STATE_BLOCKED.as_str()
        );
        assert_eq!((*crate::wire::pointer(&reply, "/answer/retryable")), false);
        let away = native.expect(StreamKind::Control, "go_away").await;
        assert_eq!(
            (*crate::wire::pointer(&away, "/error_code")),
            af_core::error_code::RUN_ADMIN_STATE_BLOCKED.as_str()
        );
        native.ended(STREAM_ANSWER_TIMEOUT).await;
        native.close().await;
        supervisor.shutdown().await;
        run.cleanup().await;
    }
}
