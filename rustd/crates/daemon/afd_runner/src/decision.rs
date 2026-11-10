//! Selection stays under the caller's runner mutation and catalogue row lock.

use af_core::id::Uuid7;
use sqlx::PgConnection;

use crate::{
    Result,
    error::{query, rejected},
    sql,
};

const CONTEXT: &str = "validate runner decision model";
const DETAIL_UNAVAILABLE: &str = "The selected decision model needs an enabled reviewed profile";

/// A requested identity whose catalogue row was checked in this transaction.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Selection<'a> {
    Unassigned,
    Enabled,
    Unavailable(&'a Uuid7),
}

impl Selection<'_> {
    /// Unrelated policy edits may retain a disabled selection; dispatch is fenced separately.
    pub(crate) fn allow_change_from(self, current: Option<&str>) -> Result<()> {
        match self {
            Self::Unassigned | Self::Enabled => Ok(()),
            Self::Unavailable(model) if current == Some(model.as_str()) => Ok(()),
            Self::Unavailable(_) => Err(rejected(DETAIL_UNAVAILABLE)),
        }
    }
}

/// Catalogue locks precede runner locks, matching model deletion's FK lock order.
pub(crate) async fn lock<'a>(
    connection: &mut PgConnection,
    model: Option<&'a Uuid7>,
) -> Result<Selection<'a>> {
    let Some(model) = model else {
        return Ok(Selection::Unassigned);
    };
    let available = sqlx::query_scalar::<_, bool>(sql::runner::SELECT_DECISION_MODEL)
        .bind(model)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(if available == Some(true) {
        Selection::Enabled
    } else {
        Selection::Unavailable(model)
    })
}
