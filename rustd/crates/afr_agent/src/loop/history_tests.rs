#![expect(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::event::message_of;
use afd_wire::lease::{LeasePayload, Turn};
use afr_providers::{Message, Replay};
use tokio_util::sync::CancellationToken;

use crate::context::{Budget, EVICTED};
use crate::fixture::{GITHUB_TOKEN, Script, budget, clean, lease, say, unbounded};
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

/// A turn's cache reads and writes ride its `chat` span beside the whole
/// prompt, so a trace shows what a follow-up's repeated prefix cost.
#[tokio::test]
async fn test_cache_tokens_recorded_on_the_chat_span() {
    use afd_core::test_util::trace::Capture;
    use afd_observability::semconv::{
        ATTR_USAGE_CACHE_CREATION_TOKENS, ATTR_USAGE_CACHE_READ_TOKENS, ATTR_USAGE_INPUT_TOKENS,
        OPERATION_CHAT,
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
        Some("50"),
        "fresh and cached together"
    );
    assert_eq!(chat.field(ATTR_USAGE_CACHE_READ_TOKENS), Some("40"));
    assert_eq!(chat.field(ATTR_USAGE_CACHE_CREATION_TOKENS), Some("6"));
}

/// A follow-up's first request repeats the previous lease's, through that
/// lease's message: the same system prompt, the same tools, and the earlier
/// message as it was sent, so the provider's cache can serve it.
#[tokio::test]
async fn test_history_prefix_is_byte_identical_across_leases() {
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

/// A write-bound lease keeps its trusted repair context in the system prompt
/// and out of every message, earlier turns included.
#[tokio::test]
async fn test_history_leaves_the_repair_context_in_the_system_prompt() {
    use afd_wire::policy::repository::{self, FIELD_REF, REFS_HEADS, REFS_PATH};
    use afd_wire::policy::{
        HttpJsonFieldRule, HttpMethod, HttpOriginPolicy, HttpPathMatch, HttpRequestRule,
        RepositoryAccess, RepositoryBinding,
    };

    const REPOSITORY: &str = "agentsfleet/linkwarden";
    const HEADING: &str = "## Trusted repair context";
    let mut lease = with_history(&[(ASKED, ANSWERED)]);
    lease.policy.repository_binding = Some(RepositoryBinding {
        repositories: vec![REPOSITORY.into()],
        access: RepositoryAccess::Write,
        base_branch: "dev".into(),
    });
    lease.policy.http_origin_policies = vec![HttpOriginPolicy {
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
        }],
    }];

    let sent = first_request(&lease).await;

    assert!(sent.instructions.contains(HEADING), "{}", sent.instructions);
    assert!(
        !format!("{:?}", sent.messages).contains(HEADING),
        "no message carries the repair context"
    );
}
