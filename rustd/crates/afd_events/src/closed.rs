//! What ending an event hands back: the row, and the fleet facts beside it.
//!
//! Both closing statements — the runner's verdict and a gate's refusal — end
//! with the same select, so one decoder reads both. The fifteen leading
//! columns are [`EventRow`]'s own and are read by its own decoder; the two
//! after them are the fleet's lifecycle status and its pending gate count,
//! which the live tail's completion frame carries so a watcher folds the
//! ending in without a read.

use sqlx::Row as _;
use sqlx::postgres::PgRow;

use crate::error::{Error, row_malformed};
use crate::history::EventRow;

/// The column the fleet's status sits in, after the row's own.
const COLUMN_FLEET_STATUS: usize = EventRow::COLUMNS;

/// The column the pending gate count sits in.
const COLUMN_PENDING_APPROVALS: usize = EventRow::COLUMNS + 1;

/// One ended event, with the fleet as it stood at the ending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    /// The terminal row, as the events list would serve it.
    pub row: EventRow,
    /// The fleet's lifecycle status after the run.
    pub fleet_status: String,
    /// How many approvals wait on the fleet after the run.
    pub pending_approvals: i64,
}

impl Closed {
    /// Decode one closing, naming the column that refused.
    ///
    /// # Errors
    /// [`Error::RowMalformed`] when a column is not the type this build reads.
    pub fn read(row: &PgRow) -> Result<Self, Error> {
        Ok(Self {
            row: EventRow::read(row)?,
            fleet_status: row
                .try_get(COLUMN_FLEET_STATUS)
                .map_err(row_malformed("fleet_status"))?,
            pending_approvals: row
                .try_get(COLUMN_PENDING_APPROVALS)
                .map_err(row_malformed("pending_approvals"))?,
        })
    }
}
