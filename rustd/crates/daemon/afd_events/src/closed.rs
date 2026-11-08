//! What ending an event hands back: the row, and the fleet facts beside it.
//!
//! Both closing statements — the runner's verdict and a gate's refusal — end
//! with the same select, so one decoder reads both. The leading columns are
//! [`EventRow`]'s own and are read by its own typed row; the four after them
//! are the fleet's lifecycle status, its pending gate count, and
//! its two activity counters, which the live tail's completion frame carries
//! so a watcher folds the ending in without a read.

use sqlx::FromRow as _;
use sqlx::postgres::PgRow;

use afd_api_wire::tail::FleetCounters;

use crate::error::{Error, row_malformed};
use crate::history::{EventRow, Stored};

/// What a refused decode is reported as.
const ROW: &str = "closed event row";

/// The closing as the statement selects it: the row by its own names, then
/// the four fleet facts by theirs.
#[derive(sqlx::FromRow)]
struct StoredClosing {
    #[sqlx(flatten)]
    row: Stored,
    fleet_status: String,
    pending_approvals: i64,
    event_count: i64,
    budget_used_nanos: i64,
}

/// One ended event, with the fleet as it stood at the ending.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closed {
    /// The terminal row, as the events list would serve it.
    pub row: EventRow,
    /// The fleet's lifecycle status after the run.
    pub fleet_status: String,
    /// How many approvals wait on the fleet after the run.
    pub pending_approvals: i64,
    /// Where the fleet's counters stand at the ending — absolute, so the
    /// completion frame's watcher assigns them rather than adding to them.
    pub counters: FleetCounters,
}

impl Closed {
    /// Decode one closing, naming the column that refused.
    ///
    /// # Errors
    /// [`Error::RowMalformed`] when a column is not the type this build reads.
    pub fn read(row: &PgRow) -> Result<Self, Error> {
        let stored = StoredClosing::from_row(row).map_err(row_malformed(ROW))?;
        Ok(Self {
            row: EventRow::from(stored.row),
            fleet_status: stored.fleet_status,
            pending_approvals: stored.pending_approvals,
            counters: FleetCounters {
                events_processed: stored.event_count,
                budget_used_nanos: stored.budget_used_nanos,
            },
        })
    }
}
