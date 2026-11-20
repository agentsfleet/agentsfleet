//! A running fleet says one line to its thread before it answers.
//!
//! The daemon is booted with `SLACK_API_URL` pointed at a loopback Slack, the
//! workspace holds a sealed Slack grant, and the event is admitted from a
//! Slack thread — the three facts a real mention leaves behind. The runner
//! never holds the bot token: it posts text to the daemon, and the daemon
//! speaks.
//!
//! The fence and the numbering under concurrency are in
//! `integration_runner_messages_fence`.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![expect(
    clippy::indexing_slicing,
    reason = "test target: a step indexes the JSON it just built"
)]

use af_wire::message_verb::MESSAGE_DELIVERY_DEADLINE;
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::definition_install::seal;
use crate::speaking::Speaking;
use crate::verbs::{CHANNEL, THREAD, posts};
use crate::wire::post;

/// The credential the scrub case's fleet declares, and the value it holds.
const CREDENTIAL: &str = "grafana";
/// See [`CREDENTIAL`].
const TOKEN: &str = "glsa_fixture_interim_secret";

/// A line far past the 4,096 bytes a line was once held to.
const LONG_LINE_BYTES: usize = 10_000;

/// How many lines the uncapped run says: past the eight it was once held to.
const LINES: u32 = 10;

/// A fleet that declares [`CREDENTIAL`].
const DECLARING: &str = r#"{"name":"e2e-fleet","x-agentsfleet":{"triggers":[],"tools":[],"credentials":["grafana"],"budget":{"daily_nanos":"1000000000"}}}"#;

/// A line reaches the event's Slack thread while the run is
/// still held, under its own marker part.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_fleet_posts_a_message_mid_run() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let (run_id, fence) = speaking.run_thread().await;

    let (status, posted) = speaking
        .say(
            (&run_id, fence),
            "fix pushed as a draft; re-checking at 09:00",
        )
        .await;
    assert_eq!(status, 200, "{posted}");
    assert_eq!(posted, json!({"delivered": true}));
    let sent = posts(&speaking.slack);
    assert_eq!(sent.len(), 1, "one line, before any answer");
    assert_eq!(sent[0]["channel"], CHANNEL);
    assert_eq!(sent[0]["thread_ts"], THREAD);
    assert_eq!(
        sent[0]["text"],
        "fix pushed as a draft; re-checking at 09:00"
    );
    assert_eq!(sent[0]["metadata"]["event_type"], "agentsfleet_interim");
    assert_eq!(
        sent[0]["metadata"]["event_payload"]["part"],
        json!({"run_id": run_id, "line": 1})
    );
    speaking.finish(supervisor).await;
}

/// No cap on a run's lines: a 10,000-byte line lands whole, and the ninth
/// and tenth lines land like the first, each its own part. An empty line is
/// still refused before anything is numbered.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_a_run_posts_long_lines_past_eight() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let (run_id, fence) = speaking.run_thread().await;

    let (status, refused) = speaking.say((&run_id, fence), "").await;
    assert_eq!(
        (status, &refused["error_code"]),
        (400, &json!("UZ-REQ-001")),
        "{refused}"
    );
    let long = "a".repeat(LONG_LINE_BYTES);
    let (status, posted) = speaking.say((&run_id, fence), &long).await;
    assert_eq!((status, &posted), (200, &json!({"delivered": true})));
    for line in 2..=LINES {
        let (status, posted) = speaking
            .say((&run_id, fence), &format!("line {line}"))
            .await;
        assert_eq!(status, 200, "line {line}: {posted}");
    }
    let sent = posts(&speaking.slack);
    assert_eq!(sent[0]["text"], json!(long), "the long line lands whole");
    let parts: Vec<Value> = sent
        .iter()
        .map(|sent| sent["metadata"]["event_payload"]["part"]["line"].clone())
        .collect();
    let expected: Vec<Value> = (1..=LINES).map(|part| json!(part)).collect();
    assert_eq!(parts, expected, "ten lines landed, each its own part");
    speaking.finish(supervisor).await;
}

/// An event from no thread has nowhere to post, and the run
/// keeps its budget.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_message_without_channel_refused() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let (run_id, fence) = speaking.run_seeded().await;

    let (status, refused) = speaking.say((&run_id, fence), "anyone there?").await;
    assert_eq!(
        (status, &refused["error_code"], &refused["current_state"]),
        (409, &json!("UZ-RUN-019"), &json!("no_thread")),
        "{refused}"
    );
    assert!(
        posts(&speaking.slack).is_empty(),
        "{:?}",
        posts(&speaking.slack)
    );
    assert_eq!(
        speaking.counted(&run_id).await,
        0,
        "a line with nowhere to go spends none of the scene's eight"
    );
    let unknown = "0195b4ba-8d3a-7fff-8abc-ffffffffffff";
    let (status, refused) = speaking.say((unknown, fence), "whose run?").await;
    assert_eq!(
        (status, &refused["error_code"]),
        (404, &json!("UZ-RUN-006")),
        "{refused}"
    );
    let (status, _) = speaking.say(("not-a-run", fence), "malformed").await;
    assert_eq!(status, 400);
    let path = format!("/v1/runners/me/runs/{run_id}/messages");
    let unread = post(
        &speaking.http,
        &speaking.scene,
        &path,
        &json!({"fencing_token": fence}),
    )
    .await;
    assert_eq!(
        unread.status().as_u16(),
        400,
        "a body with no text is malformed"
    );
    speaking.finish(supervisor).await;
}

/// A secret value the fleet declared is masked before the line
/// leaves the daemon.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_message_is_scrubbed() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, DECLARING).await;
    let body = json!({"host": "grafana.example.test", "token": TOKEN}).to_string();
    seal(
        &speaking.scene.booted,
        &speaking.scene.workspace,
        CREDENTIAL,
        &body,
        af_core::clock::now(),
    )
    .await;
    let (run_id, fence) = speaking.run_thread().await;

    let (status, posted) = speaking
        .say((&run_id, fence), &format!("the token was {TOKEN}"))
        .await;
    assert_eq!(status, 200, "{posted}");
    let sent = posts(&speaking.slack);
    assert_eq!(sent[0]["text"], "the token was «secret:grafana.token»");
    speaking.finish(supervisor).await;
}

/// A Slack that refuses the line: the call answers, `delivered` is false, and
/// the run goes on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_message_refused_by_slack_is_undelivered() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    speaking
        .slack
        .post_answers(200, r#"{"ok":false,"error":"channel_not_found"}"#);
    let (run_id, fence) = speaking.run_thread().await;

    let (status, posted) = speaking.say((&run_id, fence), "status").await;
    assert_eq!((status, posted), (200, json!({"delivered": false})));
    speaking.finish(supervisor).await;
}

/// A Slack that holds every post open: the daemon answers `delivered: false`
/// at its own deadline, inside the runner's call timeout, rather than letting
/// the runner give up first and read an unreachable daemon.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_message_to_a_stalled_slack_answers_at_the_deadline() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    speaking.slack.post_stalls();
    let (run_id, fence) = speaking.run_thread().await;

    let started = std::time::Instant::now();
    let (status, posted) = speaking.say((&run_id, fence), "status").await;
    let took = started.elapsed();
    assert_eq!((status, posted), (200, json!({"delivered": false})));
    assert!(
        took >= MESSAGE_DELIVERY_DEADLINE
            && took < MESSAGE_DELIVERY_DEADLINE + std::time::Duration::from_secs(3),
        "answered at the deadline, not after every retry ran out: {took:?}"
    );
    speaking.finish(supervisor).await;
}
