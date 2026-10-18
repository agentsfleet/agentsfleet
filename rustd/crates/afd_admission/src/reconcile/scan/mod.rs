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

use afd_dragonfly::EventId;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

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

/// One row of [`sql::SELECT_UNDELIVERED_FLEETS`].
#[derive(sqlx::FromRow)]
struct HeadRow {
    fleet_id: String,
    receipt: String,
}

/// One row of [`sql::SELECT_UNDELIVERED_ON_FLEET`].
#[derive(sqlx::FromRow)]
struct CandidateRow {
    id: String,
    receipt: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    sequence: i64,
}

impl From<HeadRow> for Unfinished {
    fn from(row: HeadRow) -> Self {
        Self {
            fleet_id: row.fleet_id,
            receipt: EventId::of(&row.receipt),
        }
    }
}

impl From<CandidateRow> for Candidate {
    fn from(row: CandidateRow) -> Self {
        Self {
            id: row.id,
            receipt: EventId::of(&row.receipt),
            key: RowKey {
                created_at: row.created_at,
                sequence: row.sequence,
            },
        }
    }
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
        sqlx::query_as::<_, HeadRow>(sql::SELECT_UNDELIVERED_FLEETS)
            .bind(fleets)
            .bind(after)
            .fetch_all(&mut *connection)
            .await
            .map(|rows| rows.into_iter().map(Unfinished::from).collect())
            .map_err(query(CONTEXT_RECONCILE))
    }

    /// Up to `rows` of one fleet's undelivered admissions, after `after`.
    pub(super) async fn undelivered_on(
        &self,
        fleet_id: &str,
        rows: i64,
        after: RowKey,
    ) -> Result<Vec<Candidate>> {
        let mut connection = self.database.acquire().await?;
        sqlx::query_as::<_, CandidateRow>(sql::SELECT_UNDELIVERED_ON_FLEET)
            .bind(fleet_id)
            .bind(rows)
            .bind(after.created_at.to_sqlx())
            .bind(after.sequence)
            .fetch_all(&mut *connection)
            .await
            .map(|found| found.into_iter().map(Candidate::from).collect())
            .map_err(query(CONTEXT_RECONCILE))
    }

    /// Forgets `receipt` on row `id`, if the row still carries it undelivered.
    pub(super) async fn void(&self, id: &str, receipt: &str) -> Result<u64> {
        let mut connection = self.database.acquire().await?;
        let voided = sqlx::query(sql::VOID_LOST_RECEIPT)
            .bind(id)
            .bind(receipt)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT_RECONCILE))?;
        Ok(voided.rows_affected())
    }
}

#[cfg(test)]
mod tests;
