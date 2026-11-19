//! The loop itself: the order every step's steps run in, and what it tells
//! its observer as each one happens.

mod call;
mod state;
pub use state::DriveState;

use std::time::Instant;

use tracing::Instrument as _;

use crate::conversation::Conversation;
use crate::extension::{Answer, Budget, Extension};
use crate::model::Model as _;
use crate::observe::{Observe as _, StepEnd, StepOutcome};
use crate::step::{Ending, Step, Waiting};
use crate::tool::{ErrorCode, ToolOutput};
use crate::transcript::Message;
use crate::window::{Fitted, Window};

/// What joins two answering steps' text in one [`Ending::Answered`].
pub const ANSWER_JOIN: &str = "\n\n";

/// Input reached the conversation before a model call.
const EVENT_INPUT_TAKEN: &str = "loop_input_taken";
/// The loop's budget ran out and it stopped.
const EVENT_BUDGET_SPENT: &str = "loop_budget_spent";
/// The loop ended.
const EVENT_ENDED: &str = "loop_ended";
/// What a second question in one step reads back: one is asked at a time.
const ONE_QUESTION: &str = "another call of this step already asked the person a question; \
                            ask this one after the answer arrives";

/// Runs `conversation`'s steps, with `extension`'s additions, to their end.
///
/// The extension's budget bounds the whole loop: a loop it stops ends
/// [`Ending::Stopped`], and the extension hears of it. A call that asks the
/// person a question ends the loop [`Ending::Waiting`] once its step's other
/// calls ran, and a state still waiting ends it so again without a model
/// call. Whatever ends the loop, its observer hears the ending once.
pub async fn drive<C, E>(
    conversation: &mut C,
    extension: &mut E,
    state: &mut DriveState,
) -> Ending<C::Failure>
where
    C: Conversation + ?Sized,
    E: Extension<C>,
{
    let mut begun = 0;
    let ending = if let Some(waiting) = state.waiting.clone() {
        Ending::Waiting(waiting)
    } else {
        let budget = extension.budget(conversation);
        bounded(conversation, extension, state, &mut begun, budget).await
    };
    let (kind, event) = (ending.kind(), EVENT_ENDED);
    tracing::debug!(steps = begun, ending = kind, event);
    conversation.observer().loop_ended(begun, &ending);
    ending
}

/// Every step, under the extension's `budget`.
async fn bounded<C, E>(
    conversation: &mut C,
    extension: &mut E,
    state: &mut DriveState,
    begun: &mut u64,
    budget: Budget,
) -> Ending<C::Failure>
where
    C: Conversation + ?Sized,
    E: Extension<C>,
{
    match budget {
        Budget::Unbounded => steps(conversation, extension, state, begun).await,
        Budget::Spent => Ending::Stopped,
        Budget::Within(limit) => {
            let bounded = steps(conversation, extension, state, begun);
            match tokio::time::timeout(limit, bounded).await {
                Ok(ending) => ending,
                Err(_elapsed) => {
                    extension.budget_spent(conversation);
                    let event = EVENT_BUDGET_SPENT;
                    tracing::debug!(event);
                    Ending::Stopped
                }
            }
        }
    }
}

/// Every step, in order, until one ends the loop. `begun` counts the steps
/// begun, so a loop its budget cuts short still says how far it got.
async fn steps<C: Conversation + ?Sized, E: Extension<C>>(
    conversation: &mut C,
    extension: &mut E,
    state: &mut DriveState,
    begun: &mut u64,
) -> Ending<C::Failure> {
    loop {
        *begun += 1;
        let number = *begun;
        if take_input(conversation).await {
            taken(number, false);
        }
        if let Some(ending) = extension.before_model(conversation, number, state).await {
            break ending;
        }
        let window = match conversation.window().await {
            Fitted::Ready(window) => window,
            Fitted::Failed(failure) => break Ending::Failed(failure),
            Fitted::Stopped => break Ending::Stopped,
        };
        conversation.transcript().begin_group();
        let step = match sample(conversation, number, window).await {
            Some(Ok(step)) => step,
            Some(Err(failure)) => break Ending::Failed(failure),
            None => break Ending::Stopped,
        };
        if let Some(ending) = extension.after_step(conversation, &step) {
            break ending;
        }
        if step.calls.is_empty() {
            match answered(conversation, extension, step, number, state).await {
                Next::End(ending) => break ending,
                Next::Again => continue,
                Next::Done => break Ending::Answered(state.answers.join(ANSWER_JOIN)),
            }
        }
        let (results, asked) = run_calls(conversation, &step).await;
        remember(conversation, step, results);
        state.completed();
        if conversation.stop().is_cancelled() {
            break Ending::Stopped;
        }
        if let Some(waiting) = asked {
            state.waiting = Some(waiting.clone());
            break Ending::Waiting(waiting);
        }
        if let Some(ending) = extension.after_group(conversation).await {
            break ending;
        }
    }
}

/// What the loop does once a step without calls is dealt with.
enum Next<F> {
    /// The loop ends with this.
    End(Ending<F>),
    /// Another step runs.
    Again,
    /// The loop ends answered, with every answer in `answers`.
    Done,
}

/// Joins the input that arrived since the last call onto the transcript;
/// answers whether any came.
async fn take_input<C: Conversation + ?Sized>(conversation: &mut C) -> bool {
    let input = conversation.take_input().await;
    let took = !input.is_empty();
    let transcript = conversation.transcript();
    for text in input {
        transcript.join(text);
    }
    took
}

/// Makes model call `number`: the embedder's request, sending `window` and
/// offering its tools, streamed from its model inside the observer's span and
/// raced against the loop's stop. `None` when the loop was stopped before the
/// step was read. The window goes with the call, before any tool runs.
async fn sample<C: Conversation + ?Sized>(
    conversation: &C,
    number: u64,
    window: Window,
) -> Option<Result<Step, C::Failure>> {
    let observer = conversation.observer();
    let span = observer.step_started(number);
    let started = Instant::now();
    let offered = conversation.offered();
    let request = conversation.request(&window, &offered);
    let stream = conversation.model().stream(request);
    let reading = Step::read(stream, observer).instrument(span.clone());
    let read = tokio::select! {
        biased;
        () = conversation.stop().cancelled() => None,
        read = reading => Some(read),
    };
    let outcome = match &read {
        Some(Ok(step)) => StepOutcome::Read(step),
        Some(Err(failure)) => StepOutcome::Failed(failure),
        None => StepOutcome::Stopped,
    };
    observer.step_ended(StepEnd {
        number,
        span: &span,
        duration: started.elapsed(),
        outcome,
    });
    read
}

/// Runs `step`'s calls in order, or answers each unrun when the step was
/// cut, and answers their results: fewer than its calls when the loop was
/// stopped partway. A call that asked the person a question has no result
/// yet: it is the question the run waits on, and a second question in the
/// same step is answered that one is asked at a time.
async fn run_calls<C: Conversation + ?Sized>(
    conversation: &mut C,
    step: &Step,
) -> (Vec<Message>, Option<Waiting>) {
    let mut results = Vec::with_capacity(step.calls.len());
    let mut asked: Option<Waiting> = None;
    for call in &step.calls {
        let Some(mut output) = call::run(&*conversation, call, step.cut).await else {
            break;
        };
        if let Some(question) = output.asks.take() {
            if asked.is_none() {
                asked = Some(Waiting {
                    call_id: call.id.clone(),
                    question,
                });
                conversation.called().await;
                continue;
            }
            output = ToolOutput::failed(ErrorCode::QUESTION_PENDING, ONE_QUESTION);
        }
        // The image a call read rides its result alone.
        results.push(Message::ToolResult {
            call_id: call.id.clone(),
            output: output.text,
            image: output.image,
        });
        conversation.called().await;
    }
    (results, asked)
}

/// Keeps what the model said and called, scrubbed by the embedder, then the
/// results of its calls.
fn remember<C: Conversation + ?Sized>(conversation: &mut C, step: Step, results: Vec<Message>) {
    let Step {
        mut text,
        mut calls,
        replay,
        ..
    } = step;
    conversation.scrub(&mut text, &mut calls);
    let transcript = conversation.transcript();
    transcript.push(Message::Assistant {
        text,
        calls,
        replay,
    });
    transcript.extend(results);
}

/// Deals with step `number`, which asked for no call: an answer, unless it is
/// the silence of a model that said nothing.
async fn answered<C, E>(
    conversation: &mut C,
    extension: &mut E,
    step: Step,
    number: u64,
    state: &mut DriveState,
) -> Next<C::Failure>
where
    C: Conversation + ?Sized,
    E: Extension<C>,
{
    // An empty first step must not step a silent failure into an answer.
    if number == 1 && step.text.trim().is_empty() {
        return Next::End(Ending::Failed(conversation.silent(step.cut)));
    }
    match extension.on_answer(conversation, &step.text).await {
        Answer::Accept => {}
        Answer::End(ending) => return Next::End(ending),
        Answer::Continue(why) => {
            remember(conversation, step, Vec::new());
            state.completed();
            conversation.transcript().push(Message::User(why));
            return Next::Again;
        }
    }
    // The answer is remembered before input is taken, so input that arrived
    // while it was written lands after it: the next call reads the answer, then
    // the correction, and no message already sent is edited.
    let said = step.text.clone();
    remember(conversation, step, Vec::new());
    state.completed();
    if !said.trim().is_empty() {
        state.answers.push(said);
    }
    if take_input(conversation).await {
        taken(number, true);
        return Next::Again;
    }
    Next::Done
}

/// Records that input reached the conversation before step `step`'s call, or
/// after its answer.
fn taken(step: u64, after_answer: bool) {
    let event = EVENT_INPUT_TAKEN;
    tracing::debug!(step, after_answer, event);
}

#[cfg(test)]
mod tests;
