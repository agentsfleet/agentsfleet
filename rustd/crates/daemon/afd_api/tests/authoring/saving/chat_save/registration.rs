//! A chat save registers its active fleet's cron trigger with the scheduler,
//! as Save's route does, and leaves an installing fleet's unregistered.
//!
//! The scheduler is the lane's `QStash` emulator, so "registered" is the
//! emulator holding the schedule under the id a fire resolves, not only the
//! sync state the row claims.

use super::*;
use af_core::env::{EnvSource as _, ProcessEnv};
use afd_auth::Scope;
use std::sync::Arc;

/// Where the lane's scheduler listens, `/v2` included.
const LIVE_URL_KNOB: &str = "AGENTSFLEET_QSTASH_LIVE_URL";
/// The credential the lane's scheduler authenticates.
const LIVE_TOKEN_KNOB: &str = "AGENTSFLEET_QSTASH_LIVE_TOKEN";
/// A destination the emulator resolves when a schedule is created; it refuses
/// a `.test` host before registering anything.
const LIVE_DESTINATION: &str = "https://example.com";
/// The cron trigger the saved setup declares.
const DAILY: &str = "0 9 * * *";

/// The lane's scheduler: its base and its token.
fn live() -> (String, String) {
    let knob = |name| {
        ProcessEnv
            .get(name)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| panic!("{name} is unset: run through make test-integration-rustd"))
    };
    (knob(LIVE_URL_KNOB), knob(LIVE_TOKEN_KNOB))
}

/// A chat-save case whose saves register through the lane's scheduler.
#[expect(
    clippy::disallowed_methods,
    reason = "the lane reaches its live scheduler with reqwest's defaults, as afd_cron's live suites do"
)]
async fn registering(url: &str, token: &str) -> Case {
    let scheduler = afd_cron::QStash::fixed(
        reqwest::Client::new(),
        af_crypto::secret::SecretString::new(token.to_owned()),
        LIVE_DESTINATION.to_owned(),
        url.to_owned(),
    );
    let base = crate::authoring::fixture::Fixture::create().await;
    let bus = crate::harness::bus().await;
    let router = base
        .services(&Scope::ALL)
        .with_fleet_queue(base.db.clone(), bus.clone())
        .with_schedule_registration(Arc::new(scheduler))
        .router();
    Case { base, router, bus }
}

/// Saves a draft declaring `config` by a chat message, as a person typing
/// "Save" does; answers the saved fleet's read.
async fn chat_saved(case: &mut Case, config: Value) -> Value {
    case.fund().await;
    let (item, _) = case.request_with(config).await;
    let input = json!({"message":"Save this setup", "save":true, "operation_id":mint()});
    accept_message(case, &item, &input).await;
    let before = read(case, &item).await;
    let (path, token, report) = finish_request(case, &before).await;
    report_once(case, &path, &token, &report).await;
    let fleet = read(case, &item).await["linked_fleet_id"]
        .as_str()
        .expect("the report saved the fleet")
        .to_owned();
    read(case, &format!("{}/{fleet}", case.collection())).await
}

/// A setup whose one trigger runs daily, needing `credentials`.
fn daily(credentials: &[&str]) -> Value {
    json!({"credentials":credentials,"triggers":[{"type":"cron","schedule":DAILY,
        "timezone":"UTC","message":"Daily digest"}]})
}

/// The fleet's one schedule as the fleet read renders it.
fn only_schedule(fleet: &Value) -> &Value {
    let schedules = fleet["schedules"].as_array().expect("schedules");
    assert_eq!(
        schedules.len(),
        1,
        "one cron trigger, one schedule: {fleet}"
    );
    &schedules[0]
}

/// The emulator's answer to reading the schedule filed under `schedule`'s
/// upstream id.
#[expect(
    clippy::disallowed_methods,
    reason = "the lane reaches its live scheduler with reqwest's defaults, as afd_cron's live suites do"
)]
async fn held(url: &str, token: &str, schedule: &Value) -> reqwest::Response {
    let id = afd_cron::upstream_id(&crate::authoring::id_field(&schedule["schedule_id"]));
    reqwest::Client::new()
        .get(format!("{url}/schedules/{id}"))
        .bearer_auth(token)
        .send()
        .await
        .expect("the lane's scheduler answers")
}

/// How many times the schedule's row has been written; every sync claim
/// bumps it, so an unclaimed row is still at its first.
async fn row_version(case: &Case, schedule: &Value) -> i64 {
    sqlx::query_scalar("SELECT resource_version FROM fleet.schedules WHERE id=$1")
        .bind(crate::authoring::id_field(&schedule["schedule_id"]))
        .fetch_one(&mut *case.base.db.acquire().await.expect("schedule connection"))
        .await
        .expect("the saved schedule row")
}

#[tokio::test]
#[ignore = "needs live Postgres, NATS and the compose qstash service: make test-integration-rustd"]
async fn a_chat_save_registers_its_cron_trigger_with_the_scheduler() {
    let (url, token) = live();
    let mut case = registering(&url, &token).await;

    let fleet = chat_saved(&mut case, daily(&[])).await;

    assert_eq!(fleet["status"], "active", "{fleet}");
    let schedule = only_schedule(&fleet);
    assert_eq!(schedule["sync"], "synced", "{schedule}");
    assert_eq!(schedule["run_state"], "scheduled", "{schedule}");
    assert!(schedule["next_run_at"].is_string(), "{schedule}");
    let answer = held(&url, &token, schedule).await;
    assert_eq!(answer.status(), reqwest::StatusCode::OK);
    let registered: Value =
        serde_json::from_str(&answer.text().await.expect("emulator body")).expect("emulator JSON");
    assert_eq!(registered["cron"], DAILY, "{registered}");
    case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres, NATS and the compose qstash service: make test-integration-rustd"]
async fn a_chat_save_still_installing_registers_nothing() {
    let (url, token) = live();
    let mut case = registering(&url, &token).await;

    let fleet = chat_saved(&mut case, daily(&["jira"])).await;

    assert_eq!(fleet["status"], "installing", "{fleet}");
    let schedule = only_schedule(&fleet);
    assert_eq!(schedule["sync"], "syncing", "{schedule}");
    assert_eq!(schedule["run_state"], "activation_pending", "{schedule}");
    assert_eq!(row_version(&case, schedule).await, 1, "never claimed");
    assert_eq!(
        held(&url, &token, schedule).await.status(),
        reqwest::StatusCode::NOT_FOUND
    );
    case.cleanup().await;
}
