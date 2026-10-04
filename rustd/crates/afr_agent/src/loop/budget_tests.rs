#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_core::test_util::trace::Capture;
use afd_wire::activity::ActivityFrame;
use afd_wire::report::{FailureClass, ResultOutcome};
use afr_egress::testing::CountingMint;
use afr_providers::{Error, Message};
use afr_tools::catalog::{FILE_READ, HTTP_REQUEST, UPDATE_PLAN, WEB_SEARCH};
use afr_tools::stub::Stub;
use tokio_util::sync::CancellationToken;

use super::EVENT_CAP_REACHED;
use super::tests::{drive, engine};
use crate::context::{CAP_REACHED, EVICTED};
use crate::engine::{AgentEngine, AgentRun, Meter};
use crate::fixture::{
    API_KEY, Canned, Frames, GITHUB_TOKEN, Script, Unreachable, budget, call, lease, say, spent,
    unbounded,
};
use crate::testing::Discard;

/// A cap no test here reaches.
const WIDE_CAP: u32 = 1000;
/// The window the capped tests run in, and a prompt past its 0.75 stage
/// fraction that is still inside it, as a provider would answer it.
const CAP: u32 = 100;
const FILLED: u64 = 80;

/// A turn that calls the plan tool once and reports `input` prompt tokens.
fn one_call(id: &str, input: u64) -> Vec<afr_providers::Chunk> {
    vec![
        call(id, UPDATE_PLAN.name(), serde_json::json!({})),
        spent(input, 0, 1),
    ]
}

/// The outputs of every tool result in `messages`.
fn results(messages: &[Message]) -> Vec<&str> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { output, .. } => Some(output.as_str()),
            Message::User(_) | Message::Assistant { .. } => None,
        })
        .collect()
}

#[tokio::test]
async fn test_loop_honours_context_budget() {
    let script = Script::new([
        one_call("a", 10),
        one_call("b", 10),
        one_call("c", 10),
        one_call("d", 10),
        vec![say("done")],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, "4")], &script);

    drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], budget(2, WIDE_CAP)),
        &CancellationToken::new(),
    )
    .await;

    let last = script.sent().pop().unwrap();
    assert_eq!(results(&last.messages), [EVICTED, EVICTED, "4", "4"]);
    assert_eq!(
        last.tools,
        [UPDATE_PLAN.name()],
        "the cap was never reached"
    );
}

#[tokio::test]
async fn reaching_the_context_cap_offers_no_tools_and_asks_for_the_answer() {
    let capture = Capture::install();
    let script = Script::new([one_call("a", FILLED), vec![say("partial answer")]]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, "4")], &script);

    let (output, _frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name(), "web_search"], budget(0, CAP)),
        &CancellationToken::new(),
    )
    .await;

    let sent = script.sent();
    assert_eq!(sent[0].tools, [UPDATE_PLAN.name()]);
    assert_eq!(sent[0].hosted, [WEB_SEARCH.name()]);
    assert!(sent[1].tools.is_empty() && sent[1].hosted.is_empty());
    assert_eq!(
        sent[1].messages.last(),
        Some(&Message::User(CAP_REACHED.to_owned()))
    );
    assert_eq!(output.result.content, "partial answer");
    let reached = capture.only(EVENT_CAP_REACHED);
    assert_eq!(reached.level, tracing::Level::INFO);
    assert_eq!(reached.field("turns"), Some("1"));
    assert_eq!(reached.field("tokens"), Some(FILLED.to_string().as_str()));
    assert_eq!(reached.field("lease_id"), Some("lease-1"));
}

#[tokio::test]
async fn a_call_made_after_the_cap_ends_the_run_with_its_text() {
    let script = Script::new([
        one_call("a", FILLED),
        vec![
            say("enough"),
            call("b", UPDATE_PLAN.name(), serde_json::json!({})),
        ],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, "4")], &script);

    let (output, _frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], budget(0, CAP)),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(output.result.content, "enough");
    assert_eq!(output.records.len(), 1, "the call past the cap never ran");
}

#[tokio::test]
async fn test_report_sums_token_usage() {
    let script = Script::new([
        vec![
            call("a", UPDATE_PLAN.name(), serde_json::json!({})),
            spent(10, 2, 5),
        ],
        vec![
            call("b", UPDATE_PLAN.name(), serde_json::json!({})),
            spent(20, 4, 6),
        ],
        vec![say("done"), spent(5, 0, 1)],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, "4")], &script);

    let (output, _frames) = drive(
        &engine,
        &lease(&[UPDATE_PLAN.name()], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    let result = output.result;
    assert_eq!(
        (
            result.input_tokens,
            result.cached_input_tokens,
            result.output_tokens
        ),
        (35, 6, 12)
    );
    assert_eq!(result.token_count, 47);
}

/// The supervisor reads the meter into every renewal while the run goes on,
/// so it holds, turn by turn, what the report later sums.
#[tokio::test]
async fn test_meter_holds_what_the_report_sums() {
    let script = Script::new([
        vec![
            call("a", UPDATE_PLAN.name(), serde_json::json!({})),
            spent(10, 2, 5),
        ],
        vec![say("done"), spent(5, 0, 1)],
    ]);
    let engine = engine(vec![Canned::boxed(&UPDATE_PLAN, "4")], &script);
    let lease = lease(&[UPDATE_PLAN.name()], unbounded());
    let frames = Frames::default();
    let sink = frames.sink();
    let meter = Meter::default();

    let output = engine
        .run(AgentRun {
            lease: &lease,
            memory: afr_memory::Seed::default(),
            executor: None,
            mint: &CountingMint::never(),
            checkpoint: &Discard,
            events: &sink,
            meter: &meter,
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap();

    let usage = meter.read();
    assert_eq!((usage.input, usage.cached_input, usage.output), (15, 2, 6));
    assert_eq!(output.result.token_count, usage.total());
    assert!(
        !frames.taken().is_empty(),
        "the run streamed its call and answer"
    );
}

#[tokio::test]
async fn a_provider_refusal_ends_the_run_as_the_fleets_error() {
    let script = Script::failing(Vec::new(), || Error::refused(401));
    let engine = engine(Vec::new(), &script);

    let (output, _frames) =
        drive(&engine, &lease(&[], unbounded()), &CancellationToken::new()).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a refused turn fails the run");
    };
    assert_eq!(
        failure.class, None,
        "the fleet's error carries no failure reason"
    );
    assert!(failure.detail.contains("401"), "{}", failure.detail);
    assert!(
        !failure.detail.contains("UZ-"),
        "the detail is a sentence, not a code"
    );
}

#[tokio::test]
async fn a_lost_provider_connection_ends_the_run_as_transport_loss() {
    let lost = || Error::lost(std::io::Error::other("connection reset"));
    let script = Script::failing(vec![say("partial ")], lost);
    let engine = engine(Vec::new(), &script);

    let (output, _frames) =
        drive(&engine, &lease(&[], unbounded()), &CancellationToken::new()).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a lost turn fails the run");
    };
    assert_eq!(failure.class, Some(FailureClass::TransportLoss));
    assert_eq!(output.result.content, "", "a failed run reports no answer");
}

#[tokio::test]
async fn test_secret_values_masked_in_outputs() {
    let echo = format!("token={GITHUB_TOKEN}\nkey={API_KEY}");
    let script = Script::new([
        vec![call(
            "a",
            "http_request",
            serde_json::json!({"auth": GITHUB_TOKEN}),
        )],
        vec![say(&format!("the key was {API_KEY}"))],
    ]);
    let engine = engine(vec![Canned::boxed(&HTTP_REQUEST, &echo)], &script);

    let (output, frames) = drive(
        &engine,
        &lease(&["http_request"], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    let masked = "token=«secret:github.token»\nkey=«secret:llm.api_key»";
    let edge = frames.iter().find_map(|frame| match frame {
        ActivityFrame::ToolCallCompleted(done) => done.output_head.clone(),
        _other => None,
    });
    assert_eq!(edge.as_deref(), Some(masked), "the frame's edge");
    let trace = output.trace.unwrap();
    assert_eq!(
        trace.calls[0].output_head.as_deref(),
        Some(masked),
        "the trace row"
    );
    assert_eq!(trace.calls[0].arguments["auth"], "«secret:github.token»");
    assert_eq!(output.records[0].output, masked, "the record");
    assert_eq!(
        results(&script.sent()[1].messages),
        [masked],
        "what the model reads back"
    );
    assert_eq!(
        output.result.content, "the key was «secret:llm.api_key»",
        "the answer"
    );
    let rendered = format!("{frames:?}{trace:?}{:?}", output.records);
    assert!(!rendered.contains(GITHUB_TOKEN) && !rendered.contains(API_KEY));
}

#[tokio::test]
async fn a_provider_that_cannot_be_reached_is_an_engine_error() {
    let engine = super::Loop::new(afr_tools::Catalog::new(Vec::new()), Unreachable);
    let lease = lease(&[], unbounded());
    let frames = Frames::default();
    let sink = frames.sink();

    let failure = engine
        .run(AgentRun {
            lease: &lease,
            memory: afr_memory::Seed::default(),
            executor: None,
            mint: &CountingMint::never(),
            checkpoint: &Discard,
            events: &sink,
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap_err();

    assert_eq!(
        failure.code(),
        afd_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(frames.taken().is_empty());
}

#[test]
fn the_loop_admits_through_its_catalog() {
    let engine = engine(
        vec![Stub::boxed(&UPDATE_PLAN), Stub::boxed(&FILE_READ)],
        &Script::new([]),
    );

    assert!(
        !engine
            .admit(&lease(&[UPDATE_PLAN.name(), "web_search"], unbounded()).policy)
            .unwrap()
            .sandbox
    );
    assert!(
        engine
            .admit(&lease(&["file_read"], unbounded()).policy)
            .unwrap()
            .sandbox
    );
    let refused = engine
        .admit(&lease(&["browser"], unbounded()).policy)
        .unwrap_err();
    assert_eq!(refused.unhosted(), Some(crate::Unhosted::Tool("browser")));
}
