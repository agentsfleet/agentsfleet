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
//! held the only worker. Both fall out of one `Instant` per attempt.

use core::time::Duration;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use afd_outbound::{Deliver, Verdict};
use afd_redis::OutboundDelivery;

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
}

/// Everything the poster saw, keyed by the job's stream entry id.
#[derive(Debug, Default)]
pub struct Seen {
    attempts: HashMap<String, Vec<Attempt>>,
    /// Jobs whose LAST verdict was terminal, so the worker acknowledged them.
    terminal: u64,
}

impl Seen {
    /// Every attempt, in the order it happened, per job.
    #[must_use]
    pub fn attempts(&self) -> &HashMap<String, Vec<Attempt>> {
        &self.attempts
    }

    /// How many jobs reached a terminal verdict.
    #[must_use]
    pub const fn terminal(&self) -> u64 {
        self.terminal
    }
}

/// The scripted destination the worker delivers to.
#[derive(Debug, Clone)]
pub struct Scripted {
    behaviours: Arc<HashMap<String, Behaviour>>,
    fast: Duration,
    slow: Duration,
    seen: Arc<Mutex<Seen>>,
}

impl Scripted {
    /// A poster over `behaviours`, keyed by destination (the job's fleet id).
    #[must_use]
    pub fn new(behaviours: HashMap<String, Behaviour>, fast: Duration, slow: Duration) -> Self {
        Self {
            behaviours: Arc::new(behaviours),
            fast,
            slow,
            seen: Arc::new(Mutex::new(Seen::default())),
        }
    }

    /// A snapshot of everything seen so far.
    ///
    /// A poisoned lock yields the default rather than a panic: a job that
    /// panicked mid-delivery has already ended the run, and the reader here
    /// is the reporter deciding what to write.
    #[must_use]
    pub fn seen(&self) -> Seen {
        self.seen
            .lock()
            .map(|seen| Seen {
                attempts: seen.attempts.clone(),
                terminal: seen.terminal,
            })
            .unwrap_or_default()
    }

    /// The behaviour scripted for a destination; a job for an unknown one is
    /// treated as fast, because the alternative is a Permanent the report
    /// would then have to explain.
    fn behaviour_of(&self, destination: &str) -> Behaviour {
        self.behaviours
            .get(destination)
            .copied()
            .unwrap_or(Behaviour::Fast)
    }

    fn stamp(&self, id: &str, behaviour: Behaviour, terminal: bool) {
        if let Ok(mut seen) = self.seen.lock() {
            seen.attempts
                .entry(id.to_owned())
                .or_default()
                .push(Attempt {
                    at: Instant::now(),
                    behaviour,
                });
            if terminal {
                seen.terminal += 1;
            }
        }
    }
}

impl Deliver for Scripted {
    fn deliver(&self, job: &OutboundDelivery) -> impl Future<Output = Verdict> + Send {
        let behaviour = self.behaviour_of(&job.fleet_id);
        let (delay, verdict) = match behaviour {
            Behaviour::Fast => (self.fast, Verdict::Delivered),
            Behaviour::Slow => (self.slow, Verdict::Delivered),
            // Retryable never resolves: the worker's ladder gives up after its
            // attempt budget and treats the job as permanent, and THAT is the
            // terminal event — counted by the reporter from the attempt count,
            // since the poster is not told the ladder ended.
            Behaviour::Retryable => (self.fast, Verdict::Retryable),
        };
        self.stamp(job.id.as_str(), behaviour, verdict == Verdict::Delivered);
        async move {
            tokio::time::sleep(delay).await;
            verdict
        }
    }
}
