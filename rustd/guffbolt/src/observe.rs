//! What a loop tells its embedder while it works: each model call from its
//! start to its end, the text it streams, each tool call from its start to
//! its end, and the loop's own end.
//!
//! ```text
//!  turn_started ─► text and usage, chunk by chunk (Listen) ─► turn_ended
//!  call_started ─► the call runs in the span its start returned ─► call_ended
//!  loop_ended
//! ```
//!
//! These are pi's agent events, and the begin and end Codex pairs with every
//! dispatch. The loop opens, runs and ends each tool call itself, so a call
//! ends exactly once on every path. One that never answers, because the loop
//! was stopped while it ran or the loop's own future was dropped, ends
//! [`CallOutcome::Interrupted`].

use std::time::Duration;

use tracing::Span;

use crate::tool::{ErrorCode, ToolOutput};
use crate::transcript::Call;
use crate::turn::{Ending, Listen, Turn};

/// How one model call ended, `F` being the embedder's failure.
#[derive(Debug)]
pub enum TurnOutcome<'a, F> {
    /// Its stream was read to its end, into this turn.
    Read(&'a Turn),
    /// Its stream failed.
    Failed(&'a F),
    /// The loop was stopped before the stream ended.
    Stopped,
}

/// One model call's end, as the loop tells its observer.
#[derive(Debug)]
pub struct TurnEnd<'a, F> {
    /// The call's number within the loop, from 1.
    pub number: u64,
    /// The span its stream was read in.
    pub span: &'a Span,
    /// How long it took, from its start to its end.
    pub duration: Duration,
    /// How it ended.
    pub outcome: TurnOutcome<'a, F>,
}

/// How one tool call ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CallOutcome {
    /// It answered with no error code and no non-zero exit.
    Succeeded,
    /// It answered with an error code, or its process exited non-zero.
    Failed,
    /// It never answered: the loop was stopped while it ran, or the loop's
    /// own future was dropped.
    Interrupted,
}

/// One tool call's end, as the loop tells its observer.
#[derive(Debug)]
pub struct CallEnd<'a> {
    /// The call the model made.
    pub call: &'a Call,
    /// The span it ran in.
    pub span: &'a Span,
    /// How long it took, from its start to its end.
    pub duration: Duration,
    /// What it answered; none when it never did. The model reads this output
    /// once the observer returns, so an observer masks here what the model
    /// must not read.
    pub output: Option<&'a mut ToolOutput>,
}

impl CallEnd<'_> {
    /// How the call ended, read from what it answered.
    #[must_use]
    pub fn outcome(&self) -> CallOutcome {
        let Some(output) = &self.output else {
            return CallOutcome::Interrupted;
        };
        let failed = output.error_code.is_some() || output.exit_code.is_some_and(|code| code != 0);
        if failed {
            CallOutcome::Failed
        } else {
            CallOutcome::Succeeded
        }
    }

    /// The exit code of the process the call ran, when it answered with one.
    #[must_use]
    pub fn exit_code(&self) -> Option<i32> {
        self.output.as_ref()?.exit_code
    }

    /// Why the call failed, when it answered with a code.
    #[must_use]
    pub fn error_code(&self) -> Option<ErrorCode> {
        self.output.as_ref()?.error_code
    }
}

/// Hears a loop's work while it happens, `F` being the embedder's failure.
///
/// The loop calls these in the order its work happens and waits on none of
/// them, so each returns promptly. A model call's chunks reach the observer
/// through [`Listen`] between its start and its end. The loop shares the
/// observer with every call it holds open, so the observer is `Sync`.
pub trait Observe<F>: Listen + Sync {
    /// What the observer keeps of one tool call from its start to its end.
    type Call: Send;

    /// Model call `number` starts, before its request is built. Its stream
    /// is read inside the span this returns.
    fn turn_started(&self, _number: u64) -> Span {
        Span::none()
    }

    /// A model call ended.
    fn turn_ended(&self, _end: TurnEnd<'_, F>) {}

    /// The model's `call` starts, before it runs or is refused. Returns what
    /// the observer keeps until the call ends, and the span the call runs in.
    fn call_started(&self, call: &Call) -> (Self::Call, Span);

    /// The call `kept` was returned for ended, as `end` says. Each call ends
    /// here exactly once.
    fn call_ended(&self, kept: Self::Call, end: CallEnd<'_>);

    /// The loop ended after `turns` model calls, as `ending` says. A loop
    /// whose own future is dropped never ends here.
    fn loop_ended(&self, _turns: u64, _ending: &Ending<F>) {}
}

/// An observer that hears nothing, for a loop no one watches.
#[derive(Debug, Clone, Copy, Default)]
pub struct Unobserved;

impl Listen for Unobserved {}

impl<F> Observe<F> for Unobserved {
    type Call = ();

    fn call_started(&self, _call: &Call) -> ((), Span) {
        ((), Span::none())
    }

    fn call_ended(&self, _kept: (), _end: CallEnd<'_>) {}
}
