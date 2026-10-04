#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_core::clock::{FixedClock, UnixMillis};
use afd_wire::activity::{ActivityFrame, StreamTextKind};
use afd_wire::tool_trace::ToolCallStatus;
use afr_egress::fixture::policy;
use afr_egress::testing::{CountingMint, RecordingTransport};
use afr_providers::{Chunk, Message};
use afr_tools::catalog::{HTTP_REQUEST, UPDATE_PLAN};
use afr_tools::{Catalog, ToolErrorCode};
use tokio_util::sync::CancellationToken;

use afd_core::test_util::trace::Capture;
use afd_observability::semconv::{
    ATTR_PROVIDER_NAME, ATTR_TOOL_CALL_ID, ATTR_TOOL_NAME, ATTR_USAGE_INPUT_TOKENS, OPERATION_CHAT,
    OPERATION_EXECUTE_TOOL, OPERATION_INVOKE_AGENT, RUNNER_SCOPE_NAME,
};

use super::Loop;
use super::tests::{completions, drive, engine};
use super::{EVENT_TURN_COMPLETED, EVENT_TURN_STARTED};
use crate::engine::{AgentEngine, AgentRun, Meter};
use crate::fixture::{
    API_KEY, Canned, Exits, Frames, GITHUB_TOKEN, Script, call, lease, say, spent, unbounded,
};
use crate::ledger::{EVENT_CALL_COMPLETED, EVENT_CALL_STARTED};
use crate::testing::Discard;

/// The event the egress vault logs each mint under, in the vault's spelling.
const EVENT_CREDENTIAL_MINTED: &str = "credential_minted";
/// The token the test mint answers, and how long it lives: an hour.
const MINTED: &str = "ghs_minted_token";
const HOUR_MILLIS: i64 = 3_600_000;

#[tokio::test]
async fn test_unlisted_tool_refused_run_continues() {
    let script = Script::new([
        vec![call(
            "x",
            "shell",
            serde_json::json!({"command": "rm -rf /"}),
        )],
        vec![say("I cannot run shell here")],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, "4")], &script);

    let (output, frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(
        output.result.content, "I cannot run shell here",
        "the next turn ran"
    );
    assert_eq!(
        completions(&frames),
        [("1".to_owned(), ToolCallStatus::Failed)]
    );
    let Message::ToolResult {
        output: refusal, ..
    } = &script.sent()[1].messages[2]
    else {
        panic!("the refusal is fed back as the call's result");
    };
    assert!(
        refusal.starts_with(&format!("[{}] shell ", ToolErrorCode::NotOffered)),
        "{refusal}"
    );
}

#[tokio::test]
async fn test_answer_streams_as_chunks() {
    let script = Script::new([vec![
        Chunk::Text {
            kind: StreamTextKind::Reasoning,
            text: "checking".to_owned(),
        },
        say("hel"),
        say("lo"),
    ]]);
    let engine = engine(Vec::new(), &script);

    let (output, frames) =
        drive(&engine, &lease(&[], unbounded()), &CancellationToken::new()).await;

    let chunks: Vec<_> = frames
        .iter()
        .filter_map(|frame| match frame {
            ActivityFrame::FleetResponseChunk(chunk) => Some(chunk),
            _other => None,
        })
        .collect();
    let kinds: Vec<_> = chunks
        .iter()
        .map(|chunk| chunk.text_kind.unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            StreamTextKind::Reasoning,
            StreamTextKind::Answer,
            StreamTextKind::Answer
        ]
    );
    let seqs: Vec<_> = chunks.iter().map(|chunk| chunk.stream_seq).collect();
    assert_eq!(seqs, [0, 1, 2]);
    assert!(chunks[0].stream_start && chunks[0].first_chunk_after_ms.is_some());
    assert!(
        chunks[1..]
            .iter()
            .all(|chunk| !chunk.stream_start && chunk.first_chunk_after_ms.is_none())
    );
    assert!(chunks.iter().all(|chunk| chunk.stream_contiguous));
    assert_eq!(
        output.result.content, "hello",
        "reasoning is not the answer"
    );
    assert!(output.trace.is_none(), "a run with no call has no trace");
}

#[tokio::test]
async fn a_call_whose_process_exits_non_zero_ends_failed() {
    let script = Script::new([
        vec![call("a", UPDATE_PLAN.name(), serde_json::json!({}))],
        vec![say("done")],
    ]);
    let engine = engine(vec![Exits::boxed(&UPDATE_PLAN, 2)], &script);

    let (output, frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(
        completions(&frames),
        [("1".to_owned(), ToolCallStatus::Failed)]
    );
    assert_eq!(output.trace.unwrap().calls[0].exit_code, Some(2));
}

#[tokio::test]
async fn a_call_whose_process_exits_zero_ends_succeeded() {
    let script = Script::new([
        vec![call("a", UPDATE_PLAN.name(), serde_json::json!({}))],
        vec![say("done")],
    ]);
    let engine = engine(vec![Exits::boxed(&UPDATE_PLAN, 0)], &script);

    let (_output, frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(
        completions(&frames),
        [("1".to_owned(), ToolCallStatus::Succeeded)]
    );
}

#[tokio::test]
async fn no_secret_value_is_sent_to_the_model_whoever_wrote_it() {
    let script = Script::new([
        vec![
            say(API_KEY),
            call(
                "c",
                UPDATE_PLAN.name(),
                serde_json::json!({"token": GITHUB_TOKEN}),
            ),
        ],
        vec![say("done")],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, GITHUB_TOKEN)], &script);
    let mut leased = lease(&[UPDATE_PLAN.name()], unbounded());
    leased.event.request_json = format!("{{\"message\":\"use {GITHUB_TOKEN}\"}}").into();
    leased.instructions = format!("the key is {API_KEY}").into();

    let (_output, _frames) = drive(&engine, &leased, &CancellationToken::new()).await;

    let sent = format!("{:?}", script.sent());
    assert!(
        !sent.contains(API_KEY),
        "the key never reaches a prompt: {sent}"
    );
    assert!(!sent.contains(GITHUB_TOKEN), "nor a credential: {sent}");
    assert!(sent.contains("«secret:github.token»") && sent.contains("«secret:llm.api_key»"));
}

#[tokio::test]
async fn every_turn_and_every_call_logs_its_start_and_its_end_once() {
    let capture = Capture::install();
    let script = Script::new([
        vec![call("a", UPDATE_PLAN.name(), serde_json::json!({}))],
        vec![say("4")],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, "4")], &script);

    let (_output, _frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    let events = capture.events();
    let count = |name: &str| {
        let named = events
            .iter()
            .filter(|event| event.field("event") == Some(name));
        named.count()
    };
    assert_eq!(count(EVENT_TURN_STARTED), 2);
    assert_eq!(count(EVENT_TURN_COMPLETED), 2);
    assert_eq!(count(EVENT_CALL_STARTED), 1);
    let ended = capture.only(EVENT_CALL_COMPLETED);
    assert_eq!(
        ended.level,
        tracing::Level::DEBUG,
        "a per-pass line is debug"
    );
    assert_eq!(ended.field("call_id"), Some("1"));
    assert_eq!(ended.field("status"), Some("Succeeded"));
}

/// A credential minted for a call is logged under the lease the loop was
/// handed, so the operator reading the line finds the run that minted it.
#[tokio::test]
async fn a_credential_minted_during_a_run_is_logged_under_the_runs_lease() {
    let capture = Capture::install();
    let script = Script::new([
        vec![call(
            "a",
            HTTP_REQUEST.name(),
            serde_json::json!({"url": "https://api.github.com/repos/acme/widgets/pulls",
                "headers": {"Authorization": "Bearer ${secrets.github.token}"}}),
        )],
        vec![say("done")],
    ]);
    let (transport, _sent) = RecordingTransport::replying(200, "[]");
    let engine = Loop::new(Catalog::hosted(Arc::new(transport)), script.replay());
    let mut leased = lease(&[], unbounded());
    leased.lease_id = "lease-77".into();
    leased.policy = policy(false);
    leased.policy.tools = vec![HTTP_REQUEST.name().into()];
    let mint = CountingMint::answering(MINTED, HOUR_MILLIS, FixedClock::at(UnixMillis::EPOCH));
    let frames = Frames::default();
    let sink = frames.sink();

    let output = engine
        .run(AgentRun {
            lease: &leased,
            memory: afr_memory::Seed::default(),
            executor: None,
            mint: &mint,
            checkpoint: &Discard,
            events: &sink,
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap();

    assert_eq!(output.result.content, "done");
    assert_eq!(
        completions(&frames.taken()),
        [("1".to_owned(), ToolCallStatus::Succeeded)],
        "the call was admitted and sent"
    );
    let minted = capture.only(EVENT_CREDENTIAL_MINTED);
    assert_eq!(minted.field("lease_id"), Some(leased.lease_id.as_ref()));
    assert!(
        (capture.events().iter()).all(|event| event.fields.values().all(|v| !v.contains(MINTED))),
        "no log line carries the minted token"
    );
}

#[tokio::test]
async fn a_run_is_traced_as_turns_and_calls_inside_one_invocation() {
    let capture = Capture::install();
    let script = Script::new([
        vec![
            call("a", UPDATE_PLAN.name(), serde_json::json!({})),
            spent(10, 0, 5),
        ],
        vec![say("4")],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, "4")], &script);

    let (_output, _frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    let spans = capture.spans();
    let named = |name: &'static str| spans.iter().filter(move |span| span.name == name);
    assert!(
        spans.iter().all(|span| span.target == RUNNER_SCOPE_NAME),
        "{spans:?}"
    );
    let invoked: Vec<_> = named(OPERATION_INVOKE_AGENT).collect();
    assert_eq!(invoked.len(), 1);
    assert_eq!(invoked[0].field(ATTR_PROVIDER_NAME), Some("anthropic"));
    let turns: Vec<_> = named(OPERATION_CHAT).collect();
    assert_eq!(turns.len(), 2);
    assert!(
        turns
            .iter()
            .all(|turn| turn.parent == Some(OPERATION_INVOKE_AGENT))
    );
    assert_eq!(turns[0].field(ATTR_USAGE_INPUT_TOKENS), Some("10"));
    let tool = named(OPERATION_EXECUTE_TOOL).next().unwrap();
    assert_eq!(tool.parent, Some(OPERATION_INVOKE_AGENT));
    assert_eq!(tool.field(ATTR_TOOL_NAME), Some(UPDATE_PLAN.name()));
    assert_eq!(tool.field(ATTR_TOOL_CALL_ID), Some("1"));
}
