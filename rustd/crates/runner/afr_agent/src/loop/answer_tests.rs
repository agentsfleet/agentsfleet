//! A person's answer to a run's question reaches the model exactly once,
//! whether the answering run resumes the asking run's checkpoint or, with no
//! checkpoint saved, starts fresh on the answer as its message.
#![expect(
    clippy::unwrap_used,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use af_wire::run::{CheckpointOperationId, RunPayload, TurnResume, TurnWorkspace};
use af_wire::turn::TurnKind;
use guffbolt::{Call, Message, Replay};
use tokio_util::sync::CancellationToken;

use super::tests::{drive, engine};
use crate::engine::{AgentEngine as _, AgentRun, Meter, RootRecovery};
use crate::fixture::{Frames, Script, payload, say, unbounded};
use crate::testing::Discard;
use crate::{TurnRecovery, TurnSnapshot};

const ANSWER: &str = "Denied: keep the release branch";
const ASKING_CALL: &str = "ask-1";

fn id(number: u32) -> CheckpointOperationId {
    CheckpointOperationId(format!("019f0000-0000-7000-8000-{number:012x}"))
}

/// A continuation turn carrying `ANSWER` to the question turn `id(3)` asked.
fn answering() -> RunPayload<'static> {
    let mut run = payload(&[], unbounded());
    run.event.workspace_id = id(1).0.into();
    run.event.fleet_id = id(2).0.into();
    run.event.turn_id = id(4).0.into();
    run.run_id = id(5).0.into();
    run.event.event_type = TurnKind::Continuation;
    run.event.request_json = serde_json::json!({ "message": ANSWER, "asked_turn_id": id(3).0 })
        .to_string()
        .into();
    run
}

/// The asking run's checkpoint: it asked through `ASKING_CALL` and waits on it.
fn asked(run: &mut RunPayload<'_>) -> TurnRecovery {
    let root = RootRecovery {
        messages: vec![
            Message::User("Clean up the merged branches".to_owned()),
            Message::Assistant {
                text: String::new(),
                calls: vec![Call {
                    id: ASKING_CALL.to_owned(),
                    name: "ask".to_owned(),
                    arguments: serde_json::json!({ "question": "Delete the release branch?" }),
                }],
                replay: Replay::default(),
            },
        ],
        driver: serde_json::from_value(serde_json::json!({
            "answers": [], "completed_steps": 1,
            "waiting": { "call_id": ASKING_CALL, "question": "Delete the release branch?" }
        }))
        .unwrap(),
        pinned: 1,
        ..RootRecovery::default()
    };
    let snapshot = TurnSnapshot::capture(run, TurnWorkspace::ApiOnly, None, root).unwrap();
    let descriptor = TurnResume {
        checkpoint_id: id(6),
        generation: 1,
        format_version: 1,
        scope: snapshot.scope,
        manifest_root: af_core::etag::sha256_hex(b"manifest"),
        history_digest: af_core::etag::sha256_hex(&snapshot.history),
    };
    run.turn_resume = Some(descriptor.clone());
    TurnRecovery {
        descriptor,
        history: snapshot.history,
        adopted: snapshot.folds,
        workspace: TurnWorkspace::ApiOnly,
    }
}

/// The messages of the first request the model was sent, and how many
/// times the answer appears in them.
fn first_messages(script: &Script) -> (Vec<Message>, usize) {
    let sent = script.sent();
    let messages = sent.first().unwrap().messages.clone();
    let count = format!("{messages:?}").matches(ANSWER).count();
    (messages, count)
}

// Value: an answer whose asking run saved no checkpoint opens the fresh run
// once, as the person's message, and is neither lost nor repeated.
#[tokio::test]
async fn a_fresh_answering_run_sends_the_answer_once_as_its_message() {
    let script = Script::new([vec![say("kept")]]);
    let engine = engine(vec![], &script);

    let (_output, _frames) = drive(&engine, &answering(), &CancellationToken::new()).await;

    let (messages, count) = first_messages(&script);
    assert_eq!(count, 1, "{messages:?}");
    assert_eq!(messages.last(), Some(&Message::User(ANSWER.to_owned())));
}

// Value: an answer that resumes the asking run's checkpoint is the asking
// call's result, once, and is not repeated as a message of its own.
#[tokio::test]
async fn a_resumed_answering_run_sends_the_answer_once_as_the_asking_calls_result() {
    let mut run = answering();
    let recovered = asked(&mut run);
    let script = Script::new([vec![say("kept")]]);
    let engine = engine(vec![], &script);
    let frames = Frames::default();
    let events = frames.sink();
    let stop = CancellationToken::new();

    engine
        .run(AgentRun {
            recovery: None,
            recovered_durable_task: None,
            recovered_turn: Some(&recovered),
            prepared_durable_task: None,
            run: &run,
            memory: afr_memory::Seed::default(),
            agent: &afr_guest_client::AgentSlot::unavailable(
                afr_guest_client::Unavailable::NoSandbox,
            ),
            mint: &af_egress::testing::CountingMint::never(),
            verbs: &afr_tools::CLOSED,
            checkpoint: &Discard,
            inbox: &crate::testing::NoInbox,
            events: &events,
            meter: &Meter::default(),
            stop: &stop,
        })
        .await
        .unwrap();
    drop(frames.taken());

    let (messages, count) = first_messages(&script);
    assert_eq!(count, 1, "{messages:?}");
    assert_eq!(
        messages.last(),
        Some(&Message::ToolResult {
            call_id: ASKING_CALL.to_owned(),
            output: ANSWER.to_owned(),
            image: None,
        })
    );
}
