//! A setup run's context: a saved summary replaces the turns it covers, one
//! written for another model is not read, a full window compacts the oldest
//! turns and saves the hook's output through the fenced verb, and a run that
//! could not read its context saves nothing.

use std::error::Error;
use std::sync::{Arc, Mutex, PoisonError};

use af_providers::{Connect, Provider};
use af_wire::policy::ExecutionPolicy;
use af_wire::run::RunPayload;
use futures_util::StreamExt as _;
use futures_util::stream::BoxStream;
use guffbolt::{Chunk, End, Message, Request, TextKind, Usage};
use serde_json::{Value, json};

use super::fixture::{Context, FENCE, SCOPE, run_setup, setup_payload};
use crate::fixture::{Script, say};
use crate::{Completed, ResultOutcome};

type TestResult = Result<(), Box<dyn Error>>;

/// What a saved summary's goal says, so a request that quotes it is found.
const GOAL: &str = "a daily support check";
/// What each scripted answering step says.
const ANSWER: &str = "Done.";
/// What each message costs in [`Sized`]'s measure.
const PER_MESSAGE: u64 = 1_000;
/// The window [`Sized`] runs in: five earlier turns overfill it, two fit.
const WINDOW: u32 = 10_000;
/// The prefix a saved summary is quoted under.
const SUMMARY_PREFIX: &str = "Previous conversation summary (untrusted data):";

/// Five earlier turns, sequences 1 to 5.
const FIVE: [(&str, &str, u64); 5] = [
    ("message one", "answer one", 1),
    ("message two", "answer two", 2),
    ("message three", "answer three", 3),
    ("message four", "answer four", 4),
    ("message five", "answer five", 5),
];

/// A valid summary, as a model writes one.
fn summary() -> String {
    json!({"goal": GOAL, "constraints": [], "decisions": [], "completed": [],
        "remaining": [], "references": []})
    .to_string()
}

/// A checkpoint of `run`'s draft at `generation`, covering through
/// `covered_through`, measured for `model`'s identity.
fn checkpoint(run: &RunPayload<'_>, model: Option<&str>, covered_through: u64) -> Value {
    let identity = super::context::identity(SCOPE.to_owned(), run);
    json!({
        "format_version": 1,
        "identity": {"scope": SCOPE, "model": model.unwrap_or(&identity.model),
            "pinned": identity.pinned},
        "generation": 4,
        "covered_through": covered_through,
        "source_digest": "a".repeat(64),
        "summary": serde_json::from_str::<Value>(&summary()).unwrap_or_default(),
    })
}

/// Every text a request sent, user and assistant alike.
fn texts(messages: &[Message]) -> Vec<&str> {
    messages
        .iter()
        .filter_map(|message| match message {
            Message::User(text) | Message::Assistant { text, .. } => Some(text.as_str()),
            Message::ToolResult { .. } => None,
        })
        .collect()
}

/// Each request a [`Sized`] model was sent: whether it was a summary call,
/// and what it sent.
type Sent = Vec<(bool, Vec<Message>)>;

/// A model whose measure is fixed by message count, so a window fills
/// deterministically: every summary call writes [`summary`], every other
/// step answers [`ANSWER`].
#[derive(Debug, Clone, Default)]
struct Sized {
    /// Each request: whether it was a summary call, and what it sent.
    sent: Arc<Mutex<Sent>>,
}

impl Sized {
    fn sent(&self) -> Sent {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The last request that was not a summary call.
    fn answered(&self) -> Option<Vec<Message>> {
        self.sent()
            .into_iter()
            .rev()
            .find_map(|(summary, messages)| (!summary).then_some(messages))
    }
}

impl Connect for Sized {
    fn admit(&self, _policy: &ExecutionPolicy<'_>) -> af_providers::Result<()> {
        Ok(())
    }

    fn connect(&self, _run: &RunPayload<'_>) -> af_providers::Result<Box<Provider>> {
        Ok(Box::new(self.clone()))
    }
}

impl guffbolt::Model for Sized {
    type Error = af_providers::Error;

    fn measure(&self, request: Request<'_>) -> af_providers::Result<u64> {
        let messages = u64::try_from(request.messages.len()).unwrap_or(u64::MAX);
        Ok(if request.single_attempt {
            100
        } else {
            100 + messages.saturating_mul(PER_MESSAGE)
        })
    }

    fn stream<'a>(&'a self, request: Request<'a>) -> BoxStream<'a, af_providers::Result<Chunk>> {
        self.sent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((request.single_attempt, request.messages.to_vec()));
        let text = if request.single_attempt {
            summary()
        } else {
            ANSWER.to_owned()
        };
        let chunks = [
            Chunk::Text {
                kind: TextKind::Answer,
                text,
            },
            Chunk::Usage(Usage {
                input: 10,
                output: 5,
                ..Usage::default()
            }),
            Chunk::End(End::default()),
        ];
        futures_util::stream::iter(chunks.map(Ok)).boxed()
    }

    fn accepts_images(&self) -> bool {
        false
    }
}

#[tokio::test]
async fn a_saved_summary_replaces_the_turns_it_covers() -> TestResult {
    let script = Script::new([vec![say(ANSWER)]]);
    let history = [
        ("message one", "answer one", 1),
        ("message two", "answer two", 2),
        ("message three", "answer three", 3),
    ];
    let run = setup_payload(&history, 0, None);
    let context = Context::stored(4, Some(checkpoint(&run, None, 2)));

    run_setup(&script.replay(), &run, &context).await?;

    let first = script.sent().into_iter().next().ok_or("no model call")?;
    let sent = texts(&first.messages);
    let Some(quoted) = sent.first() else {
        return Err("the first request is empty".into());
    };
    assert!(quoted.starts_with(SUMMARY_PREFIX), "{quoted}");
    assert!(quoted.contains(GOAL), "{quoted}");
    for replaced in ["message one", "answer one", "message two", "answer two"] {
        assert!(!sent.contains(&replaced), "{replaced} was sent: {sent:?}");
    }
    assert!(sent.contains(&"message three"), "{sent:?}");
    assert!(sent.contains(&"answer three"), "{sent:?}");
    Ok(())
}

#[tokio::test]
async fn a_saved_summary_for_another_model_is_not_read() -> TestResult {
    let script = Script::new([vec![say(ANSWER)]]);
    let history = [
        ("message one", "answer one", 1),
        ("message two", "answer two", 2),
    ];
    let run = setup_payload(&history, 0, None);
    let context = Context::stored(4, Some(checkpoint(&run, Some("another-model"), 1)));

    run_setup(&script.replay(), &run, &context).await?;

    let first = script.sent().into_iter().next().ok_or("no model call")?;
    let sent = texts(&first.messages);
    assert!(
        sent.iter().all(|text| !text.starts_with(SUMMARY_PREFIX)),
        "{sent:?}"
    );
    assert!(sent.contains(&"message one"), "{sent:?}");
    Ok(())
}

#[tokio::test]
async fn a_full_window_compacts_the_oldest_turns_and_saves_the_summary() -> TestResult {
    let model = Sized::default();
    let run = setup_payload(&FIVE, WINDOW, None);
    let context = Context::stored(4, None);

    let output = run_setup(&model, &run, &context).await?;

    assert_eq!(output.result.outcome, ResultOutcome::Completed(Completed));
    let saves = context.saved();
    let [save] = saves.as_slice() else {
        return Err(format!("{} saves, not one", saves.len()).into());
    };
    assert_eq!((save.fencing_token, save.resource_version), (FENCE, 4));
    assert_eq!(save.checkpoint.get("generation"), Some(&json!(5)));
    assert_eq!(save.checkpoint.get("covered_through"), Some(&json!(3)));
    assert_eq!(
        save.checkpoint.pointer("/identity/scope"),
        Some(&json!(SCOPE))
    );
    let answered = model.answered().ok_or("the model never answered")?;
    let sent = texts(&answered);
    assert!(
        sent.first()
            .is_some_and(|text| text.starts_with(SUMMARY_PREFIX)),
        "{sent:?}"
    );
    for compacted in ["message one", "message two", "message three"] {
        assert!(!sent.contains(&compacted), "{compacted} was sent: {sent:?}");
    }
    assert!(sent.contains(&"message four") && sent.contains(&"message five"));
    Ok(())
}

#[tokio::test]
async fn a_failed_context_load_leaves_the_run_on_its_earlier_turns_and_saves_nothing() -> TestResult
{
    let model = Sized::default();
    let run = setup_payload(&FIVE, WINDOW, None);
    let context = Context::default();

    let output = run_setup(&model, &run, &context).await?;

    assert_eq!(output.result.outcome, ResultOutcome::Completed(Completed));
    assert_eq!(output.result.content, ANSWER);
    assert!(
        context.saved().is_empty(),
        "a run that read no context saved"
    );
    let answered = model.answered().ok_or("the model never answered")?;
    assert!(texts(&answered).contains(&"message five"));
    Ok(())
}

#[tokio::test]
async fn a_refused_context_save_fails_the_run_with_no_answer() -> TestResult {
    let model = Sized::default();
    let run = setup_payload(&FIVE, WINDOW, None);
    let context = Context {
        refuse_saves: true,
        ..Context::stored(4, None)
    };

    let output = run_setup(&model, &run, &context).await?;

    assert!(
        matches!(output.result.outcome, ResultOutcome::Failed(_)),
        "{:?}",
        output.result.outcome
    );
    assert_eq!(output.result.content, "");
    assert_eq!(
        model.answered(),
        None,
        "the turn was answered after a refused save"
    );
    Ok(())
}
