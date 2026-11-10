//! Renewal outcomes and folds preserve their complete native answers and durable fences.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use af_stream::Transport;
use af_wire::protocol::StreamKind;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use super::raw::Native;
use crate::{e2e::scenario, wire};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn native_renewal_keeps_usage_acknowledgements_and_independent_refusals() {
    for transport in [Transport::H2, Transport::H3] {
        let mut supervisor = Supervisor::new();
        let run = scenario(&mut supervisor).await;
        let mut native = Native::connect(&run, transport).await;
        native.heartbeat().await;
        native.ready().await;
        let offer = native.expect(StreamKind::Work, "offer").await;
        let (lease, fence) = wire::claim(offer.pointer("/lease").expect("native response field"));
        let report = wire::report_body(&lease, &run.event_id, fence);
        mixed_renewal(&mut native, &lease, &report).await;
        empty_folds(&mut native, &lease, fence).await;
        cancellation(&run, &mut native, &lease).await;
        final_report(&run, &mut native, &report).await;
        native.close().await;
        supervisor.shutdown().await;
        run.cleanup().await;
    }
}

async fn mixed_renewal(native: &mut Native, lease: &str, report: &serde_json::Value) {
    let missing = afd_db::test_util::mint_id();
    native
        .send(
            StreamKind::Control,
            &json!({
                "type":"renew_batch", "request_id":4,
                "leases":[
                    {"lease_id":lease,"request":{"requests":(*crate::wire::pointer(report, "/requests"))}},
                    {"lease_id":missing,"request":{"requests":[]}}
                ]
            }),
        )
        .await;
    let renewed = native.expect(StreamKind::Control, "renewed").await;
    let answers = (*crate::wire::pointer(&renewed, "/leases"))
        .as_array()
        .expect("one answer per renewal");
    assert_eq!(answers.len(), 2);
    let accepted = answers
        .iter()
        .find(|answer| (*crate::wire::pointer(answer, "/lease_id")) == lease)
        .expect("live lease answer");
    assert_eq!((*crate::wire::pointer(accepted, "/answer/status")), "ok");
    assert_eq!(
        (*crate::wire::pointer(accepted, "/answer/response/recorded_through")),
        1
    );
    assert!((*crate::wire::pointer(accepted, "/answer/response/lease_expires_at")).is_string());
    let refused = answers
        .iter()
        .find(|answer| (*crate::wire::pointer(answer, "/lease_id")) == missing.as_str())
        .expect("missing lease answer");
    assert_eq!(
        (*crate::wire::pointer(refused, "/answer/status")),
        "refused"
    );
    assert_eq!(
        (*crate::wire::pointer(refused, "/answer/error_code")),
        af_core::error_code::RUN_LEASE_NOT_FOUND.as_str()
    );
    assert_eq!((*crate::wire::pointer(refused, "/answer/retryable")), false);
}

async fn empty_folds(native: &mut Native, lease: &str, fence: u64) {
    native
        .send(
            StreamKind::Work,
            &json!({
                "type":"folds", "request_id":5, "lease_id":lease,
                "request":{"fencing_token":fence,"after_sequence":0}
            }),
        )
        .await;
    let folded = native.expect(StreamKind::Work, "folded").await;
    assert_eq!(
        (*crate::wire::pointer(&folded, "/answer")),
        json!({"status":"ok", "response":{"inputs":[]}})
    );
}

async fn cancellation(run: &crate::e2e::Scenario, native: &mut Native, lease: &str) {
    sqlx::query("UPDATE runner.leases SET cancel_requested_at=now() WHERE id=$1")
        .bind(af_core::id::Uuid7::parse(lease).expect("issued UUIDv7"))
        .execute(
            &mut *run
                .booted
                .database
                .acquire()
                .await
                .expect("cancellation connection"),
        )
        .await
        .expect("durable cancellation without a bus publish");
    native
        .send(
            StreamKind::Control,
            &json!({
                "type":"renew_batch", "request_id":6,
                "leases":[{"lease_id":lease,"request":{"requests":[]}}]
            }),
        )
        .await;
    let cancelled = native.expect(StreamKind::Control, "renewed").await;
    assert_eq!(
        (*crate::wire::pointer(&cancelled, "/leases/0/answer/error_code")),
        af_core::error_code::RUN_LEASE_CANCELLED.as_str()
    );
    assert_eq!(
        (*crate::wire::pointer(&cancelled, "/leases/0/answer/retryable")),
        false
    );
}

async fn final_report(run: &crate::e2e::Scenario, native: &mut Native, report: &serde_json::Value) {
    native
        .send(
            StreamKind::Reports,
            &json!({"type":"report", "request_id":7, "request":report}),
        )
        .await;
    let settled = native.expect(StreamKind::Reports, "report_ack").await;
    assert_eq!((*crate::wire::pointer(&settled, "/answer/status")), "ok");
    let lines: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing.usage_lines WHERE event_id=$1 AND seq=1")
            .bind(&run.event_id)
            .fetch_one(&mut *run.booted.database.acquire().await.expect("usage observer"))
            .await
            .expect("read usage after renewal and final report");
    assert_eq!(
        lines, 1,
        "the acknowledged provider request is not charged twice"
    );
}
