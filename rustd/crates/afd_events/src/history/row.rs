//! One row of history, as an operator sees it.
//!
//! # Decoded by name, then mapped
//!
//! [`Stored`] is the row as the table spells it — `token_count`,
//! `failure_reason` — decoded by `sqlx::FromRow`, which binds by COLUMN NAME,
//! so a `SELECT` list edited into another order cannot swap two same-typed
//! columns the way a positional decoder could. [`EventRow`] is the row as the
//! wire spells it (`afd_wire::event::EventSummary`), and the one `From` between
//! them is the only place the two vocabularies meet.

use std::borrow::Cow;

use afd_wire::event::EventSummary;
use jiff::Timestamp;
use sqlx::FromRow as _;
use sqlx::postgres::PgRow;

use crate::error::{Error, row_malformed};

/// What a refused row decode is reported as: the row, not one column, since
/// `sqlx` names the column in the cause it carries.
const ROW: &str = "event row";

/// One event, with everything the console renders about it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EventRow {
    /// The fleet this event belongs to.
    pub fleet_id: String,
    /// The canonical event identifier.
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
    /// Wall milliseconds the run took, absent until a runner reports. A span,
    /// so it stays a count.
    pub wall_ms: Option<i64>,
    /// What refused or failed the run, absent on a clean one.
    pub failure_label: Option<String>,
    /// The operator-readable cause line, absent when none was carried.
    pub failure_detail: Option<String>,
    /// The event this one continues, set on a continuation.
    pub resumes_event_id: Option<String>,
    /// When the row was created.
    pub created_at: Timestamp,
    /// When the row last changed.
    pub updated_at: Timestamp,
    /// Summed `charged_nanos` over this event's charge rows.
    ///
    /// `None` when the event recorded no charge. That is rendered as UNKNOWN
    /// and never as a zero charge — an unbilled run and a free run are
    /// different facts. Cost is server truth.
    pub cost_nanos: Option<i64>,
}

/// The row as `fleet.events` spells it.
#[derive(sqlx::FromRow)]
pub(crate) struct Stored {
    fleet_id: String,
    event_id: String,
    workspace_id: String,
    actor: String,
    event_type: String,
    status: String,
    token_count: Option<i64>,
    wall_ms: Option<i64>,
    failure_reason: Option<String>,
    failure_detail: Option<String>,
    resumes_event_id: Option<String>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: Timestamp,
    cost_nanos: Option<i64>,
}

impl From<Stored> for EventRow {
    fn from(stored: Stored) -> Self {
        Self {
            fleet_id: stored.fleet_id,
            event_id: stored.event_id,
            workspace_id: stored.workspace_id,
            actor: stored.actor,
            event_type: stored.event_type,
            status: stored.status,
            tokens: stored.token_count,
            wall_ms: stored.wall_ms,
            failure_label: stored.failure_reason,
            failure_detail: stored.failure_detail,
            resumes_event_id: stored.resumes_event_id,
            created_at: stored.created_at,
            updated_at: stored.updated_at,
            cost_nanos: stored.cost_nanos,
        }
    }
}

impl EventRow {
    /// Decode one row by column name.
    ///
    /// # Errors
    /// [`Error::RowMalformed`] when a column is missing or not the type this
    /// build reads — a schema this binary was not built for, not a caller's
    /// mistake.
    pub(crate) fn read(row: &PgRow) -> Result<Self, Error> {
        Stored::from_row(row)
            .map(Self::from)
            .map_err(row_malformed(ROW))
    }

    /// This row as the wire serves it — the listing's shape, borrowed.
    ///
    /// One mapping for the two surfaces that emit a row: the events listing
    /// and the live tail's completion frame. Declared on the row rather than
    /// beside either emitter, so a column added here cannot reach one and not
    /// the other.
    #[must_use]
    pub fn summary(&self) -> EventSummary<'_> {
        EventSummary {
            fleet_id: Cow::Borrowed(&self.fleet_id),
            event_id: Cow::Borrowed(&self.event_id),
            workspace_id: Cow::Borrowed(&self.workspace_id),
            actor: Cow::Borrowed(&self.actor),
            event_type: Cow::Borrowed(&self.event_type),
            status: Cow::Borrowed(&self.status),
            tokens: self.tokens,
            wall_ms: self.wall_ms,
            failure_label: self.failure_label.as_deref().map(Cow::Borrowed),
            failure_detail: self.failure_detail.as_deref().map(Cow::Borrowed),
            resumes_event_id: self.resumes_event_id.as_deref().map(Cow::Borrowed),
            created_at: self.created_at,
            updated_at: self.updated_at,
            cost_nanos: self.cost_nanos,
        }
    }
}
