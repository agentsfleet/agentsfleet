//! One event expanded: the listing row, and the bodies beside it.
//!
//! # Why this is not seventeen fields
//!
//! An expanded event is a listing row plus a trigger payload plus an answer.
//! Spelling all fifteen shared columns again here would be fifteen chances for
//! the two decoders to disagree — and two of them, `failure_label` and
//! `failure_detail`, are the same Rust type, so a column order that moved would
//! swap them with nothing failing. Holding the [`EventRow`] instead means there
//! is exactly one decoder for those columns, and it is the one every listing
//! already exercises.
//!
//! The bodies are therefore selected LAST rather than in wire order. SQL column
//! order and JSON field order are independent; only the second is a contract a
//! client reads, and it is `afd_wire::event::EventDetail` that declares it.
//!
//! # Decoded by name
//!
//! [`StoredDetail`] flattens the listing's own typed row and adds the three
//! bodies, all bound by column name through `sqlx::FromRow`, so neither half
//! counts offsets into the other's column list.

use sqlx::FromRow as _;
use sqlx::postgres::PgRow;

use super::row::{EventRow, Stored};
use crate::error::{Error, row_malformed};

/// What a refused decode is reported as.
const ROW: &str = "event detail row";

/// The row as the detail and thread reads select it.
#[derive(sqlx::FromRow)]
struct StoredDetail {
    #[sqlx(flatten)]
    row: Stored,
    request_json: String,
    response_text: Option<String>,
    tool_calls: Option<String>,
}

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
    /// The run's tool trace as stored, serialized to text.
    ///
    /// `None` when nothing was recorded: a run before the column existed, a
    /// runner that sent no trace, or a trace the daemon dropped. Never read as
    /// "no tools ran".
    pub tool_calls: Option<String>,
}

impl EventDetailRow {
    /// Decode one row, naming the column that refused.
    ///
    /// # Errors
    /// [`Error::RowMalformed`] when a column is not the type this build reads —
    /// which is a schema this binary was not built for, not a caller's mistake.
    pub(crate) fn read(row: &PgRow) -> Result<Self, Error> {
        let stored = StoredDetail::from_row(row).map_err(row_malformed(ROW))?;
        Ok(Self {
            row: EventRow::from(stored.row),
            request_json: stored.request_json,
            response_text: stored.response_text,
            tool_calls: stored.tool_calls,
        })
    }
}

/// The fleet every fixture row belongs to.
#[cfg(feature = "test-util")]
const FIXTURE_FLEET_ID: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// The workspace that fleet belongs to.
#[cfg(feature = "test-util")]
const FIXTURE_WORKSPACE_ID: &str = "01924f4e-0000-7000-8000-000000000001";

/// The actor a fixture row records — a steer, which is what a thread carries.
#[cfg(feature = "test-util")]
const FIXTURE_ACTOR: &str = "steer:user_2fixture";

/// The event type a thread row carries.
#[cfg(feature = "test-util")]
const FIXTURE_EVENT_TYPE: &str = "chat";

/// A finished run's status.
#[cfg(feature = "test-util")]
const FIXTURE_STATUS: &str = "processed";

/// The trigger payload a steer stores.
#[cfg(feature = "test-util")]
const FIXTURE_REQUEST_JSON: &str = r#"{"message":"hello"}"#;

/// The row a suite outside this crate needs to CONSTRUCT.
///
/// M-TEST-UTIL. A thread page's byte budget and the cursor it hands back are
/// decided by `afd_api`'s message handler, and neither decision reaches
/// Postgres — so proving them needs a row this crate would otherwise only ever
/// decode out of a `PgRow`. `#[non_exhaustive]` is what makes that impossible
/// without a seam, and this is the whole seam: three parameters, because the
/// identity and the answer's SIZE are the only columns those two decisions
/// read. Every other column is fixed, so a suite cannot accidentally make its
/// claim depend on one.
#[cfg(feature = "test-util")]
impl EventDetailRow {
    /// One row of a thread, identified by `event_id` at `created_at`.
    #[must_use]
    pub fn fixture(event_id: &str, created_at: jiff::Timestamp, response_text: String) -> Self {
        Self {
            row: EventRow {
                fleet_id: FIXTURE_FLEET_ID.to_owned(),
                event_id: event_id.to_owned(),
                workspace_id: FIXTURE_WORKSPACE_ID.to_owned(),
                actor: FIXTURE_ACTOR.to_owned(),
                event_type: FIXTURE_EVENT_TYPE.to_owned(),
                status: FIXTURE_STATUS.to_owned(),
                tokens: None,
                wall_ms: None,
                failure_label: None,
                failure_detail: None,
                resumes_event_id: None,
                created_at,
                updated_at: created_at,
                cost_nanos: None,
            },
            request_json: FIXTURE_REQUEST_JSON.to_owned(),
            response_text: Some(response_text),
            tool_calls: None,
        }
    }

    /// The same row, carrying `tool_calls` as its stored trace.
    #[must_use]
    pub fn with_tool_calls(self, tool_calls: String) -> Self {
        Self {
            tool_calls: Some(tool_calls),
            ..self
        }
    }
}
