//! Lock before checking a gate, so its answer cannot be followed by a stale park.

use crate::{
    error::{Result, query},
    lease::{
        affinity::Fence,
        sql::{self, queue},
        store::Leases,
    },
};
use afd_core::id::Uuid7;
use sqlx::Connection as _;
const CONTEXT: &str = "admission queue transition";
/// Statement name for the lapsed-park sweep's failures.
const CONTEXT_LAPSED: &str = "lapsed park sweep";
/// How long the sweep's unpark waits on an admission row lock: its slots are
/// already held, so a longer wait is a stalled writer, and the next pass retries.
const SWEEP_LOCK_TIMEOUT: &str = "2000ms";
/// How long any one statement of the sweep may run.
const SWEEP_STATEMENT_TIMEOUT: &str = "5000ms";
/// What a park sends: its transaction's begin and commit around the lock, the
/// park and the release.
const PARK_STATEMENTS: u64 = 5;

impl Leases {
    /// Park and free the slot atomically. An answer that already committed is
    /// visible to the second statement; an answer waiting on this lock un-parks
    /// after this transaction commits. Neither ordering can strand the row.
    ///
    /// # Errors
    /// Preserves database failures. A superseded claim changes nothing.
    pub async fn park(&self, fleet: &Uuid7, event: &str, fence: Fence) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        // Begin, lock, park, release, commit.
        self.tally().statements(PARK_STATEMENTS);
        let mut transaction = connection.begin().await.map_err(query(CONTEXT))?;
        if self
            .lock_admission_on(&mut transaction, fleet, event, fence)
            .await?
        {
            sqlx::query(queue::PARK_WAITING)
                .bind(fleet.as_str())
                .bind(event)
                .bind(fence.as_i64())
                .execute(&mut *transaction)
                .await
                .map_err(query(CONTEXT))?;
            sqlx::query(sql::lease::RELEASE_UNLEASED_SLOT)
                .bind(fleet.as_str())
                .bind(fence.as_i64())
                .bind(std::time::Duration::ZERO)
                .execute(&mut *transaction)
                .await
                .map_err(query(CONTEXT))?;
        }
        transaction.commit().await.map_err(query(CONTEXT))?;
        Ok(())
    }

    pub(crate) async fn lock_admission_on(
        &self,
        connection: &mut sqlx::PgConnection,
        fleet: &Uuid7,
        event: &str,
        fence: Fence,
    ) -> Result<bool> {
        sqlx::query_scalar(queue::LOCK_ADMISSION)
            .bind(fleet.as_str())
            .bind(event)
            .bind(fence.as_i64())
            .fetch_one(connection)
            .await
            .map_err(query(CONTEXT))
    }

    /// Re-arms every fleet whose parked head no card holds any more: no
    /// standing card is pending, and every pending card on the head is past its
    /// deadline. Answers how many fleets it re-armed.
    ///
    /// The cards are left pending. A person may still answer an event card
    /// late, and only an answer resolves standing access, so this pass expires
    /// nothing: it only lets the head be claimed again, where the approval gate
    /// decides what a lapsed card means for the run. The slots are locked in
    /// fleet order in a statement of their own before the unpark, so the pass
    /// keeps the slot-first order and two replicas' passes cannot deadlock.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn resume_lapsed_parks(&self) -> Result<u64> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_LAPSED))?;
        sqlx::query(queue::BOUND_TRANSACTION)
            .bind(SWEEP_LOCK_TIMEOUT)
            .bind(SWEEP_STATEMENT_TIMEOUT)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_LAPSED))?;
        let fleets: Vec<String> = sqlx::query_scalar(queue::LOCK_LAPSED_PARK_SLOTS)
            .fetch_all(&mut *transaction)
            .await
            .map_err(query(CONTEXT_LAPSED))?;
        if fleets.is_empty() {
            return Ok(0);
        }
        let resumed: Vec<String> = sqlx::query_scalar(queue::UNPARK_LAPSED_HEADS)
            .bind(&fleets)
            .fetch_all(&mut *transaction)
            .await
            .map_err(query(CONTEXT_LAPSED))?;
        transaction.commit().await.map_err(query(CONTEXT_LAPSED))?;
        drop(connection);
        for fleet in &resumed {
            afd_dragonfly::doorbell::publish(self.queue(), fleet).await;
        }
        Ok(u64::try_from(resumed.len()).unwrap_or(u64::MAX))
    }
}
