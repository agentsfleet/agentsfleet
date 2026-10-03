#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afd_wire::activity::{ActivityFrame, StreamTextKind};
use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::{Chunk, Message};
use afr_tools::ToolErrorCode;
use afr_tools::catalog::CALCULATOR;
use tokio_util::sync::CancellationToken;

use super::tests::{completions, drive, engine};
use crate::fixture::{Canned, Exits, Script, call, lease, say, unbounded};

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
    let engine = engine(vec![Canned::boxed(&CALCULATOR, "4")], &script);

    let (output, frames) = drive(
        &engine,
        &lease(&[CALCULATOR.name()], unbounded()),
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
        vec![call("a", CALCULATOR.name(), serde_json::json!({}))],
        vec![say("done")],
    ]);
    let engine = engine(vec![Exits::boxed(&CALCULATOR, 2)], &script);

    let (output, frames) = drive(
        &engine,
        &lease(&[CALCULATOR.name()], unbounded()),
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
        vec![call("a", CALCULATOR.name(), serde_json::json!({}))],
        vec![say("done")],
    ]);
    let engine = engine(vec![Exits::boxed(&CALCULATOR, 0)], &script);

    let (_output, frames) = drive(
        &engine,
        &lease(&[CALCULATOR.name()], unbounded()),
        &CancellationToken::new(),
    )
    .await;

    assert_eq!(
        completions(&frames),
        [("1".to_owned(), ToolCallStatus::Succeeded)]
    );
}
