#![expect(
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::event::message_of;
use afd_wire::lease::{LeasePayload, Turn};
use afr_providers::{Message, Replay};
use tokio_util::sync::CancellationToken;

use crate::context::{Budget, EVICTED};
use crate::fixture::{GITHUB_TOKEN, Script, budget, lease, say, unbounded};
use crate::harness::tests::{drive, engine};
use crate::prompt::Prompt;

/// What the fleet was asked before, and what it answered.
const ASKED: &str = "which tests failed?";
const ANSWERED: &str = "two: a and b";
/// The current event's message, as the fixture lease carries it.
const CURRENT: &str = "triage the failed run";

/// The fixture lease, carrying `turns` as its earlier history.
fn with_history(turns: &[(&str, &str)]) -> LeasePayload<'static> {
    let mut lease = lease(&[], unbounded());
    lease.history = turns
        .iter()
        .map(|(message, answer)| Turn {
            message: (*message).to_owned().into(),
            answer: (*answer).to_owned().into(),
        })
        .collect();
    lease
}

/// The first request a lease's run sends.
async fn first_request(lease: &LeasePayload<'_>) -> crate::fixture::Sent {
    let script = Script::new([vec![say("done")]]);
    let engine = engine(Vec::new(), &script);
    drive(&engine, lease, &CancellationToken::new()).await;
    script.sent().remove(0)
}

/// The turns lead the conversation, ahead of the current message, and leave
/// the system prompt as a lease without them has it.
#[tokio::test]
async fn test_history_leads_the_conversation() {
    let with = first_request(&with_history(&[(ASKED, ANSWERED)])).await;
    let without = first_request(&with_history(&[])).await;

    assert_eq!(
        with.messages,
        [
            Message::User(ASKED.to_owned()),
            Message::Assistant {
                text: ANSWERED.to_owned(),
                calls: Vec::new(),
                replay: Replay::default(),
            },
            Message::User(CURRENT.to_owned()),
        ]
    );
    assert_eq!(with.instructions, without.instructions);
}

/// A secret said in an earlier turn, asked or answered, is scrubbed as the
/// current message's would be.
#[tokio::test]
async fn test_history_is_scrubbed() {
    let asked = format!("use {GITHUB_TOKEN}");
    let answered = format!("used {GITHUB_TOKEN}");
    let sent = first_request(&with_history(&[(&asked, &answered)])).await;

    let rendered = format!("{:?}", sent.messages);
    assert!(!rendered.contains(GITHUB_TOKEN), "{rendered}");
}

/// A message reads the same as a turn as it read when it was current: both
/// the daemon's turn and the runner's prompt read it through `message_of`.
#[test]
fn test_history_message_matches_its_first_reading() {
    for request in [
        r#"{"message":"fix the second one","ref":"main"}"#,
        r#"{"ref":"main"}"#,
    ] {
        let mut lease = lease(&[], unbounded());
        lease.event.request_json = request.into();
        assert_eq!(Prompt::new(&lease).message, message_of(request));
    }
}

/// Eviction rewrites old tool results only; every turn stays as it was.
#[test]
fn test_eviction_leaves_history_intact() {
    let lease = lease(&[], budget(1, 0));
    let turns = [
        Message::User(ASKED.to_owned()),
        Message::Assistant {
            text: ANSWERED.to_owned(),
            calls: Vec::new(),
            replay: Replay::default(),
        },
        Message::User(CURRENT.to_owned()),
    ];
    let mut messages = turns.to_vec();
    for call in ["1", "2", "3"] {
        messages.push(Message::ToolResult {
            call_id: call.to_owned(),
            output: "large".to_owned(),
            image: None,
        });
    }

    Budget::new(&lease.policy.context).evict(&mut messages);

    assert_eq!(messages[..3], turns, "the turns are untouched");
    assert!(
        matches!(&messages[3], Message::ToolResult { output, .. } if output == EVICTED),
        "an old tool result is what eviction rewrites"
    );
}
