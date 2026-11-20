//! One tool call, from its start to its end: opened before it runs or is
//! refused, run inside the observer's span against the loop's stop, and ended
//! exactly once.

use std::future::ready;
use std::time::Instant;

use tracing::{Instrument as _, Span};

use crate::conversation::Conversation;
use crate::observe::{CallEnd, Observe};
use crate::tool::{ToolFuture, ToolOutput};
use crate::transcript::{Call, CallRef};

/// Runs `call`, which sits at `call_ref`, or answers it unrun when its step
/// was `cut` at the output limit, its arguments possibly incomplete. `None`
/// when the loop was stopped before the call started or while it ran.
pub(super) async fn run<C: Conversation + ?Sized>(
    conversation: &C,
    call: &Call,
    call_ref: &CallRef,
    cut: bool,
) -> Option<ToolOutput> {
    let stop = conversation.stop();
    if stop.is_cancelled() {
        return None;
    }
    // Opened before its work exists, so the work is always dropped first:
    // a call the work's own drop ends, such as one inside it, ends before
    // this one.
    let open = Open::start(conversation.observer(), call);
    let work: ToolFuture<'_> = if cut {
        Box::pin(ready(ToolOutput::cut(&call.name)))
    } else {
        conversation.dispatch(call, call_ref)
    };
    let work = work.instrument(open.span.clone());
    let output = tokio::select! {
        biased;
        () = stop.cancelled() => None,
        output = work => Some(output),
    }?;
    Some(open.answered(output))
}

/// One call between its start and its end, which comes once: when the call
/// answers, or `interrupted` when the guard is dropped first.
struct Open<'a, O: Observe<F> + ?Sized, F> {
    observer: &'a O,
    call: &'a Call,
    span: Span,
    started: Instant,
    /// What the observer keeps of the call; taken when the call ends, so it
    /// ends once.
    kept: Option<O::Call>,
}

impl<'a, O: Observe<F> + ?Sized, F> Open<'a, O, F> {
    /// Tells `observer` that `call` starts.
    fn start(observer: &'a O, call: &'a Call) -> Self {
        let (kept, span) = observer.call_started(call);
        Self {
            observer,
            call,
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
