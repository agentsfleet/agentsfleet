#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::activity::ActivityFrame;
use afr_egress::testing::CountingMint;
use afr_providers::Message;
use afr_tools::catalog::{FILE_READ, HTTP_REQUEST, UPDATE_PLAN};
use afr_tools::stub::Stub;
use tokio_util::sync::CancellationToken;

use super::tests::{drive, engine};
use crate::engine::{AgentEngine, AgentRun, Meter};
use crate::fixture::{
    API_KEY, Canned, Frames, GITHUB_TOKEN, Script, call, lease, say, spent, unbounded,
};
use crate::testing::Discard;

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
    assert_eq!(
        result.token_count, 53,
        "the whole prompt, cache reads included, and the completion"
    );
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
            verbs: &afr_tools::CLOSED,
            checkpoint: &Discard,
            context_checkpoints: &crate::testing::ContextMemory::default(),
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
