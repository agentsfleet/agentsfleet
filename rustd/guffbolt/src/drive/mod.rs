//! The loop itself: the order every turn's steps run in, and what it tells
//! its observer as each one happens.

mod call;
mod state;
pub use state::DriveState;

use std::time::Instant;

use tracing::Instrument as _;

use crate::conversation::Conversation;
use crate::extension::{Answer, Budget, Extension};
use crate::model::Model as _;
use crate::observe::{Observe as _, TurnEnd, TurnOutcome};
use crate::transcript::Message;
use crate::turn::{Ending, Turn};
use crate::window::{Fitted, Window};

/// What joins two answering turns' text in one [`Ending::Answered`].
pub const ANSWER_JOIN: &str = "\n\n";

/// Input reached the conversation before a model call.
const EVENT_INPUT_TAKEN: &str = "loop_input_taken";
/// The loop's budget ran out and it stopped.
const EVENT_BUDGET_SPENT: &str = "loop_budget_spent";
/// The loop ended.
const EVENT_ENDED: &str = "loop_ended";

/// Runs `conversation`'s turns, with `extension`'s additions, to their end.
///
/// The extension's budget bounds the whole loop: a loop it stops ends
/// [`Ending::Stopped`], and the extension hears of it. Whatever ends the
/// loop, its observer hears the ending once.
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
    let ending = match extension.budget(conversation) {
        Budget::Unbounded => turns(conversation, extension, state, &mut begun).await,
        Budget::Spent => Ending::Stopped,
        Budget::Within(limit) => {
            let bounded = turns(conversation, extension, state, &mut begun);
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
    };
    let (kind, event) = (ending.kind(), EVENT_ENDED);
    tracing::debug!(turns = begun, ending = kind, event);
    conversation.observer().loop_ended(begun, &ending);
    ending
}

/// Every turn, in order, until one ends the loop. `begun` counts the turns
/// begun, so a loop its budget cuts short still says how far it got.
async fn turns<C: Conversation + ?Sized, E: Extension<C>>(
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
        let turn = match sample(conversation, number, window).await {
            Some(Ok(turn)) => turn,
            Some(Err(failure)) => break Ending::Failed(failure),
            None => break Ending::Stopped,
        };
        if let Some(ending) = extension.after_turn(conversation, &turn) {
            break ending;
        }
        if turn.calls.is_empty() {
            match answered(conversation, extension, turn, number, state).await {
                Next::End(ending) => break ending,
                Next::Again => continue,
                Next::Done => break Ending::Answered(state.answers.join(ANSWER_JOIN)),
            }
        }
        let results = run_calls(conversation, &turn).await;
        remember(conversation, turn, results);
        state.completed();
        if conversation.stop().is_cancelled() {
            break Ending::Stopped;
        }
        if let Some(ending) = extension.after_group(conversation).await {
            break ending;
        }
    }
}

/// What the loop does once a turn without calls is dealt with.
enum Next<F> {
    /// The loop ends with this.
    End(Ending<F>),
    /// Another turn runs.
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
/// turn was read. The window goes with the call, before any tool runs.
async fn sample<C: Conversation + ?Sized>(
    conversation: &C,
    number: u64,
    window: Window,
) -> Option<Result<Turn, C::Failure>> {
    let observer = conversation.observer();
    let span = observer.turn_started(number);
    let started = Instant::now();
    let offered = conversation.offered();
    let request = conversation.request(&window, &offered);
    let stream = conversation.model().stream(request);
    let reading = Turn::read(stream, observer).instrument(span.clone());
    let read = tokio::select! {
        biased;
        () = conversation.stop().cancelled() => None,
        read = reading => Some(read),
    };
    let outcome = match &read {
        Some(Ok(turn)) => TurnOutcome::Read(turn),
        Some(Err(failure)) => TurnOutcome::Failed(failure),
        None => TurnOutcome::Stopped,
    };
    observer.turn_ended(TurnEnd {
        number,
        span: &span,
        duration: started.elapsed(),
        outcome,
    });
    read
}

/// Runs `turn`'s calls in order, or answers each unrun when the turn was
/// cut, and answers their results: fewer than its calls when the loop was
/// stopped partway.
async fn run_calls<C: Conversation + ?Sized>(conversation: &mut C, turn: &Turn) -> Vec<Message> {
    let mut results = Vec::with_capacity(turn.calls.len());
    for call in &turn.calls {
        let Some(output) = call::run(&*conversation, call, turn.cut).await else {
            break;
        };
        // The image a call read rides its result alone.
        results.push(Message::ToolResult {
            call_id: call.id.clone(),
            output: output.text,
            image: output.image,
        });
        conversation.called().await;
    }
    results
}

/// Keeps what the model said and called, scrubbed by the embedder, then the
/// results of its calls.
fn remember<C: Conversation + ?Sized>(conversation: &mut C, turn: Turn, results: Vec<Message>) {
    let Turn {
        mut text,
        mut calls,
        replay,
        ..
    } = turn;
    conversation.scrub(&mut text, &mut calls);
    let transcript = conversation.transcript();
    transcript.push(Message::Assistant {
        text,
        calls,
        replay,
    });
    transcript.extend(results);
}

/// Deals with turn `number`, which asked for no call: an answer, unless it is
/// the silence of a model that said nothing.
async fn answered<C, E>(
    conversation: &mut C,
    extension: &mut E,
    turn: Turn,
    number: u64,
    state: &mut DriveState,
) -> Next<C::Failure>
where
    C: Conversation + ?Sized,
    E: Extension<C>,
{
    // An empty first turn must not turn a silent failure into an answer.
    if number == 1 && turn.text.trim().is_empty() {
        return Next::End(Ending::Failed(conversation.silent(turn.cut)));
    }
    match extension.on_answer(conversation, &turn.text).await {
        Answer::Accept => {}
        Answer::End(ending) => return Next::End(ending),
        Answer::Continue(why) => {
            remember(conversation, turn, Vec::new());
            state.completed();
            conversation.transcript().push(Message::User(why));
            return Next::Again;
        }
    }
    // The answer is remembered before input is taken, so input that arrived
    // while it was written lands after it: the next call reads the answer, then
    // the correction, and no message already sent is edited.
    let said = turn.text.clone();
    remember(conversation, turn, Vec::new());
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

/// Records that input reached the conversation before turn `turn`'s call, or
/// after its answer.
fn taken(turn: u64, after_answer: bool) {
    let event = EVENT_INPUT_TAKEN;
    tracing::debug!(turn, after_answer, event);
}

#[cfg(test)]
mod tests;
