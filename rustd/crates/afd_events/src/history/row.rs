//! One row of history, as an operator sees it.

use sqlx::Row as _;
use sqlx::postgres::PgRow;

use crate::error::{Error, row_malformed};

/// One event, with everything the console renders about it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EventRow {
    /// The fleet this event belongs to.
    pub fleet_id: String,
    /// The canonical event identifier — the stream entry id that produced it.
    pub event_id: String,
    /// The workspace the fleet belongs to.
    pub workspace_id: String,
    /// Who or what produced the event.
    pub actor: String,
    /// How the event entered the system, as stored.
    pub event_type: String,
    /// Where the event's run got to, as stored.
    pub status: String,
    /// Tokens the run spent, absent until a runner reports.
    pub tokens: Option<i64>,
    /// Wall milliseconds the run took, absent until a runner reports.
    pub wall_ms: Option<i64>,
    /// What refused or failed the run, absent on a clean one.
    pub failure_label: Option<String>,
    /// The operator-readable cause line, absent when none was carried.
    pub failure_detail: Option<String>,
    /// The session checkpoint this run wrote, when it wrote one.
    pub checkpoint_id: Option<String>,
    /// The event this one continues, set on a continuation.
    pub resumes_event_id: Option<String>,
    /// Epoch milliseconds the row was created.
    pub created_at: i64,
    /// Epoch milliseconds the row last changed.
    pub updated_at: i64,
    /// Summed `credit_deducted_nanos` over this event's telemetry rows.
    ///
    /// `None` when the event recorded no telemetry. That is rendered as
    /// UNKNOWN and never as a zero charge — an unbilled run and a free run are
    /// different facts, and a client deriving cost from tokens would be
    /// inventing the second from the first. Cost is server truth.
    pub cost_nanos: Option<i64>,
}

impl EventRow {
    /// Decode one row, naming the column that refused.
    ///
    /// # Errors
    /// [`Error::RowMalformed`] when a column is not the type this build reads —
    /// which is a schema this binary was not built for, not a caller's mistake.
    pub(crate) fn read(row: &PgRow) -> Result<Self, Error> {
        Ok(Self {
            fleet_id: row.try_get(0).map_err(row_malformed("fleet_id"))?,
            event_id: row.try_get(1).map_err(row_malformed("event_id"))?,
            workspace_id: row.try_get(2).map_err(row_malformed("workspace_id"))?,
            actor: row.try_get(3).map_err(row_malformed("actor"))?,
            event_type: row.try_get(4).map_err(row_malformed("event_type"))?,
            status: row.try_get(5).map_err(row_malformed("status"))?,
            tokens: row.try_get(6).map_err(row_malformed("tokens"))?,
            wall_ms: row.try_get(7).map_err(row_malformed("wall_ms"))?,
            failure_label: row.try_get(8).map_err(row_malformed("failure_label"))?,
            failure_detail: row.try_get(9).map_err(row_malformed("failure_detail"))?,
            checkpoint_id: row.try_get(10).map_err(row_malformed("checkpoint_id"))?,
            resumes_event_id: row.try_get(11).map_err(row_malformed("resumes_event_id"))?,
            created_at: row.try_get(12).map_err(row_malformed("created_at"))?,
            updated_at: row.try_get(13).map_err(row_malformed("updated_at"))?,
            cost_nanos: row.try_get(14).map_err(row_malformed("cost_nanos"))?,
        })
    }
}
