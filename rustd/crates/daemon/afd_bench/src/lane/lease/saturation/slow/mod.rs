//! A slow writer: one tenant's drafts held locked, over and over, while the
//! window runs.
//!
//! The failure this workstream exists to measure is a chat claim arriving
//! while some other write holds a draft's lock — a slow revision apply, a
//! long edit — and waiting on it with its host row held, so every other
//! tenant's message on that poll waits too. Production writes that slow are
//! rare and unscheduled; this makes one deterministic. Every [`HOLD`] it takes
//! the first chat tenant's draft rows `FOR UPDATE` on a connection of its own,
//! keeps them, and lets go for [`GAP`]. A claim that waits on them shows in the
//! chat lease tail; a claim that skips them does not.
//!
//! The connection is outside the pool on purpose: a writer holding a pooled
//! connection would also shrink the pool, and the two effects would read as
//! one.

use core::time::Duration;

use sqlx::{Connection as _, PgConnection};

use super::requester::Shared;
use crate::error::{ErrorKind, Result};

/// How long one hold keeps the tenant's drafts locked.
pub(super) const HOLD: Duration = Duration::from_secs(1);

/// How long the writer lets go between holds.
pub(super) const GAP: Duration = Duration::from_millis(250);

/// Every draft row of one tenant, locked.
const HOLD_DRAFTS: &str = include_str!("../sql/hold_drafts.sql");

/// Names the writer's session, so the hold sampler can leave its holds out.
const NAME_WRITER: &str = include_str!("../sql/name_writer.sql");

/// The writer's session name.
pub(super) const APPLICATION: &str = "bench-slow-writer";

/// Name `connection` as the slow writer's.
///
/// # Errors
///
/// `SamplerUnavailable` when the connection would not answer.
pub(super) async fn named(mut connection: PgConnection) -> Result<PgConnection> {
    sqlx::query(NAME_WRITER)
        .bind(APPLICATION)
        .execute(&mut connection)
        .await
        .map_err(|source| ErrorKind::SamplerUnavailable { source })?;
    Ok(connection)
}

/// What the writer holds its locks through.
pub(super) trait Locker {
    /// Lock `tenant`'s drafts, keep them for `hold`, then commit.
    ///
    /// # Errors
    ///
    /// `SamplerUnavailable` when the connection would not answer.
    fn hold(&mut self, tenant: &str, hold: Duration) -> impl Future<Output = Result<()>> + Send;
}

impl Locker for PgConnection {
    async fn hold(&mut self, tenant: &str, hold: Duration) -> Result<()> {
        let unavailable = |source| ErrorKind::SamplerUnavailable { source };
        let mut tx = self.begin().await.map_err(unavailable)?;
        sqlx::query(HOLD_DRAFTS)
            .bind(tenant)
            .fetch_all(&mut *tx)
            .await
            .map_err(unavailable)?;
        tokio::time::sleep(hold).await;
        tx.commit().await.map_err(unavailable)?;
        Ok(())
    }
}

/// Hold `tenant`'s drafts every [`HOLD`] while the window is open, answering
/// how many holds were taken.
///
/// # Errors
///
/// The locker's connection would not answer.
pub(super) async fn write<L: Locker>(locker: &mut L, tenant: &str, shared: &Shared) -> Result<u64> {
    let mut holds = 0;
    while shared.open() {
        locker.hold(tenant, HOLD).await?;
        holds += 1;
        tokio::time::sleep(GAP).await;
    }
    Ok(holds)
}

#[cfg(test)]
mod tests;
