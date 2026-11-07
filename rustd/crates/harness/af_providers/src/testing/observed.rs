//! An observer a test reads back, and the scope that routes one future's
//! measurements to it.
//!
//! The scope is thread-local and set around every poll of the future it
//! wraps, so a test on any runtime sees exactly what its own future recorded:
//! work it spawned elsewhere, and every other test in the binary, record into
//! whatever is installed, never into this.

use std::cell::RefCell;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::task::{Context, Poll};
use std::time::Duration;

use af_wire::usage::ObservedTokens;

use crate::observe::Observer;
use crate::observe::labels::{
    CallOutcome, DecisionOutcome, DecisionProvider, DecisionPurpose, Provider, RetryReason,
};

/// One measurement, as a test reads it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Observed {
    /// [`Observer::decision`].
    Decision(DecisionProvider, DecisionPurpose, DecisionOutcome, Duration),
    /// [`Observer::model_call`].
    ModelCall(String, Provider, CallOutcome, Duration, Option<Duration>),
    /// [`Observer::model_tokens`].
    ModelTokens(String, Provider, ObservedTokens),
    /// [`Observer::retry`].
    Retry(Provider, RetryReason),
}

/// An observer that hands every measurement to its receiver.
#[derive(Debug)]
pub struct Tally(Sender<Observed>);

impl Tally {
    /// An observer, and where what it observes arrives.
    #[must_use]
    pub fn new() -> (Arc<Self>, Receiver<Observed>) {
        let (sent, received) = mpsc::channel();
        (Arc::new(Self(sent)), received)
    }

    fn send(&self, observed: Observed) {
        // A test that dropped its receiver asserts nothing about telemetry.
        let _unread = self.0.send(observed);
    }
}

impl Observer for Tally {
    fn decision(
        &self,
        provider: DecisionProvider,
        purpose: DecisionPurpose,
        outcome: DecisionOutcome,
        elapsed: Duration,
    ) {
        self.send(Observed::Decision(provider, purpose, outcome, elapsed));
    }

    fn model_call(
        &self,
        model: &str,
        provider: Provider,
        outcome: CallOutcome,
        elapsed: Duration,
        first_text: Option<Duration>,
    ) {
        self.send(Observed::ModelCall(
            model.to_owned(),
            provider,
            outcome,
            elapsed,
            first_text,
        ));
    }

    fn model_tokens(&self, model: &str, provider: Provider, usage: ObservedTokens) {
        self.send(Observed::ModelTokens(model.to_owned(), provider, usage));
    }

    fn retry(&self, provider: Provider, reason: RetryReason) {
        self.send(Observed::Retry(provider, reason));
    }
}

thread_local! {
    /// The observer the future being polled on this thread routes to.
    static SCOPED: RefCell<Option<Arc<dyn Observer>>> = const { RefCell::new(None) };
}

/// The observer the future being polled on this thread routes to, if any.
pub(crate) fn scoped_observer() -> Option<Arc<dyn Observer>> {
    SCOPED.with(|slot| slot.borrow().clone())
}

/// `future`, observing into `observer` whenever it is polled.
pub fn scoped<F: Future>(observer: Arc<dyn Observer>, future: F) -> Scoped<F> {
    Scoped {
        observer,
        inner: Box::pin(future),
    }
}

/// A future whose measurements go to one observer. See [`scoped`].
#[derive(Debug)]
pub struct Scoped<F> {
    observer: Arc<dyn Observer>,
    inner: Pin<Box<F>>,
}

/// Puts the previous scope back when a poll ends, panicking or not.
struct Restore(Option<Arc<dyn Observer>>);

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
        let previous = SCOPED.with(|slot| slot.replace(Some(Arc::clone(&this.observer))));
        let _restore = Restore(previous);
        this.inner.as_mut().poll(cx)
    }
}
