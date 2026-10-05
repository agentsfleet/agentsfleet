//! §2 — a running fleet says one line to its thread before it answers.
//!
//! The daemon is booted with `SLACK_API_URL` pointed at a loopback Slack, the
//! workspace holds a sealed Slack grant, and the event is admitted from a
//! Slack thread — the three facts a real mention leaves behind. The runner
//! never holds the bot token: it posts text to the daemon, and the daemon
//! speaks.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly, and a step \
              indexes the JSON it just built"
)]

use afd_connector::test_util::FakeSlack;
use afd_core::id::Uuid7;
use afd_wire::message_verb::{MESSAGE_MAX_BYTES, MESSAGES_PER_RUN_MAX};
use agentsfleetd::preflight::SLACK_API_URL_KNOB;
use agentsfleetd::supervisor::Supervisor;
use serde_json::{Value, json};

use crate::bundle_install::seal;
use crate::e2e::{Scenario, scenario_with};
use crate::tail::lease;
use crate::verbs::{CHANNEL, THREAD, enqueue_from_thread, posts, seal_slack_grant};
use crate::wire::{json as body_of, poll_for_lease, post, report_body};

/// The credential the scrub case's fleet declares, and the value it holds.
const CREDENTIAL: &str = "grafana";
/// See [`CREDENTIAL`].
const TOKEN: &str = "glsa_fixture_interim_secret";

/// How far past the byte bound the oversized line runs: a kibibyte, so it is
/// plainly over rather than off by one.
const OVER_BY: usize = 1024;

/// A fleet that declares [`CREDENTIAL`].
const DECLARING: &str = r#"{"name":"e2e-fleet","x-agentsfleet":{"triggers":[{"type":"api"}],"tools":[],"credentials":["grafana"],"budget":{"daily_dollars":1.0}}}"#;

/// A booted daemon pointed at a loopback Slack.
struct Speaking {
    run: Scenario,
    http: reqwest::Client,
    slack: FakeSlack,
}

impl Speaking {
    async fn boot(supervisor: &mut Supervisor, config: &str) -> Self {
        let slack = FakeSlack::start().await;
        let api = slack.api_base();
        let run = scenario_with(supervisor, None, &[(SLACK_API_URL_KNOB, &api)], config).await;
        seal_slack_grant(&run).await;
        Self {
            run,
            http: reqwest::Client::new(),
            slack,
        }
    }

    /// Leases the seeded event, which came from no thread.
    async fn lease_seeded(&self) -> (String, u64) {
        lease(&self.http, &self.run).await
    }

    /// Reports `(lease_id, fence)` done, then admits an event from the Slack
    /// thread and leases it.
    async fn lease_from_thread(&self, (lease_id, fence): (&str, u64)) -> (String, u64) {
        let report = report_body(lease_id, &self.run.event_id, fence);
        let reported = post(&self.http, &self.run, "/v1/runners/me/reports", &report).await;
        assert_eq!(reported.status().as_u16(), 200, "the seeded lease settles");
        let event = enqueue_from_thread(&self.run).await;
        poll_for_lease(&self.http, &self.run, &event).await
    }

    async fn say(&self, (lease_id, fence): (&str, u64), text: &str) -> (u16, Value) {
        let path = format!("/v1/runners/me/leases/{lease_id}/messages");
        let body = json!({"fencing_token": fence, "text": text});
        let response = post(&self.http, &self.run, &path, &body).await;
        let status = response.status().as_u16();
        (status, body_of(response).await)
    }

    async fn finish(self, supervisor: Supervisor) {
        supervisor.shutdown().await;
        self.run.cleanup().await;
    }
}

/// Dimension 2.1. A line reaches the event's Slack thread while the run is
/// still leased, under its own marker part.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_fleet_posts_a_message_mid_run() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let seeded = speaking.lease_seeded().await;
    let (lease_id, fence) = speaking.lease_from_thread((&seeded.0, seeded.1)).await;

    let (status, posted) = speaking
        .say(
            (&lease_id, fence),
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
    assert_eq!(sent[0]["metadata"]["event_payload"]["part"], 1);
    speaking.finish(supervisor).await;
}

/// Dimension 2.2. A holder the fleet has moved past posts nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_stale_fence_message_refused() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let (lease_id, fence) = speaking.lease_seeded().await;

    let (status, refused) = speaking.say((&lease_id, fence - 1), "late").await;
    assert_eq!(
        (status, &refused["error_code"]),
        (409, &json!("UZ-RUN-005")),
        "{refused}"
    );
    assert!(posts(&speaking.slack).is_empty());
    speaking.finish(supervisor).await;
}

/// Dimension 2.3. Eight lines a run, then the cap; and a line over the byte
/// bound is refused before anything is counted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_message_caps_refuse() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let seeded = speaking.lease_seeded().await;
    let (lease_id, fence) = speaking.lease_from_thread((&seeded.0, seeded.1)).await;

    let (status, refused) = speaking
        .say((&lease_id, fence), &"a".repeat(MESSAGE_MAX_BYTES + OVER_BY))
        .await;
    assert_eq!(
        (status, &refused["error_code"]),
        (400, &json!("UZ-REQ-001")),
        "{refused}"
    );
    for line in 1..=MESSAGES_PER_RUN_MAX {
        let (status, posted) = speaking
            .say((&lease_id, fence), &format!("line {line}"))
            .await;
        assert_eq!(status, 200, "line {line}: {posted}");
    }
    let (status, refused) = speaking.say((&lease_id, fence), "one too many").await;
    assert_eq!(
        (status, &refused["error_code"]),
        (429, &json!("UZ-RUN-020")),
        "{refused}"
    );
    let parts: Vec<Value> = posts(&speaking.slack)
        .iter()
        .map(|sent| sent["metadata"]["event_payload"]["part"].clone())
        .collect();
    let expected: Vec<Value> = (1..=MESSAGES_PER_RUN_MAX).map(|part| json!(part)).collect();
    assert_eq!(parts, expected, "eight lines landed, each its own part");
    speaking.finish(supervisor).await;
}

/// Dimension 2.4. An event from no thread has nowhere to post, and the run
/// keeps its budget.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_message_without_channel_refused() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    let (lease_id, fence) = speaking.lease_seeded().await;

    let (status, refused) = speaking.say((&lease_id, fence), "anyone there?").await;
    assert_eq!(
        (status, &refused["error_code"]),
        (409, &json!("UZ-RUN-019")),
        "{refused}"
    );
    assert!(posts(&speaking.slack).is_empty());
    let unknown = "0195b4ba-8d3a-7fff-8abc-ffffffffffff";
    let (status, refused) = speaking.say((unknown, fence), "whose lease?").await;
    assert_eq!(
        (status, &refused["error_code"]),
        (404, &json!("UZ-RUN-006")),
        "{refused}"
    );
    let (status, _) = speaking.say(("not-a-lease", fence), "malformed").await;
    assert_eq!(status, 400);
    speaking.finish(supervisor).await;
}

/// Dimension 2.5. A secret value the fleet declared is masked before the line
/// leaves the daemon.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_message_is_scrubbed() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, DECLARING).await;
    let workspace = Uuid7::parse(&speaking.run.workspace).expect("a minted workspace id");
    let body = json!({"host": "grafana.example.test", "token": TOKEN}).to_string();
    seal(
        &speaking.run.booted,
        &workspace,
        CREDENTIAL,
        &body,
        afd_core::clock::now(),
    )
    .await;
    let seeded = speaking.lease_seeded().await;
    let (lease_id, fence) = speaking.lease_from_thread((&seeded.0, seeded.1)).await;

    let (status, posted) = speaking
        .say((&lease_id, fence), &format!("the token was {TOKEN}"))
        .await;
    assert_eq!(status, 200, "{posted}");
    let sent = posts(&speaking.slack);
    assert_eq!(sent[0]["text"], "the token was «secret:grafana.token»");
    speaking.finish(supervisor).await;
}

/// A Slack that refuses the line: the call answers, `delivered` is false, and
/// the run goes on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Dragonfly: make test-integration-rustd"]
async fn test_message_refused_by_slack_is_undelivered() {
    let mut supervisor = Supervisor::new();
    let speaking = Speaking::boot(&mut supervisor, crate::e2e_seed::FLEET_CONFIG_JSON).await;
    speaking
        .slack
        .post_answers(200, r#"{"ok":false,"error":"channel_not_found"}"#);
    let seeded = speaking.lease_seeded().await;
    let (lease_id, fence) = speaking.lease_from_thread((&seeded.0, seeded.1)).await;

    let (status, posted) = speaking.say((&lease_id, fence), "status").await;
    assert_eq!((status, posted), (200, json!({"delivered": false})));
    speaking.finish(supervisor).await;
}
