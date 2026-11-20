#![expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use af_wire::run::{RunPayload, Turn};
use af_wire::turn::message_of;
use guffbolt::agents::{SPAWN_AGENT, WAIT_AGENT};
use guffbolt::{Message, Replay};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::fixture::{GITHUB_TOKEN, Script, call, clean, payload, say, unbounded};
use crate::harness::tests::{drive, engine};
use crate::prompt::Prompt;

/// What the fleet was asked before, and what it answered.
const ASKED: &str = "which tests failed?";
const ANSWERED: &str = "two: a and b";
/// The current event's message, as the fixture run carries it.
const CURRENT: &str = "triage the failed run";
/// The task the root hands a child, which opens the child's conversation.
const DELEGATED: &str = "read the build log";

/// The fixture run, carrying `turns` as its earlier history.
fn with_history(turns: &[(&str, &str)]) -> RunPayload<'static> {
    let mut run = payload(&[], unbounded());
    run.history = turns
        .iter()
        .map(|(message, answer)| Turn {
            message: (*message).to_owned().into(),
            answer: (*answer).to_owned().into(),
        })
        .collect();
    run
}

/// The first request a run's run sends.
async fn first_request(run: &RunPayload<'_>) -> crate::fixture::Sent {
    let script = Script::new([vec![say("done")]]);
    let engine = engine(Vec::new(), &script);
    drive(&engine, run, &CancellationToken::new()).await;
    script.sent().remove(0)
}

/// The turns lead the conversation, ahead of the current message, and leave
/// the system prompt as a run without them has it.
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
    assert_eq!(
        sent.messages[..2],
        [
            Message::User(clean(&asked).into_inner()),
            Message::Assistant {
                text: clean(&answered).into_inner(),
                calls: Vec::new(),
                replay: Replay::default(),
            },
        ],
        "the turn is sent, in its masked form"
    );
}

/// A message reads the same as a turn as it read when it was current: both
/// the daemon's turn and the runner's prompt read it through `message_of`.
#[test]
fn test_history_message_matches_its_first_reading() {
    for request in [
        r#"{"message":"fix the second one","ref":"main"}"#,
        r#"{"ref":"main"}"#,
    ] {
        let mut run = payload(&[], unbounded());
        run.event.request_json = request.into();
        assert_eq!(Prompt::new(&run).message, message_of(request));
    }
}

/// A turn's cache reads and writes ride its `chat` span beside the whole
/// prompt, so a trace shows what a follow-up's repeated prefix cost.
#[tokio::test]
async fn test_cache_tokens_recorded_on_the_chat_span() {
    use af_core::test_util::trace::Capture;
    use af_observability::semconv::{
        ATTR_USAGE_CACHE_CREATION_TOKENS, ATTR_USAGE_CACHE_READ_TOKENS, ATTR_USAGE_INPUT_TOKENS,
        ATTR_USAGE_OUTPUT_TOKENS, OPERATION_CHAT,
    };

    let capture = Capture::install();
    let script = Script::new([vec![
        say("done"),
        crate::fixture::spent_caching(10, 40, 6, 2),
    ]]);
    let engine = engine(Vec::new(), &script);
    drive(
        &engine,
        &with_history(&[(ASKED, ANSWERED)]),
        &CancellationToken::new(),
    )
    .await;

    let chat = capture
        .spans()
        .into_iter()
        .find(|span| span.name == OPERATION_CHAT)
        .unwrap();
    assert_eq!(
        chat.field(ATTR_USAGE_INPUT_TOKENS),
        Some("56"),
        "fresh, read and written together"
    );
    assert_eq!(chat.field(ATTR_USAGE_OUTPUT_TOKENS), Some("2"));
    assert_eq!(chat.field(ATTR_USAGE_CACHE_READ_TOKENS), Some("40"));
    assert_eq!(chat.field(ATTR_USAGE_CACHE_CREATION_TOKENS), Some("6"));
}

/// Every request of a run files the provider's prompt cache under the
/// run's fleet, a spawned child's as well as the root's, so a follow-up and
/// every child of the fleet read the one cache.
#[tokio::test]
async fn test_every_request_keys_the_cache_by_the_fleet() {
    let script = Script::new([
        vec![call(
            "p1",
            SPAWN_AGENT,
            json!({"message": DELEGATED, "fork_turns": "none"}),
        )],
        vec![call("p2", WAIT_AGENT, json!({"target": 1}))],
        vec![say("done")],
    ])
    .with_child(DELEGATED, [vec![say("read")]]);
    let engine = engine(Vec::new(), &script);
    let run = payload(&[SPAWN_AGENT, WAIT_AGENT], unbounded());

    drive(&engine, &run, &CancellationToken::new()).await;

    let sent = script.sent();
    let child = sent.iter().filter(|request| {
        matches!(request.messages.first(), Some(Message::User(text)) if text.starts_with(DELEGATED))
    });
    assert_eq!(
        (sent.len(), child.count()),
        (4, 1),
        "three root turns and the child's one"
    );
    for request in &sent {
        assert_eq!(request.cache_key, run.event.fleet_id, "{request:?}");
    }
}

/// A follow-up's first request repeats the previous run's, through that
/// run's message: the same system prompt, the same tools, and the earlier
/// message as it was sent, so the provider's cache can serve it.
#[tokio::test]
async fn test_history_prefix_is_byte_identical_across_runs() {
    let first = first_request(&with_history(&[])).await;
    let mut follow_up = with_history(&[(CURRENT, ANSWERED)]);
    follow_up.event.request_json = r#"{"message":"fix the second one"}"#.into();
    let second = first_request(&follow_up).await;

    assert_eq!(second.instructions, first.instructions);
    assert_eq!(second.tools, first.tools);
    assert_eq!(
        second.messages[..first.messages.len()],
        first.messages[..],
        "the earlier request is the follow-up's prefix"
    );
}

/// A write-bound run keeps its trusted repair context in the system prompt
/// and out of every message, earlier turns included.
#[tokio::test]
async fn test_history_leaves_the_repair_context_in_the_system_prompt() {
    use af_wire::policy::repository::{self, FIELD_REF, REFS_HEADS, REFS_PATH};
    use af_wire::policy::{
        HttpJsonFieldRule, HttpMethod, HttpOriginPolicy, HttpPathMatch, HttpRequestRule,
        RepositoryAccess, RepositoryBinding,
    };

    const REPOSITORY: &str = "agentsfleet/linkwarden";
    const HEADING: &str = "## Trusted repair context";
    let mut run = with_history(&[(ASKED, ANSWERED)]);
    run.policy.repository_binding = Some(RepositoryBinding {
        repositories: vec![REPOSITORY.into()],
        access: RepositoryAccess::Write,
        base_branch: "dev".into(),
    });
    run.policy.http_origin_policies = vec![HttpOriginPolicy {
        host: "api.github.com".into(),
        credential_names: vec!["github".into()],
        requests: vec![HttpRequestRule {
            method: HttpMethod::Post,
            path: repository::path(REPOSITORY, REFS_PATH).into(),
            path_match: HttpPathMatch::Exact,
            json_fields: vec![HttpJsonFieldRule {
                name: FIELD_REF.into(),
                string_value: Some(format!("{REFS_HEADS}agentsfleet-repair/run-41").into()),
                boolean_value: None,
            }],
            permitted_fields: None,
        }],
    }];

    let sent = first_request(&run).await;

    assert!(sent.instructions.contains(HEADING), "{}", sent.instructions);
    assert!(
        !format!("{:?}", sent.messages).contains(HEADING),
        "no message carries the repair context"
    );
}
