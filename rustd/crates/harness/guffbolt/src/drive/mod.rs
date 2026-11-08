//! The loop itself: the order every turn's steps run in.

use crate::conversation::{Conversation, Prepared, Sampled};
use crate::extension::{Answer, Budget, Extension};
use crate::turn::{Ending, ModelTurn as _};

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
/// [`Ending::Stopped`], and the extension hears of it.
pub async fn drive<C, E>(conversation: &mut C, extension: &mut E) -> Ending<C::Failure>
where
    C: Conversation + ?Sized,
    E: Extension<C>,
{
    match extension.budget(conversation) {
        Budget::Unbounded => turns(conversation, extension).await,
        Budget::Spent => Ending::Stopped,
        Budget::Within(limit) => {
            match tokio::time::timeout(limit, turns(conversation, extension)).await {
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

/// Every turn, in order, until one ends the loop.
async fn turns<C, E>(conversation: &mut C, extension: &mut E) -> Ending<C::Failure>
where
    C: Conversation + ?Sized,
    E: Extension<C>,
{
    let mut number: u64 = 0;
    let mut answers: Vec<String> = Vec::new();
    let ending = loop {
        number += 1;
        if conversation.take_input().await {
            taken(number, false);
        }
        if let Some(ending) = extension.before_model(conversation, number).await {
            break ending;
        }
        match conversation.prepare().await {
            Prepared::Ready => {}
            Prepared::Failed(failure) => break Ending::Failed(failure),
            Prepared::Stopped => break Ending::Stopped,
        }
        conversation.begin_group();
        let turn = match conversation.sample(number).await {
            Sampled::Turn(turn) => turn,
            Sampled::Failed(failure) => break Ending::Failed(failure),
            Sampled::Stopped => break Ending::Stopped,
        };
        if let Some(ending) = extension.after_turn(conversation, &turn) {
            break ending;
        }
        if !turn.has_calls() {
            match answered(conversation, extension, turn, number, &mut answers).await {
                Next::End(ending) => break ending,
                Next::Again => continue,
                Next::Done => break Ending::Answered(answers.join(ANSWER_JOIN)),
            }
        }
        let results = conversation.run_calls(&turn).await;
        conversation.remember(turn, results);
        if conversation.stopped() {
            break Ending::Stopped;
        }
        if let Some(ending) = extension.after_group(conversation).await {
            break ending;
        }
    };
    let (turns, kind, event) = (number, ending.kind(), EVENT_ENDED);
    tracing::debug!(turns, ending = kind, event);
    ending
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

/// Deals with turn `number`, which asked for no call: an answer, unless it is
/// the silence of a model that said nothing.
async fn answered<C, E>(
    conversation: &mut C,
    extension: &mut E,
    turn: C::Turn,
    number: u64,
    answers: &mut Vec<String>,
) -> Next<C::Failure>
where
    C: Conversation + ?Sized,
    E: Extension<C>,
{
    // An empty first turn must not turn a silent failure into an answer.
    if number == 1 && turn.text().trim().is_empty() {
        return Next::End(Ending::Failed(conversation.silent(turn.cut())));
    }
    match extension.on_answer(conversation, turn.text()).await {
        Answer::Accept => {}
        Answer::End(ending) => return Next::End(ending),
        Answer::Continue(why) => {
            conversation.remember(turn, C::Results::default());
            conversation.push_user(why);
            return Next::Again;
        }
    }
    // The answer is remembered before input is taken, so input that arrived
    // while it was written lands after it: the next call reads the answer, then
    // the correction, and no message already sent is edited.
    let said = turn.text().to_owned();
    conversation.remember(turn, C::Results::default());
    if !said.trim().is_empty() {
        answers.push(said);
    }
    if conversation.take_input().await {
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
