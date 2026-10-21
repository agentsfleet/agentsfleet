//! One simulated host under saturation: poll, renew what it holds, never
//! report.
//!
//! # What one iteration is
//!
//! One [`Host::poll`] — in production `Plane::claim`, the verb the runner
//! route serves, which tries a setup chat and a fleet lease in the order the
//! host's ticket names — timed end to end. Every [`RENEW_EVERY`] polls the host
//! renews each chat lease it holds, so renewals compete for the pool the way a
//! host mid-conversation makes them compete.
//!
//! # A refusal is counted, never propagated
//!
//! As in the other windows: a datastore that would not answer is an outcome
//! to count and hand to the abort monitor. Under saturation, an acquire that
//! timed out is exactly that outcome.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use afd_core::id::Uuid7;

use crate::abort::Abort;
use crate::error::Result;
use crate::report::Latency;

/// Polls between one host's renewal rounds.
const RENEW_EVERY: u64 = 4;

/// A chat lease a host holds, as its renewal names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Held {
    pub(super) lease: Uuid7,
    pub(super) fence: String,
}

/// What one poll answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Polled {
    /// The path would not answer; counted, never propagated.
    Refused,
    /// No work this poll.
    Nothing,
    /// A setup chat lease, held until the window ends.
    Chat(Held),
    /// A fleet lease.
    Fleet,
}

/// The two verbs a host calls.
pub(super) trait Host: Sync {
    /// Ask for work once.
    ///
    /// # Errors
    ///
    /// Only an answer that would not parse; a refusal is [`Polled::Refused`].
    fn poll(&self, runner: &Uuid7) -> impl Future<Output = Result<Polled>> + Send;

    /// Renew one held chat lease, answering whether the path accepted it.
    fn renew(&self, runner: &Uuid7, held: &Held) -> impl Future<Output = bool> + Send;
}

/// What every requester shares: when to stop, and how far they have got.
#[derive(Debug)]
pub(super) struct Shared {
    /// The window's end, whatever else happens.
    pub(super) deadline: Instant,
    /// Leases of either kind issued so far.
    pub(super) leased: AtomicU64,
    /// Stop once `leased` reaches the population.
    pub(super) population: u64,
    /// The monitor every outcome is reported to.
    pub(super) abort: Arc<Abort>,
}

impl Shared {
    /// Whether this window should keep polling.
    pub(super) fn open(&self) -> bool {
        Instant::now() < self.deadline
            && !self.abort.token().is_cancelled()
            && self.leased.load(Ordering::Relaxed) < self.population
    }
}

/// What one host did in the window.
#[derive(Debug)]
pub(super) struct Tally {
    /// Polls made, lease or not.
    pub(super) polls: u64,
    /// Setup chat leases issued to this host.
    pub(super) chat: u64,
    /// Fleet leases issued to this host.
    pub(super) fleet: u64,
    /// Polls or renewals the path refused.
    pub(super) failures: u64,
    /// Renewals the path accepted.
    pub(super) renewals: u64,
    /// How long a poll that issued a lease of either kind took.
    pub(super) lease_latency: Latency,
    /// How long a poll that issued a setup chat lease took: the tail a claim
    /// waiting on a locked draft shows in.
    pub(super) chat_latency: Latency,
}

impl Tally {
    /// An empty tally.
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            polls: 0,
            chat: 0,
            fleet: 0,
            failures: 0,
            renewals: 0,
            lease_latency: Latency::new()?,
            chat_latency: Latency::new()?,
        })
    }

    /// Fold another host's tally into this one.
    ///
    /// # Errors
    ///
    /// When the two latency distributions will not merge.
    pub(super) fn absorb(&mut self, other: &Self) -> Result<()> {
        self.polls += other.polls;
        self.chat += other.chat;
        self.fleet += other.fleet;
        self.failures += other.failures;
        self.renewals += other.renewals;
        self.chat_latency.merge(&other.chat_latency)?;
        self.lease_latency.merge(&other.lease_latency)
    }

    /// Count one outcome, reporting it to the monitor.
    fn outcome(&mut self, accepted: bool, shared: &Shared) {
        if !accepted {
            self.failures += 1;
        }
        shared.abort.record(accepted);
    }
}

/// Poll and renew until the window closes, answering what this host did.
///
/// # Errors
///
/// A latency the histogram would not hold, or an answer that does not parse.
pub(super) async fn drive<H: Host>(host: &H, runner: &Uuid7, shared: &Shared) -> Result<Tally> {
    let mut tally = Tally::new()?;
    let mut held = Vec::new();
    while shared.open() {
        tally.polls += 1;
        if tally.polls % RENEW_EVERY == 0 {
            for lease in &held {
                let renewed = host.renew(runner, lease).await;
                tally.renewals += u64::from(renewed);
                tally.outcome(renewed, shared);
            }
        }
        let started = Instant::now();
        let polled = host.poll(runner).await?;
        let took = started.elapsed();
        tally.outcome(polled != Polled::Refused, shared);
        match polled {
            Polled::Refused | Polled::Nothing => continue,
            Polled::Chat(lease) => {
                tally.chat += 1;
                tally.chat_latency.record(took)?;
                held.push(lease);
            }
            Polled::Fleet => tally.fleet += 1,
        }
        tally.lease_latency.record(took)?;
        shared.leased.fetch_add(1, Ordering::Relaxed);
    }
    Ok(tally)
}

#[cfg(test)]
mod tests;
