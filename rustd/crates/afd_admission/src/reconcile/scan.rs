//! The reconciliation pass's reads and its one write.
//!
//! Split from the pass that drives them for the reason
//! [`crate::sql`] gives for being split from the code that runs it: the
//! statements here bind five parameters across three shapes, two of them
//! cursors, and a transposition between a batch limit and a resume key
//! compiles clean and quietly narrows recovery. Keeping the binding beside the
//! decoding — and both away from the probe loop — is what makes each one
//! readable on its own.
//!
//! Nothing here holds a connection across a probe. Every read collects its rows
//! and returns its connection to the pool before the caller asks the datastore
//! anything, and every void is its own short statement. A probe is a round trip
//! to the OTHER datastore, and the rows a lock here would hold are the ones a
//! live producer recording its own receipt waits behind.

use afd_core::clock::UnixMillis;
use afd_dragonfly::EventId;
use sqlx::Row as _;

use super::progress::RowKey;
use super::{CONTEXT_RECONCILE, Unfinished};
use crate::error::{Result, query};
use crate::{Admissions, sql};

/// One fleet's undelivered row, as the walk reads it.
pub(super) struct Candidate {
    /// The row to void.
    pub(super) id: String,
    /// The receipt the probe will be answered for.
    pub(super) receipt: EventId,
    /// Where a walk stopping here resumes.
    pub(super) key: RowKey,
}

impl Admissions {
    /// One row per fleet that holds undelivered work: its oldest such row,
    /// starting after `after`.
    pub(super) async fn unfinished_fleets(
        &self,
        fleets: i64,
        after: &str,
    ) -> Result<Vec<Unfinished>> {
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_UNDELIVERED_FLEETS)
            .bind(fleets)
            .bind(after)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECONCILE))?;
        rows.iter()
            .map(|row| {
                Ok(Unfinished {
                    fleet_id: row.try_get(0).map_err(query(CONTEXT_RECONCILE))?,
                    receipt: EventId::of(
                        &row.try_get::<String, _>(1)
                            .map_err(query(CONTEXT_RECONCILE))?,
                    ),
                })
            })
            .collect()
    }

    /// One fleet's receipted-but-undelivered rows after `after`, read and
    /// released.
    ///
    /// Collected rather than streamed so the connection is back in the pool
    /// before the first probe, which is the whole point of the shape.
    pub(super) async fn undelivered_on(
        &self,
        fleet_id: &str,
        rows: i64,
        after: RowKey,
    ) -> Result<Vec<Candidate>> {
        let mut connection = self.database.acquire().await?;
        let unfinished = sqlx::query(sql::SELECT_UNDELIVERED_ON_FLEET)
            .bind(fleet_id)
            .bind(rows)
            .bind(after.created_at)
            .bind(after.seq)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECONCILE))?;
        unfinished
            .iter()
            .map(|row| {
                let id: String = row.try_get(0).map_err(query(CONTEXT_RECONCILE))?;
                let receipt: String = row.try_get(1).map_err(query(CONTEXT_RECONCILE))?;
                let created_at: i64 = row.try_get(2).map_err(query(CONTEXT_RECONCILE))?;
                let seq: i64 = row.try_get(3).map_err(query(CONTEXT_RECONCILE))?;
                Ok(Candidate {
                    id,
                    receipt: EventId::of(&receipt),
                    key: RowKey { created_at, seq },
                })
            })
            .collect()
    }

    /// Forgets one row's receipt, answering whether this statement did it.
    ///
    /// Zero is not a failure: it means the row no longer carries the receipt
    /// this pass probed, so somebody else already repaired it or a delivery
    /// landed first.
    pub(super) async fn void(&self, id: &str, receipt: &str, now: UnixMillis) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let voided = sqlx::query(sql::VOID_LOST_RECEIPT)
            .bind(id)
            .bind(now.as_millis())
            .bind(receipt)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECONCILE))?;
        Ok(voided.rows_affected())
    }
}
