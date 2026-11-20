//! Every refusal the schedules verbs make before a schedule is touched,
//! one family to a case.
//!
//! Booted as `integration_runner_schedules` is; after every case the fake
//! scheduler has been told nothing.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![expect(
    clippy::indexing_slicing,
    reason = "test target: a step indexes the JSON it was answered"
)]

use agentsfleetd::supervisor::Supervisor;
use reqwest::Method;
use serde_json::{Value, json};

use crate::schedules::{Assigned, WEEKLY};
use crate::verbs::FakeQStash;

/// A schedule id no fleet holds.
const UNKNOWN: &str = "0195b4ba-8d3a-7fff-8abc-ffffffffffff";

/// One refusal case: the request, then the status and code it must answer.
type Case = (Method, String, Option<Value>, u16, &'static str);

/// Boots a run, sends each of `cases(run)`, and checks every answer and
/// that the scheduler heard nothing.
async fn refuses(cases: impl FnOnce(&Assigned) -> Vec<Case>) {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let held = Assigned::boot(&mut supervisor, &qstash).await;
    for (method, path, body, status, code) in cases(&held) {
        let (answered, refusal) = held.call(method.clone(), &path, body.as_ref()).await;
        assert_eq!(
            (answered, refusal["error_code"].as_str()),
            (status, Some(code)),
            "{method} {path}: {refusal}"
        );
    }
    assert!(qstash.told().is_empty(), "no refusal reached the scheduler");
    held.finish(supervisor).await;
}

/// A run the path does not name properly, or that is not this runner's.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_schedules_refuse_a_run_that_is_not_held() {
    Box::pin(refuses(|held| {
        vec![
            (
                Method::GET,
                "/v1/runners/me/runs/not-a-run/schedules?fencing_token=1".to_owned(),
                None,
                400,
                "UZ-REQ-001",
            ),
            (
                Method::GET,
                format!("/v1/runners/me/runs/{UNKNOWN}/schedules?fencing_token=1"),
                None,
                404,
                "UZ-RUN-006",
            ),
            (Method::GET, held.path(""), None, 400, "UZ-REQ-001"),
        ]
    }))
    .await;
}

/// A body this verb cannot read, or one naming a field it does not take.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_schedules_refuse_a_body_they_do_not_take() {
    Box::pin(refuses(|held| {
        let fence = held.fence;
        vec![
            (
                Method::POST,
                held.path(""),
                Some(json!({"fencing_token": fence, "cron": "* * * * * *", "message": "m"})),
                400,
                "UZ-REQ-001",
            ),
            (
                Method::POST,
                held.path(""),
                Some(
                    json!({"fencing_token": fence, "cron": WEEKLY, "message": "m",
                            "fleet_id": "x"}),
                ),
                400,
                "UZ-REQ-001",
            ),
            (
                Method::PATCH,
                held.path(&format!("/{UNKNOWN}")),
                Some(json!({"fencing_token": fence, "timezone": "Mars/Olympus"})),
                400,
                "UZ-REQ-001",
            ),
            (
                Method::PATCH,
                held.path(&format!("/{UNKNOWN}")),
                Some(json!({"fencing_token": fence, "desired_status": "deleting"})),
                400,
                "UZ-REQ-001",
            ),
            (
                Method::POST,
                held.path(&format!("/{UNKNOWN}/runs")),
                Some(json!({"fencing_token": fence, "now": true})),
                400,
                "UZ-REQ-001",
            ),
        ]
    }))
    .await;
}

/// A schedule the path does not name properly, or that the fleet does not
/// hold.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_schedules_refuse_a_schedule_that_is_not_held() {
    Box::pin(refuses(|held| {
        let fence = json!({"fencing_token": held.fence});
        vec![
            (
                Method::PATCH,
                held.path(&format!("/{UNKNOWN}")),
                Some(fence.clone()),
                404,
                "UZ-SCHED-002",
            ),
            (
                Method::PATCH,
                held.path("/not-a-schedule"),
                Some(fence.clone()),
                400,
                "UZ-REQ-001",
            ),
            (
                Method::POST,
                held.path(&format!("/{UNKNOWN}/runs")),
                Some(fence.clone()),
                404,
                "UZ-SCHED-002",
            ),
            (
                Method::POST,
                held.path("/not-a-schedule/runs"),
                Some(fence),
                400,
                "UZ-REQ-001",
            ),
            (
                Method::GET,
                held.fenced(&format!("/{UNKNOWN}/runs")),
                None,
                404,
                "UZ-SCHED-002",
            ),
        ]
    }))
    .await;
}

/// A runs page asked for with a size or a cursor this daemon does not serve.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_schedule_runs_refuse_a_page_they_do_not_serve() {
    Box::pin(refuses(|held| {
        let runs = held.fenced(&format!("/{UNKNOWN}/runs"));
        vec![
            (
                Method::GET,
                format!("{runs}&limit=0"),
                None,
                400,
                "UZ-REQ-001",
            ),
            (
                Method::GET,
                format!("{runs}&starting_after=@@"),
                None,
                400,
                "UZ-REQ-001",
            ),
        ]
    }))
    .await;
}
