//! A poster that answers from a script, and the waits a worker case polls with.
//!
//! Split from the worker cases at the file cap: every case drives the real
//! `Worker` against this stand-in for a destination.

use super::*;

/// What one scripted poster remembers, behind whatever handles point at it.
#[derive(Debug)]
pub(super) struct Script {
    /// Drained per attempt, its LAST entry repeating — so a test asserting the
    /// attempt ceiling is not accidentally asserting the length of its own
    /// script. A destination that is down stays down.
    answers: Mutex<Vec<Verdict>>,
    attempts: AtomicUsize,
    /// Answers seen, in order, so a test can assert WHICH job was delivered
    /// rather than only how many times something was.
    seen: Mutex<Vec<String>>,
    /// Cancels the supervisor's token from inside an attempt, which is the only
    /// way to reach "shutdown arrived mid-delivery" deterministically.
    cancel_on: Option<(usize, CancellationToken)>,
}

/// A poster that answers from a script and records what it was asked.
///
/// # Why this is a cloneable handle rather than the state itself
///
/// `Worker::new` takes its posters BY VALUE — correctly, since a worker owns
/// them for its whole run — and every assertion here is about what the poster
/// saw. Both need to hold it. The shared half is behind one `Arc` inside the
/// handle rather than the test wrapping the poster in an `Arc` of its own,
/// because `Deliver` is this crate's trait and `Arc` is not this crate's type:
/// the orphan rule refuses `impl Deliver for Arc<Scripted>` from a test target.
#[derive(Debug, Clone)]
pub(super) struct Scripted {
    script: std::sync::Arc<Script>,
}

impl Scripted {
    /// A poster that answers `answers`, the last repeating.
    pub(super) fn new(answers: &[Verdict]) -> Self {
        Self::build(answers, None)
    }

    /// As [`Self::new`], but cancels `token` at the start of attempt `on`.
    pub(super) fn cancelling(answers: &[Verdict], on: usize, token: CancellationToken) -> Self {
        Self::build(answers, Some((on, token)))
    }

    /// The one constructor both shapes go through.
    fn build(answers: &[Verdict], cancel_on: Option<(usize, CancellationToken)>) -> Self {
        assert!(!answers.is_empty(), "a script needs at least one answer");
        Self {
            script: std::sync::Arc::new(Script {
                // Reversed so each call is a `pop` off the end rather than a
                // remove from the front, which would be O(n) per attempt for
                // no reason.
                answers: Mutex::new(answers.iter().copied().rev().collect()),
                attempts: AtomicUsize::new(0),
                seen: Mutex::new(Vec::new()),
                cancel_on,
            }),
        }
    }

    /// How many attempts this poster was asked for.
    pub(super) fn attempts(&self) -> usize {
        self.script.attempts.load(Ordering::Relaxed)
    }

    /// The answers this poster was handed, in the order it saw them.
    pub(super) fn seen(&self) -> Vec<String> {
        self.script
            .seen
            .lock()
            .expect("no test panics holding this")
            .clone()
    }
}

impl Deliver for Scripted {
    fn deliver(&self, job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        let index = self.script.attempts.fetch_add(1, Ordering::Relaxed);
        self.script
            .seen
            .lock()
            .expect("no test panics holding this")
            .push(job.answer.clone());
        if let Some((on, token)) = &self.script.cancel_on
            && index == *on
        {
            token.cancel();
        }

        let mut answers = self
            .script
            .answers
            .lock()
            .expect("no test panics holding this");
        let answer = if answers.len() > 1 {
            answers.pop().expect("length checked above")
        } else {
            *answers.last().expect("a script is never empty")
        };
        std::future::ready(answer)
    }
}

/// Polls `condition` until it holds or [`PROGRESS_BUDGET`] runs out.
///
/// A poll rather than a channel because what is being waited on is the worker's
/// EFFECT — an acknowledgement in Dragonfly, a counter in a poster — and wiring a
/// signal into the worker to observe it would be testing the signal. `note`
/// names what was being waited for, so a timeout says which claim failed rather
/// than that a duration elapsed.
pub(super) async fn await_until<F>(note: &str, mut condition: F)
where
    F: AsyncFnMut() -> bool,
{
    let deadline = Instant::now() + PROGRESS_BUDGET;
    while Instant::now() < deadline {
        if condition().await {
            return;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    panic!("timed out after {PROGRESS_BUDGET:?} waiting for {note}");
}

/// Queues one answer, returning nothing: the id comes back off the stream.
pub(super) async fn enqueue(harness: &OutboundHarness, answer: &str) {
    harness
        .queue
        .enqueue(OutboundJob {
            provider: PROVIDER,
            destination: DESTINATION,
            workspace_id: WORKSPACE_ID,
            fleet_id: FLEET_ID,
            event_id: "1700000000000-0",
            answer,
        })
        .await
        .expect("the lane's Dragonfly must accept an enqueue");
}
