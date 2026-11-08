//! The embedder's half of a loop: everything a turn touches.

use crate::turn::ModelTurn;

/// Whether the conversation fits the next model call.
#[derive(Debug)]
pub enum Prepared<F> {
    /// It fits.
    Ready,
    /// Fitting it failed, and the run with it.
    Failed(F),
    /// The loop was stopped while it was being fitted.
    Stopped,
}

/// What one model call came back with.
#[derive(Debug)]
pub enum Sampled<T, F> {
    /// The turn, read to its end.
    Turn(T),
    /// The call failed.
    Failed(F),
    /// The loop was stopped during the call.
    Stopped,
}

/// One loop's conversation, as the embedder keeps it: its input, its window,
/// its model and its tools.
///
/// [`crate::drive`] calls these in a fixed order and decides nothing about
/// their content. A turn, its tools' results and a failure are the embedder's
/// own types: the loop reads only what [`ModelTurn`] says of a turn, so it
/// links no model client, no tool and no secret store. An embedder writes
/// each async method as an `async fn`; its future must be `Send`.
pub trait Conversation: Send {
    /// One model turn, as the embedder reads it.
    type Turn: ModelTurn;
    /// What a turn's tool calls answered.
    type Results: Default + Send;
    /// Why a model call failed.
    type Failure: Send;

    /// Moves the input that arrived since the last call into the
    /// conversation, joined onto a waiting user message so no provider sees
    /// two in a row; answers whether any came. Input taken after an answer
    /// follows that answer, which the loop has remembered first.
    fn take_input(&mut self) -> impl Future<Output = bool> + Send;

    /// Fits the conversation to the next call's window, compacting it when
    /// it is over budget.
    fn prepare(&mut self) -> impl Future<Output = Prepared<Self::Failure>> + Send;

    /// Marks where the action group the next turn starts begins.
    fn begin_group(&mut self);

    /// Makes one model call; `number` counts the loop's turns from 1.
    fn sample(
        &mut self,
        number: u64,
    ) -> impl Future<Output = Sampled<Self::Turn, Self::Failure>> + Send;

    /// Runs `turn`'s calls in order, or answers each unrun when the turn was
    /// cut, and answers their results: fewer than its calls when the loop was
    /// stopped partway.
    fn run_calls(&mut self, turn: &Self::Turn) -> impl Future<Output = Self::Results> + Send;

    /// Keeps what the model said and called, then the results of its calls.
    fn remember(&mut self, turn: Self::Turn, results: Self::Results);

    /// Adds a user message to the conversation.
    fn push_user(&mut self, text: String);

    /// Whether the loop has been told to stop.
    fn stopped(&self) -> bool;

    /// The failure a blank first turn is: a model that answered nothing,
    /// `cut` when its turn stopped at the output limit.
    fn silent(&self, cut: bool) -> Self::Failure;
}
