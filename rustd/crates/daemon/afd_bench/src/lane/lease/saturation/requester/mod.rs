//! One simulated host under saturation: offer, renew what it holds, never
//! report.
//!
//! # What one iteration is
//!
//! One [`Host::offer`] — in production `Plane::claim`, the verb the runner
//! route serves, which tries a setup chat and a fleet lease in the order the
//! host's ticket names — timed end to end. Every [`RENEW_EVERY`] claims the host
//! renews each chat lease it holds, so renewals compete for the pool the way a
//! host mid-conversation makes them compete.
//!
//! Offer failures end the measurement with their cause. Renewal refusals are
//! counted and observed by the load generator before the final correctness check.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use af_core::id::Uuid7;

use crate::abort::Abort;
use crate::error::Result;
use crate::report::Latency;

/// Successful offer rounds between eager renewal rounds.
const RENEW_EVERY: u64 = 4;

/// A chat lease a host holds, as its renewal names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Held {
    pub(super) lease: Uuid7,
    pub(super) fence: String,
}

/// What one offer answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Offered {
    /// No work this offer.
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
    /// Preserves gateway, decoding and domain refusal failures.
    fn offer(&self, runner: &Uuid7) -> impl Future<Output = Result<Offered>> + Send;

    /// Renew one held chat lease, answering whether the path accepted it.
    fn renew(&self, runner: &Uuid7, held: &Held) -> impl Future<Output = Result<()>> + Send;

    /// Wait for a new scheduling notification after an empty offer pass.
    fn wait(&self, _runner: &Uuid7) -> impl Future<Output = Result<()>> + Send {
        async { Ok(()) }
    }
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
    pub(super) done: tokio_util::sync::CancellationToken,
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
    pub(super) claims: u64,
    /// Setup chat leases issued to this host.
    pub(super) chat: u64,
    /// Fleet leases issued to this host.
    pub(super) fleet: u64,
    /// Polls or renewals the path refused.
    pub(super) failures: u64,
    /// Renewals the path accepted.
    pub(super) renewals: u64,
    /// How long a offer that issued a lease of either kind took.
    pub(super) lease_latency: Latency,
    /// How long a offer that issued a setup chat lease took: the tail a claim
    /// waiting on a locked draft shows in.
    pub(super) chat_latency: Latency,
}

impl Tally {
    /// An empty tally.
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            claims: 0,
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
        self.claims += other.claims;
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

/// Process offers and renew held work until the window closes.
///
/// # Errors
///
/// A latency the histogram would not hold, or an answer that does not parse.
pub(super) async fn drive<H: Host>(host: &H, runner: &Uuid7, shared: &Shared) -> Result<Tally> {
    let mut tally = Tally::new()?;
    let stop = shared.abort.token();
    let mut held = Vec::new();
    while shared.open() {
        tally.claims += 1;
        if tally.claims % RENEW_EVERY == 0 {
            for lease in &held {
                let renewed = renewal(host.renew(runner, lease).await);
                tally.renewals += u64::from(renewed);
                tally.outcome(renewed, shared);
            }
        }
        let started = Instant::now();
        let offered = tokio::select! {
            () = stop.cancelled() => break,
            () = shared.done.cancelled() => break,
            () = tokio::time::sleep_until(shared.deadline.into()) => break,
            result = host.offer(runner) => result?,
        };
        let took = started.elapsed();
        tally.outcome(true, shared);
        match offered {
            Offered::Nothing => {
                wait(host, runner, &held, &mut tally, shared).await?;
                continue;
            }
            Offered::Chat(lease) => {
                tally.chat += 1;
                tally.chat_latency.record(took)?;
                held.push(lease);
            }
            Offered::Fleet => tally.fleet += 1,
        }
        tally.lease_latency.record(took)?;
        let leased = shared.leased.fetch_add(1, Ordering::Relaxed) + 1;
        if leased >= shared.population {
            shared.done.cancel();
        }
    }
    Ok(tally)
}

/// Renewal failures remain visible before the load generator records their outcome.
fn renewal(result: Result<()>) -> bool {
    match result {
        Ok(()) => true,
        Err(error) => {
            eprintln!("native saturation renewal refused: {error}");
            false
        }
    }
}

/// Idle capacity sends no repeated claim; held authoring work still renews on its clock.
async fn wait<H: Host>(
    host: &H,
    runner: &Uuid7,
    held: &[Held],
    tally: &mut Tally,
    shared: &Shared,
) -> Result<()> {
    let stop = shared.abort.token();
    let notified = host.wait(runner);
    tokio::pin!(notified);
    let mut renew = tokio::time::interval(af_core::timing::RENEWAL_TICK);
    renew.tick().await;
    loop {
        tokio::select! {
            () = stop.cancelled() => return Ok(()),
            () = shared.done.cancelled() => return Ok(()),
            () = tokio::time::sleep_until(shared.deadline.into()) => return Ok(()),
            result = &mut notified => return result,
            _ = renew.tick(), if !held.is_empty() => {
                for lease in held {
                    let accepted = renewal(host.renew(runner, lease).await);
                    tally.renewals += u64::from(accepted);
                    tally.outcome(accepted, shared);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
