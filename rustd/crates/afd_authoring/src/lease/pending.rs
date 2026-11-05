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

#[derive(PartialEq, Eq)]
pub(crate) enum Pending {
    Running,
    UsageUnknown,
}

/// The caller holds the draft lock. Admission permits only one running attempt
/// above its watermark; stopped attempts remain available to reports, and an
/// attempt awaiting accounting no longer holds the draft.
pub(crate) async fn current(
    connection: &mut PgConnection,
    draft: &Locked,
) -> Result<Option<Pending>> {
    let unknown: Option<bool> = sqlx::query_scalar(sql::PENDING)
        .bind(draft.id.as_str())
        .bind(&FINISHED[..])
        .bind(draft.cancelled_through_sequence)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(unknown.map(|unknown| {
        if unknown {
            Pending::UsageUnknown
        } else {
            Pending::Running
        }
    }))
}
