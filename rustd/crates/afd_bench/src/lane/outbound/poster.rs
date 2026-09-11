//! A destination that answers on a script, and remembers when it was asked.
//!
//! # Why an in-process poster and not a loopback vendor
//!
//! The spec pointed at `hanging_queue.rs` as the model for a stub vendor. That
//! server exists to prove the HTTP client's deadline, which is not what this
//! lane measures: head-of-line cost and retry occupancy are properties of the
//! worker's LOOP — one job at a time, the retry ladder awaited inline — and
//! the loop takes its poster through the [`Deliver`] trait. Scripting the trait
//! drives the real `Worker` with the real ladder and skips a socket that would
//! only have added its own latency to the number.
//!
//! # Every attempt is stamped, and that is the whole instrument
//!
//! Delivery latency is enqueue-to-first-attempt; retry occupancy is the gap
//! between one attempt and the next on the same job, which is time the ladder
//! held the only worker. Both fall out of one `Instant` per attempt — taken
//! BEFORE the lock, so a reader snapshotting the map never delays a stamp.
//!
//! The window is a different instant. A job settles when the vendor ANSWERS,
//! which is the attempt plus the delay the script gave it; each attempt
//! carries that delay, [`Attempt::settled_at`] adds it, and the settled count
//! moves only after the answer. A drain that closed on the last attempt's
//! start left the last answer out of the denominator.
//!
//! # Only this run's jobs count
//!
//! The worker reads the shared stream, so a concurrently queued foreign entry
//! can reach this poster after the empty-stream preflight. Its destination is
//! one this script never named. The poster cancels the worker and returns a
//! retryable verdict, so the worker neither posts nor acknowledges that entry.

use core::time::Duration;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use afd_outbound::retry::DELIVERY_ATTEMPTS;
use afd_outbound::{Deliver, Verdict};
use afd_redis::OutboundDelivery;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// What one destination does when asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    /// Answers after a short delay, as a healthy vendor does.
    Fast,
    /// Answers after a long delay, so the jobs queued behind it wait.
    Slow,
    /// Refuses every time, so the worker walks the whole retry ladder.
    Retryable,
}

/// One attempt the worker made against this poster.
#[derive(Debug, Clone, Copy)]
pub struct Attempt {
    /// When the worker called `deliver`.
    pub at: Instant,
    /// Which destination the job was for.
    pub behaviour: Behaviour,
    /// How long the scripted vendor took to answer.
    pub delay: Duration,
}

impl Attempt {
    /// When the vendor answered: the attempt plus its scripted delay.
    #[must_use]
    pub fn settled_at(&self) -> Instant {
        self.at.checked_add(self.delay).unwrap_or(self.at)
    }
}

/// Everything the poster saw of THIS run's jobs, keyed by stream entry id.
#[derive(Debug, Default)]
pub struct Seen {
    attempts: HashMap<String, Vec<Attempt>>,
    /// Jobs answered with a foreign destination: left by an earlier run.
    foreign: u64,
}

impl Seen {
    /// Every attempt, in the order it happened, per job.
    #[must_use]
    pub const fn attempts(&self) -> &HashMap<String, Vec<Attempt>> {
        &self.attempts
    }

    /// Jobs this poster answered that were not this run's.
    #[must_use]
    pub const fn foreign(&self) -> u64 {
        self.foreign
    }
}

/// The scripted destination the worker delivers to.
#[derive(Debug, Clone)]
pub struct Scripted {
    behaviours: Arc<BTreeMap<String, Behaviour>>,
    fast: Duration,
    slow: Duration,
    seen: Arc<Mutex<Seen>>,
    /// Jobs of this run that reached a terminal state: delivered, or the
    /// ladder exhausted. Read without the lock by whoever waits for the drain.
    settled: Arc<AtomicU64>,
    /// Woken on every settlement, so the drain need not poll.
    settled_signal: Arc<Notify>,
    /// Cancels the lane before a foreign entry can be acknowledged.
    cancellation: CancellationToken,
}

impl Scripted {
    /// A poster over `behaviours`, keyed by destination (the job's fleet id).
    #[must_use]
    pub fn new(behaviours: BTreeMap<String, Behaviour>, fast: Duration, slow: Duration) -> Self {
        Self::with_cancellation(behaviours, fast, slow, CancellationToken::new())
    }

    /// A script that can stop its worker when the shared stream yields foreign work.
    #[must_use]
    pub fn with_cancellation(
        behaviours: BTreeMap<String, Behaviour>,
        fast: Duration,
        slow: Duration,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            behaviours: Arc::new(behaviours),
            fast,
            slow,
            seen: Arc::new(Mutex::new(Seen::default())),
            settled: Arc::new(AtomicU64::new(0)),
            settled_signal: Arc::new(Notify::new()),
            cancellation,
        }
    }

    /// How many of this run's jobs have reached a terminal state.
    #[must_use]
    pub fn settled(&self) -> u64 {
        self.settled.load(Ordering::Acquire)
    }

    /// Wait until a settlement is recorded, or the given time passes.
    pub async fn settlement(&self, at_most: Duration) {
        let _ = tokio::time::timeout(at_most, self.settled_signal.notified()).await;
    }

    /// A snapshot of everything seen so far.
    ///
    /// Taken once, by the reporter, after the worker is stopped: cloning the
    /// map under the lock while the worker is still stamping would delay a
    /// stamp by the length of the clone.
    #[must_use]
    pub fn seen(&self) -> Seen {
        self.seen
            .lock()
            .map(|seen| Seen {
                attempts: seen.attempts.clone(),
                foreign: seen.foreign,
            })
            .unwrap_or_default()
    }

    /// The behaviour scripted for a destination, or `None` for a job that is
    /// not this run's.
    fn behaviour_of(&self, destination: &str) -> Option<Behaviour> {
        self.behaviours.get(destination).copied()
    }

    /// Record an attempt at the instant it was made, and say whether the
    /// answer to it will settle the job.
    fn stamp(
        &self,
        id: &str,
        behaviour: Behaviour,
        at: Instant,
        delay: Duration,
        terminal: bool,
    ) -> bool {
        let attempts_so_far = if let Ok(mut seen) = self.seen.lock() {
            let attempts = seen.attempts.entry(id.to_owned()).or_default();
            attempts.push(Attempt {
                at,
                behaviour,
                delay,
            });
            attempts.len()
        } else {
            0
        };
        // Terminal for the drain means delivered OR the ladder exhausted,
        // which is this many attempts on one job; the poster is not told
        // when the worker gives up, so it counts.
        terminal || attempts_so_far == DELIVERY_ATTEMPTS
    }

    fn foreign(&self) {
        if let Ok(mut seen) = self.seen.lock() {
            seen.foreign += 1;
        }
    }
}

impl Deliver for Scripted {
    fn deliver(&self, job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        let at = Instant::now();
        let scripted = self.behaviour_of(&job.fleet_id);
        let (delay, verdict) = match scripted {
            Some(Behaviour::Fast) => (self.fast, Verdict::Delivered),
            Some(Behaviour::Slow) => (self.slow, Verdict::Delivered),
            Some(Behaviour::Retryable) => (self.fast, Verdict::Retryable),
            // Foreign work stays unacknowledged. Cancellation makes the real
            // worker stop its retry ladder before it can post or acknowledge.
            None => {
                self.cancellation.cancel();
                (Duration::ZERO, Verdict::Retryable)
            }
        };
        let settles = if let Some(behaviour) = scripted {
            self.stamp(
                job.id.as_str(),
                behaviour,
                at,
                delay,
                verdict == Verdict::Delivered,
            )
        } else {
            self.foreign();
            false
        };
        let settled = Arc::clone(&self.settled);
        let settled_signal = Arc::clone(&self.settled_signal);
        async move {
            tokio::time::sleep(delay).await;
            if settles {
                settled.fetch_add(1, Ordering::Release);
                settled_signal.notify_waiters();
            }
            verdict
        }
    }
}

#[cfg(test)]
mod tests;
