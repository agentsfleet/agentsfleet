//! The polling loop under measurement, run once per simulated runner.
//!
//! # What one iteration is
//!
//! Exactly one `Leases::select` — the real assignment pass, timed end to end.
//! No retry, no backoff, no sleep: a runner in production re-polls on the
//! backoff its reply carries, and modelling that here would measure the model.
//! What this measures is how fast the pass answers when asked continuously.
//!
//! # The window ends when the POPULATION is exhausted
//!
//! Every runner shares one lease total, and every runner stops the moment it
//! reaches the population. A ceiling checked against a runner's OWN count
//! never fires with more than one runner — no runner leases everything — and
//! the window then runs to its deadline in miss mode, which is what made the
//! first baselines report leases over the whole window rather than over the
//! time it took to hand the work out.
//!
//! # A miss is data; a refusal is counted and judged, not propagated
//!
//! `Ok(None)` is a poll that cost a peek and possibly a candidate query and
//! produced no work — under contention, the interesting case. `Err` is a
//! datastore that would not answer: it is counted, reported, and handed to
//! the abort monitor, which decides when refusing often enough ends the run.

use core::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_fleet::lease::Leases;

use crate::abort::Abort;
use crate::error::Result;
use crate::lane::outcomes::Outcomes;

/// What every runner in a window shares: the deadline, the lease total and
/// the ceiling it stops at, and the monitor that can end the window early.
#[derive(Debug)]
pub struct Shared {
    /// When the window ends whatever else happens.
    pub deadline: Instant,
    /// Leases issued by every runner so far.
    pub leased: AtomicU64,
    /// Stop once `leased` reaches this; `None` polls to the deadline.
    pub stop_after: Option<u64>,
    /// The monitor every outcome is reported to.
    pub abort: Arc<Abort>,
}

impl Shared {
    /// Whether the population is exhausted.
    fn exhausted(&self) -> bool {
        self.stop_after
            .is_some_and(|ceiling| self.leased.load(Ordering::Relaxed) >= ceiling)
    }

    /// The instant the last lease was issued, for a window's true length.
    #[must_use]
    pub fn leased_so_far(&self) -> u64 {
        self.leased.load(Ordering::Relaxed)
    }
}

/// Poll until the population is exhausted, the deadline passes, or the monitor fires.
///
/// Answers what this runner did and WHEN it issued its last lease, so the lane
/// can end the window at exhaustion rather than at the deadline.
///
/// # Errors
///
/// [`crate::Error::LatencyUnavailable`] or [`crate::Error::LatencyUnrecordable`]
/// from the histogram; never a refusal from the path, which is counted.
pub async fn poll_until(
    leases: &Leases,
    runner: &Uuid7,
    shared: &Shared,
) -> Result<(Outcomes, Option<Instant>)> {
    let mut outcomes = Outcomes::new()?;
    let mut last_lease = None;
    let token = shared.abort.token();
    while Instant::now() < shared.deadline && !token.is_cancelled() && !shared.exhausted() {
        let started = Instant::now();
        match leases.select(runner, now()).await {
            Ok(Some(_acquired)) => {
                outcomes.succeeded(started.elapsed())?;
                shared.leased.fetch_add(1, Ordering::Relaxed);
                last_lease = Some(Instant::now());
                shared.abort.record(true);
            }
            Ok(None) => {
                outcomes.missed(started.elapsed())?;
                shared.abort.record(true);
            }
            Err(_refused) => {
                outcomes.failed();
                shared.abort.record(false);
            }
        }
    }
    Ok((outcomes, last_lease))
}

/// The wall clock, in the shape the assignment pass is given.
fn now() -> UnixMillis {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    UnixMillis::from_millis(i64::try_from(millis).unwrap_or(i64::MAX))
}

/// A window's length: from its start to its last lease when the population
/// was exhausted, or to its end when it was not.
#[must_use]
pub fn window_length(
    started: Instant,
    ended: Instant,
    last_lease: Option<Instant>,
    exhausted: bool,
) -> Duration {
    match (exhausted, last_lease) {
        (true, Some(last)) => last.saturating_duration_since(started),
        _ => ended.saturating_duration_since(started),
    }
}
