//! A fleet's run schedules itself through its own tool: the Rust runner's
//! real loop, `guffbolt`'s `cron_add` built over the run's verbs as its
//! scheduler, against the daemon's schedules route and a fake `QStash`.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![expect(
    clippy::indexing_slicing,
    reason = "test target: a step indexes the transcript it just read"
)]

use afd_cron::Source;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::definition_run::run_event;
use crate::fake_model::{FakeModel, call, say};
use crate::https::Upstream;
use crate::schedules::{KOLKATA, WEEKLY, schedules_held, scheduling};
use crate::verbs::{FakeQStash, Told};

/// A fleet whose policy offers the run `cron_add` and nothing else.
const SCHEDULING_FLEET: &str = r#"{"name":"e2e-fleet","x-agentsfleet":{"triggers":[],"tools":["cron_add"],"budget":{"daily_nanos":"1000000000"}}}"#;

/// A fleet whose policy offers the run no tool.
const TOOLLESS_FLEET: &str = r#"{"name":"e2e-fleet","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}"#;

/// What the model answers once its call came back.
const ANSWER: &str = "Scheduled the weekly check.";

/// The model's `cron_add` call, firing on `cron`.
fn adding(cron: &str) -> Vec<guffbolt::Chunk> {
    vec![call(
        "add-1",
        "cron_add",
        json!({"cron": cron, "timezone": KOLKATA, "message": "weekly check"}),
    )]
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_fleet_runs_cron_add_creates_its_schedule() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let scene = scheduling(&mut supervisor, &qstash, SCHEDULING_FLEET).await;
    let upstream = Upstream::serve(Vec::new()).await;
    let (model, transcript) = FakeModel::new(vec![adding(WEEKLY), vec![say(ANSWER)]]);

    let settled = run_event(&scene, &scene.turn_id, &upstream, model).await;

    assert_eq!(
        (settled.status.as_str(), settled.answer.as_str()),
        ("processed", ANSWER)
    );
    let asked = transcript.asked();
    assert_eq!(asked[0].tools, ["cron_add"], "the run offered its one tool");
    let result = &asked[1].results[0];
    assert!(result.contains(r#""source":"fleet""#), "{result}");
    assert_eq!(schedules_held(&scene, Some(Source::Fleet)).await, 1);
    assert_eq!(
        schedules_held(&scene, None).await,
        1,
        "one schedule, the fleet's"
    );
    assert_eq!(qstash.told(), [Told::Upsert(WEEKLY.to_owned())]);
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// The daemon refuses a schedule it would not run; the model reads its code,
/// and nothing is kept or registered.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_schedule_the_daemon_refuses_reaches_the_model_and_keeps_nothing() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let scene = scheduling(&mut supervisor, &qstash, SCHEDULING_FLEET).await;
    let upstream = Upstream::serve(Vec::new()).await;
    let (model, transcript) = FakeModel::new(vec![adding("61 * * * *"), vec![say(ANSWER)]]);

    let settled = run_event(&scene, &scene.turn_id, &upstream, model).await;

    assert_eq!(settled.status, "processed");
    let result = &transcript.asked()[1].results[0];
    assert!(result.starts_with("[agentsfleetd_refused] UZ-"), "{result}");
    assert_eq!(schedules_held(&scene, None).await, 0);
    assert!(qstash.told().is_empty(), "{:?}", qstash.told());
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// A run whose fleet does not offer `cron_add` cannot schedule by calling it
/// anyway: the call is refused as not offered, and nothing is kept.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_fleet_not_offered_cron_add_creates_no_schedule() {
    let mut supervisor = Supervisor::new();
    let qstash = FakeQStash::start().await;
    let scene = scheduling(&mut supervisor, &qstash, TOOLLESS_FLEET).await;
    let upstream = Upstream::serve(Vec::new()).await;
    let (model, transcript) = FakeModel::new(vec![adding(WEEKLY), vec![say(ANSWER)]]);

    let settled = run_event(&scene, &scene.turn_id, &upstream, model).await;

    assert_eq!(settled.status, "processed");
    let result = &transcript.asked()[1].results[0];
    assert!(
        result.starts_with("[tool_not_offered] cron_add"),
        "{result}"
    );
    assert_eq!(schedules_held(&scene, None).await, 0);
    assert!(qstash.told().is_empty(), "{:?}", qstash.told());
    supervisor.shutdown().await;
    scene.cleanup().await;
}
