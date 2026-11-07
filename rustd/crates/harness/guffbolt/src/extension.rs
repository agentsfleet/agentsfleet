//! What a run of one kind adds to the loop, at the points it may add it.

use std::time::Duration;

use crate::conversation::Conversation;
use crate::turn::Ending;

/// What becomes of an answer the model gave without a call.
#[derive(Debug)]
pub enum Answer<F> {
    /// The loop ends with it, unless input arrived while it was written.
    Accept,
    /// The loop ends here instead.
    End(Ending<F>),
    /// The answer is kept and the loop goes on, the model told why.
    Continue(String),
}

/// How long the loop may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Budget {
    /// Until it ends on its own.
    Unbounded,
    /// For at most this long; then it stops.
    Within(Duration),
    /// Not at all: the time is already spent.
    Spent,
}

/// A run's additions to the loop, typed against the embedder's conversation
/// so each hook reaches what that embedder keeps.
///
/// Every hook defaults to adding nothing, so an extension names only the
/// points it uses. [`Plain`] uses none. An extension writes each async hook
/// as an `async fn`; its future must be `Send`.
pub trait Extension<C: Conversation + ?Sized>: Send {
    /// How long the loop may run.
    fn budget(&self, _conversation: &C) -> Budget {
        Budget::Unbounded
    }

    /// The budget ran out and the loop stopped.
    fn budget_spent(&mut self, _conversation: &mut C) {}

    /// Before every model call, once the input is taken; an ending ends the
    /// loop before the call is made.
    fn before_model(
        &mut self,
        _conversation: &mut C,
        _number: u64,
    ) -> impl Future<Output = Option<Ending<C::Failure>>> + Send {
        async { None }
    }

    /// A turn arrived, before its calls run or its answer is read.
    fn after_turn(&mut self, _conversation: &C, _turn: &C::Turn) -> Option<Ending<C::Failure>> {
        None
    }

    /// The model answered without a call.
    fn on_answer(
        &mut self,
        _conversation: &mut C,
        _answer: &str,
    ) -> impl Future<Output = Answer<C::Failure>> + Send {
        async { Answer::Accept }
    }

    /// A group's calls ran and were remembered.
    fn after_group(
        &mut self,
        _conversation: &mut C,
    ) -> impl Future<Output = Option<Ending<C::Failure>>> + Send {
        async { None }
    }
}

/// The extension of a run of no special kind: it adds nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct Plain;

impl<C: Conversation + ?Sized> Extension<C> for Plain {}
