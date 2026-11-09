//! Heartbeat policy, hold release and liveness use the authenticated native session.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "integration preconditions fail loudly")]

use af_wire::runner::HeartbeatRequest;
use agentsfleetd::supervisor::Supervisor;
use jiff_sqlx::ToSqlx as _;
use serde_json::json;

use crate::{e2e::scenario, wire::capable_beat};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn heartbeat_reply_names_the_holds_to_release() {
    let mut supervisor = Supervisor::new();
    let mut run = scenario(&mut supervisor).await;
    let never_held = afd_db::test_util::mint_id();
    let mut request = capable_beat();
    request
        .as_object_mut()
        .expect("a heartbeat object")
        .insert("holds".into(), json!([never_held]));
    let bytes = serde_json::to_vec(&request).expect("a heartbeat encodes");
    let request: HeartbeatRequest<'_> = serde_json::from_slice(&bytes).expect("a current beat");
    let reply = run
        .native_session()
        .await
        .heartbeat(&request)
        .await
        .expect("native heartbeat");
    assert_eq!(reply.get("release_holds"), Some(&json!([never_held])));
    run.native
        .get_mut()
        .expect("the session opened")
        .close()
        .await
        .expect("native shutdown");
    supervisor.shutdown().await;
    run.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_unreadable_assignment_is_refused_on_the_native_beat_and_self_read() {
    let mut supervisor = Supervisor::new();
    let mut run = scenario(&mut supervisor).await;
    let native = run.native_session().await;
    let mut connection = run
        .booted
        .database
        .acquire()
        .await
        .expect("a fixture connection");
    let tier: String = sqlx::query_scalar("SELECT sandbox_tier FROM runner.runners WHERE id = $1")
        .bind(&run.runner_id)
        .fetch_one(&mut *connection)
        .await
        .expect("the enrolled tier");
    sqlx::query("UPDATE runner.runners SET sandbox_tier = $2 WHERE id = $1")
        .bind(&run.runner_id)
        .bind("not-a-tier")
        .execute(&mut *connection)
        .await
        .expect("corrupt only this runner");
    drop(connection);
    let bytes = serde_json::to_vec(&capable_beat()).expect("a heartbeat encodes");
    let request: HeartbeatRequest<'_> = serde_json::from_slice(&bytes).expect("a current beat");
    let failure = native
        .heartbeat(&request)
        .await
        .expect_err("a guessed policy is unsafe");
    assert_eq!(
        failure.refusal_code(),
        Some(af_core::error_code::INTERNAL_DB_QUERY)
    );
    let http = afd_identity::direct(af_core::timing::STREAM_ANSWER_TIMEOUT)
        .build()
        .expect("a loopback client");
    let me = crate::wire::get(&http, &run, "/v1/runners/me").await;
    assert_eq!(me.status(), reqwest::StatusCode::INTERNAL_SERVER_ERROR);
    let reply: serde_json::Value = me.json().await.expect("a problem response");
    assert_eq!(
        reply.get("error_code"),
        Some(&json!(af_core::error_code::INTERNAL_DB_QUERY.as_str()))
    );
    sqlx::query("UPDATE runner.runners SET sandbox_tier = $2 WHERE id = $1")
        .bind(&run.runner_id)
        .bind(tier)
        .execute(
            &mut *run
                .booted
                .database
                .acquire()
                .await
                .expect("restore connection"),
        )
        .await
        .expect("restore the runner before platform reads");
    run.native
        .get_mut()
        .expect("the session opened")
        .close()
        .await
        .expect("native shutdown");
    supervisor.shutdown().await;
    run.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_native_heartbeat_records_liveness_without_waiting_for_work() {
    let mut supervisor = Supervisor::new();
    let mut run = scenario(&mut supervisor).await;
    let native = run.native_session().await;
    sqlx::query("UPDATE runner.runners SET last_seen_at = $2 WHERE id = $1")
        .bind(&run.runner_id)
        .bind(jiff::Timestamp::UNIX_EPOCH.to_sqlx())
        .execute(
            &mut *run
                .booted
                .database
                .acquire()
                .await
                .expect("fixture connection"),
        )
        .await
        .expect("an independently old liveness marker");
    let bytes = serde_json::to_vec(&capable_beat()).expect("a heartbeat encodes");
    let request: HeartbeatRequest<'_> = serde_json::from_slice(&bytes).expect("a current beat");
    let reply = tokio::time::timeout(
        af_core::timing::STREAM_ANSWER_TIMEOUT,
        native.heartbeat(&request),
    )
    .await
    .expect("heartbeat does not hold for work")
    .expect("native heartbeat");
    let recorded: bool =
        sqlx::query_scalar("SELECT last_seen_at > $2 FROM runner.runners WHERE id = $1")
            .bind(&run.runner_id)
            .bind(jiff::Timestamp::UNIX_EPOCH.to_sqlx())
            .fetch_one(
                &mut *run
                    .booted
                    .database
                    .acquire()
                    .await
                    .expect("reading connection"),
            )
            .await
            .expect("the runner remains enrolled");
    assert!(recorded);
    assert!(reply.get("woken").is_none());
    assert!(reply.get("assigned_policy").is_some());
    run.native
        .get_mut()
        .expect("the session opened")
        .close()
        .await
        .expect("native shutdown");
    supervisor.shutdown().await;
    run.cleanup().await;
}
