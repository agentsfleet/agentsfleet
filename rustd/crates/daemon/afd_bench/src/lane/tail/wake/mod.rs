//! The wake ladder: how long a ring takes to reach a replica's doorbell, as
//! the number of replicas listening grows.
//!
//! # A hub is a replica
//!
//! ```text
//!   ringer ──wake::publish──► NATS ──► hub 1: own connection ─► Wake (16 shards)
//!                                  ├─► hub 2: own connection ─► Wake
//!                                  └─► hub N: …
//! ```
//!
//! Each hub is what one `agentsfleetd` replica runs as M243 leaves it: its own
//! Bus connection ([`Datastores::replica`]) and its own [`Wake`] listening on
//! every shard. A ring goes to every replica, so the ladder times one ring
//! from publish to the FIRST hub that takes it, over 1, 2 and 4 hubs.
//!
//! # A ring is missed, never late
//!
//! The ringer keeps at most [`WINDOW`] rings unresolved, the doorbell's own
//! hint cap, and each ring names a fleet no other ring names. A ring no hub
//! takes within [`ledger::MISSED_AFTER`] counts in `wake_missed_hubs_<n>` and
//! never enters `wake_p50_ms_hubs_<n>` or `wake_p99_ms_hubs_<n>`: a scheduler
//! that waited that long would already have found the work by its own probe.
//!
//! # Every hub is proved to hear before a ring is timed
//!
//! A subscription reaches the other cluster nodes asynchronously, so each hub
//! first takes a probe ring of its own, and a warm-up batch then crosses every
//! shard before the measured rings start.

mod ledger;

use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_bus::wake::{self as doorbell, DEFAULT_WAKE_QUEUE_CAPACITY, Wake};
use futures_util::future::select_all;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use self::ledger::{Ledger, MISSED_PREFIX, P50_PREFIX, P99_PREFIX};
use super::publish::{PROBE_INTERVAL, RUNG_DEADLINE};
use crate::datastores::Datastores;
use crate::error::Result;
use crate::lane::joined;
use crate::report::Report;

/// Hubs listening, rung by rung.
pub(super) const HUB_LADDER: [usize; 3] = [1, 2, 4];

/// Rings each rung times.
pub(super) const RINGS: u64 = 1_000;

/// Rings sent through every hub before the timed ones, and not timed.
const WARMUP: u64 = 64;

/// Unresolved rings the ringer allows: the doorbell's hint cap.
pub(super) const WINDOW: usize = DEFAULT_WAKE_QUEUE_CAPACITY;

/// The task role a lost shard listener is reported under.
const LISTENER_ROLE: &str = "wake listener";

/// Run every rung and write its three numbers.
///
/// # Errors
///
/// A replica connection or subscription that will not open, a probe the
/// entropy source cannot mint, or a listener task that was lost. A ring no hub
/// took is a measurement, not an error.
pub(super) async fn ladder(stores: &Datastores, report: &mut Report) -> Result<()> {
    for hubs in HUB_LADDER {
        rung(stores, hubs).await?.record(report, hubs);
    }
    Ok(())
}

/// One rung: `hubs` replicas listening, [`RINGS`] timed rings.
async fn rung(stores: &Datastores, hubs: usize) -> Result<Ledger> {
    let stop = CancellationToken::new();
    let (mut replicas, mut wakes, mut listeners) = (Vec::new(), Vec::new(), Vec::new());
    for _hub in 0..hubs {
        let replica = stores.replica().await?;
        let wake = Wake::default();
        listeners.push(wake.listen(replica.bus(), &stop).await?);
        wakes.push(wake);
        replicas.push(replica);
    }
    let (entropy, publisher) = (Entropy::new(), &stores.bus);
    for wake in &wakes {
        heard(publisher, wake, &entropy).await?;
    }
    let ring = |fleet: Uuid7| doorbell::publish(publisher, fleet);
    drive(&wakes, WARMUP, &entropy, ring).await?;
    let measured = drive(&wakes, RINGS, &entropy, ring).await;
    stop.cancel();
    for listener in &mut listeners {
        stopped(listener).await?;
    }
    for replica in replicas {
        replica.close().await?;
    }
    measured
}

/// Ring probes until `wake` takes one, or until the rung's deadline, then
/// leave its queue empty. A hub that never hears is not an error: its share of
/// the timed rings then shows as misses.
async fn heard(publisher: &afd_bus::Handle, wake: &Wake, entropy: &Entropy) -> Result<()> {
    let deadline = Instant::now() + RUNG_DEADLINE;
    loop {
        let probe = entropy.uuid7(af_core::clock::now())?;
        doorbell::publish(publisher, probe.clone()).await;
        tokio::time::sleep(PROBE_INTERVAL).await;
        let mut answered = false;
        while let Some(fleet) = wake.take() {
            answered |= fleet == probe;
        }
        if answered || Instant::now() >= deadline {
            return Ok(());
        }
    }
}

/// Ring `rings` fresh fleets through `ring`, never more than [`WINDOW`]
/// unresolved, and resolve each against the first of `hubs` to take it.
///
/// One task owns the ledger, every hub's queue and the ringer, so nothing is
/// shared and a take is stamped the moment the loop sees it.
///
/// # Errors
///
/// A fleet the entropy source cannot mint, a latency the histogram will not
/// hold, or a hub whose doorbell closed.
async fn drive<R, F>(hubs: &[Wake], rings: u64, entropy: &Entropy, mut ring: R) -> Result<Ledger>
where
    R: FnMut(Uuid7) -> F,
    F: Future<Output = ()>,
{
    let mut ledger = Ledger::new()?;
    // Subscribed before the first take, so a ring that lands between a drain
    // and the wait below still wakes the loop.
    let mut generations: Vec<_> = hubs.iter().map(Wake::generations).collect();
    let mut remaining = rings;
    loop {
        for wake in hubs {
            while let Some(fleet) = wake.take() {
                ledger.took(&fleet, Instant::now())?;
            }
        }
        ledger.expire(Instant::now());
        if remaining > 0 && ledger.outstanding() < WINDOW {
            let fleet = entropy.uuid7(af_core::clock::now())?;
            ledger.rung(fleet.clone(), Instant::now());
            ring(fleet).await;
            remaining -= 1;
            continue;
        }
        let Some(expiry) = ledger.next_expiry() else {
            return Ok(ledger);
        };
        tokio::select! {
            (changed, _hub, _rest) = select_all(generations.iter_mut().map(|seen| Box::pin(seen.changed()))) => changed?,
            () = tokio::time::sleep_until(expiry) => {}
        }
    }
}

/// Wait for every shard listener of one hub to return after the stop.
async fn stopped(listener: &mut JoinSet<()>) -> Result<()> {
    while let Some(done) = listener.join_next().await {
        joined(done, LISTENER_ROLE)?;
    }
    Ok(())
}

/// One summary line per hub rung the report holds, in ladder order.
pub(super) fn summary_lines(report: &Report) -> Vec<String> {
    HUB_LADDER
        .iter()
        .filter_map(|hubs| {
            let fields: Vec<String> = [P50_PREFIX, P99_PREFIX, MISSED_PREFIX]
                .iter()
                .filter_map(|prefix| {
                    let key = format!("{prefix}{hubs}");
                    let value = report.measurements.get(&key)?;
                    Some(super::pair(&key, value))
                })
                .collect();
            (!fields.is_empty()).then(|| fields.join(" "))
        })
        .collect()
}

#[cfg(test)]
mod tests;
