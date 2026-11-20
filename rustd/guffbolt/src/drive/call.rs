//! One tool call, from its start to its end: opened before it runs or is
//! refused, run inside the observer's span against the loop's stop, and ended
//! exactly once.
//!
//! A subagent call answers with an order rather than its result; the order
//! runs here, inside the call, in the place of the loop that made it, so the
//! observer hears the call end once, with what the model reads.

use std::future::ready;
use std::time::Instant;

use tracing::{Instrument as _, Span};

use super::Mark;
use crate::agents::{Caller, Me};
use crate::conversation::Conversation;
use crate::observe::{CallEnd, Observe};
use crate::tool::{ToolFuture, ToolOutput};
use crate::transcript::{Call, CallRef, Message};

/// What one call is run with: where it sits, the newest input's mark when its
/// step began, the loop that made it, and that loop's transcript before the
/// step when the step starts a child.
#[derive(Clone, Copy)]
pub(super) struct Lent<'a> {
    pub(super) call_ref: &'a CallRef,
    pub(super) mark: Option<&'a Mark>,
    pub(super) me: Me,
    pub(super) history: &'a [Message],
}

/// Runs `call` as `lent` places it, or answers it unrun when its step was
/// `cut` at the output limit, its arguments possibly incomplete. `None` when
/// the loop was stopped before the call started or while it ran.
pub(super) async fn run<C: Conversation + ?Sized>(
    conversation: &C,
    call: &Call,
    lent: Lent<'_>,
    cut: bool,
) -> Option<ToolOutput> {
    let stop = conversation.stop();
    if stop.is_cancelled() {
        return None;
    }
    // Opened before its work exists, so the work is always dropped first:
    // a call the work's own drop ends, such as one inside it, ends before
    // this one.
    let open = Open::start(conversation.observer(), call, lent.call_ref, lent.mark);
    let work: ToolFuture<'_> = if cut {
        Box::pin(ready(ToolOutput::cut(&call.name)))
    } else {
        conversation.dispatch(call, lent.call_ref)
    };
    let work = ordered(conversation, work, lent).instrument(open.span.clone());
    let output = tokio::select! {
        biased;
        () = stop.cancelled() => None,
        output = work => Some(output),
    }?;
    Some(open.answered(output))
}

/// What `work` answers, or, when it answered with an order, what running the
/// order in the calling loop's place answers.
async fn ordered<C: Conversation + ?Sized>(
    conversation: &C,
    work: ToolFuture<'_>,
    lent: Lent<'_>,
) -> ToolOutput {
    let mut output = work.await;
    let Some(order) = output.order.take() else {
        return output;
    };
    let tools: Vec<String> = if order.spawns() {
        let offered = conversation.offered();
        offered.iter().map(|spec| spec.name.to_owned()).collect()
    } else {
        Vec::new()
    };
    let caller = Caller {
        me: lent.me,
        tools: &tools,
        history: lent.history,
    };
    order.run(caller).await
}

/// One call between its start and its end, which comes once: when the call
/// answers, or `interrupted` when the guard is dropped first.
struct Open<'a, O: Observe<F> + ?Sized, F> {
    observer: &'a O,
    call: &'a Call,
    at: &'a CallRef,
    mark: Option<&'a Mark>,
    span: Span,
    started: Instant,
    /// What the observer keeps of the call; taken when the call ends, so it
    /// ends once.
    kept: Option<O::Call>,
}

impl<'a, O: Observe<F> + ?Sized, F> Open<'a, O, F> {
    /// Tells `observer` that `call`, at `at`, starts.
    fn start(observer: &'a O, call: &'a Call, at: &'a CallRef, mark: Option<&'a Mark>) -> Self {
        let (kept, span) = observer.call_started(call);
        Self {
            observer,
            call,
            at,
            mark,
            span,
            started: Instant::now(),
            kept: Some(kept),
        }
    }

    /// Ends the call with what it answered, and hands that back as the
    /// observer left it.
    fn answered(mut self, mut output: ToolOutput) -> ToolOutput {
        self.end(Some(&mut output));
        output
    }

    /// Ends the call, unless it already ended: `output` is what it
    /// answered, none when it never did.
    fn end(&mut self, output: Option<&mut ToolOutput>) {
        if let Some(kept) = self.kept.take() {
            let end = CallEnd {
                call: self.call,
                at: self.at,
                mark: self.mark,
                span: &self.span,
                duration: self.started.elapsed(),
                output,
            };
            self.observer.call_ended(kept, end);
        }
    }
}

impl<O: Observe<F> + ?Sized, F> Drop for Open<'_, O, F> {
    fn drop(&mut self) {
        self.end(None);
    }
}
