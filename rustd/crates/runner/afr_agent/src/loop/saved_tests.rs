//! A fleet chat's saved summary: the next run opens on it in place of the
//! turns it covers, a compaction that covers earlier turns whole saves the
//! thread's next summary at the stored version, and a save at a superseded
//! fence fails the run.
#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use af_core::error_code;
use af_providers::{Connect, Provider};
use af_secrets::Scrub;
use af_wire::context::{ContextLoadResponse, ContextSaveRequest, ContextSaveResponse};
use af_wire::policy::ExecutionPolicy;
use af_wire::run::{RunPayload, Turn};
use afr_tools::catalog::UPDATE_PLAN;
use afr_tools::{RunVerbs, ScheduleCall, Unanswered};
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use guffbolt::{Chunk, End, Message, Request};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::Loop;
use crate::fixture::{Canned, Frames, payload, say, spent, unbounded};
use crate::prompt::Prompt;
use crate::testing::Discard;
use crate::{AgentEngine as _, AgentRun, Meter, ResultOutcome, RunOutput};

/// The scope the daemon assigns the fixture fleet's checkpoints.
const SCOPE: &str = "fleet:fleet-1";
/// The prefix a saved summary is quoted under.
const SUMMARY_PREFIX: &str = "Previous conversation summary (untrusted data):";
/// What the saved summary's goal says, so a request that quotes it is found.
const GOAL: &str = "triage the nightly failures";
/// What the fixture run's model answers.
const ANSWER: &str = "done";
/// What each message costs in [`Sized`]'s measure.
const PER_MESSAGE: u64 = 1_000;
/// The window [`Sized`] runs in: its trigger is 9,000 tokens, nine messages.
const WINDOW: u32 = 10_000;
/// The stored version a saved checkpoint sits at.
const STORED: u64 = 4;

/// A valid summary, as a model writes one.
fn summary() -> Value {
    json!({"goal": GOAL, "constraints": [], "decisions": [], "completed": [],
        "remaining": [], "references": []})
}

/// A fleet chat carrying `count` earlier turns, sequences from 1, in a window
/// of [`WINDOW`] tokens.
fn chat(count: u64) -> RunPayload<'static> {
    let mut run = payload(&[UPDATE_PLAN.name()], unbounded());
    run.history = (1..=count)
        .map(|sequence| Turn {
            sequence,
            message: format!("message {sequence}").into(),
            calls: Vec::new(),
            answer: format!("answer {sequence}").into(),
        })
        .collect();
    run.policy.model_context_tokens = WINDOW;
    run.policy.context.context_cap_tokens = WINDOW;
    run.policy.context.stage_chunk_threshold = 0.90;
    run.policy.inference = Some(af_wire::inference::ModelInference {
        api: af_wire::inference::InferenceApi::Chat,
        base_url: "https://models.example.com/v1".into(),
        dialect: None,
        max_output_tokens: 2_048,
    });
    run
}

/// A saved checkpoint of `run`'s thread, covering through `covered_through`,
/// written for this run's model and prompt.
fn checkpoint(run: &RunPayload<'_>, covered_through: u64) -> Value {
    let scrub = Scrub::new(&run.policy).unwrap();
    let instructions = scrub.clean(Prompt::new(run).instructions).into_inner();
    let (model, pinned) = super::context::digests(
        &run.policy.context.model,
        run.policy.inference.as_ref(),
        run.policy.model_context_tokens,
        &instructions,
    )
    .unwrap();
    json!({
        "format_version": 1,
        "identity": {"scope": SCOPE, "model": model, "pinned": pinned},
        "generation": STORED,
        "covered_through": covered_through,
        "source_digest": "a".repeat(64),
        "summary": summary(),
    })
}

/// The fleet's thread, as a fake of the daemon's context verbs serves it.
#[derive(Debug, Default)]
struct Thread {
    /// What a load answers; none leaves it unanswered.
    loaded: Option<ContextLoadResponse>,
    /// What every save is refused with; none stores it.
    refusal: Option<Unanswered>,
    /// Every save the run asked for, in order.
    saves: Mutex<Vec<ContextSaveRequest>>,
}

impl Thread {
    /// A thread whose stored context is `checkpoint` at `version`.
    fn stored(version: u64, checkpoint: Option<Value>) -> Self {
        Self {
            loaded: Some(ContextLoadResponse {
                scope: SCOPE.to_owned(),
                resource_version: version,
                checkpoint,
            }),
            ..Self::default()
        }
    }

    /// The saves asked for so far.
    fn saved(&self) -> Vec<ContextSaveRequest> {
        self.saves
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl RunVerbs for Thread {
    async fn schedules(&self, _call: ScheduleCall<'_>) -> Result<String, Unanswered> {
        Err(Unanswered::Unreachable)
    }

    async fn message(&self, _text: &str) -> Result<bool, Unanswered> {
        Err(Unanswered::Unreachable)
    }

    async fn context_load(&self) -> Result<ContextLoadResponse, Unanswered> {
        self.loaded.clone().ok_or(Unanswered::Unreachable)
    }

    async fn context_save(
        &self,
        request: ContextSaveRequest,
    ) -> Result<ContextSaveResponse, Unanswered> {
        if let Some(refusal) = self.refusal {
            return Err(refusal);
        }
        let resource_version = request.resource_version + 1;
        self.saves
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request);
        Ok(ContextSaveResponse { resource_version })
    }
}

/// A model that costs [`PER_MESSAGE`] a message, writes a summary when asked
/// for one and answers [`ANSWER`] otherwise, keeping every answering
/// request's messages.
#[derive(Debug, Clone, Default)]
struct Sized {
    answering: Arc<Mutex<Vec<Vec<Message>>>>,
    summaries: Arc<AtomicUsize>,
}

impl Sized {
    fn answering(&self) -> Vec<Vec<Message>> {
        self.answering
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Connect for Sized {
    fn admit(&self, _: &ExecutionPolicy<'_>) -> af_providers::Result<()> {
        Ok(())
    }
    fn connect(&self, _: &RunPayload<'_>) -> af_providers::Result<Box<Provider>> {
        Ok(Box::new(self.clone()))
    }
}

impl guffbolt::Model for Sized {
    type Error = af_providers::Error;

    fn measure(&self, request: Request<'_>) -> af_providers::Result<u64> {
        Ok(if request.single_attempt {
            100
        } else {
            100 + request.messages.len() as u64 * PER_MESSAGE
        })
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, af_providers::Result<Chunk>> {
        let mut chunks = if request.single_attempt {
            self.summaries.fetch_add(1, Ordering::SeqCst);
            vec![say(&summary().to_string()), spent(100, 0, 5)]
        } else {
            self.answering
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(request.messages.to_vec());
            vec![say(ANSWER), spent(100, 0, 1)]
        };
        chunks.push(Chunk::End(End::default()));
        futures_util::stream::iter(chunks.into_iter().map(Ok)).boxed()
    }

    fn accepts_images(&self) -> bool {
        false
    }
}

/// Runs `run` to its end on `model`, its thread served by `thread`.
async fn drive(model: &Sized, run: &RunPayload<'_>, thread: &Thread) -> RunOutput {
    let engine = Loop::new(
        afr_tools::Catalog::new(vec![Canned::boxed(&UPDATE_PLAN, "planned")]),
        model.clone(),
    );
    let frames = Frames::default();
    let output = engine
        .run(AgentRun {
            recovery: None,
            recovered_task: None,
            recovered_turn: None,
            prepared_task: None,
            run,
            memory: afr_memory::Seed::default(),
            executor: &afr_executor::ExecutorSlot::unavailable(
                afr_executor::Unavailable::NoSandbox,
            ),
            mint: &af_egress::testing::CountingMint::never(),
            verbs: thread,
            checkpoint: &Discard,
            inbox: &crate::testing::NoInbox,
            events: &frames.sink(),
            meter: &Meter::default(),
            stop: &CancellationToken::new(),
        })
        .await
        .unwrap();
    // The frames are the compaction's progress and the answer, which other
    // suites prove.
    drop(frames.taken());
    output
}

/// Every user and assistant text `messages` holds.
fn texts(messages: &[Message]) -> Vec<&str> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::User(text) | Message::Assistant { text, .. } => Some(text.as_str()),
            Message::ToolResult { .. } => None,
        })
        .collect()
}

/// The next run's first request opens on the saved summary, carries none
/// of the turns it covers, and carries every turn after them.
#[tokio::test]
async fn the_next_fleet_run_opens_on_the_saved_summary() {
    let run = chat(4);
    let thread = Thread::stored(STORED, Some(checkpoint(&run, 2)));
    let model = Sized::default();

    let output = drive(&model, &run, &thread).await;

    assert_eq!(output.result.content, ANSWER);
    let first = &model.answering()[0];
    let sent = texts(first);
    assert!(sent[0].starts_with(SUMMARY_PREFIX), "{sent:?}");
    assert!(sent[0].contains(GOAL), "{sent:?}");
    for covered in ["message 1", "answer 1", "message 2", "answer 2"] {
        assert!(!sent.contains(&covered), "{covered} was sent: {sent:?}");
    }
    assert_eq!(
        sent[1..],
        [
            "message 3",
            "answer 3",
            "message 4",
            "answer 4",
            "triage the failed run"
        ]
    );
    assert!(
        thread.saved().is_empty(),
        "nothing compacted, nothing saved"
    );
}

/// A summary saved for another model or prompt is not read: every turn is
/// sent.
#[tokio::test]
async fn a_fleet_summary_for_another_model_is_not_read() {
    // Three turns and the message fit the window, so nothing compacts.
    let run = chat(3);
    let mut stored = checkpoint(&run, 2);
    stored["identity"]["model"] = json!("another-model");
    let thread = Thread::stored(STORED, Some(stored));
    let model = Sized::default();

    drive(&model, &run, &thread).await;

    let sent = texts(&model.answering()[0]).join("\n");
    assert!(!sent.contains(SUMMARY_PREFIX), "{sent}");
    assert!(sent.contains("message 1"), "{sent}");
}

/// A first compaction over earlier turns saves the thread's first summary,
/// covering the turns it holds whole, at version zero; the run answers on.
#[tokio::test]
async fn a_fleet_compaction_saves_the_turns_it_covers() {
    // Six turns and the message are thirteen messages, past the trigger.
    let run = chat(6);
    let thread = Thread::stored(0, None);
    let model = Sized::default();

    let output = drive(&model, &run, &thread).await;

    assert_eq!(output.result.content, ANSWER);
    let saves = thread.saved();
    assert_eq!(saves.len(), 1, "{saves:?}");
    assert_eq!(saves[0].resource_version, 0);
    let checkpoint = &saves[0].checkpoint;
    assert_eq!(checkpoint["identity"]["scope"], json!(SCOPE));
    assert_eq!(checkpoint["generation"], json!(1));
    // The planner keeps the newest two turns whole: four are covered.
    assert_eq!(checkpoint["covered_through"], json!(4));
    assert_eq!(checkpoint["summary"]["goal"], json!(GOAL));
}

/// A run that opened on a saved summary saves the next one over the stored
/// version, its watermark past the stored one.
#[tokio::test]
async fn a_fleet_compaction_saves_past_the_stored_summary() {
    let run = chat(8);
    let thread = Thread::stored(STORED, Some(checkpoint(&run, 2)));
    let model = Sized::default();

    let output = drive(&model, &run, &thread).await;

    assert_eq!(output.result.content, ANSWER);
    let saves = thread.saved();
    assert_eq!(saves.len(), 1, "{saves:?}");
    assert_eq!(saves[0].resource_version, STORED);
    let checkpoint = &saves[0].checkpoint;
    assert_eq!(checkpoint["generation"], json!(STORED + 1));
    // The summary and turns 3 to 8 are seven groups; the newest two stay.
    assert_eq!(checkpoint["covered_through"], json!(6));
}

/// A save at a superseded fence is refused, and the run ends there: its
/// report would be refused too.
#[tokio::test]
async fn a_fleet_summary_at_a_superseded_fence_fails_the_run() {
    let run = chat(6);
    let thread = Thread {
        refusal: Some(Unanswered::Refused(Some(
            error_code::RUN_STALE_FENCING_TOKEN,
        ))),
        ..Thread::stored(0, None)
    };
    let model = Sized::default();

    let output = drive(&model, &run, &thread).await;

    assert!(
        matches!(output.result.outcome, ResultOutcome::Failed(_)),
        "{:?}",
        output.result.outcome
    );
    assert!(model.answering().is_empty(), "the run asked the model on");
}

/// Any other refused save is logged and the run answers on, saving nothing
/// more.
#[tokio::test]
async fn a_refused_fleet_summary_leaves_the_run_answering() {
    let run = chat(6);
    let thread = Thread {
        refusal: Some(Unanswered::Refused(None)),
        ..Thread::stored(0, None)
    };
    let model = Sized::default();

    let output = drive(&model, &run, &thread).await;

    assert_eq!(output.result.content, ANSWER);
    let saves = thread.saved();
    assert!(saves.is_empty(), "{saves:?}");
}

/// A run whose load went unanswered sends every turn and saves nothing: it
/// knows no version to save over.
#[tokio::test]
async fn a_failed_fleet_context_load_saves_nothing() {
    let run = chat(6);
    let thread = Thread::default();
    let model = Sized::default();

    let output = drive(&model, &run, &thread).await;

    assert_eq!(output.result.content, ANSWER);
    let saves = thread.saved();
    assert!(saves.is_empty(), "{saves:?}");
    assert_eq!(
        model.summaries.load(Ordering::SeqCst),
        1,
        "it still compacts"
    );
}
