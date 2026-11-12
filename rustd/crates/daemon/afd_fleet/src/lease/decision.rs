//! Freeze the locked host's reviewed selection through the shared runner resolver.

use crate::Result;
use af_core::id::Uuid7;
use af_wire::decision::DecisionAssignment;
use sqlx::PgConnection;

/// Caller keeps the host lock through writing both lease and snapshot.
pub(super) async fn freeze(
    connection: &mut PgConnection,
    runner: &Uuid7,
    decision_selected: bool,
) -> Result<Option<Box<DecisionAssignment>>> {
    if !decision_selected {
        return Ok(None);
    }
    Ok(afd_runner::decision_assignment_on(connection, runner)
        .await?
        .map(|selected| selected.assignment))
}
