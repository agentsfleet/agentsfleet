//! What the pool itself suffers under saturation, measured apart from the
//! verbs that saturate it.
//!
//! # Wait, from the requester's side
//!
//! A poll's latency under saturation is mostly the acquire queue, and folding
//! the two would report the harness's queue as the path's cost. So a probe
//! takes one pooled connection every [`PROBE_EVERY`], times the acquire alone,
//! and hands it straight back: the wait any requester would have seen then.
//!
//! # Hold, from the server's side
//!
//! How long a connection stays checked out is the transaction it carries, and
//! only Postgres sees every one of those. A sampler on a connection of its own
//! — never a pooled one, which would queue behind what it measures — reads
//! the age of every open transaction on the lane's database every
//! [`SAMPLE_EVERY`].
//!
//! # Recovery, after
//!
//! Once the requesters stop, every connection the pool may open is asked for
//! and held at once ([`Pool::serves`]). All of them arriving inside the acquire
//! budget is what "the pool serves normally afterwards" means; one timing out
//! is a connection the saturation left stuck.

use core::time::Duration;
use std::time::Instant;

use afd_db::Db;
use futures_util::future::join_all;
use sqlx::PgConnection;

use super::requester::Shared;
use crate::error::{ErrorKind, Result};
use crate::report::Latency;

/// How often the probe takes one pooled connection.
const PROBE_EVERY: Duration = Duration::from_millis(20);

/// How often the sampler reads open transactions' ages.
const SAMPLE_EVERY: Duration = Duration::from_millis(20);

/// Milliseconds per second, for the sampler's ages.
const MILLIS_PER_SECOND: f64 = 1_000.0;

/// The age, in milliseconds, of every transaction open on this database but
/// the sampler's own.
const OPEN_TRANSACTIONS: &str = include_str!("../sql/open_transactions.sql");

/// A pool the probe asks for connections.
pub(super) trait Pool: Sync {
    /// Take one connection and hand it straight back, answering whether it
    /// arrived inside the pool's budget.
    fn acquire(&self) -> impl Future<Output = bool> + Send;

    /// Take `count` connections and hold them all at once, answering whether
    /// every one arrived inside the budget.
    fn serves(&self, count: u32) -> impl Future<Output = bool> + Send;
}

impl Pool for Db {
    async fn acquire(&self) -> bool {
        Self::acquire(self).await.is_ok()
    }

    async fn serves(&self, count: u32) -> bool {
        // Collected before any is dropped, so all `count` are out together.
        let held = join_all((0..count).map(|_| Self::acquire(self))).await;
        held.iter().all(core::result::Result::is_ok)
    }
}

/// A view of every transaction open on the database.
pub(super) trait Sampler {
    /// Each open transaction's age, in milliseconds.
    ///
    /// # Errors
    ///
    /// `SamplerUnavailable` when the view would not answer.
    fn ages(&mut self) -> impl Future<Output = Result<Vec<f64>>> + Send;
}

impl Sampler for PgConnection {
    async fn ages(&mut self) -> Result<Vec<f64>> {
        sqlx::query_scalar(OPEN_TRANSACTIONS)
            .fetch_all(self)
            .await
            .map_err(|source| ErrorKind::SamplerUnavailable { source }.into())
    }
}

/// What the probe saw while the window ran.
#[derive(Debug)]
pub(super) struct Watched {
    /// How long one acquire waited.
    pub(super) wait: Latency,
    /// Acquires the pool refused inside its budget.
    pub(super) refused: u64,
    /// How long transactions had been open when sampled.
    pub(super) hold: Latency,
}

/// Probe `pool` and sample `sampler` while the window is open.
///
/// # Errors
///
/// The sampler would not answer, or a latency would not record.
pub(super) async fn watch<P: Pool, S: Sampler + Send>(
    pool: &P,
    sampler: &mut S,
    shared: &Shared,
) -> Result<Watched> {
    let (wait, hold) = tokio::join!(waits(pool, shared), holds(sampler, shared));
    let (wait, refused) = wait?;
    Ok(Watched {
        wait,
        refused,
        hold: hold?,
    })
}

/// Time one acquire at a time while the window is open.
async fn waits<P: Pool>(pool: &P, shared: &Shared) -> Result<(Latency, u64)> {
    let mut wait = Latency::new()?;
    let mut refused = 0;
    while shared.open() {
        let started = Instant::now();
        if pool.acquire().await {
            wait.record(started.elapsed())?;
        } else {
            refused += 1;
        }
        tokio::time::sleep(PROBE_EVERY).await;
    }
    Ok((wait, refused))
}

/// Sample every open transaction's age while the window is open.
async fn holds<S: Sampler>(sampler: &mut S, shared: &Shared) -> Result<Latency> {
    let mut hold = Latency::new()?;
    while shared.open() {
        for age in sampler.ages().await? {
            hold.record(Duration::from_secs_f64(age.max(0.0) / MILLIS_PER_SECOND))?;
        }
        tokio::time::sleep(SAMPLE_EVERY).await;
    }
    Ok(hold)
}

#[cfg(test)]
mod tests;
