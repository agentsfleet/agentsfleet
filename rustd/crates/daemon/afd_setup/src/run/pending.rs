//! Chat ownership ends at Stop; the old attempt still owns its accounting.

use super::{FINISHED, sql};
use crate::{Result, draft::read::Locked, error::raise::query};
use sqlx::PgConnection;

const CONTEXT: &str = "read current authoring work";

#[cfg(feature = "test-util")]
impl crate::Drafts {
    /// The compiled admission statement, for real database query-plan measurements.
    pub const CURRENT_ATTEMPT_QUERY: &'static str = sql::PENDING;
}

/// Whether a running attempt still holds the draft. The caller holds the draft
/// lock; an attempt whose turn Stop cancelled remains available to reports,
/// and an attempt awaiting accounting no longer holds the draft.
pub(crate) async fn running(connection: &mut PgConnection, draft: &Locked) -> Result<bool> {
    let running: Option<bool> = sqlx::query_scalar(sql::PENDING)
        .bind(&draft.id)
        .bind(&FINISHED[..])
        .bind(crate::types::CANCELLED)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(running.is_some())
}
