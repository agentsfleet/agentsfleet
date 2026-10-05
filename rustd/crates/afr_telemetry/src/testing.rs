//! A recorder a test reads back, and the scope that routes one future's
//! measurements to it.
//!
//! The scope is thread-local and set around every poll of the future it
//! wraps, so a test on any runtime sees exactly what its own future recorded:
//! the work it spawned elsewhere and every other test in the binary record
//! into whatever is installed, never into this.

use std::cell::RefCell;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::task::{Context, Poll};
use std::time::Duration;

use afd_observability::metrics::label::http::{DiscardReason, Signal};

use crate::labels::{
    FrameDrop, Provider, PushFailure, RetryReason, SandboxStart, Tool, ToolOutcome, TurnOutcome,
};
use crate::record::Recorder;

/// One measurement, as a test reads it back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Recorded {
    /// [`Recorder::turn`].
    Turn(Provider, TurnOutcome, Duration),
    /// [`Recorder::retry`].
    Retry(Provider, RetryReason),
    /// [`Recorder::sandbox_start`].
    SandboxStart(SandboxStart, Duration),
    /// [`Recorder::frames_dropped`].
    FramesDropped(FrameDrop, u64),
    /// [`Recorder::push_failed`].
    PushFailed(PushFailure),
    /// [`Recorder::tool_call`].
    ToolCall(Tool, ToolOutcome, Duration),
    /// [`Recorder::spans_suppressed`].
    SpansSuppressed(u64),
    /// [`Recorder::export_discarded`].
    ExportDiscarded(Signal, DiscardReason, u64),
}

/// A recorder that hands every measurement to its receiver.
#[derive(Debug)]
pub struct Tally(Sender<Recorded>);

impl Tally {
    /// A recorder, and where what it records arrives.
    #[must_use]
    pub fn new() -> (Arc<Self>, Receiver<Recorded>) {
        let (sent, received) = mpsc::channel();
        (Arc::new(Self(sent)), received)
    }

    fn send(&self, recorded: Recorded) {
        // A test that dropped its receiver asserts nothing about telemetry.
        let _unread = self.0.send(recorded);
    }
}

impl Recorder for Tally {
    fn turn(&self, provider: Provider, outcome: TurnOutcome, elapsed: Duration) {
        self.send(Recorded::Turn(provider, outcome, elapsed));
    }

    fn retry(&self, provider: Provider, reason: RetryReason) {
        self.send(Recorded::Retry(provider, reason));
    }

    fn sandbox_start(&self, outcome: SandboxStart, elapsed: Duration) {
        self.send(Recorded::SandboxStart(outcome, elapsed));
    }

    fn frames_dropped(&self, reason: FrameDrop, frames: u64) {
        self.send(Recorded::FramesDropped(reason, frames));
    }

    fn push_failed(&self, reason: PushFailure) {
        self.send(Recorded::PushFailed(reason));
    }

    fn tool_call(&self, tool: Tool, outcome: ToolOutcome, elapsed: Duration) {
        self.send(Recorded::ToolCall(tool, outcome, elapsed));
    }

    fn spans_suppressed(&self, spans: u64) {
        self.send(Recorded::SpansSuppressed(spans));
    }

    fn export_discarded(&self, signal: Signal, reason: DiscardReason, count: u64) {
        self.send(Recorded::ExportDiscarded(signal, reason, count));
    }
}

thread_local! {
    /// The recorder the future being polled on this thread routes to.
    static SCOPED: RefCell<Option<Arc<dyn Recorder>>> = const { RefCell::new(None) };
}

/// The recorder the future being polled on this thread routes to, if any.
pub(crate) fn scoped_recorder() -> Option<Arc<dyn Recorder>> {
    SCOPED.with(|slot| slot.borrow().clone())
}

/// `future`, recording into `recorder` whenever it is polled.
pub fn scoped<F: Future>(recorder: Arc<dyn Recorder>, future: F) -> Scoped<F> {
    Scoped {
        recorder,
        inner: Box::pin(future),
    }
}

/// A future whose measurements go to one recorder. See [`scoped`].
#[derive(Debug)]
pub struct Scoped<F> {
    recorder: Arc<dyn Recorder>,
    inner: Pin<Box<F>>,
}

/// Puts the previous scope back when a poll ends, panicking or not.
struct Restore(Option<Arc<dyn Recorder>>);

impl Drop for Restore {
    fn drop(&mut self) {
        let previous = self.0.take();
        SCOPED.with(|slot| *slot.borrow_mut() = previous);
    }
}

impl<F: Future> Future for Scoped<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = self.get_mut();
        let previous = SCOPED.with(|slot| slot.replace(Some(Arc::clone(&this.recorder))));
        let _restore = Restore(previous);
        this.inner.as_mut().poll(cx)
    }
}
