//! A lost native acknowledgement retries the same durable report rather than issuing more work.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use af_core::timing::STREAM_ANSWER_TIMEOUT;
use af_stream::Transport;
use af_wire::protocol::StreamKind;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use super::raw::Native;
use crate::{e2e::scenario, wire};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_report_resent_after_a_native_cut_is_charged_once() {
    for transport in [Transport::H2, Transport::H3] {
        let mut supervisor = Supervisor::new();
        let run = scenario(&mut supervisor).await;
        let mut first = Native::connect(&run, transport).await;
        first.heartbeat().await;
        first.ready().await;
        let offer = first.expect(StreamKind::Work, "offer").await;
        let lease = offer.pointer("/lease").expect("native response field");
        let (id, fence) = wire::claim(lease);
        assert_eq!(
            (*crate::wire::pointer(lease, "/event/event_id")),
            run.event_id.as_str()
        );
        let body = wire::report_body(&id, &run.event_id, fence);
        first
            .send(
                StreamKind::Reports,
                &json!({"type":"report", "request_id":4, "request":body}),
            )
            .await;
        wait_for_settlement(&run, &id).await;
        let original = first.expect(StreamKind::Reports, "report_ack").await;
        first.close().await;
        let mut second = Native::connect(&run, transport).await;
        second
            .send(
                StreamKind::Reports,
                &json!({"type":"report", "request_id":5, "request":body}),
            )
            .await;
        let replay = second.expect(StreamKind::Reports, "report_ack").await;
        assert_eq!(
            (*crate::wire::pointer(&replay, "/answer")),
            json!({"status":"ok", "response":{"ok":true}})
        );
        assert_eq!(
            crate::wire::pointer(&replay, "/answer"),
            crate::wire::pointer(&original, "/answer"),
            "{transport:?}: the resent report answers with the saved result"
        );
        crate::reads::relay_settled(&run).await;
        let counts: (i64, i64) = sqlx::query_as("SELECT count(*) FILTER(WHERE seq=1), count(*) FILTER(WHERE settled_at IS NULL) FROM billing.usage_lines WHERE event_id=$1")
            .bind(&run.event_id)
            .fetch_one(&mut *run.booted.database.acquire().await.expect("receipt observer"))
            .await.expect("read scoped receipt counts");
        assert_eq!(counts, (1, 0));
        second.close().await;
        supervisor.shutdown().await;
        run.cleanup().await;
    }
}

async fn wait_for_settlement(run: &crate::e2e::Scenario, id: &str) {
    tokio::time::timeout(STREAM_ANSWER_TIMEOUT, async {
        loop {
            let committed: bool =
                sqlx::query_scalar("SELECT status = 'reported' FROM runner.leases WHERE id=$1")
                    .bind(af_core::id::Uuid7::parse(id).expect("issued lease identity"))
                    .fetch_one(
                        &mut *run
                            .booted
                            .database
                            .acquire()
                            .await
                            .expect("settlement observer"),
                    )
                    .await
                    .expect("durable settlement read");
            if committed {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("report commits before acknowledgement is read");
}
