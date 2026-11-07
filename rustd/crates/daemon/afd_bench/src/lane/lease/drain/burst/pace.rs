//! Pacing steers, and timing each one until a runner holds its lease.

use core::time::Duration;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::Mutex;
use tokio::time::{Instant, MissedTickBehavior};

use crate::error::Result;
use crate::report::Latency;

/// Call `send` once every `1 / rate` seconds until `window` has passed,
/// answering how many times it ran.
///
/// A late tick is delayed, never bunched: a burst is a steady rate, and a pacer
/// that caught up after a stall would measure its own catch-up.
pub(super) async fn pace<F, Fut>(rate: u64, window: Duration, mut send: F) -> u64
where
    F: FnMut() -> Fut,
    Fut: Future<Output = ()>,
{
    let per_second = u32::try_from(rate).unwrap_or(u32::MAX).max(1);
    let mut ticks = tokio::time::interval(Duration::from_secs(1) / per_second);
    ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let end = Instant::now() + window;
    let mut ran = 0;
    loop {
        ticks.tick().await;
        if Instant::now() >= end {
            break;
        }
        send().await;
        ran += 1;
    }
    ran
}

/// Every steer the burst appended, and how long each waited for its lease.
#[derive(Debug)]
pub(in crate::lane::lease::drain) struct Pickups {
    /// When each appended event's append returned, by event id.
    appended: Mutex<HashMap<String, Instant>>,
    /// Append returned → lease received, for every burst event a runner took.
    waited: Mutex<Latency>,
    /// Burst events a runner took.
    leased: AtomicU64,
    /// Of those, the ones a live run folded in rather than leased.
    folded: AtomicU64,
}

impl Pickups {
    /// Nothing appended yet.
    ///
    /// # Errors
    ///
    /// When the latency histogram will not build.
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            appended: Mutex::new(HashMap::new()),
            waited: Mutex::new(Latency::new()?),
            leased: AtomicU64::new(0),
            folded: AtomicU64::new(0),
        })
    }

    /// One steer's append returned at `at`.
    pub(super) async fn appended(&self, event_id: String, at: Instant) {
        self.appended.lock().await.insert(event_id, at);
    }

    /// A runner received the lease for `event_id`. An event the burst did not
    /// append (the population's seeded one) is not a pickup.
    ///
    /// # Errors
    ///
    /// When the histogram will not hold the wait.
    pub(in crate::lane::lease::drain) async fn leased(&self, event_id: &str) -> Result<()> {
        self.took(event_id).await.map(|_burst| ())
    }

    /// A live run folded `event_id` in: a pickup like a lease, counted apart.
    ///
    /// # Errors
    ///
    /// When the histogram will not hold the wait.
    pub(in crate::lane::lease::drain) async fn folded(&self, event_id: &str) -> Result<()> {
        if self.took(event_id).await? {
            self.folded.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Records the pickup of `event_id`; answers whether the burst appended it.
    async fn took(&self, event_id: &str) -> Result<bool> {
        let Some(at) = self.appended.lock().await.remove(event_id) else {
            return Ok(false);
        };
        self.leased.fetch_add(1, Ordering::Relaxed);
        self.waited.lock().await.record(at.elapsed())?;
        Ok(true)
    }

    /// Burst events a runner took.
    pub(super) fn leased_count(&self) -> u64 {
        self.leased.load(Ordering::Relaxed)
    }

    /// Of those, the ones a live run folded in.
    pub(super) fn folded_count(&self) -> u64 {
        self.folded.load(Ordering::Relaxed)
    }

    /// The pickup waits recorded so far.
    pub(super) async fn waits(&self) -> Latency {
        self.waited.lock().await.clone()
    }
}

/// What the pacers' appends answered.
#[derive(Debug)]
pub(super) struct Appends {
    /// Steers the intake accepted.
    pub(super) accepted: AtomicU64,
    /// Steers the intake refused.
    pub(super) refused: AtomicU64,
    /// How long each accepted append took.
    pub(super) took: Mutex<Latency>,
}

impl Appends {
    /// Nothing appended yet.
    ///
    /// # Errors
    ///
    /// When the latency histogram will not build.
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            accepted: AtomicU64::new(0),
            refused: AtomicU64::new(0),
            took: Mutex::new(Latency::new()?),
        })
    }
}
