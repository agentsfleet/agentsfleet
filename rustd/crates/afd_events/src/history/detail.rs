//! One event expanded: the listing row, and the two bodies beside it.
//!
//! # Why this is not seventeen fields
//!
//! An expanded event is a listing row plus a trigger payload plus an answer.
//! Spelling all fifteen shared columns again here would be fifteen chances for
//! the two decoders to disagree — and two of them, `failure_label` and
//! `checkpoint_id`, are the same Rust type, so a column order that moved would
//! swap them with nothing failing. Holding the [`EventRow`] instead means there
//! is exactly one decoder for those columns, and it is the one every listing
//! already exercises.
//!
//! The bodies are therefore selected LAST rather than in wire order. SQL column
//! order and JSON field order are independent; only the second is a contract a
//! client reads, and it is `afd_wire::event::EventDetail` that declares it.
//!
//! # And why these two are read by NAME
//!
//! `EventRow::read` reads by index because it decodes a whole statement it owns
//! end to end. These two are spliced onto that statement, so an index here
//! would be a hand-counted offset into somebody else's column list — the exact
//! coupling this file exists to avoid. Postgres names the output column after
//! the column being cast, so `request_json::text` arrives as `request_json`.

use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::row::EventRow;
use crate::error::{Error, row_malformed};

/// The stored trigger payload's column, named once (RULE UFS).
const COLUMN_REQUEST_JSON: &str = "request_json";

/// The agent's answer's column, named once (RULE UFS).
const COLUMN_RESPONSE_TEXT: &str = "response_text";

/// One event with everything recorded about it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EventDetailRow {
    /// Everything the listing already carries about this event.
    pub row: EventRow,
    /// The trigger payload as stored, serialized to text.
    pub request_json: String,
    /// The agent's full answer.
    ///
    /// `None` while a run is in flight, and on a run that failed before
    /// producing one.
    pub response_text: Option<String>,
}

impl EventDetailRow {
    /// Decode one row, naming the column that refused.
    ///
    /// # Errors
    /// [`Error::RowMalformed`] when a column is not the type this build reads —
    /// which is a schema this binary was not built for, not a caller's mistake.
    pub(crate) fn read(row: &PgRow) -> Result<Self, Error> {
        Ok(Self {
            row: EventRow::read(row)?,
            request_json: row
                .try_get(COLUMN_REQUEST_JSON)
                .map_err(row_malformed(COLUMN_REQUEST_JSON))?,
            response_text: row
                .try_get(COLUMN_RESPONSE_TEXT)
                .map_err(row_malformed(COLUMN_RESPONSE_TEXT))?,
        })
    }
}
