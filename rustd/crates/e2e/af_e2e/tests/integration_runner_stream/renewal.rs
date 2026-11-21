//! Renewal outcomes and folds preserve their complete native answers and durable fences.
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use af_stream::Transport;
use af_wire::protocol::StreamKind;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use super::raw::Native;
use crate::{e2e::scenario, wire};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn native_renewal_keeps_usage_acknowledgements_and_independent_refusals() {
    for transport in [Transport::H2, Transport::H3] {
        let mut supervisor = Supervisor::new();
        let scene = scenario(&mut supervisor).await;
        let mut native = Native::connect(&scene, transport).await;
        native.heartbeat().await;
        native.ready().await;
        let assignment = native.expect(StreamKind::Work, "run").await;
        let (run, fence) =
            wire::run_and_fence(assignment.pointer("/run").expect("native response field"));
        let report = wire::report_body(&run, &scene.turn_id, fence);
        mixed_renewal(&mut native, &run, &report).await;
        empty_folds(&mut native, &run, fence).await;
        cancellation(&scene, &mut native, &run).await;
        final_report(&scene, &mut native, &report).await;
        native.close().await;
        supervisor.shutdown().await;
        scene.cleanup().await;
    }
}

async fn mixed_renewal(native: &mut Native, run: &str, report: &serde_json::Value) {
    let missing = afd_db::test_util::mint_id();
    native
        .send(
            StreamKind::Control,
            &json!({
                "type":"renew_batch", "request_id":4,
                "runs":[
                    {"run_id":run,"request":{"requests":(*wire::pointer(report, "/requests"))}},
                    {"run_id":missing,"request":{"requests":[]}}
                ]
            }),
        )
        .await;
    let renewed = native.expect(StreamKind::Control, "renewed").await;
    let answers = (*wire::pointer(&renewed, "/runs"))
        .as_array()
        .expect("one answer per renewal");
    assert_eq!(answers.len(), 2);
    let accepted = answers
        .iter()
        .find(|answer| (*wire::pointer(answer, "/run_id")) == run)
        .expect("live run answer");
    assert_eq!((*wire::pointer(accepted, "/answer/status")), "ok");
    assert_eq!(
        (*wire::pointer(accepted, "/answer/response/recorded_through")),
        1
    );
    assert!((*wire::pointer(accepted, "/answer/response/run_expires_at")).is_string());
    let refused = answers
        .iter()
        .find(|answer| (*wire::pointer(answer, "/run_id")) == missing.as_str())
        .expect("missing run answer");
    assert_eq!((*wire::pointer(refused, "/answer/status")), "refused");
    assert_eq!(
        (*wire::pointer(refused, "/answer/error_code")),
        af_core::error_code::RUN_NOT_FOUND.as_str()
    );
    assert_eq!((*wire::pointer(refused, "/answer/retryable")), false);
}

async fn empty_folds(native: &mut Native, run: &str, fence: u64) {
    native
        .send(
            StreamKind::Work,
            &json!({
                "type":"folds", "request_id":5, "run_id":run,
                "request":{"fencing_token":fence,"after_sequence":0}
            }),
        )
        .await;
    let folded = native.expect(StreamKind::Work, "folded").await;
    assert_eq!(
        (*wire::pointer(&folded, "/answer")),
        json!({"status":"ok", "response":{"inputs":[]}})
    );
}

async fn cancellation(scene: &crate::e2e::Scenario, native: &mut Native, run: &str) {
    sqlx::query("UPDATE thread.runs SET cancel_requested_at=now() WHERE id=$1")
        .bind(af_core::id::Uuid7::parse(run).expect("issued UUIDv7"))
        .execute(
            &mut *scene
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
                "runs":[{"run_id":run,"request":{"requests":[]}}]
            }),
        )
        .await;
    let cancelled = native.expect(StreamKind::Control, "renewed").await;
    assert_eq!(
        (*wire::pointer(&cancelled, "/runs/0/answer/error_code")),
        af_core::error_code::RUN_CANCELLED.as_str()
    );
    assert_eq!(
        (*wire::pointer(&cancelled, "/runs/0/answer/retryable")),
        false
    );
}

async fn final_report(
    scene: &crate::e2e::Scenario,
    native: &mut Native,
    report: &serde_json::Value,
) {
    native
        .send(
            StreamKind::Reports,
            &json!({"type":"report", "request_id":7, "request":report}),
        )
        .await;
    let settled = native.expect(StreamKind::Reports, "report_ack").await;
    assert_eq!((*wire::pointer(&settled, "/answer/status")), "ok");
    let lines: i64 =
        sqlx::query_scalar("SELECT count(*) FROM billing.usage_lines WHERE turn_id=$1 AND seq=1")
            .bind(&scene.turn_id)
            .fetch_one(
                &mut *scene
                    .booted
                    .database
                    .acquire()
                    .await
                    .expect("usage observer"),
            )
            .await
            .expect("read usage after renewal and final report");
    assert_eq!(
        lines, 1,
        "the acknowledged provider request is not charged twice"
    );
}
