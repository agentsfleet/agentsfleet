//! The private-chat-to-fleet funnel steps: once per real transition, after the
//! commit, never for a replay or a refusal, and never in the way of the write.

use super::{
    fixture::{Fixture, now},
    mint,
};
use afd_authoring::{Drafts, types::SendMessage};
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_observability::{Analytics, Telemetry};
use serde_json::Value;

/// The funnel's acceptance step.
pub(super) const ACCEPTED: &str = "draft_message_accepted";
/// The funnel's terminal-message step.
pub(super) const FINISHED: &str = "draft_message_finished";

/// One reported property, as text.
pub(super) fn property(event: &Telemetry, key: &str) -> Option<String> {
    let event = event.event();
    event
        .properties()
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// The one event in `events`; fails, saying `why`, on any other count.
pub(super) fn only(events: Vec<Telemetry>, why: &str) -> Telemetry {
    let [event]: [Telemetry; 1] = events.try_into().expect(why);
    event
}

/// Asserts `event` is attributed to the fixture owner, in the fixture
/// workspace, about `draft`.
pub(super) fn about(f: &Fixture, event: &Telemetry, draft: &Uuid7) {
    assert_eq!(
        event.actor(),
        Some(f.owner.as_str()),
        "the creator's subject"
    );
    let workspace = property(event, "workspace_id");
    assert_eq!(workspace.as_deref(), Some(f.workspace.as_str()));
    assert_eq!(property(event, "draft_id").as_deref(), Some(draft.as_str()));
}

fn follow_up(text: &str) -> SendMessage {
    SendMessage {
        message: text.to_owned(),
        operation_id: mint(),
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_report_one_accepted_step_per_new_message_and_none_on_replay_or_refusal() {
    let f = Fixture::create().await;
    let key = mint();
    let opening = f.request(key, Some("First question"));
    let created = f
        .drafts
        .create(f.access(), opening.clone(), now())
        .await
        .expect("opening message accepted");
    let draft = created.draft.id.clone();
    assert_eq!(f.reported(ACCEPTED).len(), 1, "the opening message");
    let replayed = f
        .drafts
        .create(f.access(), opening, now())
        .await
        .expect("exact creation replay");
    assert!(replayed.accepted.expect("replayed acceptance").replayed);
    f.drafts
        .create(f.access(), f.request(mint(), None), now())
        .await
        .expect("a draft with no message");
    assert_eq!(f.reported(ACCEPTED).len(), 1, "no new message, no step");

    let second = follow_up("Second question");
    let sent = f
        .drafts
        .send(f.access(), &draft, second.clone(), now())
        .await
        .expect("follow-up accepted");
    assert!(!sent.replayed);
    let again = f
        .drafts
        .send(f.access(), &draft, second.clone(), now())
        .await
        .expect("exact send replay");
    assert!(again.replayed);
    let changed = SendMessage {
        message: "A different body".to_owned(),
        ..second
    };
    f.drafts
        .send(f.access(), &draft, changed, now())
        .await
        .expect_err("a changed body under the same key is refused");
    f.drafts
        .send(f.member_access(), &draft, follow_up("Not mine"), now())
        .await
        .expect_err("another member's draft is hidden");

    let accepted = f.reported(ACCEPTED);
    assert_eq!(accepted.len(), 2, "one per new message, none otherwise");
    for event in &accepted {
        about(&f, event, &draft);
    }
    assert!(f.reported(FINISHED).is_empty(), "nothing has finished yet");
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_report_a_stopped_message_finished_once_however_often_stop_retries() {
    let f = Fixture::create().await;
    let created = f
        .drafts
        .create(f.access(), f.request(mint(), Some("Stop me")), now())
        .await
        .expect("queued message");
    let draft = created.draft.id;
    for _ in 0..2 {
        f.drafts
            .cancel(f.access(), &draft, 1)
            .await
            .expect("Stop commits");
    }
    f.drafts
        .cancel(f.access(), &draft, 2)
        .await
        .expect_err("a future cutoff is refused");
    let finished = only(f.reported(FINISHED), "the retried Stop moved nothing");
    about(&f, &finished, &draft);
    assert_eq!(property(&finished, "state").as_deref(), Some("cancelled"));
    f.cleanup().await;
}

/// A reporter whose ingestion host refuses every connection changes neither
/// the answer nor the durable rows: capture queues and returns.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn should_answer_the_same_when_the_analytics_endpoint_is_unreachable() {
    let f = Fixture::create().await;
    let unreachable = Analytics::resolve("test-project", Some("http://127.0.0.1:9")).await;
    let drafts = Drafts::new(f.db.clone(), Entropy::new()).with_analytics(unreachable);
    let created = drafts
        .create(f.access(), f.request(mint(), Some("Still accepted")), now())
        .await
        .expect("acceptance does not wait on analytics");
    let accepted = created.accepted.expect("opening message");
    assert_eq!(
        (accepted.sequence.as_str(), accepted.replayed),
        ("1", false)
    );
    let stopped = drafts
        .cancel(f.access(), &created.draft.id, 1)
        .await
        .expect("Stop does not wait on analytics");
    assert_eq!(stopped.stopped.len(), 1);
    assert_eq!(f.counts().await, (1, 1, 1), "the rows committed regardless");
    f.cleanup().await;
}
