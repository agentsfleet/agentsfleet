//! Drain must answer an accepted native renewal before announcing the final connection close.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use agentsfleetd::serve::drain::DRAIN_TIMEOUT;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;
use sqlx::{Postgres, Transaction};

use super::raw::Native;
use crate::{
    e2e::{Scenario, scenario},
    wire,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn draining_sends_go_away_after_an_in_flight_native_renewal_answer() {
    for transport in [Transport::H2, Transport::H3] {
        let mut supervisor = Supervisor::new();
        let run = scenario(&mut supervisor).await;
        let mut native = Native::connect(&run, transport).await;
        native.heartbeat().await;
        native.ready().await;
        let offer = native.expect(StreamKind::Work, "offer").await;
        let (lease, _) = wire::claim(offer.pointer("/lease").expect("native response field"));
        let (host, blocker) = hold_host(&run).await;
        native
            .send(
                StreamKind::Control,
                &json!({
                    "type":"renew_batch", "request_id":4,
                    "leases":[{"lease_id":lease,"request":{"requests":[]}}]
                }),
            )
            .await;
        wait_for_host_wait(&run, blocker).await;
        let drain = run.booted.drain.clone();
        let settled = tokio::spawn(async move { drain.settle(DRAIN_TIMEOUT).await });
        run.booted.drain.accepting().cancelled().await;
        host.commit()
            .await
            .expect("release accepted renewal during drain");
        let renewed = native.expect(StreamKind::Control, "renewed").await;
        assert_eq!((*crate::wire::pointer(&renewed, "/request_id")), 4);
        assert_eq!(
            (*crate::wire::pointer(&renewed, "/leases/0/answer/status")),
            "ok"
        );
        assert!(
            renewed
                .pointer("/leases/0/answer/response/recorded_through")
                .is_none()
        );
        let away = native.expect(StreamKind::Control, "go_away").await;
        assert_eq!(
            (*crate::wire::pointer(&away, "/error_code")),
            serde_json::Value::Null
        );
        native.ended(STREAM_ANSWER_TIMEOUT).await;
        native.close().await;
        let drained = settled.await.expect("drain task joins");
        assert!(drained.is_clean(), "native drain: {drained:?}");
        supervisor.shutdown().await;
        run.cleanup().await;
    }
}

async fn hold_host(run: &Scenario) -> (Transaction<'static, Postgres>, i32) {
    let mut host = Transaction::begin(
        run.booted
            .database
            .acquire()
            .await
            .expect("host connection"),
        None,
    )
    .await
    .expect("host transaction");
    let blocker = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *host)
        .await
        .expect("owned blocker identity");
    let held: af_core::id::Uuid7 =
        sqlx::query_scalar("SELECT id FROM runner.runners WHERE id=$1 FOR UPDATE")
            .bind(&run.runner_id)
            .fetch_one(&mut *host)
            .await
            .expect("hold enrolled host");
    assert_eq!(held, run.runner_id);
    (host, blocker)
}

async fn wait_for_host_wait(run: &Scenario, blocker: i32) {
    tokio::time::timeout(STREAM_ANSWER_TIMEOUT, async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(blocker)
            .fetch_one(&mut *run.booted.database.acquire().await.expect("lock observer"))
            .await
            .expect("observe this host's real wait");
            if waiting {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("renewal reaches the held host lock");
}
